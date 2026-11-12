//! Formularios: texto de los campos → modelo validado. Solo el monto es
//! obligatorio en un movimiento; lo demás se deduce o queda vacío.

use crate::data::Data;
use crate::format::{AmountError, amount_for_input, parse_amount};
use crate::i18n::{Lang, Texts};
use caudal_core::model::{
    Account, AccountKind, Category, CategoryKind, Hlc, Profile, SyncMeta, Tag, Transaction,
    TransactionKind, TransactionStatus,
};
use caudal_core::money::Currency;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormError {
    AmountRequired,
    AmountInvalid,
    TooManyDecimals,
    NegativeNotAllowed,
    TransferNeedsAccounts,
    TransferSameAccount,
    ReceivedAmountRequired,
    CurrencyInvalid,
    NameRequired,
    AccountCurrencyMismatch,
    SymbolRequired,
    QuantityRequired,
    AssetRequired,
    CategoryRequired,
}

impl FormError {
    pub fn message(self, texts: &Texts) -> &'static str {
        match self {
            FormError::AmountRequired => texts.amount_required,
            FormError::AmountInvalid => texts.amount_invalid,
            FormError::TooManyDecimals => texts.too_many_decimals,
            FormError::NegativeNotAllowed => texts.negative_not_allowed,
            FormError::TransferNeedsAccounts => texts.transfer_needs_accounts,
            FormError::TransferSameAccount => texts.transfer_same_account,
            FormError::ReceivedAmountRequired => texts.received_amount_required,
            FormError::CurrencyInvalid => texts.currency_invalid,
            FormError::NameRequired => texts.name_required,
            FormError::AccountCurrencyMismatch => texts.account_currency_mismatch,
            FormError::SymbolRequired => texts.symbol_required,
            FormError::QuantityRequired => texts.quantity_required,
            FormError::AssetRequired => texts.asset_required,
            FormError::CategoryRequired => texts.category_required,
        }
    }
}

pub(crate) fn amount(input: &str, lang: Lang, allow_negative: bool) -> Result<Decimal, FormError> {
    let value = parse_amount(input, lang).map_err(|error| match error {
        AmountError::Empty => FormError::AmountRequired,
        AmountError::Invalid => FormError::AmountInvalid,
        AmountError::TooManyDecimals => FormError::TooManyDecimals,
    })?;
    if value.is_sign_negative() && !value.is_zero() && !allow_negative {
        return Err(FormError::NegativeNotAllowed);
    }
    Ok(value)
}

pub(crate) fn optional_amount(input: &str, lang: Lang) -> Result<Option<Decimal>, FormError> {
    if input.trim().is_empty() {
        Ok(None)
    } else {
        amount(input, lang, false).map(Some)
    }
}

pub(crate) fn currency(input: &str) -> Result<Currency, FormError> {
    Currency::new(&input.trim().to_uppercase()).map_err(|_| FormError::CurrencyInvalid)
}

pub(crate) fn optional_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub(crate) fn required_name(input: &str) -> Result<String, FormError> {
    optional_text(input).ok_or(FormError::NameRequired)
}

