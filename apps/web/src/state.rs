//! Estado de la web y acciones asíncronas. Equivale al hilo de fondo de la app nativa.

use caudal_app_state::auth::{
    self, Session as AuthSession, SessionUser, SignUpOutcome, SupabaseClient,
};
use caudal_app_state::debts::{DebtDraft, SharedDraft};
use caudal_app_state::forms::{AccountForm, CategoryForm, TagForm, TransactionForm};
use caudal_app_state::model::{
    Account, Budget, Category, Contact, DebtPayment, Goal, GoalContribution, Profile,
    RecurringRule, Tag, Transaction,
};
use caudal_app_state::views::TransactionFilter;
use caudal_app_state::{AppCore, Data, Lang, SyncState, Table, texts};
use caudal_store_idb::IdbStore;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const DEMO_USER: Uuid = Uuid::from_u128(0xDE30);
const SESSION_KEY: &str = "caudal.session";
const PREFS_KEY: &str = "caudal.prefs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Transactions,
    Accounts,
    Investments,
    Reports,
    Plans,
    Debts,
    Recurring,
    Categories,
    Rules,
    Activity,
    Trash,
    Settings,
}

impl Page {
    pub const ALL: [Page; 13] = [
        Page::Home,
        Page::Transactions,
        Page::Accounts,
        Page::Investments,
        Page::Reports,
        Page::Plans,
        Page::Debts,
        Page::Recurring,
        Page::Categories,
        Page::Rules,
        Page::Activity,
        Page::Trash,
        Page::Settings,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let texts = texts(lang);
        match self {
            Page::Home => texts.home,
            Page::Transactions => texts.transactions,
            Page::Accounts => texts.accounts,
            Page::Investments => texts.investments,
            Page::Reports => texts.reports,
            Page::Plans => texts.plans,
            Page::Debts => texts.debts,
            Page::Recurring => texts.recurring,
            Page::Categories => texts.categories,
            Page::Rules => texts.rules_title,
            Page::Activity => texts.activity,
            Page::Trash => texts.trash,
            Page::Settings => texts.settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Session {
    Loading,
    SignedOut,
    SignedIn { email: Option<String>, demo: bool },
}

#[derive(Debug, Clone, PartialEq)]
// Solo existe un diálogo a la vez: encajonar la variante grande no ahorra nada.
#[allow(clippy::large_enum_variant)]
pub enum Dialog {
    Transaction(TransactionForm),
    Account(AccountForm),
    Category(CategoryForm),
    Tag(TagForm),
    Adjust {
        account_id: Uuid,
        value: String,
    },
    ConfirmDelete {
        table: Table,
        id: Uuid,
        label: String,
    },
    ConfirmSignOut,
    Contact(caudal_app_state::debts::ContactForm),
    Debt(caudal_app_state::debts::DebtForm),
    Payment(caudal_app_state::debts::PaymentForm),
    Shared(caudal_app_state::debts::SharedExpenseForm),
    Recurring(caudal_app_state::recurring::RecurringForm),
    Budget(caudal_app_state::planning::BudgetForm),
    Goal(caudal_app_state::planning::GoalForm),
    Contribution(caudal_app_state::planning::ContributionForm),
    Asset(caudal_app_state::investments::AssetForm),
    Trade(caudal_app_state::investments::TradeForm),
    Alert(caudal_app_state::investments::AlertForm),
    ConfirmDeleteAll,
    ConfirmDeleteAccount(bool),
    SetPin(String),
    Rule(caudal_app_state::rules::RuleForm),
    Template(caudal_app_state::rules::TemplateForm),
    Import(Box<crate::ui_data::ImportState>),
    Attachment {
        attachment: caudal_app_state::model::Attachment,
        url: Option<String>,
        loaded: bool,
    },
    Simulate(caudal_app_state::model::Debt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Asset,
    Trade,
    Alert,
    ConfirmDeleteAll,
    ConfirmDeleteAccount,
    SetPin,
    Rule,
    Template,
    Import,
    Attachment,
    Budget,
    Goal,
    Contribution,
    Simulate,
    Contact,
    Debt,
    Payment,
    Shared,
    Recurring,
    Transaction,
    Account,
    Category,
    Tag,
    Adjust,
    ConfirmDelete,
    ConfirmSignOut,
}

impl Dialog {
    pub fn kind(&self) -> DialogKind {
        match self {
            Dialog::Transaction(_) => DialogKind::Transaction,
            Dialog::Account(_) => DialogKind::Account,
            Dialog::Category(_) => DialogKind::Category,
            Dialog::Tag(_) => DialogKind::Tag,
            Dialog::Adjust { .. } => DialogKind::Adjust,
            Dialog::ConfirmDelete { .. } => DialogKind::ConfirmDelete,
            Dialog::ConfirmSignOut => DialogKind::ConfirmSignOut,
            Dialog::Contact(_) => DialogKind::Contact,
            Dialog::Debt(_) => DialogKind::Debt,
            Dialog::Payment(_) => DialogKind::Payment,
            Dialog::Shared(_) => DialogKind::Shared,
            Dialog::Recurring(_) => DialogKind::Recurring,
            Dialog::Budget(_) => DialogKind::Budget,
            Dialog::Goal(_) => DialogKind::Goal,
            Dialog::Contribution(_) => DialogKind::Contribution,
            Dialog::Simulate(_) => DialogKind::Simulate,
            Dialog::Asset(_) => DialogKind::Asset,
            Dialog::Trade(_) => DialogKind::Trade,
            Dialog::Alert(_) => DialogKind::Alert,
            Dialog::ConfirmDeleteAll => DialogKind::ConfirmDeleteAll,
            Dialog::ConfirmDeleteAccount(_) => DialogKind::ConfirmDeleteAccount,
            Dialog::SetPin(_) => DialogKind::SetPin,
            Dialog::Rule(_) => DialogKind::Rule,
            Dialog::Template(_) => DialogKind::Template,
            Dialog::Import(_) => DialogKind::Import,
            Dialog::Attachment { .. } => DialogKind::Attachment,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    Transaction(Transaction),
    Account(Account),
    Category(Category),
    Tag(Tag),
    Profile(Profile),
    Contact(Contact),
    Rule(RecurringRule),
    Budget(Budget),
    Goal(Goal),
    Contribution(GoalContribution),
    Asset(caudal_app_state::model::Asset),
    Trade(caudal_app_state::investments::TradeDraft),
    Alert(caudal_app_state::model::PriceAlert),
    CategoryRule(caudal_app_state::model::CategoryRule),
    Template(caudal_app_state::model::TransactionTemplate),
    Debt(DebtDraft),
    Payment(DebtPayment, Option<Transaction>),
    Shared(SharedDraft),
    Confirm(Uuid),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Prefs {
    lang: Option<String>,
    theme: Option<String>,
}

/// PIN y modo privado de este navegador, aparte de las preferencias.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Privacy {
    pub pin_hash: Option<String>,
    pub pin_salt: Option<String>,
    pub private: bool,
}

const PRIVACY_KEY: &str = "caudal.privacy";

impl Privacy {
    pub fn load() -> Self {
        read_json(PRIVACY_KEY).unwrap_or_default()
    }

    pub fn save(&self) {
        write_json(PRIVACY_KEY, self);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSession {
    refresh_token: String,
    user_id: Uuid,
    email: Option<String>,
}

/// Todo es `Copy`: se mueve libremente a los cierres de la interfaz.
#[derive(Clone, Copy)]
pub struct AppState {
    pub lang: RwSignal<Lang>,
    pub theme: RwSignal<String>,
    pub session: RwSignal<Session>,
    pub data: RwSignal<Arc<Data>>,
    pub sync: RwSignal<SyncState>,
    pub first_sync_done: RwSignal<bool>,
    pub page: RwSignal<Page>,
    pub dialog: RwSignal<Option<Dialog>>,
    pub dialog_error: RwSignal<Option<String>>,
    pub filter: RwSignal<TransactionFilter>,
    pub limit: RwSignal<usize>,
    pub show_archived: RwSignal<bool>,
    pub busy: RwSignal<bool>,
    pub auth_error: RwSignal<Option<String>>,
    pub auth_info: RwSignal<Option<String>>,
    pub recovery_verified: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub toast: RwSignal<Option<String>>,
    pub privacy: RwSignal<Privacy>,
    pub locked: RwSignal<bool>,
    pub pending_attachments: RwSignal<std::collections::BTreeSet<Uuid>>,
    syncing: RwSignal<bool>,
    sync_again: RwSignal<bool>,
    core: StoredValue<Option<Rc<AppCore<IdbStore>>>, LocalStorage>,
    client: StoredValue<Rc<SupabaseClient>, LocalStorage>,
}

pub const PAGE_SIZE: usize = 50;

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

fn read_json<T: for<'de> Deserialize<'de>>(key: &str) -> Option<T> {
    let text = storage()?.get_item(key).ok().flatten()?;
    serde_json::from_str(&text).ok()
}

fn write_json<T: Serialize>(key: &str, value: &T) {
    if let (Some(storage), Ok(text)) = (storage(), serde_json::to_string(value)) {
        let _ = storage.set_item(key, &text);
    }
}

fn remove_key(key: &str) {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(key);
    }
}

pub fn now_millis() -> u64 {
    js_sys::Date::now() as u64
}

/// Fecha local del navegador.
pub fn today() -> chrono::NaiveDate {
    let date = js_sys::Date::new_0();
    chrono::NaiveDate::from_ymd_opt(
        date.get_full_year() as i32,
        date.get_month() + 1,
        date.get_date(),
    )
    .unwrap_or_default()
}

fn random_node() -> u64 {
    Uuid::new_v4().as_u64_pair().0
}

fn browser_lang() -> Lang {
    window()
        .navigator()
        .language()
        .map_or(Lang::Es, |code| Lang::parse(&code))
}

/// `?demo` en una compilación de desarrollo abre el modo demostración.
fn demo_requested() -> bool {
    cfg!(debug_assertions)
        && window()
            .location()
            .search()
            .is_ok_and(|search| search.contains("demo"))
}

impl AppState {
    pub fn new() -> Self {
        let prefs: Prefs = read_json(PREFS_KEY).unwrap_or_default();
        let privacy = Privacy::load();
        caudal_app_state::privacy::set_private(privacy.private);
        let lang = prefs.lang.as_deref().map_or_else(browser_lang, Lang::parse);
        let state = Self {
            lang: RwSignal::new(lang),
            theme: RwSignal::new(prefs.theme.unwrap_or_else(|| "system".into())),
            session: RwSignal::new(Session::Loading),
            data: RwSignal::new(Arc::new(Data::default())),
            sync: RwSignal::new(SyncState::Synced),
            first_sync_done: RwSignal::new(false),
            page: RwSignal::new(Page::Home),
            dialog: RwSignal::new(None),
            dialog_error: RwSignal::new(None),
            filter: RwSignal::new(TransactionFilter::default()),
            limit: RwSignal::new(PAGE_SIZE),
            show_archived: RwSignal::new(false),
            busy: RwSignal::new(false),
            auth_error: RwSignal::new(None),
            auth_info: RwSignal::new(None),
            recovery_verified: RwSignal::new(false),
            error: RwSignal::new(None),
            toast: RwSignal::new(None),
            privacy: RwSignal::new(privacy.clone()),
            locked: RwSignal::new(privacy.pin_hash.is_some()),
            pending_attachments: RwSignal::new(Default::default()),
            syncing: RwSignal::new(false),
            sync_again: RwSignal::new(false),
            core: StoredValue::new_local(None),
            client: StoredValue::new_local(Rc::new(auth::client())),
        };
        state.apply_theme();
        leptos::task::spawn_local(async move { state.start().await });
        set_interval(move || state.request_sync(), Duration::from_secs(60));
        window_event_listener(leptos::ev::online, move |_| state.request_sync());
        state
    }

    fn client(&self) -> Rc<SupabaseClient> {
        self.client.get_value()
    }

    fn core(&self) -> Option<Rc<AppCore<IdbStore>>> {
        self.core.get_value()
    }

    fn save_prefs(&self) {
        write_json(
            PREFS_KEY,
            &Prefs {
                lang: Some(self.lang.get_untracked().code().to_owned()),
                theme: Some(self.theme.get_untracked()),
            },
        );
    }

    pub fn set_lang(&self, lang: Lang) {
        self.lang.set(lang);
        self.save_prefs();
        let _ = document()
            .document_element()
            .map(|html| html.set_attribute("lang", lang.code()));
    }

    pub fn set_theme(&self, theme: &str) {
        self.theme.set(theme.to_owned());
        self.save_prefs();
        self.apply_theme();
    }

    fn apply_theme(&self) {
        if let Some(html) = document().document_element() {
            let _ = html.set_attribute("data-theme", &self.theme.get_untracked());
        }
    }

    pub fn go(&self, page: Page) {
        self.page.set(page);
        self.limit.set(PAGE_SIZE);
        window().scroll_to_with_x_and_y(0.0, 0.0);
    }

    pub fn show_toast(&self, message: &str) {
        self.toast.set(Some(message.to_owned()));
        let toast = self.toast;
        set_timeout(move || toast.set(None), Duration::from_millis(2_500));
    }

    pub fn open_new_transaction(&self) {
        let data = self.data.get_untracked();
        self.dialog_error.set(None);
        self.dialog
            .set(Some(Dialog::Transaction(TransactionForm::new(
                &data,
                today(),
            ))));
    }

    fn message(&self, error: impl std::fmt::Display) -> String {
        error.to_string()
    }

    async fn start(self) {
        if demo_requested() {
            if self.open(DEMO_USER, "caudal-demo").await {
                self.seed_demo().await;
                self.session.set(Session::SignedIn {
                    email: None,
                    demo: true,
                });
                self.sync.set(SyncState::LocalOnly);
                self.first_sync_done.set(true);
            }
            return;
        }
        match read_json::<StoredSession>(SESSION_KEY) {
            Some(stored) => {
                let user = SessionUser {
                    id: stored.user_id,
                    email: stored.email.clone(),
                };
                self.client().set_session(Some(AuthSession {
                    access_token: String::new(),
                    refresh_token: stored.refresh_token,
                    expires_at: 0,
                    user,
                }));
                if self
                    .open(stored.user_id, &format!("caudal-{}", stored.user_id))
                    .await
                {
                    self.session.set(Session::SignedIn {
                        email: stored.email,
                        demo: false,
                    });
                    self.request_sync();
                }
            }
            None => self.session.set(Session::SignedOut),
        }
    }

    async fn seed_demo(&self) {
        let Some(core) = self.core() else { return };
        let demo =
            caudal_app_state::defaults::demo::sample(DEMO_USER, today(), self.lang.get_untracked());
        core.set_market(demo.market.clone());
        let existing = core.load().await.ok();
        // Una demostración creada antes de las inversiones recibe solo la cartera.
        if existing.as_ref().is_some_and(|data| data.assets.is_empty()) || existing.is_none() {
            for asset in &demo.assets {
                let _ = core.save(Table::Assets, asset).await;
            }
            for draft in &demo.trades {
                let _ = core.save_trade(draft).await;
            }
            for alert in &demo.alerts {
                let _ = core.save(Table::PriceAlerts, alert).await;
            }
        }
        if existing
            .as_ref()
            .is_none_or(|data| data.templates.is_empty())
        {
            for rule in &demo.category_rules {
                let _ = core.save(Table::CategoryRules, rule).await;
            }
            for template in &demo.templates {
                let _ = core.save(Table::TransactionTemplates, template).await;
            }
        }
        if existing.is_some_and(|data| data.profile.is_some()) {
            self.reload().await;
            return;
        }
        let _ = core.save(Table::Profiles, &demo.profile).await;
        for account in &demo.accounts {
            let _ = core.save(Table::Accounts, account).await;
        }
        for category in &demo.categories {
            let _ = core.save(Table::Categories, category).await;
        }
        for tx in &demo.transactions {
            let _ = core.save(Table::Transactions, tx).await;
        }
        let _ = core.save_debt(&demo.debt).await;
        for tx in &demo.history {
            let _ = core.save(Table::Transactions, tx).await;
        }
        let _ = core.save(Table::Budgets, &demo.budget).await;
        let _ = core.save(Table::Goals, &demo.goal).await;
        for item in &demo.contributions {
            let _ = core.save(Table::GoalContributions, item).await;
        }
        for rule in &demo.rules {
            let _ = core.save(Table::RecurringRules, rule).await;
        }
        self.reload().await;
    }

    async fn open(&self, user_id: Uuid, name: &str) -> bool {
        let store = match IdbStore::open(name).await {
            Ok(store) => store,
            Err(error) => {
                self.auth_error.set(Some(self.message(error)));
                return false;
            }
        };
        match AppCore::open(store, user_id, now_millis, random_node).await {
            Ok(core) => {
                self.core.set_value(Some(Rc::new(core)));
                self.reload().await;
                true
            }
            Err(error) => {
                self.auth_error
                    .set(Some(error.message(self.lang.get_untracked())));
                false
            }
        }
    }

    pub async fn reload(&self) {
        let Some(core) = self.core() else { return };
        if let Err(error) = core.generate_recurring(today()).await {
            self.error
                .set(Some(error.message(self.lang.get_untracked())));
        }
        if let Ok(pending) = core.pending_attachments().await {
            self.pending_attachments.set(pending);
        }
        match core.load().await {
            Ok(data) => {
                if let Some(locale) = data.profile.as_ref().and_then(|p| p.locale.clone()) {
                    let lang = Lang::parse(&locale);
                    if lang != self.lang.get_untracked() {
                        self.set_lang(lang);
                    }
                }
                self.data.set(Arc::new(data));
            }
            Err(error) => self
                .error
                .set(Some(error.message(self.lang.get_untracked()))),
        }
        if matches!(self.sync.get_untracked(), SyncState::LocalOnly) || self.syncing.get_untracked()
        {
            return;
        }
        if let Ok(pending) = core.pending_changes().await
            && pending > 0
        {
            self.sync.set(SyncState::Pending(pending));
        }
    }

    async fn signed_in(&self) {
        let Some(session) = self.client().session() else {
            return;
        };
        write_json(
            SESSION_KEY,
            &StoredSession {
                refresh_token: session.refresh_token.clone(),
                user_id: session.user.id,
                email: session.user.email.clone(),
            },
        );
        if self
            .open(session.user.id, &format!("caudal-{}", session.user.id))
            .await
        {
            self.session.set(Session::SignedIn {
                email: session.user.email,
                demo: false,
            });
            self.auth_error.set(None);
            self.auth_info.set(None);
            self.request_sync();
        }
    }

    fn auth_failed(&self, error: &auth::SupabaseError) {
        self.auth_error
            .set(Some(auth::error_message(error, self.lang.get_untracked())));
        self.auth_info.set(None);
    }

    pub fn sign_in(&self, email: String, password: String) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            match state.client().sign_in(email.trim(), &password).await {
                Ok(_) => state.signed_in().await,
                Err(error) => state.auth_failed(&error),
            }
            state.busy.set(false);
        });
    }

    pub fn sign_up(&self, email: String, password: String) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            match state.client().sign_up(email.trim(), &password).await {
                Ok(SignUpOutcome::SignedIn(_)) => state.signed_in().await,
                Ok(SignUpOutcome::ConfirmationRequired { .. }) => {
                    state.auth_error.set(None);
                    state
                        .auth_info
                        .set(Some(texts(state.lang.get_untracked()).check_email.into()));
                }
                Err(error) => state.auth_failed(&error),
            }
            state.busy.set(false);
        });
    }

    pub fn send_recovery(&self, email: String) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            match state.client().send_password_recovery(email.trim()).await {
                Ok(()) => {
                    state.auth_error.set(None);
                    state
                        .auth_info
                        .set(Some(texts(state.lang.get_untracked()).code_sent.into()));
                }
                Err(error) => state.auth_failed(&error),
            }
            state.busy.set(false);
        });
    }

    pub fn verify_recovery(&self, email: String, code: String) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            match state
                .client()
                .verify_recovery_code(email.trim(), &code)
                .await
            {
                Ok(_) => {
                    state.auth_error.set(None);
                    state.auth_info.set(None);
                    state.recovery_verified.set(true);
                }
                Err(error) => state.auth_failed(&error),
            }
            state.busy.set(false);
        });
    }

    pub fn update_password(&self, password: String) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            match state.client().update_password(&password).await {
                Ok(()) => state.signed_in().await,
                Err(error) => state.auth_failed(&error),
            }
            state.busy.set(false);
        });
    }

    pub fn sign_out(&self) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let demo = matches!(
                state.session.get_untracked(),
                Session::SignedIn { demo: true, .. }
            );
            if !demo {
                let _ = state.client().sign_out().await;
                remove_key(SESSION_KEY);
            }
            state.client().set_session(None);
            state.core.set_value(None);
            state.data.set(Arc::new(Data::default()));
            state.dialog.set(None);
            state.first_sync_done.set(false);
            state.recovery_verified.set(false);
            state.sync.set(SyncState::Synced);
            state.page.set(Page::Home);
            state.session.set(Session::SignedOut);
        });
    }

    pub fn onboard(&self, profile: Profile, suggested: bool, lang: Lang) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            match core.complete_onboarding(&profile, suggested, lang).await {
                Ok(()) => state.after_write().await,
                Err(error) => state.error.set(Some(error.message(lang))),
            }
        });
    }

    pub fn save(&self, item: Item) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            let result = match &item {
                Item::Transaction(model) => core.save(Table::Transactions, model).await,
                Item::Account(model) => core.save(Table::Accounts, model).await,
                Item::Category(model) => core.save(Table::Categories, model).await,
                Item::Tag(model) => core.save(Table::Tags, model).await,
                Item::Profile(model) => core.save(Table::Profiles, model).await,
                Item::Contact(model) => core.save(Table::Contacts, model).await,
                Item::Rule(model) => core.save(Table::RecurringRules, model).await,
                Item::Budget(model) => core.save(Table::Budgets, model).await,
                Item::Goal(model) => core.save(Table::Goals, model).await,
                Item::Contribution(model) => core.save(Table::GoalContributions, model).await,
                Item::Asset(model) => core.save(Table::Assets, model).await,
                Item::Trade(draft) => core.save_trade(draft).await,
                Item::Alert(model) => core.save(Table::PriceAlerts, model).await,
                Item::CategoryRule(model) => core.save(Table::CategoryRules, model).await,
                Item::Template(model) => core.save(Table::TransactionTemplates, model).await,
                Item::Debt(draft) => core.save_debt(draft).await,
                Item::Payment(payment, movement) => {
                    core.save_payment(payment, movement.as_ref()).await
                }
                Item::Shared(draft) => core.save_shared(draft).await,
                Item::Confirm(id) => core.confirm(*id).await,
            };
            match result {
                Ok(()) => {
                    state.show_toast(texts(state.lang.get_untracked()).saved);
                    state.after_write().await;
                }
                Err(error) => state
                    .error
                    .set(Some(error.message(state.lang.get_untracked()))),
            }
        });
    }

    /// Cambia el modo privado y vuelve a dibujar los montos.
    pub fn set_private(&self, private: bool) {
        caudal_app_state::privacy::set_private(private);
        self.privacy.update(|p| p.private = private);
        self.privacy.get_untracked().save();
        self.data.update(|_| {});
    }

    pub fn set_pin(&self, pin: Option<(String, String)>) {
        self.privacy.update(|p| {
            p.pin_hash = pin.as_ref().map(|(hash, _)| hash.clone());
            p.pin_salt = pin.as_ref().map(|(_, salt)| salt.clone());
        });
        self.privacy.get_untracked().save();
    }

    fn report(&self, error: caudal_app_state::AppError) {
        self.error
            .set(Some(error.message(self.lang.get_untracked())));
    }

    pub fn export_backup(&self) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            let day = today().to_string();
            match core.export_backup(&day).await {
                Ok(text) => crate::files::download(
                    &format!("caudal-copia-{day}.json"),
                    "application/json",
                    text.as_bytes(),
                ),
                Err(error) => state.report(error),
            }
        });
    }

    pub fn restore_backup(&self, text: String) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            let t = texts(state.lang.get_untracked());
            match core.restore_backup(&text).await {
                Ok(report) => {
                    state.show_toast(&format!(
                        "{}: {} · {}: {}",
                        t.restored, report.restored, t.skipped, report.skipped
                    ));
                    state.after_write().await;
                }
                Err(caudal_app_state::backup::RestoreError::Format(problem)) => {
                    state.error.set(Some(
                        match problem {
                            caudal_app_state::backup::BackupError::NotABackup => t.backup_invalid,
                            caudal_app_state::backup::BackupError::NewerVersion => t.backup_newer,
                        }
                        .to_owned(),
                    ))
                }
                Err(caudal_app_state::backup::RestoreError::App(error)) => state.report(error),
            }
        });
    }

    pub fn delete_all(&self) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            match core.delete_all_data().await {
                Ok(_) => state.after_write().await,
                Err(error) => state.report(error),
            }
        });
    }

    pub fn delete_account(&self) {
        let state = *self;
        state.busy.set(true);
        leptos::task::spawn_local(async move {
            let demo = matches!(
                state.session.get_untracked(),
                Session::SignedIn { demo: true, .. }
            );
            if demo {
                if let Some(core) = state.core() {
                    let _ = core.clear_local().await;
                }
            } else {
                let user_id = state.client().session().map(|s| s.user.id);
                if let Some(core) = state.core()
                    && let Err(error) = core.delete_account(&state.client()).await
                {
                    state.report(error);
                    state.busy.set(false);
                    return;
                }
                remove_key(SESSION_KEY);
                // IndexedDB no borra una base con la conexión abierta: la petición
                // queda en espera. Se suelta la conexión y el borrado se completa solo,
                // sin que la app se quede esperando.
                state.core.set_value(None);
                if let Some(id) = user_id {
                    leptos::task::spawn_local(async move {
                        let _ = caudal_store_idb::delete_database(&format!("caudal-{id}")).await;
                    });
                }
            }
            state.client().set_session(None);
            state.core.set_value(None);
            state.data.set(Arc::new(Data::default()));
            state.dialog.set(None);
            state.first_sync_done.set(false);
            state.recovery_verified.set(false);
            state.sync.set(SyncState::Synced);
            state.page.set(Page::Home);
            state.busy.set(false);
            state.session.set(Session::SignedOut);
        });
    }

    pub fn import(&self, transactions: Vec<Transaction>) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            let count = transactions.len();
            for tx in &transactions {
                if let Err(error) = core.save(Table::Transactions, tx).await {
                    state.report(error);
                    return;
                }
            }
            state.show_toast(&format!(
                "{}: {count}",
                texts(state.lang.get_untracked()).imported
            ));
            state.after_write().await;
        });
    }

    pub fn add_attachment(&self, attachment: caudal_app_state::model::Attachment, bytes: Vec<u8>) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            match core.add_attachment(&attachment, &bytes).await {
                Ok(()) => state.after_write().await,
                Err(error) => state.report(error),
            }
        });
    }

    /// Prepara el comprobante para verlo; el resultado queda en el diálogo.
    pub fn load_attachment(&self, attachment: caudal_app_state::model::Attachment) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            let client = state.client();
            let online = !matches!(state.sync.get_untracked(), SyncState::LocalOnly)
                && client.session().is_some();
            let result = core
                .attachment_bytes(online.then_some(client.as_ref()), &attachment)
                .await;
            let url = match result {
                Ok(Some(bytes)) => crate::files::object_url(&attachment.mime_type, &bytes),
                Ok(None) => None,
                Err(error) => {
                    state.report(error);
                    None
                }
            };
            state.dialog.update(|dialog| {
                if let Some(Dialog::Attachment {
                    attachment: shown,
                    url: slot,
                    loaded,
                }) = dialog
                    && shown.meta.id == attachment.meta.id
                {
                    *slot = url.clone();
                    *loaded = true;
                }
            });
        });
    }

    pub fn delete(&self, table: Table, id: Uuid) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            match core.delete(table, id).await {
                Ok(()) => state.after_write().await,
                Err(error) => state
                    .error
                    .set(Some(error.message(state.lang.get_untracked()))),
            }
        });
    }

    pub fn restore(&self, table: Table, id: Uuid) {
        let state = *self;
        leptos::task::spawn_local(async move {
            let Some(core) = state.core() else { return };
            match core.restore(table, id).await {
                Ok(()) => state.after_write().await,
                Err(error) => state
                    .error
                    .set(Some(error.message(state.lang.get_untracked()))),
            }
        });
    }

    async fn after_write(&self) {
        self.reload().await;
        let state = *self;
        set_timeout(move || state.request_sync(), Duration::from_millis(1_500));
    }

    pub fn request_sync(&self) {
        let demo = matches!(
            self.session.get_untracked(),
            Session::SignedIn { demo: true, .. }
        );
        if demo || self.client().session().is_none() || self.core().is_none() {
            return;
        }
        if self.syncing.get_untracked() {
            self.sync_again.set(true);
            return;
        }
        self.syncing.set(true);
        let state = *self;
        leptos::task::spawn_local(async move {
            loop {
                state.sync_once().await;
                if !state.sync_again.get_untracked() {
                    break;
                }
                state.sync_again.set(false);
            }
            state.syncing.set(false);
        });
    }

    async fn sync_once(&self) {
        let Some(core) = self.core() else { return };
        self.sync.set(SyncState::Syncing);
        let client = self.client();
        let result = core.sync(&client, today()).await;
        if let Some(session) = client.session() {
            write_json(
                SESSION_KEY,
                &StoredSession {
                    refresh_token: session.refresh_token,
                    user_id: session.user.id,
                    email: session.user.email,
                },
            );
        }
        let state = match result {
            Ok(state) => state,
            Err(error) => {
                self.error
                    .set(Some(error.message(self.lang.get_untracked())));
                SyncState::Offline {
                    pending: core.pending_changes().await.unwrap_or(0),
                }
            }
        };
        self.syncing.set(false);
        self.reload().await;
        self.syncing.set(true);
        self.first_sync_done.set(true);
        self.sync.set(state);
    }
}

/// Atajos para crear diálogos desde la interfaz.
impl AppState {
    pub fn open_dialog(&self, dialog: Dialog) {
        self.dialog_error.set(None);
        self.dialog.set(Some(dialog));
    }

    pub fn new_account(&self) {
        let data = self.data.get_untracked();
        self.open_dialog(Dialog::Account(AccountForm::new(&data)));
    }

    pub fn new_category(&self) {
        self.open_dialog(Dialog::Category(CategoryForm::new()));
    }

    pub fn new_tag(&self) {
        self.open_dialog(Dialog::Tag(TagForm::default()));
    }
}
