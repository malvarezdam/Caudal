//! Copia en memoria de los datos activos del usuario, base de todas las vistas.

use caudal_core::fx::FxRate;
use caudal_core::model::{
    Account, Asset, AssetPrice, Budget, Category, Contact, Debt, DebtInstallment, DebtPayment,
    Goal, GoalContribution, Profile, RecurringRule, Tag, Trade, Transaction, TransactionKind,
    TransactionSplit,
};
use caudal_sync::Record;
use uuid::Uuid;

/// Elemento de la papelera, de cualquier tabla.
#[derive(Debug, Clone, PartialEq)]
pub struct TrashItem {
    pub record: Record,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct Data {
    pub user_id: Uuid,
    pub profile: Option<Profile>,
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    pub transactions: Vec<Transaction>,
    pub splits: Vec<TransactionSplit>,
    pub contacts: Vec<Contact>,
    pub debts: Vec<Debt>,
    pub installments: Vec<DebtInstallment>,
    pub payments: Vec<DebtPayment>,
    pub rules: Vec<RecurringRule>,
    pub budgets: Vec<Budget>,
    pub goals: Vec<Goal>,
    pub contributions: Vec<GoalContribution>,
    pub assets: Vec<Asset>,
    pub trades: Vec<Trade>,
    /// Precios diarios del worker (tabla global `asset_prices`).
    pub prices: Vec<AssetPrice>,
    pub alerts: Vec<caudal_core::model::PriceAlert>,
    pub category_rules: Vec<caudal_core::model::CategoryRule>,
    pub templates: Vec<caudal_core::model::TransactionTemplate>,
    pub attachments: Vec<caudal_core::model::Attachment>,
    pub fx_rates: Vec<FxRate>,
    pub trash: Vec<TrashItem>,
    /// Filas locales que no se pudieron leer (versión de app antigua, datos dañados).
    pub unreadable: usize,
}

impl Data {
    pub fn account(&self, id: Uuid) -> Option<&Account> {
        self.accounts.iter().find(|account| account.meta.id == id)
    }

    pub fn category(&self, id: Uuid) -> Option<&Category> {
        self.categories
            .iter()
            .find(|category| category.meta.id == id)
    }

    pub fn contact(&self, id: Uuid) -> Option<&Contact> {
        self.contacts.iter().find(|contact| contact.meta.id == id)
    }

    pub fn debt(&self, id: Uuid) -> Option<&Debt> {
        self.debts.iter().find(|debt| debt.meta.id == id)
    }

    pub fn asset(&self, id: Uuid) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.meta.id == id)
    }

    pub fn rule(&self, id: Uuid) -> Option<&RecurringRule> {
        self.rules.iter().find(|rule| rule.meta.id == id)
    }

    pub fn contact_name(&self, id: Option<Uuid>) -> Option<&str> {
        id.and_then(|id| self.contact(id))
            .map(|contact| contact.name.as_str())
    }

    pub fn tag(&self, id: Uuid) -> Option<&Tag> {
        self.tags.iter().find(|tag| tag.meta.id == id)
    }

    pub fn transaction(&self, id: Uuid) -> Option<&Transaction> {
        self.transactions.iter().find(|tx| tx.meta.id == id)
    }

    pub fn account_name(&self, id: Option<Uuid>) -> Option<&str> {
        id.and_then(|id| self.account(id))
            .map(|account| account.name.as_str())
    }

    /// "Padre / hija" si la categoría está dentro de otra.
    pub fn category_path(&self, id: Uuid) -> Option<String> {
        let category = self.category(id)?;
        match category.parent_id.and_then(|parent| self.category(parent)) {
            Some(parent) if parent.meta.id != id => {
                Some(format!("{} / {}", parent.name, category.name))
            }
            _ => Some(category.name.clone()),
        }
    }

    /// Categorías ordenadas por ruta, aptas para el tipo de movimiento.
    pub fn categories_for(&self, kind: TransactionKind) -> Vec<(Uuid, String)> {
        use caudal_core::model::CategoryKind;
        let mut options: Vec<(Uuid, String)> = self
            .categories
            .iter()
            .filter(|category| match kind {
                TransactionKind::Income => category.kind != CategoryKind::Expense,
                TransactionKind::Expense => category.kind != CategoryKind::Income,
                _ => true,
            })
            .filter_map(|category| Some((category.meta.id, self.category_path(category.meta.id)?)))
            .collect();
        options.sort_by_key(|(_, path)| path.to_lowercase());
        options
    }

    /// Cuentas no archivadas, más la indicada (para editar movimientos antiguos).
    pub fn selectable_accounts(&self, keep: Option<Uuid>) -> Vec<&Account> {
        let mut accounts: Vec<&Account> = self
            .accounts
            .iter()
            .filter(|account| !account.archived || Some(account.meta.id) == keep)
            .collect();
        accounts.sort_by_key(|account| account.name.to_lowercase());
        accounts
    }
}

impl Data {
    /// Cuentas no archivadas en esa moneda: el dinero de una deuda o un gasto
    /// compartido solo puede moverse en una cuenta de la misma moneda.
    pub fn accounts_in(&self, currency: &str) -> Vec<(Uuid, String)> {
        let code = currency.trim().to_uppercase();
        self.selectable_accounts(None)
            .into_iter()
            .filter(|account| account.currency.as_str() == code)
            .map(|account| (account.meta.id, account.name.clone()))
            .collect()
    }
}