/// Columnas de sincronización para un registro nuevo o editado. El repositorio
/// pone `user_id` y `updated_at`.
pub(crate) fn meta(id: Option<Uuid>, user_id: Uuid) -> SyncMeta {
    SyncMeta {
        id: id.unwrap_or_else(Uuid::now_v7),
        user_id,
        updated_at: Hlc(String::new()),
        deleted_at: None,
        server_seq: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionForm {
    pub kind: TransactionKind,
    pub amount: String,
    pub currency: String,
    pub date: NaiveDate,
    pub account_id: Option<Uuid>,
    pub to_account_id: Option<Uuid>,
    pub received_amount: String,
    pub category_id: Option<Uuid>,
    pub tag_ids: Vec<Uuid>,
    pub description: String,
    pub note: String,
    /// Al editar, el original: se conservan los campos que el formulario no muestra.
    pub original: Option<Transaction>,
}

impl TransactionForm {
    /// Nuevo gasto con la fecha de hoy, en la moneda principal o la de la única cuenta.
    pub fn new(data: &Data, today: NaiveDate) -> Self {
        let accounts = data.selectable_accounts(None);
        let account = (accounts.len() == 1).then(|| accounts[0]);
        let currency = account
            .map(|account| account.currency.to_string())
            .or_else(|| data.profile.as_ref().map(|p| p.base_currency.to_string()))
            .unwrap_or_else(|| "USD".into());
        Self {
            kind: TransactionKind::Expense,
            amount: String::new(),
            currency,
            date: today,
            account_id: account.map(|account| account.meta.id),
            to_account_id: None,
            received_amount: String::new(),
            category_id: None,
            tag_ids: Vec::new(),
            description: String::new(),
            note: String::new(),
            original: None,
        }
    }

    pub fn edit(transaction: &Transaction, lang: Lang) -> Self {
        Self {
            kind: transaction.kind,
            amount: amount_for_input(transaction.amount, lang),
            currency: transaction.currency.to_string(),
            date: transaction.occurred_on,
            account_id: transaction.account_id,
            to_account_id: transaction.to_account_id,
            received_amount: transaction
                .to_amount
                .map(|value| amount_for_input(value, lang))
                .unwrap_or_default(),
            category_id: transaction.category_id,
            tag_ids: transaction.tag_ids.clone(),
            description: transaction.description.clone().unwrap_or_default(),
            note: transaction.note.clone().unwrap_or_default(),
            original: Some(transaction.clone()),
        }
    }

    /// Al elegir una cuenta, el movimiento pasa a su moneda.
    pub fn select_account(&mut self, data: &Data, account_id: Option<Uuid>) {
        self.account_id = account_id;
        if let Some(account) = account_id.and_then(|id| data.account(id)) {
            self.currency = account.currency.to_string();
        }
    }

    /// Hay que pedir el monto recibido: transferencia entre monedas distintas.
    pub fn needs_received_amount(&self, data: &Data) -> bool {
        self.kind == TransactionKind::Transfer
            && self
                .to_account_id
                .and_then(|id| data.account(id))
                .is_some_and(|to| to.currency.as_str() != self.currency.trim().to_uppercase())
    }

    pub fn validate(
        &self,
        data: &Data,
        user_id: Uuid,
        lang: Lang,
    ) -> Result<Transaction, FormError> {
        let amount = amount(&self.amount, lang, self.kind == TransactionKind::Adjustment)?;
        let currency = currency(&self.currency)?;
        let mut to_account_id = None;
        let mut to_amount = None;
        if self.kind == TransactionKind::Transfer {
            let (Some(from), Some(to)) = (self.account_id, self.to_account_id) else {
                return Err(FormError::TransferNeedsAccounts);
            };
            if from == to {
                return Err(FormError::TransferSameAccount);
            }
            to_account_id = Some(to);
            to_amount = optional_amount(&self.received_amount, lang)?;
            if to_amount.is_none() && self.needs_received_amount(data) {
                return Err(FormError::ReceivedAmountRequired);
            }
        }
        let category_id = match self.kind {
            TransactionKind::Transfer | TransactionKind::Adjustment => None,
            _ => self.category_id,
        };

        let mut transaction = match &self.original {
            Some(original) => original.clone(),
            None => Transaction {
                meta: meta(None, user_id),
                kind: self.kind,
                status: TransactionStatus::Confirmed,
                amount,
                currency: currency.clone(),
                occurred_on: self.date,
                account_id: None,
                to_account_id: None,
                account_amount: None,
                to_amount: None,
                base_amount: None,
                base_currency: None,
                category_id: None,
                tag_ids: Vec::new(),
                contact_id: None,
                recurring_rule_id: None,
                occurrence_date: None,
                description: None,
                note: None,
            },
        };
        // La copia en moneda base deja de valer si cambia el importe o la moneda.
        if transaction.amount != amount || transaction.currency != currency {
            transaction.base_amount = None;
            transaction.base_currency = None;
        }
        if transaction.account_id != self.account_id || transaction.currency != currency {
            transaction.account_amount = None;
        }
        transaction.kind = self.kind;
        transaction.amount = amount;
        transaction.currency = currency;
        transaction.occurred_on = self.date;
        transaction.account_id = self.account_id;
        transaction.to_account_id = to_account_id;
        transaction.to_amount = to_amount;
        transaction.category_id = category_id;
        transaction.tag_ids = self.tag_ids.clone();
        transaction.description = optional_text(&self.description);
        transaction.note = optional_text(&self.note);
        Ok(transaction)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountForm {
    pub name: String,
    pub kind: AccountKind,
    pub currency: String,
    pub opening_balance: String,
    pub archived: bool,
    pub note: String,
    pub original: Option<Account>,
}

impl AccountForm {
    pub fn new(data: &Data) -> Self {
        Self {
            name: String::new(),
            kind: AccountKind::Bank,
            currency: data
                .profile
                .as_ref()
                .map_or_else(|| "USD".into(), |profile| profile.base_currency.to_string()),
            opening_balance: String::new(),
            archived: false,
            note: String::new(),
            original: None,
        }
    }

    pub fn edit(account: &Account, lang: Lang) -> Self {
        Self {
            name: account.name.clone(),
            kind: account.kind,
            currency: account.currency.to_string(),
            opening_balance: amount_for_input(account.opening_balance, lang),
            archived: account.archived,
            note: account.note.clone().unwrap_or_default(),
            original: Some(account.clone()),
        }
    }

    pub fn validate(&self, user_id: Uuid, lang: Lang) -> Result<Account, FormError> {
        let opening_balance = if self.opening_balance.trim().is_empty() {
            Decimal::ZERO
        } else {
            amount(&self.opening_balance, lang, true)?
        };
        Ok(Account {
            meta: self
                .original
                .as_ref()
                .map_or_else(|| meta(None, user_id), |original| original.meta.clone()),
            name: required_name(&self.name)?,
            kind: self.kind,
            currency: currency(&self.currency)?,
            opening_balance,
            archived: self.archived,
            color: self
                .original
                .as_ref()
                .and_then(|original| original.color.clone()),
            icon: self
                .original
                .as_ref()
                .and_then(|original| original.icon.clone()),
            note: optional_text(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryForm {
    pub name: String,
    pub kind: CategoryKind,
    pub parent_id: Option<Uuid>,
    pub color: String,
    pub original: Option<Category>,
}

impl CategoryForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            kind: CategoryKind::Expense,
            parent_id: None,
            color: String::new(),
            original: None,
        }
    }

    pub fn edit(category: &Category) -> Self {
        Self {
            name: category.name.clone(),
            kind: category.kind,
            parent_id: category.parent_id,
            color: category.color.clone().unwrap_or_default(),
            original: Some(category.clone()),
        }
    }

    pub fn validate(&self, user_id: Uuid) -> Result<Category, FormError> {
        let meta = self
            .original
            .as_ref()
            .map_or_else(|| meta(None, user_id), |original| original.meta.clone());
        // Una categoría no puede estar dentro de sí misma.
        let parent_id = self.parent_id.filter(|parent| *parent != meta.id);
        Ok(Category {
            meta,
            name: required_name(&self.name)?,
            parent_id,
            kind: self.kind,
            color: optional_text(&self.color),
            icon: self
                .original
                .as_ref()
                .and_then(|original| original.icon.clone()),
        })
    }
}

impl Default for CategoryForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TagForm {
    pub name: String,
    pub color: String,
    pub original: Option<Tag>,
}

impl TagForm {
    pub fn edit(tag: &Tag) -> Self {
        Self {
            name: tag.name.clone(),
            color: tag.color.clone().unwrap_or_default(),
            original: Some(tag.clone()),
        }
    }

    pub fn validate(&self, user_id: Uuid) -> Result<Tag, FormError> {
        Ok(Tag {
            meta: self
                .original
                .as_ref()
                .map_or_else(|| meta(None, user_id), |original| original.meta.clone()),
            name: required_name(&self.name)?,
            color: optional_text(&self.color),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileForm {
    pub base_currency: String,
    pub lang: Lang,
    pub display_name: String,
}

impl ProfileForm {
    pub fn new(lang: Lang) -> Self {
        Self {
            base_currency: "USD".into(),
            lang,
            display_name: String::new(),
        }
    }

    pub fn edit(profile: &Profile) -> Self {
        Self {
            base_currency: profile.base_currency.to_string(),
            lang: profile.locale.as_deref().map_or(Lang::Es, Lang::parse),
            display_name: profile.display_name.clone().unwrap_or_default(),
        }
    }

    pub fn validate(&self, user_id: Uuid) -> Result<Profile, FormError> {
        Ok(Profile {
            meta: meta(Some(user_id), user_id),
            base_currency: currency(&self.base_currency)?,
            locale: Some(self.lang.code().to_owned()),
            display_name: optional_text(&self.display_name),
        })
    }
}

/// Movimiento de ajuste que lleva el saldo de una cuenta al valor real indicado.
pub fn balance_adjustment(
    account: &Account,
    current_balance: Decimal,
    real_balance: &str,
    user_id: Uuid,
    today: NaiveDate,
    lang: Lang,
) -> Result<Option<Transaction>, FormError> {
    let target = amount(real_balance, lang, true)?;
    let delta = target - current_balance;
    if delta.is_zero() {
        return Ok(None);
    }
    Ok(Some(Transaction {
        meta: meta(None, user_id),
        kind: TransactionKind::Adjustment,
        status: TransactionStatus::Confirmed,
        amount: delta,
        currency: account.currency.clone(),
        occurred_on: today,
        account_id: Some(account.meta.id),
        to_account_id: None,
        account_amount: None,
        to_amount: None,
        base_amount: None,
        base_currency: None,
        category_id: None,
        tag_ids: Vec::new(),
        contact_id: None,
        recurring_rule_id: None,
        occurrence_date: None,
        description: None,
        note: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const USER: Uuid = Uuid::from_u128(1);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    fn account(name: &str, currency: &str) -> Account {
        AccountForm {
            name: name.into(),
            currency: currency.into(),
            ..AccountForm::new(&Data::default())
        }
        .validate(USER, Lang::Es)
        .unwrap()
    }

    #[test]
    fn un_gasto_solo_necesita_monto() {
        let data = Data::default();
        let mut form = TransactionForm::new(&data, today());
        assert_eq!(
            form.validate(&data, USER, Lang::Es),
            Err(FormError::AmountRequired)
        );
        form.amount = "12,50".into();
        let tx = form.validate(&data, USER, Lang::Es).unwrap();
        assert_eq!(tx.amount, Decimal::from_str("12.5").unwrap());
        assert_eq!(tx.kind, TransactionKind::Expense);
        assert_eq!(tx.occurred_on, today());
        assert!(tx.account_id.is_none() && tx.category_id.is_none() && tx.description.is_none());
    }

    #[test]
    fn usa_la_unica_cuenta_y_su_moneda() {
        let data = Data {
            accounts: vec![account("Banco", "CLP")],
            ..Data::default()
        };
        let form = TransactionForm::new(&data, today());
        assert_eq!(form.account_id, Some(data.accounts[0].meta.id));
        assert_eq!(form.currency, "CLP");
    }

    #[test]
    fn transferencias_validan_cuentas_y_monedas() {
        let usd = account("USD", "USD");
        let clp = account("CLP", "CLP");
        let data = Data {
            accounts: vec![usd.clone(), clp.clone()],
            ..Data::default()
        };
        let mut form = TransactionForm::new(&data, today());
        form.kind = TransactionKind::Transfer;
        form.amount = "10".into();
        assert_eq!(
            form.validate(&data, USER, Lang::Es),
            Err(FormError::TransferNeedsAccounts)
        );
        form.select_account(&data, Some(usd.meta.id));
        form.to_account_id = Some(usd.meta.id);
        assert_eq!(
            form.validate(&data, USER, Lang::Es),
            Err(FormError::TransferSameAccount)
        );
        form.to_account_id = Some(clp.meta.id);
        assert!(form.needs_received_amount(&data));
        assert_eq!(
            form.validate(&data, USER, Lang::Es),
            Err(FormError::ReceivedAmountRequired)
        );
        form.received_amount = "9.500".into();
        form.category_id = Some(Uuid::from_u128(9));
        let tx = form.validate(&data, USER, Lang::Es).unwrap();
        assert_eq!(tx.to_amount, Some(Decimal::from(9500)));
        assert_eq!(tx.category_id, None);
    }

    #[test]
    fn editar_conserva_lo_que_el_formulario_no_muestra() {
        let data = Data::default();
        let mut form = TransactionForm::new(&data, today());
        form.amount = "10".into();
        let mut original = form.validate(&data, USER, Lang::Es).unwrap();
        original.recurring_rule_id = Some(Uuid::from_u128(5));
        original.base_amount = Some(Decimal::from(9000));
        original.base_currency = Some(Currency::new("CLP").unwrap());

        let mut edit = TransactionForm::edit(&original, Lang::Es);
        edit.description = "  Pan  ".into();
        let kept = edit.validate(&data, USER, Lang::Es).unwrap();
        assert_eq!(kept.meta.id, original.meta.id);
        assert_eq!(kept.recurring_rule_id, original.recurring_rule_id);
        assert_eq!(kept.base_amount, original.base_amount);
        assert_eq!(kept.description.as_deref(), Some("Pan"));

        edit.amount = "11".into();
        let changed = edit.validate(&data, USER, Lang::Es).unwrap();
        assert_eq!(changed.base_amount, None);
    }

    #[test]
    fn solo_el_ajuste_admite_negativos() {
        let data = Data::default();
        let mut form = TransactionForm::new(&data, today());
        form.amount = "-5".into();
        assert_eq!(
            form.validate(&data, USER, Lang::Es),
            Err(FormError::NegativeNotAllowed)
        );
    }

    #[test]
    fn ajuste_de_saldo_crea_la_diferencia() {
        let bank = account("Banco", "USD");
        let tx = balance_adjustment(&bank, Decimal::from(100), "70", USER, today(), Lang::Es)
            .unwrap()
            .unwrap();
        assert_eq!(tx.amount, Decimal::from(-30));
        assert_eq!(tx.kind, TransactionKind::Adjustment);
        assert!(
            balance_adjustment(&bank, Decimal::from(70), "70", USER, today(), Lang::Es)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn nombres_y_monedas() {
        let form = AccountForm {
            name: "   ".into(),
            ..AccountForm::new(&Data::default())
        };
        assert_eq!(form.validate(USER, Lang::Es), Err(FormError::NameRequired));
        let form = AccountForm {
            name: "Billetera".into(),
            currency: " clp ".into(),
            ..AccountForm::new(&Data::default())
        };
        assert_eq!(
            form.validate(USER, Lang::Es).unwrap().currency.as_str(),
            "CLP"
        );
        let form = ProfileForm {
            base_currency: "$ pesos".into(),
            ..ProfileForm::new(Lang::Es)
        };
        assert_eq!(form.validate(USER), Err(FormError::CurrencyInvalid));
    }

    #[test]
    fn categoria_no_puede_ser_su_propia_madre() {
        let category = CategoryForm {
            name: "Comida".into(),
            ..CategoryForm::new()
        }
        .validate(USER)
        .unwrap();
        let mut edit = CategoryForm::edit(&category);
        edit.parent_id = Some(category.meta.id);
        assert_eq!(edit.validate(USER).unwrap().parent_id, None);
    }
}
