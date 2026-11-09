//! Entidades del dominio. Los nombres de campo coinciden con las columnas de
//! `supabase/migrations`.

use crate::money::Currency;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Marca del reloj lógico híbrido, comparable como texto. Se genera en la Fase 2.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hlc(pub String);

/// Columnas comunes de toda tabla sincronizable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncMeta {
    pub id: Uuid,
    pub user_id: Uuid,
    pub updated_at: Hlc,
    pub deleted_at: Option<Hlc>,
    /// Lo asigna el servidor; nunca se envía.
    #[serde(default, skip_serializing)]
    pub server_seq: Option<i64>,
}

impl SyncMeta {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// Preferencias del usuario; su `id` es siempre el `user_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub base_currency: Currency,
    pub locale: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Cash,
    Bank,
    Card,
    Wallet,
    Broker,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    #[serde(default)]
    pub kind: AccountKind,
    pub currency: Currency,
    #[serde(default)]
    pub opening_balance: Decimal,
    #[serde(default)]
    pub archived: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryKind {
    Income,
    Expense,
    #[default]
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub kind: CategoryKind,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Income,
    #[default]
    Expense,
    Transfer,
    /// Corrección manual de saldo; es el único tipo con importe negativo.
    Adjustment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    #[default]
    Confirmed,
    /// Generado por una regla periódica, a la espera de confirmación.
    Pending,
}

/// Movimiento. Solo son obligatorios importe, moneda y fecha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    #[serde(flatten)]
    pub meta: SyncMeta,
    #[serde(default)]
    pub kind: TransactionKind,
    #[serde(default)]
    pub status: TransactionStatus,
    pub amount: Decimal,
    pub currency: Currency,
    pub occurred_on: NaiveDate,
    pub account_id: Option<Uuid>,
    pub to_account_id: Option<Uuid>,
    /// Importe en la moneda de `account_id`, si difiere de `currency`.
    pub account_amount: Option<Decimal>,
    /// Importe recibido por `to_account_id`, en la moneda de esa cuenta.
    pub to_amount: Option<Decimal>,
    /// Copia del importe en moneda base, para que el histórico no cambie.
    pub base_amount: Option<Decimal>,
    pub base_currency: Option<Currency>,
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
    pub contact_id: Option<Uuid>,
    pub recurring_rule_id: Option<Uuid>,
    pub occurrence_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub note: Option<String>,
}

/// Parte de un movimiento asignada a una categoría.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionSplit {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub transaction_id: Uuid,
    pub category_id: Option<Uuid>,
    pub amount: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub phone: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebtDirection {
    OwedToMe,
    IOwe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debt {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub contact_id: Option<Uuid>,
    pub direction: DebtDirection,
    pub principal: Decimal,
    pub currency: Currency,
    /// Porcentaje anual opcional.
    pub interest_rate: Option<Decimal>,
    pub started_on: Option<NaiveDate>,
    pub due_on: Option<NaiveDate>,
    #[serde(default)]
    pub closed: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebtInstallment {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub debt_id: Uuid,
    pub due_on: NaiveDate,
    pub amount: Decimal,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebtPayment {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub debt_id: Uuid,
    pub installment_id: Option<Uuid>,
    pub transaction_id: Option<Uuid>,
    pub amount: Decimal,
    pub paid_on: NaiveDate,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurringMode {
    Auto,
    #[default]
    Confirm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringRule {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: Option<String>,
    pub frequency: Frequency,
    /// Cada cuántas unidades de `frequency` se repite.
    pub interval_count: i32,
    #[serde(default)]
    pub last_day_of_month: bool,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    #[serde(default)]
    pub mode: RecurringMode,
    pub active: bool,
    #[serde(default)]
    pub kind: TransactionKind,
    pub amount: Decimal,
    pub currency: Currency,
    pub account_id: Option<Uuid>,
    pub to_account_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPeriod {
    #[default]
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    #[serde(flatten)]
    pub meta: SyncMeta,
    /// Sin categoría, el presupuesto cubre todo el gasto.
    pub category_id: Option<Uuid>,
    pub amount: Decimal,
    pub currency: Currency,
    #[serde(default)]
    pub period: BudgetPeriod,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<NaiveDate>,
    #[serde(default)]
    pub rollover: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    pub target_amount: Decimal,
    pub currency: Currency,
    pub target_date: Option<NaiveDate>,
    pub account_id: Option<Uuid>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalContribution {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub goal_id: Uuid,
    pub transaction_id: Option<Uuid>,
    pub amount: Decimal,
    pub contributed_on: NaiveDate,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Stock,
    Etf,
    Crypto,
    Fund,
    Bond,
    RealEstate,
    #[default]
    Other,
}

/// Fuente del precio; `Manual` no se actualiza automáticamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceProviderId {
    Eodhd,
    Coingecko,
    #[default]
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub name: String,
    #[serde(default)]
    pub kind: AssetKind,
    pub currency: Currency,
    #[serde(default)]
    pub provider: PriceProviderId,
    pub provider_symbol: Option<String>,
    pub manual_price: Option<Decimal>,
    pub manual_price_on: Option<NaiveDate>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    Buy,
    Sell,
    Dividend,
    Fee,
    Split,
    TransferIn,
    TransferOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    #[serde(flatten)]
    pub meta: SyncMeta,
    pub asset_id: Uuid,
    pub kind: TradeKind,
    pub traded_on: NaiveDate,
    pub quantity: Option<Decimal>,
    pub price: Option<Decimal>,
    pub fee: Option<Decimal>,
    pub currency: Currency,
    /// Acciones nuevas por cada una antigua, en un split.
    pub split_ratio: Option<Decimal>,
    pub account_id: Option<Uuid>,
    pub transaction_id: Option<Uuid>,
    pub note: Option<String>,
}

/// Precio diario común a todos los usuarios; solo lo escribe el worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPrice {
    pub provider: PriceProviderId,
    pub symbol: String,
    pub price_on: NaiveDate,
    pub price: Decimal,
    pub currency: Currency,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_movimiento_solo_exige_importe_moneda_y_fecha() {
        let json = r#"{
            "id": "018f0000-0000-7000-8000-000000000001",
            "user_id": "018f0000-0000-7000-8000-000000000002",
            "updated_at": "0001",
            "amount": "12.50",
            "currency": "USD",
            "occurred_on": "2026-11-09"
        }"#;
        let transaction: Transaction = serde_json::from_str(json).unwrap();
        assert_eq!(transaction.kind, TransactionKind::Expense);
        assert_eq!(transaction.status, TransactionStatus::Confirmed);
        assert!(transaction.account_id.is_none());
        assert!(transaction.category_id.is_none());
        assert!(!transaction.meta.is_deleted());
    }

    #[test]
    fn no_envia_el_cursor_del_servidor() {
        let meta = SyncMeta {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            updated_at: Hlc("0001".into()),
            deleted_at: None,
            server_seq: Some(7),
        };
        assert!(!serde_json::to_string(&meta).unwrap().contains("server_seq"));
    }

    #[test]
    fn enums_usan_los_valores_de_la_base() {
        assert_eq!(
            serde_json::to_string(&DebtDirection::OwedToMe).unwrap(),
            "\"owed_to_me\""
        );
        assert_eq!(
            serde_json::to_string(&AssetKind::RealEstate).unwrap(),
            "\"real_estate\""
        );
    }
}
