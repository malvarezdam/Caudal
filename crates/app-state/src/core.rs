//! Núcleo de la app para un usuario: lectura, escritura y sincronización.

use crate::auth::SupabaseClient;
use crate::data::{Data, TrashItem};
use crate::defaults::suggested_categories;
use crate::forms::FormError;
use crate::i18n::{Lang, texts};
use caudal_core::fx::FxRate;
use caudal_core::model::{
    Account, Asset, Budget, Category, Contact, Debt, DebtInstallment, DebtPayment, Goal,
    GoalContribution, Profile, RecurringRule, Tag, Trade, Transaction, TransactionSplit,
};
use caudal_core::model::{AssetPrice, PriceAlert};
use caudal_supabase::SupabaseError;
use caudal_sync::{LocalStore, Record, Repository, Table, sync};
use chrono::NaiveDate;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;
use uuid::Uuid;

const LAST_FULL_SYNC: &str = "last_full_sync";
const BATCH_SIZE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    Synced,
    Syncing,
    Pending(usize),
    Offline {
        pending: usize,
    },
    SignInRequired {
        pending: usize,
    },
    /// Modo demostración: sin servidor.
    LocalOnly,
}

impl SyncState {
    pub fn label(&self, lang: Lang) -> String {
        let texts = texts(lang);
        match self {
            SyncState::Synced => texts.synced.into(),
            SyncState::Syncing => texts.syncing.into(),
            SyncState::Pending(count) => texts.pending_changes(*count),
            SyncState::Offline { pending } if *pending > 0 => {
                format!("{} · {}", texts.offline, texts.pending_changes(*pending))
            }
            SyncState::Offline { .. } => texts.offline.into(),
            SyncState::SignInRequired { .. } => texts.session_expired.into(),
            SyncState::LocalOnly => texts.demo_mode.into(),
        }
    }

    pub fn is_problem(&self) -> bool {
        matches!(
            self,
            SyncState::Offline { .. } | SyncState::SignInRequired { .. }
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("formulario: {0:?}")]
    Form(FormError),
    #[error("almacén local: {0}")]
    Storage(String),
    #[error("{message}")]
    Server {
        message: String,
        offline: bool,
        sign_in_required: bool,
    },
}

impl AppError {
    pub fn message(&self, lang: Lang) -> String {
        match self {
            AppError::Form(error) => error.message(texts(lang)).into(),
            other => other.to_string(),
        }
    }

    pub(crate) fn storage(error: impl std::fmt::Display) -> Self {
        AppError::Storage(error.to_string())
    }
}

impl From<FormError> for AppError {
    fn from(error: FormError) -> Self {
        AppError::Form(error)
    }
}

impl From<&SupabaseError> for AppError {
    fn from(error: &SupabaseError) -> Self {
        AppError::Server {
            message: error.to_string(),
            offline: matches!(error, SupabaseError::Http(_)),
            sign_in_required: error.requires_sign_in(),
        }
    }
}

/// Precios y tipos de cambio que se muestran (de solo lectura para el usuario).
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct Market {
    pub prices: Vec<AssetPrice>,
    pub fx_rates: Vec<FxRate>,
}

/// Copia local de los datos de mercado: sin red se sigue mostrando el último precio.
const MARKET: &str = "market";
/// Milisegundos de la última consulta de mercado.
const MARKET_CHECKED: &str = "market_checked_at";
/// El worker escribe 1 o 2 veces al día; consultar cada hora basta.
const MARKET_EVERY_MS: u64 = 60 * 60 * 1000;
/// Historial que se guarda en el dispositivo, para los gráficos de 12 meses.
const MARKET_HISTORY_DAYS: u64 = 400;

pub struct AppCore<S> {
    repo: Repository<S>,
    now_millis: std::rc::Rc<dyn Fn() -> u64>,
    /// Datos de mercado fijos (modo demostración): no se leen ni se consultan.
    demo_market: std::cell::RefCell<Option<Market>>,
}

impl<S: LocalStore> AppCore<S> {
    pub async fn open(
        store: S,
        user_id: Uuid,
        now_millis: impl Fn() -> u64 + 'static,
        new_node: impl FnOnce() -> u64,
    ) -> Result<Self, AppError> {
        let now_millis: std::rc::Rc<dyn Fn() -> u64> = std::rc::Rc::new(now_millis);
        let clock = now_millis.clone();
        let repo = Repository::open(store, user_id, move || clock(), new_node)
            .await
            .map_err(AppError::storage)?;
        Ok(Self {
            repo,
            now_millis,
            demo_market: std::cell::RefCell::new(None),
        })
    }

