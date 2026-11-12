//! Deudas, pagos y gastos compartidos: formularios y vistas.
//!
//! El dinero que entra o sale por una deuda se registra como ajuste de saldo:
//! mueve la cuenta, pero no cuenta como ingreso ni gasto del mes.

use crate::data::Data;
use crate::format::{amount_for_input, currency_decimals, format_date, format_money};
use crate::forms::{FormError, amount, currency, meta, optional_amount, optional_text};
use crate::i18n::{Lang, texts};
use crate::views::Tone;
use caudal_core::debts::{DebtSummary, split_installments, summarize_debt};
use caudal_core::model::{
    Contact, Debt, DebtDirection, DebtInstallment, DebtPayment, Transaction, TransactionKind,
    TransactionStatus,
};
use caudal_core::money::{Currency, Money};
use chrono::{Months, NaiveDate};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Ajuste de saldo ligado a una deuda: `signed` positivo si entra dinero.
pub(crate) fn movement(
    user_id: Uuid,
    account_id: Uuid,
    signed: Decimal,
    currency: Currency,
    date: NaiveDate,
    contact_id: Option<Uuid>,
    description: String,
) -> Transaction {
    Transaction {
        meta: meta(None, user_id),
        kind: TransactionKind::Adjustment,
        status: TransactionStatus::Confirmed,
        amount: signed,
        currency,
        occurred_on: date,
        account_id: Some(account_id),
        to_account_id: None,
        account_amount: None,
        to_amount: None,
        base_amount: None,
        base_currency: None,
        category_id: None,
        tag_ids: Vec::new(),
        contact_id,
        recurring_rule_id: None,
        occurrence_date: None,
        description: Some(description),
        note: None,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ContactForm {
    pub name: String,
    pub phone: String,
    pub note: String,
    pub original: Option<Contact>,
}

impl ContactForm {
    pub fn edit(contact: &Contact) -> Self {
        Self {
            name: contact.name.clone(),
            phone: contact.phone.clone().unwrap_or_default(),
            note: contact.note.clone().unwrap_or_default(),
            original: Some(contact.clone()),
        }
    }

    pub fn validate(&self, user_id: Uuid) -> Result<Contact, FormError> {
        Ok(Contact {
            meta: self
                .original
                .as_ref()
                .map_or_else(|| meta(None, user_id), |original| original.meta.clone()),
            name: crate::forms::required_name(&self.name)?,
            phone: optional_text(&self.phone),
            note: optional_text(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebtForm {
    pub direction: DebtDirection,
    pub contact_id: Option<Uuid>,
    /// Si no se elige contacto, se crea uno con este nombre.
    pub new_contact: String,
    pub principal: String,
    pub currency: String,
    pub interest_rate: String,
    pub started_on: NaiveDate,
    pub due_on: Option<NaiveDate>,
    pub installments: String,
    pub first_installment: NaiveDate,
    /// Solo al crear: cuenta de la que sale o a la que entra el dinero.
    pub account_id: Option<Uuid>,
    pub description: String,
    pub closed: bool,
    pub original: Option<Debt>,
}

/// Lo que se guarda al crear o editar una deuda.
#[derive(Debug, Clone, PartialEq)]
pub struct DebtDraft {
    pub contact: Option<Contact>,
    pub debt: Debt,
    pub installments: Vec<DebtInstallment>,
    pub movement: Option<Transaction>,
}

impl DebtForm {
    pub fn new(data: &Data, today: NaiveDate) -> Self {
        Self {
            direction: DebtDirection::OwedToMe,
            contact_id: None,
            new_contact: String::new(),
            principal: String::new(),
            currency: crate::views::base_currency(data).to_string(),
            interest_rate: String::new(),
            started_on: today,
            due_on: None,
            installments: String::new(),
            first_installment: today.checked_add_months(Months::new(1)).unwrap_or(today),
            account_id: None,
            description: String::new(),
            closed: false,
            original: None,
        }
    }

    pub fn edit(debt: &Debt, today: NaiveDate, lang: Lang) -> Self {
        Self {
            direction: debt.direction,
            contact_id: debt.contact_id,
            new_contact: String::new(),
            principal: amount_for_input(debt.principal, lang),
            currency: debt.currency.to_string(),
            interest_rate: debt
                .interest_rate
                .map(|rate| amount_for_input(rate, lang))
                .unwrap_or_default(),
            started_on: debt.started_on.unwrap_or(today),
            due_on: debt.due_on,
            installments: String::new(),
            first_installment: today,
            account_id: None,
            description: debt.description.clone().unwrap_or_default(),
            closed: debt.closed,
            original: Some(debt.clone()),
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<DebtDraft, FormError> {
        let user_id = data.user_id;
        let principal = amount(&self.principal, lang, false)?;
        let currency = currency(&self.currency)?;
        let interest_rate = optional_amount(&self.interest_rate, lang)?;
        if self.original.is_none() {
            check_account(data, self.account_id, &currency)?;
        }
        let contact = match (self.contact_id, optional_text(&self.new_contact)) {
            (None, Some(name)) => Some(Contact {
                meta: meta(None, user_id),
                name,
                phone: None,
                note: None,
            }),
            _ => None,
        };
        let contact_id = self
            .contact_id
            .or_else(|| contact.as_ref().map(|contact| contact.meta.id));
        let debt = Debt {
            meta: self
                .original
                .as_ref()
                .map_or_else(|| meta(None, user_id), |original| original.meta.clone()),
            contact_id,
            direction: self.direction,
            principal,
            currency: currency.clone(),
            interest_rate,
            started_on: Some(self.started_on),
            due_on: self.due_on,
            closed: self.closed,
            description: optional_text(&self.description),
        };

        let count: u32 = match self.installments.trim() {
            "" => 0,
            text => text
                .parse()
                .ok()
                .filter(|n| (1..=360).contains(n))
                .ok_or(FormError::AmountInvalid)?,
        };
        let installments = if self.original.is_some() || count == 0 {
            Vec::new()
        } else {
            split_installments(principal, count, currency_decimals(&currency))
                .into_iter()
                .enumerate()
                .map(|(index, part)| DebtInstallment {
                    meta: meta(None, user_id),
                    debt_id: debt.meta.id,
                    due_on: self
                        .first_installment
                        .checked_add_months(Months::new(index as u32))
                        .unwrap_or(self.first_installment),
                    amount: part,
                    note: None,
                })
                .collect()
        };

        let who = contact
            .as_ref()
            .map(|contact| contact.name.clone())
            .or_else(|| data.contact_name(contact_id).map(str::to_owned));
        let movement = match (self.original.is_none(), self.account_id) {
            (true, Some(account_id)) => {
                // Prestar saca dinero de la cuenta; pedir prestado lo trae.
                let signed = match self.direction {
                    DebtDirection::OwedToMe => -principal,
                    DebtDirection::IOwe => principal,
                };
                let label = texts(lang).lent_label;
                Some(movement(
                    user_id,
                    account_id,
                    signed,
                    currency,
                    self.started_on,
                    contact_id,
                    who.map_or_else(|| label.to_owned(), |who| format!("{label} · {who}")),
                ))
            }
            _ => None,
        };
        Ok(DebtDraft {
            contact,
            debt,
            installments,
            movement,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaymentForm {
    pub debt: Debt,
    pub amount: String,
    pub date: NaiveDate,
    pub account_id: Option<Uuid>,
    pub note: String,
}

impl PaymentForm {
    /// Propone el monto de la próxima cuota o todo lo pendiente.
    pub fn new(data: &Data, debt: &Debt, today: NaiveDate, lang: Lang) -> Self {
        let suggested = summary(data, debt, today)
            .ok()
            .and_then(|summary| {
                summary
                    .next_due
                    .map(|(_, due)| due)
                    .or(Some(summary.outstanding))
            })
            .filter(|value| !value.is_zero())
            .map(|value| amount_for_input(value, lang))
            .unwrap_or_default();
        Self {
            debt: debt.clone(),
            amount: suggested,
            date: today,
            account_id: None,
            note: String::new(),
        }
    }

    pub fn validate(
        &self,
        data: &Data,
        lang: Lang,
    ) -> Result<(DebtPayment, Option<Transaction>), FormError> {
        let user_id = data.user_id;
        let value = amount(&self.amount, lang, false)?;
        check_account(data, self.account_id, &self.debt.currency)?;
        let movement = self.account_id.map(|account_id| {
            let signed = match self.debt.direction {
                DebtDirection::OwedToMe => value,
                DebtDirection::IOwe => -value,
            };
            let label = texts(lang).debt_payment_label;
            let who = data
                .contact_name(self.debt.contact_id)
                .map(str::to_owned)
                .or_else(|| self.debt.description.clone());
            movement(
                user_id,
                account_id,
                signed,
                self.debt.currency.clone(),
                self.date,
                self.debt.contact_id,
                who.map_or_else(|| label.to_owned(), |who| format!("{label} · {who}")),
            )
        });
        let payment = DebtPayment {
            meta: meta(None, user_id),
            debt_id: self.debt.meta.id,
            installment_id: None,
            transaction_id: movement.as_ref().map(|tx| tx.meta.id),
            amount: value,
            paid_on: self.date,
            note: optional_text(&self.note),
        };
        Ok((payment, movement))
    }
}

/// Gasto pagado por mí y repartido en partes iguales.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedExpenseForm {
    pub amount: String,
    pub currency: String,
    pub date: NaiveDate,
    pub account_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub description: String,
    pub contacts: Vec<Uuid>,
    pub include_me: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SharedDraft {
    /// Mi parte, como gasto normal del mes.
    pub expense: Option<Transaction>,
    /// Lo que adelanté por los demás: sale de la cuenta sin ser gasto propio.
    pub advance: Option<Transaction>,
    pub debts: Vec<Debt>,
}

impl SharedExpenseForm {
    pub fn new(data: &Data, today: NaiveDate) -> Self {
        let accounts = data.selectable_accounts(None);
        Self {
            amount: String::new(),
            currency: crate::views::base_currency(data).to_string(),
            date: today,
            account_id: (accounts.len() == 1).then(|| accounts[0].meta.id),
            category_id: None,
            description: String::new(),
            contacts: Vec::new(),
            include_me: true,
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<SharedDraft, FormError> {
        let user_id = data.user_id;
        let total = amount(&self.amount, lang, false)?;
        let currency = currency(&self.currency)?;
        check_account(data, self.account_id, &currency)?;
        if self.contacts.is_empty() {
            return Err(FormError::NameRequired);
        }
        let people = self.contacts.len() as u32 + u32::from(self.include_me);
        let share = (total / Decimal::from(people)).round_dp_with_strategy(
            currency_decimals(&currency),
            rust_decimal::RoundingStrategy::ToZero,
        );
        let others = share * Decimal::from(self.contacts.len() as u32);
        let mine = total - others;
        let description = optional_text(&self.description);
        let expense = (self.include_me && !mine.is_zero()).then(|| Transaction {
            meta: meta(None, user_id),
            kind: TransactionKind::Expense,
            status: TransactionStatus::Confirmed,
            amount: mine,
            currency: currency.clone(),
            occurred_on: self.date,
            account_id: self.account_id,
            to_account_id: None,
            account_amount: None,
            to_amount: None,
            base_amount: None,
            base_currency: None,
            category_id: self.category_id,
            tag_ids: Vec::new(),
            contact_id: None,
            recurring_rule_id: None,
            occurrence_date: None,
            description: description.clone(),
            note: None,
        });
        let names: Vec<&str> = self
            .contacts
            .iter()
            .filter_map(|id| data.contact(*id).map(|contact| contact.name.as_str()))
            .collect();
        let advance = self.account_id.map(|account_id| {
            movement(
                user_id,
                account_id,
                -others,
                currency.clone(),
                self.date,
                None,
                format!("{}: {}", texts(lang).shared_label, names.join(", ")),
            )
        });
        let debts = self
            .contacts
            .iter()
            .map(|contact_id| Debt {
                meta: meta(None, user_id),
                contact_id: Some(*contact_id),
                direction: DebtDirection::OwedToMe,
                principal: share,
                currency: currency.clone(),
                interest_rate: None,
                started_on: Some(self.date),
                due_on: None,
                closed: false,
                description: description.clone(),
            })
            .collect();
        Ok(SharedDraft {
            expense,
            advance,
            debts,
        })
    }
}

pub fn summary(
    data: &Data,
    debt: &Debt,
    today: NaiveDate,
) -> Result<DebtSummary, caudal_core::MoneyError> {
    summarize_debt(debt, &data.installments, &data.payments, today)
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallmentView {
    pub due_text: String,
    pub amount_text: String,
    pub settled: bool,
    pub overdue: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebtView {
    pub id: Uuid,
    pub title: String,
    pub direction: DebtDirection,
    pub direction_label: &'static str,
    pub outstanding_text: String,
    pub detail: String,
    /// Fracción pagada, de 0 a 1, para una barra de progreso.
    pub progress: f32,
    pub next_due_text: Option<String>,
    pub overdue: bool,
    pub settled: bool,
    pub tone: Tone,
    pub installments: Vec<InstallmentView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactView {
    pub id: Uuid,
    pub name: String,
    /// "Te debe 10.000 CLP · Le debes 5 USD".
    pub balance_text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebtsView {
    pub owed_to_me: Vec<String>,
    pub i_owe: Vec<String>,
    pub contacts: Vec<ContactView>,
    pub open: Vec<DebtView>,
    pub settled: Vec<DebtView>,
}

pub fn debts_view(data: &Data, today: NaiveDate, lang: Lang) -> DebtsView {
    let t = texts(lang);
    let mut owed_to_me: BTreeMap<Currency, Decimal> = BTreeMap::new();
    let mut i_owe: BTreeMap<Currency, Decimal> = BTreeMap::new();
    let mut by_contact: BTreeMap<Uuid, BTreeMap<Currency, Decimal>> = BTreeMap::new();
    let mut open = Vec::new();
    let mut settled = Vec::new();

    for debt in &data.debts {
        let Ok(summary) = summary(data, debt, today) else {
            continue;
        };
        let money = |value: Decimal| format_money(&Money::new(value, debt.currency.clone()), lang);
        if !summary.settled {
            let (totals, signed) = match debt.direction {
                DebtDirection::OwedToMe => (&mut owed_to_me, summary.outstanding),
                DebtDirection::IOwe => (&mut i_owe, -summary.outstanding),
            };
            *totals.entry(debt.currency.clone()).or_default() += summary.outstanding;
            if let Some(contact) = debt.contact_id {
                *by_contact
                    .entry(contact)
                    .or_default()
                    .entry(debt.currency.clone())
                    .or_default() += signed;
            }
        }
        let who = data.contact_name(debt.contact_id);
        let title = match (who, &debt.description) {
            (Some(who), Some(what)) => format!("{who} · {what}"),
            (Some(who), None) => who.to_owned(),
            (None, Some(what)) => what.clone(),
            (None, None) => t.direction(debt.direction).to_owned(),
        };
        let progress = if summary.total.is_zero() {
            1.0
        } else {
            (summary.paid / summary.total)
                .min(Decimal::ONE)
                .to_string()
                .parse()
                .unwrap_or(0.0)
        };
        let view = DebtView {
            id: debt.meta.id,
            title,
            direction: debt.direction,
            direction_label: t.direction(debt.direction),
            outstanding_text: money(summary.outstanding),
            detail: format!(
                "{} {} · {} {}",
                t.paid,
                money(summary.paid),
                t.total,
                money(summary.total)
            ),
            progress,
            next_due_text: summary.next_due.map(|(date, value)| {
                format!("{} {} · {}", t.next, format_date(date, lang), money(value))
            }),
            overdue: summary.overdue,
            settled: summary.settled,
            tone: match debt.direction {
                DebtDirection::OwedToMe => Tone::Positive,
                DebtDirection::IOwe => Tone::Negative,
            },
            installments: summary
                .installments
                .iter()
                .map(|item| InstallmentView {
                    due_text: format_date(item.due_on, lang),
                    amount_text: money(item.amount),
                    settled: item.settled,
                    overdue: item.overdue,
                })
                .collect(),
        };
        if view.settled {
            settled.push(view);
        } else {
            open.push(view);
        }
    }
    open.sort_by_key(|view| (!view.overdue, view.title.to_lowercase()));

    let totals = |map: BTreeMap<Currency, Decimal>| {
        map.into_iter()
            .filter(|(_, value)| !value.is_zero())
            .map(|(currency, value)| format_money(&Money::new(value, currency), lang))
            .collect::<Vec<_>>()
    };
    let contacts = data
        .contacts
        .iter()
        .map(|contact| {
            let balances = by_contact
                .get(&contact.meta.id)
                .cloned()
                .unwrap_or_default();
            let parts: Vec<String> = balances
                .iter()
                .filter(|(_, value)| !value.is_zero())
                .map(|(currency, value)| {
                    let label = if value.is_sign_positive() {
                        t.they_owe_you
                    } else {
                        t.you_owe
                    };
                    format!(
                        "{label} {}",
                        format_money(&Money::new(value.abs(), currency.clone()), lang)
                    )
                })
                .collect();
            let net: Decimal = balances.values().copied().sum();
            ContactView {
                id: contact.meta.id,
                name: contact.name.clone(),
                balance_text: if parts.is_empty() {
                    t.settled.to_owned()
                } else {
                    parts.join(" · ")
                },
                tone: if net.is_zero() {
                    Tone::Neutral
                } else if net.is_sign_positive() {
                    Tone::Positive
                } else {
                    Tone::Negative
                },
            }
        })
        .collect();

    DebtsView {
        owed_to_me: totals(owed_to_me),
        i_owe: totals(i_owe),
        contacts,
        open,
        settled,
    }
}

/// El dinero de una deuda o gasto compartido solo se mueve en cuentas de su moneda.
fn check_account(
    data: &Data,
    account_id: Option<Uuid>,
    currency: &Currency,
) -> Result<(), FormError> {
    match account_id.and_then(|id| data.account(id)) {
        Some(account) if &account.currency != currency => Err(FormError::AccountCurrencyMismatch),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::AccountForm;
    use caudal_core::ledger::account_balances;
    use std::str::FromStr;

    const USER: Uuid = Uuid::from_u128(1);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    fn data_with_account() -> Data {
        let mut data = Data {
            user_id: USER,
            ..Data::default()
        };
        let account = AccountForm {
            name: "Banco".into(),
            currency: "CLP".into(),
            opening_balance: "500000".into(),
            ..AccountForm::new(&data)
        }
        .validate(USER, Lang::Es)
        .unwrap();
        data.accounts.push(account);
        data
    }

    fn balance(data: &Data) -> Decimal {
        account_balances(&data.accounts, &data.transactions).unwrap()[&data.accounts[0].meta.id]
            .balance
            .amount
    }

    #[test]
    fn prestamo_en_cuotas_y_pago_parcial_mueven_cuenta_y_deuda() {
        let mut data = data_with_account();
        let account = data.accounts[0].meta.id;
        let form = DebtForm {
            new_contact: "Ana".into(),
            principal: "100.000".into(),
            currency: "CLP".into(),
            installments: "3".into(),
            account_id: Some(account),
            ..DebtForm::new(&data, today())
        };
        let draft = form.validate(&data, Lang::Es).unwrap();
        assert_eq!(draft.contact.as_ref().unwrap().name, "Ana");
        let amounts: Vec<_> = draft.installments.iter().map(|i| i.amount).collect();
        assert_eq!(
            amounts,
            vec![
                Decimal::from(33333),
                Decimal::from(33333),
                Decimal::from(33334)
            ]
        );
        data.contacts.push(draft.contact.clone().unwrap());
        data.debts.push(draft.debt.clone());
        data.installments.extend(draft.installments.clone());
        data.transactions.push(draft.movement.clone().unwrap());
        assert_eq!(balance(&data), Decimal::from(400_000));

        let mut payment = PaymentForm::new(&data, &draft.debt, today(), Lang::Es);
        assert_eq!(payment.amount, "33333");
        payment.amount = "20000".into();
        payment.account_id = Some(account);
        let (paid, tx) = payment.validate(&data, Lang::Es).unwrap();
        assert_eq!(paid.transaction_id, tx.as_ref().map(|tx| tx.meta.id));
        data.payments.push(paid);
        data.transactions.push(tx.unwrap());
        assert_eq!(balance(&data), Decimal::from(420_000));

        let view = debts_view(&data, today(), Lang::Es);
        assert_eq!(view.owed_to_me, vec!["80.000 CLP"]);
        assert_eq!(view.contacts[0].balance_text, "Te debe 80.000 CLP");
        assert_eq!(view.open[0].outstanding_text, "80.000 CLP");
        assert!(!view.open[0].installments[0].settled);

        // El préstamo y su devolución no son ingresos ni gastos del mes.
        let month = crate::views::dashboard(&data, today(), Lang::Es).month;
        assert_eq!(month.income, "0,00 USD");
    }

    #[test]
    fn deuda_propia_sin_cuenta_ni_contacto() {
        let data = data_with_account();
        let form = DebtForm {
            direction: DebtDirection::IOwe,
            principal: "50".into(),
            currency: "USD".into(),
            description: "Tarjeta".into(),
            ..DebtForm::new(&data, today())
        };
        let draft = form.validate(&data, Lang::Es).unwrap();
        assert!(
            draft.contact.is_none() && draft.movement.is_none() && draft.installments.is_empty()
        );
        let mut data = data;
        data.debts.push(draft.debt);
        let view = debts_view(&data, today(), Lang::Es);
        assert_eq!(view.i_owe, vec!["50,00 USD"]);
        assert_eq!(view.open[0].title, "Tarjeta");
    }

    #[test]
    fn gasto_compartido_reparte_y_crea_deudas() {
        let mut data = data_with_account();
        let account = data.accounts[0].meta.id;
        for name in ["Ana", "Luis"] {
            data.contacts.push(
                ContactForm {
                    name: name.into(),
                    ..ContactForm::default()
                }
                .validate(USER)
                .unwrap(),
            );
        }
        let form = SharedExpenseForm {
            amount: "30.001".into(),
            currency: "CLP".into(),
            account_id: Some(account),
            description: "Asado".into(),
            contacts: data.contacts.iter().map(|c| c.meta.id).collect(),
            ..SharedExpenseForm::new(&data, today())
        };
        let draft = form.validate(&data, Lang::Es).unwrap();
        assert_eq!(draft.debts.len(), 2);
        assert!(
            draft
                .debts
                .iter()
                .all(|d| d.principal == Decimal::from(10000))
        );
        let expense = draft.expense.clone().unwrap();
        assert_eq!(expense.amount, Decimal::from(10001));
        let advance = draft.advance.clone().unwrap();
        assert_eq!(advance.amount, Decimal::from(-20000));
        data.transactions.extend([expense, advance]);
        // La cuenta baja el total; el gasto del mes es solo mi parte.
        assert_eq!(balance(&data), Decimal::from(469_999));
        data.debts.extend(draft.debts);
        let view = debts_view(&data, today(), Lang::Es);
        assert_eq!(view.owed_to_me, vec!["20.000 CLP"]);

        let empty = SharedExpenseForm {
            contacts: Vec::new(),
            ..form
        };
        assert!(empty.validate(&data, Lang::Es).is_err());
    }

    #[test]
    fn cuotas_invalidas() {
        let data = data_with_account();
        let form = DebtForm {
            principal: "10".into(),
            installments: "0".into(),
            ..DebtForm::new(&data, today())
        };
        assert_eq!(
            form.validate(&data, Lang::Es),
            Err(FormError::AmountInvalid)
        );
        let _ = Decimal::from_str("1");
    }
}

#[cfg(test)]
mod currency_tests {
    use super::*;
    use crate::forms::AccountForm;

    #[test]
    fn rechaza_cuenta_de_otra_moneda() {
        let user = Uuid::from_u128(1);
        let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let mut data = Data {
            user_id: user,
            ..Data::default()
        };
        let usd = AccountForm {
            name: "USD".into(),
            currency: "USD".into(),
            ..AccountForm::new(&data)
        }
        .validate(user, Lang::Es)
        .unwrap();
        data.accounts.push(usd.clone());
        let form = DebtForm {
            principal: "1000".into(),
            currency: "CLP".into(),
            account_id: Some(usd.meta.id),
            ..DebtForm::new(&data, today)
        };
        assert_eq!(
            form.validate(&data, Lang::Es),
            Err(FormError::AccountCurrencyMismatch)
        );
        let draft = DebtForm {
            account_id: None,
            ..form
        }
        .validate(&data, Lang::Es)
        .unwrap();
        let payment = PaymentForm {
            account_id: Some(usd.meta.id),
            amount: "10".into(),
            ..PaymentForm::new(&data, &draft.debt, today, Lang::Es)
        };
        assert_eq!(
            payment.validate(&data, Lang::Es),
            Err(FormError::AccountCurrencyMismatch)
        );
    }
}