    /// Fija los datos de mercado en memoria, para el modo demostración.
    pub fn set_market(&self, market: Market) {
        *self.demo_market.borrow_mut() = Some(market);
    }

    async fn stored_market(&self) -> Result<Market, AppError> {
        if let Some(market) = self.demo_market.borrow().clone() {
            return Ok(market);
        }
        let text = self
            .repo
            .store()
            .meta(MARKET)
            .await
            .map_err(AppError::storage)?;
        // Una copia dañada no impide abrir la app: se vuelve a bajar.
        Ok(text
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    /// Baja los precios de los activos del usuario y los tipos de cambio de sus
    /// monedas, y los fusiona con la copia local. Nunca habla con los proveedores.
    pub async fn refresh_market(
        &self,
        client: &SupabaseClient,
        data: &Data,
        today: NaiveDate,
    ) -> Result<bool, AppError> {
        if self.demo_market.borrow().is_some() {
            return Ok(false);
        }
        let mut market = self.stored_market().await?;
        let symbols: BTreeSet<String> = data
            .assets
            .iter()
            .filter(|asset| {
                !asset.meta.is_deleted()
                    && asset.provider != caudal_core::model::PriceProviderId::Manual
            })
            .filter_map(|asset| asset.provider_symbol.clone())
            .map(|symbol| symbol.trim().to_owned())
            .filter(|symbol| !symbol.is_empty())
            .collect();
        let quotes = currencies_in_use(data);
        let oldest = today
            .checked_sub_days(chrono::Days::new(MARKET_HISTORY_DAYS))
            .unwrap_or(today);

        // Desde el último dato guardado (menos unos días, por si el worker
        // corrigió un cierre), o todo el historial si hay algo nuevo.
        let price_since = {
            let known: BTreeSet<String> = market
                .prices
                .iter()
                .map(|price| price.symbol.to_uppercase())
                .collect();
            let all_known = symbols.iter().all(|s| known.contains(&s.to_uppercase()));
            let last = market.prices.iter().map(|price| price.price_on).max();
            match last {
                Some(last) if all_known => last
                    .checked_sub_days(chrono::Days::new(3))
                    .unwrap_or(last)
                    .max(oldest),
                _ => oldest,
            }
        };
        let rate_since = {
            let known: BTreeSet<String> = market
                .fx_rates
                .iter()
                .map(|rate| rate.quote.to_string())
                .collect();
            let last = market.fx_rates.iter().map(|rate| rate.rate_on).max();
            match last {
                Some(last) if quotes.is_subset(&known) => last
                    .checked_sub_days(chrono::Days::new(3))
                    .unwrap_or(last)
                    .max(oldest),
                _ => oldest,
            }
        };
        let prices = client
            .fetch_prices(&symbols, price_since)
            .await
            .map_err(|error| AppError::from(&error))?;
        let rates = client
            .fetch_rates(&quotes, rate_since)
            .await
            .map_err(|error| AppError::from(&error))?;

        let before = market.clone();
        merge_market(&mut market, prices, rates, oldest);
        if market == before {
            return Ok(false);
        }
        let text = serde_json::to_string(&market).map_err(AppError::storage)?;
        self.repo
            .store()
            .set_meta(MARKET, &text)
            .await
            .map_err(AppError::storage)?;
        Ok(true)
    }

    pub(crate) fn repo(&self) -> &Repository<S> {
        &self.repo
    }

    pub fn user_id(&self) -> Uuid {
        self.repo.user_id()
    }

    pub async fn load(&self) -> Result<Data, AppError> {
        let mut data = Data {
            user_id: self.user_id(),
            ..Data::default()
        };
        let mut unreadable = 0;
        let profiles: Vec<Profile> = self.read(Table::Profiles, &mut unreadable).await?;
        data.profile = profiles
            .into_iter()
            .find(|profile| profile.meta.id == self.user_id());
        data.accounts = self
            .read::<Account>(Table::Accounts, &mut unreadable)
            .await?;
        data.categories = self
            .read::<Category>(Table::Categories, &mut unreadable)
            .await?;
        data.tags = self.read::<Tag>(Table::Tags, &mut unreadable).await?;
        data.transactions = self
            .read::<Transaction>(Table::Transactions, &mut unreadable)
            .await?;
        data.splits = self
            .read::<TransactionSplit>(Table::TransactionSplits, &mut unreadable)
            .await?;
        data.contacts = self
            .read::<Contact>(Table::Contacts, &mut unreadable)
            .await?;
        data.debts = self.read::<Debt>(Table::Debts, &mut unreadable).await?;
        data.installments = self
            .read::<DebtInstallment>(Table::DebtInstallments, &mut unreadable)
            .await?;
        data.payments = self
            .read::<DebtPayment>(Table::DebtPayments, &mut unreadable)
            .await?;
        data.rules = self
            .read::<RecurringRule>(Table::RecurringRules, &mut unreadable)
            .await?;
        data.budgets = self.read::<Budget>(Table::Budgets, &mut unreadable).await?;
        data.goals = self.read::<Goal>(Table::Goals, &mut unreadable).await?;
        data.contributions = self
            .read::<GoalContribution>(Table::GoalContributions, &mut unreadable)
            .await?;
        data.assets = self.read::<Asset>(Table::Assets, &mut unreadable).await?;
        data.trades = self.read::<Trade>(Table::Trades, &mut unreadable).await?;
        data.assets.sort_by_key(|asset| asset.name.to_lowercase());
        let market = self.stored_market().await?;
        data.prices = market.prices;
        data.fx_rates = market.fx_rates;
        data.alerts = self
            .read::<PriceAlert>(Table::PriceAlerts, &mut unreadable)
            .await?;
        data.category_rules = self.read(Table::CategoryRules, &mut unreadable).await?;
        data.templates = self
            .read(Table::TransactionTemplates, &mut unreadable)
            .await?;
        data.attachments = self.read(Table::Attachments, &mut unreadable).await?;
        data.contacts
            .sort_by_key(|contact| contact.name.to_lowercase());
        data.tags.sort_by_key(|tag| tag.name.to_lowercase());
        data.categories
            .sort_by_key(|category| category.name.to_lowercase());

        for table in [
            Table::Transactions,
            Table::Accounts,
            Table::Categories,
            Table::Tags,
            Table::Contacts,
            Table::Debts,
            Table::RecurringRules,
            Table::Budgets,
            Table::Goals,
            Table::Assets,
            Table::Trades,
            Table::PriceAlerts,
            Table::CategoryRules,
            Table::TransactionTemplates,
            Table::Attachments,
        ] {
            for record in self
                .repo
                .list_trash(table)
                .await
                .map_err(AppError::storage)?
            {
                let label = trash_label(&record);
                data.trash.push(TrashItem { record, label });
            }
        }
        data.trash
            .sort_by(|a, b| b.record.updated_at.cmp(&a.record.updated_at));
        data.unreadable = unreadable;
        Ok(data)
    }

    async fn read<T: DeserializeOwned>(
        &self,
        table: Table,
        unreadable: &mut usize,
    ) -> Result<Vec<T>, AppError> {
        let records = self
            .repo
            .list_active(table)
            .await
            .map_err(AppError::storage)?;
        let mut models = Vec::with_capacity(records.len());
        for record in records {
            match record.to_model::<T>() {
                Ok(model) => models.push(model),
                Err(_) => *unreadable += 1,
            }
        }
        Ok(models)
    }

    pub async fn save<T: Serialize>(&self, table: Table, model: &T) -> Result<(), AppError> {
        self.repo
            .save_model(table, model)
            .await
            .map(|_| ())
            .map_err(AppError::storage)
    }

    /// Borrar o restaurar una operación hace lo mismo con el movimiento que generó.
    async fn linked_movement(&self, table: Table, id: Uuid) -> Result<Option<Uuid>, AppError> {
        if table != Table::Trades {
            return Ok(None);
        }
        let record = self.repo.get(table, id).await.map_err(AppError::storage)?;
        Ok(record.and_then(|record| {
            record
                .row
                .get("transaction_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|text| text.parse().ok())
        }))
    }

    pub async fn delete(&self, table: Table, id: Uuid) -> Result<(), AppError> {
        if let Some(movement) = self.linked_movement(table, id).await? {
            self.repo
                .delete(Table::Transactions, movement)
                .await
                .map_err(AppError::storage)?;
        }
        self.repo
            .delete(table, id)
            .await
            .map(|_| ())
            .map_err(AppError::storage)
    }

    pub async fn restore(&self, table: Table, id: Uuid) -> Result<(), AppError> {
        if let Some(movement) = self.linked_movement(table, id).await? {
            self.repo
                .restore(Table::Transactions, movement)
                .await
                .map_err(AppError::storage)?;
        }
        self.repo
            .restore(table, id)
            .await
            .map(|_| ())
            .map_err(AppError::storage)
    }

    /// Guarda el perfil y, si se pide, las categorías sugeridas que aún no existan.
    pub async fn complete_onboarding(
        &self,
        profile: &Profile,
        suggested: bool,
        lang: Lang,
    ) -> Result<(), AppError> {
        self.save(Table::Profiles, profile).await?;
        if suggested {
            for category in suggested_categories(self.user_id(), lang) {
                let exists = self
                    .repo
                    .get(Table::Categories, category.meta.id)
                    .await
                    .map_err(AppError::storage)?
                    .is_some();
                if !exists {
                    self.save(Table::Categories, &category).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn save_debt(&self, draft: &crate::debts::DebtDraft) -> Result<(), AppError> {
        if let Some(contact) = &draft.contact {
            self.save(Table::Contacts, contact).await?;
        }
        self.save(Table::Debts, &draft.debt).await?;
        for installment in &draft.installments {
            self.save(Table::DebtInstallments, installment).await?;
        }
        if let Some(movement) = &draft.movement {
            self.save(Table::Transactions, movement).await?;
        }
        Ok(())
    }

    pub async fn save_payment(
        &self,
        payment: &DebtPayment,
        movement: Option<&Transaction>,
    ) -> Result<(), AppError> {
        if let Some(movement) = movement {
            self.save(Table::Transactions, movement).await?;
        }
        self.save(Table::DebtPayments, payment).await
    }

    pub async fn save_shared(&self, draft: &crate::debts::SharedDraft) -> Result<(), AppError> {
        for tx in draft.expense.iter().chain(draft.advance.iter()) {
            self.save(Table::Transactions, tx).await?;
        }
        for debt in &draft.debts {
            self.save(Table::Debts, debt).await?;
        }
        Ok(())
    }

    /// Guarda la operación y su movimiento en la cuenta asociada, si hay.
    pub async fn save_trade(&self, draft: &crate::investments::TradeDraft) -> Result<(), AppError> {
        if let Some(movement) = &draft.movement {
            self.save(Table::Transactions, movement).await?;
        }
        if let Some(id) = draft.remove_movement {
            self.delete(Table::Transactions, id).await?;
        }
        self.save(Table::Trades, &draft.trade).await
    }

    /// Un movimiento pendiente pasa a confirmado.
    pub async fn confirm(&self, id: Uuid) -> Result<(), AppError> {
        let Some(record) = self
            .repo
            .get(Table::Transactions, id)
            .await
            .map_err(AppError::storage)?
        else {
            return Ok(());
        };
        let mut tx: Transaction = record.to_model().map_err(AppError::storage)?;
        tx.status = caudal_core::model::TransactionStatus::Confirmed;
        self.save(Table::Transactions, &tx).await
    }

    /// Crea las ocurrencias de las reglas activas hasta hoy. Usa ids fijos por
    /// regla y fecha, y la marca más baja: repetirlo en otro dispositivo no
    /// duplica ni pisa lo que el usuario ya confirmó, editó o borró.
    pub async fn generate_recurring(&self, today: NaiveDate) -> Result<usize, AppError> {
        let rules: Vec<RecurringRule> = {
            let mut unreadable = 0;
            self.read(Table::RecurringRules, &mut unreadable).await?
        };
        let mut known: BTreeSet<Uuid> = BTreeSet::new();
        for record in self
            .repo
            .list_active(Table::Transactions)
            .await
            .map_err(AppError::storage)?
            .into_iter()
            .chain(
                self.repo
                    .list_trash(Table::Transactions)
                    .await
                    .map_err(AppError::storage)?,
            )
        {
            known.insert(record.id);
        }
        let mut created = 0;
        for rule in rules.iter().filter(|rule| rule.active) {
            for date in caudal_core::recurrence::occurrences_between(rule, rule.starts_on, today) {
                let tx = caudal_core::recurrence::occurrence_transaction(rule, date);
                if known.contains(&tx.meta.id) {
                    continue;
                }
                let row = Record::from_model(Table::Transactions, &tx)
                    .map_err(AppError::storage)?
                    .row;
                if self
                    .repo
                    .save_generated(Table::Transactions, row)
                    .await
                    .map_err(AppError::storage)?
                    .is_some()
                {
                    created += 1;
                }
            }
        }
        Ok(created)
    }

    /// Cambios locales distintos que esperan subir.
    pub async fn pending_changes(&self) -> Result<usize, AppError> {
        let entries = self
            .repo
            .store()
            .outbox(usize::MAX)
            .await
            .map_err(AppError::storage)?;
        let distinct: BTreeSet<(Table, Uuid)> = entries
            .iter()
            .map(|entry| (entry.table, entry.id))
            .collect();
        Ok(distinct.len())
    }

    /// Sube y baja cambios. Una vez al día vuelve a leer todo del servidor,
    /// por si se saltó alguna fila; es seguro porque aplicar es idempotente.
    /// Tras sincronizar, como mucho una vez por hora. Un fallo no cambia el
    /// estado de la sincronización: se conserva el último precio.
    async fn refresh_market_if_due(&self, client: &SupabaseClient, today: NaiveDate) {
        let store = self.repo.store();
        let now = (self.now_millis)();
        let last: u64 = store
            .meta(MARKET_CHECKED)
            .await
            .ok()
            .flatten()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0);
        if now.saturating_sub(last) < MARKET_EVERY_MS {
            return;
        }
        let Ok(data) = self.load().await else { return };
        if self.refresh_market(client, &data, today).await.is_ok() {
            let _ = store.set_meta(MARKET_CHECKED, &now.to_string()).await;
        }
    }

    pub async fn sync(
        &self,
        client: &SupabaseClient,
        today: NaiveDate,
    ) -> Result<SyncState, AppError> {
        let store = self.repo.store();
        let day = today.to_string();
        let last_full = store
            .meta(LAST_FULL_SYNC)
            .await
            .map_err(AppError::storage)?;
        let full = last_full.as_deref() != Some(day.as_str());
        if full {
            for table in Table::ALL {
                store
                    .set_cursor(table, 0)
                    .await
                    .map_err(AppError::storage)?;
            }
        }
        let result = sync(&self.repo, client, BATCH_SIZE).await;
        let pending = self.pending_changes().await?;
        match result {
            Ok(_) => {
                if full {
                    store
                        .set_meta(LAST_FULL_SYNC, &day)
                        .await
                        .map_err(AppError::storage)?;
                }
                self.refresh_market_if_due(client, today).await;
                // Un fallo deja los archivos esperando; no cambia el estado.
                let _ = self.upload_pending_attachments(client).await;
                Ok(if pending == 0 {
                    SyncState::Synced
                } else {
                    SyncState::Pending(pending)
                })
            }
            Err(caudal_sync::SyncError::Remote(error)) => {
                if error.requires_sign_in() {
                    Ok(SyncState::SignInRequired { pending })
                } else if matches!(error, SupabaseError::Http(_)) {
                    Ok(SyncState::Offline { pending })
                } else {
                    Err(AppError::from(&error))
                }
            }
            Err(caudal_sync::SyncError::Store(error)) => Err(AppError::storage(error)),
        }
    }

    /// Borra todos los datos locales de este dispositivo (tablas, perfil, cola y caché).
    pub async fn clear_local(&self) -> Result<(), AppError> {
        let store = self.repo.store();
        for record in store
            .list(Table::Attachments)
            .await
            .map_err(AppError::storage)?
        {
            let _ = store
                .set_meta(&format!("attachment:{}", record.id), "")
                .await;
        }
        for table in Table::ALL {
            for record in self
                .repo
                .list_active(table)
                .await
                .map_err(AppError::storage)?
            {
                self.repo
                    .delete(table, record.id)
                    .await
                    .map_err(AppError::storage)?;
            }
            store
                .set_cursor(table, 0)
                .await
                .map_err(AppError::storage)?;
        }
        let entries = store.outbox(usize::MAX).await.map_err(AppError::storage)?;
        store.ack(&entries).await.map_err(AppError::storage)?;
        for key in [
            LAST_FULL_SYNC,
            MARKET,
            MARKET_CHECKED,
            "attachments_pending",
        ] {
            store.set_meta(key, "").await.map_err(AppError::storage)?;
        }
        Ok(())
    }

    /// Elimina por completo la cuenta del usuario:
    /// 1. Borra los archivos de `receipts/{user_id}/` con la API de Storage.
    /// 2. Borra el usuario en `auth.users` (y todas sus filas en cascada) mediante
    ///    `public.delete_account()`.
    /// 3. Limpia los datos locales del dispositivo.
    pub async fn delete_account(&self, client: &SupabaseClient) -> Result<(), AppError> {
        let mut paths = Vec::new();
        for record in self
            .repo
            .store()
            .list(Table::Attachments)
            .await
            .map_err(AppError::storage)?
        {
            if let Some(path) = record
                .row
                .get("storage_path")
                .and_then(serde_json::Value::as_str)
            {
                paths.push(path.to_owned());
            }
        }
        client
            .delete_user_receipts(&paths)
            .await
            .map_err(|error| AppError::from(&error))?;
        client
            .delete_account()
            .await
            .map_err(|error| AppError::from(&error))?;
        self.clear_local().await
    }
}

/// Monedas del usuario salvo el dólar, que es la base de los tipos guardados.
fn currencies_in_use(data: &Data) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    if let Some(profile) = &data.profile {
        set.insert(profile.base_currency.to_string());
    }
    set.extend(data.accounts.iter().map(|a| a.currency.to_string()));
    set.extend(data.assets.iter().map(|a| a.currency.to_string()));
    set.extend(data.transactions.iter().map(|t| t.currency.to_string()));
    set.extend(data.debts.iter().map(|d| d.currency.to_string()));
    set.extend(data.budgets.iter().map(|b| b.currency.to_string()));
    set.extend(data.goals.iter().map(|g| g.currency.to_string()));
    set.remove("USD");
    set
}

/// Un precio por símbolo y día, y un tipo por moneda y día: lo nuevo
/// reemplaza a lo guardado. Lo anterior a `oldest` se descarta.
fn merge_market(
    market: &mut Market,
    prices: Vec<AssetPrice>,
    rates: Vec<FxRate>,
    oldest: NaiveDate,
) {
    use std::collections::BTreeMap;
    let mut by_price: BTreeMap<(String, String, NaiveDate), AssetPrice> = BTreeMap::new();
    for price in market.prices.drain(..).chain(prices) {
        if price.price_on >= oldest {
            let key = (
                format!("{:?}", price.provider),
                price.symbol.to_uppercase(),
                price.price_on,
            );
            by_price.insert(key, price);
        }
    }
    market.prices = by_price.into_values().collect();
    let mut by_rate: BTreeMap<(String, String, NaiveDate), FxRate> = BTreeMap::new();
    for rate in market.fx_rates.drain(..).chain(rates) {
        if rate.rate_on >= oldest {
            by_rate.insert(
                (rate.base.to_string(), rate.quote.to_string(), rate.rate_on),
                rate,
            );
        }
    }
    market.fx_rates = by_rate.into_values().collect();
}

fn trash_label(record: &Record) -> String {
    let text = |key: &str| {
        record
            .row
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    match record.table {
        Table::Transactions => {
            let amount = text("amount").unwrap_or_default();
            let currency = text("currency").unwrap_or_default();
            let what =
                text("description").unwrap_or_else(|| text("occurred_on").unwrap_or_default());
            format!("{what} · {amount} {currency}")
        }
        Table::Debts => {
            let what = text("description").unwrap_or_else(|| text("direction").unwrap_or_default());
            format!(
                "{what} · {} {}",
                text("principal").unwrap_or_default(),
                text("currency").unwrap_or_default()
            )
        }
        Table::Trades => {
            let number = |key: &str| {
                record
                    .row
                    .get(key)
                    .filter(|value| !value.is_null())
                    .map(|value| value.to_string().trim_matches('"').to_owned())
            };
            let detail = [number("quantity"), number("price")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" × ");
            format!(
                "{} · {detail} {} · {}",
                text("kind").unwrap_or_default(),
                text("currency").unwrap_or_default(),
                text("traded_on").unwrap_or_default()
            )
        }
        _ => text("name")
            .or_else(|| text("description"))
            .or_else(|| text("pattern"))
            .or_else(|| text("file_name"))
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{AccountForm, ProfileForm, TransactionForm};
    use caudal_sync::memory::MemoryStore;
    use pollster::block_on;

    const USER: Uuid = Uuid::from_u128(1);

    fn core() -> AppCore<MemoryStore> {
        block_on(AppCore::open(MemoryStore::default(), USER, || 1_000, || 7)).unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    #[test]
    fn periodicos_dos_dispositivos_sin_duplicar_y_respetando_confirmacion() {
        use caudal_core::model::{Frequency, TransactionStatus};
        use caudal_sync::memory::MemoryRemote;
        let remote = MemoryRemote::default();
        let a = block_on(AppCore::open(
            MemoryStore::default(),
            USER,
            || 1_000,
            || 0xA,
        ))
        .unwrap();
        let b = block_on(AppCore::open(
            MemoryStore::default(),
            USER,
            || 9_000_000,
            || 0xB,
        ))
        .unwrap();
        let rule = crate::recurring::RecurringForm {
            name: "Arriendo".into(),
            amount: "450000".into(),
            currency: "CLP".into(),
            frequency: Frequency::Monthly,
            starts_on: NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
            ..crate::recurring::RecurringForm::new(&Data::default(), today())
        }
        .validate(
            &Data {
                user_id: USER,
                ..Data::default()
            },
            Lang::Es,
        )
        .unwrap();
        block_on(a.save(Table::RecurringRules, &rule)).unwrap();
        block_on(caudal_sync::sync(&a.repo, &remote, 50)).unwrap();
        block_on(caudal_sync::sync(&b.repo, &remote, 50)).unwrap();

        // Ambos generan sin conexión; A confirma la de septiembre.
        assert_eq!(block_on(a.generate_recurring(today())).unwrap(), 3);
        assert_eq!(block_on(b.generate_recurring(today())).unwrap(), 3);
        assert_eq!(block_on(a.generate_recurring(today())).unwrap(), 0);
        let september = caudal_core::recurrence::occurrence_id(
            rule.meta.id,
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        );
        block_on(a.confirm(september)).unwrap();

        for core in [&b, &a, &b] {
            block_on(caudal_sync::sync(&core.repo, &remote, 50)).unwrap();
        }
        for core in [&a, &b] {
            let data = block_on(core.load()).unwrap();
            assert_eq!(data.transactions.len(), 3);
            let confirmed = data
                .transactions
                .iter()
                .filter(|tx| tx.status == TransactionStatus::Confirmed)
                .count();
            assert_eq!(confirmed, 1);
        }
    }

    #[test]
    fn bienvenida_crea_perfil_y_categorias_una_sola_vez() {
        let core = core();
        let profile = ProfileForm::new(Lang::Es).validate(USER).unwrap();
        block_on(core.complete_onboarding(&profile, true, Lang::Es)).unwrap();
        block_on(core.complete_onboarding(&profile, true, Lang::Es)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert!(data.profile.is_some());
        assert_eq!(data.categories.len(), 10);
        assert_eq!(block_on(core.pending_changes()).unwrap(), 11);
    }

    #[test]
    fn borrar_una_compra_borra_su_movimiento_y_restaurarla_lo_devuelve() {
        use crate::investments::{AssetForm, TradeForm};
        let core = core();
        let account = AccountForm {
            name: "Broker".into(),
            currency: "USD".into(),
            ..AccountForm::new(&Data::default())
        }
        .validate(USER, Lang::Es)
        .unwrap();
        block_on(core.save(Table::Accounts, &account)).unwrap();
        let data = block_on(core.load()).unwrap();
        let mut asset = AssetForm::new(&data, today());
        asset.name = "ACME".into();
        asset.currency = "USD".into();
        let asset = asset.validate(&data, Lang::Es).unwrap();
        block_on(core.save(Table::Assets, &asset)).unwrap();
        let data = block_on(core.load()).unwrap();
        let draft = TradeForm {
            quantity: "2".into(),
            price: "10".into(),
            account_id: Some(account.meta.id),
            ..TradeForm::new(Some(asset.meta.id), today())
        }
        .validate(&data, Lang::Es)
        .unwrap();
        block_on(core.save_trade(&draft)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert_eq!(data.trades.len(), 1);
        assert_eq!(data.transactions.len(), 1);

        block_on(core.delete(Table::Trades, draft.trade.meta.id)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert!(data.trades.is_empty());
        assert!(data.transactions.is_empty());
        assert_eq!(data.trash.len(), 2);
        let labels: Vec<String> = crate::views::trash(&data, Lang::Es)
            .into_iter()
            .map(|item| item.label)
            .collect();
        assert!(
            labels
                .iter()
                .any(|label| label == "Compra: ACME · 09/11/2026"),
            "{labels:?}"
        );

        block_on(core.restore(Table::Trades, draft.trade.meta.id)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert_eq!(data.trades.len(), 1);
        assert_eq!(data.transactions.len(), 1);
    }

    #[test]
    fn guardar_borrar_y_recuperar() {
        let core = core();
        let account = AccountForm {
            name: "Banco".into(),
            ..AccountForm::new(&Data::default())
        }
        .validate(USER, Lang::Es)
        .unwrap();
        block_on(core.save(Table::Accounts, &account)).unwrap();
        let data = block_on(core.load()).unwrap();
        let mut form = TransactionForm::new(&data, today());
        form.amount = "10".into();
        form.description = "Pan".into();
        let tx = form.validate(&data, USER, Lang::Es).unwrap();
        block_on(core.save(Table::Transactions, &tx)).unwrap();

        block_on(core.delete(Table::Transactions, tx.meta.id)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert!(data.transactions.is_empty());
        assert_eq!(data.trash.len(), 1);
        assert_eq!(
            data.trash[0].label,
            "Pan · 10.000000000000 USD".replace(".000000000000", "")
        );

        block_on(core.restore(Table::Transactions, tx.meta.id)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert_eq!(data.transactions.len(), 1);
        assert!(data.trash.is_empty());
    }

    #[test]
    fn filas_ilegibles_se_cuentan_y_no_rompen() {
        let core = core();
        let mut row = serde_json::Map::new();
        row.insert(
            "id".into(),
            serde_json::json!(Uuid::from_u128(9).to_string()),
        );
        row.insert("name".into(), serde_json::json!(5));
        block_on(core.repo.save(Table::Tags, row)).unwrap();
        let data = block_on(core.load()).unwrap();
        assert_eq!(data.unreadable, 1);
        assert!(data.tags.is_empty());
    }
}
