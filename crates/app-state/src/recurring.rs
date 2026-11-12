//! Movimientos periódicos y calendario de vencimientos.

use crate::data::Data;
use crate::debts::summary;
use crate::format::{amount_for_input, format_date, format_money};
use crate::forms::{FormError, amount, currency, meta, optional_text};
use crate::i18n::{Lang, texts};
use crate::views::{Tone, TransactionView, transaction_view};
use caudal_core::model::{
    Frequency, RecurringMode, RecurringRule, TransactionKind, TransactionStatus,
};
use caudal_core::money::{Currency, Money};
use caudal_core::recurrence::{monthly_equivalent, next_occurrence, occurrences_between};
use chrono::{Days, NaiveDate};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct RecurringForm {
    pub name: String,
    pub kind: TransactionKind,
    pub amount: String,
    pub currency: String,
    pub account_id: Option<Uuid>,
    pub to_account_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub frequency: Frequency,
    pub interval: String,
    pub last_day_of_month: bool,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub mode: RecurringMode,
    pub active: bool,
    pub original: Option<RecurringRule>,
}

impl RecurringForm {
    pub fn new(data: &Data, today: NaiveDate) -> Self {
        let accounts = data.selectable_accounts(None);
        let account = (accounts.len() == 1).then(|| accounts[0]);
        Self {
            name: String::new(),
            kind: TransactionKind::Expense,
            amount: String::new(),
            currency: account.map_or_else(
                || crate::views::base_currency(data).to_string(),
                |account| account.currency.to_string(),
            ),
            account_id: account.map(|account| account.meta.id),
            to_account_id: None,
            category_id: None,
            frequency: Frequency::Monthly,
            interval: "1".into(),
            last_day_of_month: false,
            starts_on: today,
            ends_on: None,
            mode: RecurringMode::Confirm,
            active: true,
            original: None,
        }
    }

    pub fn edit(rule: &RecurringRule, lang: Lang) -> Self {
        Self {
            name: rule.name.clone().unwrap_or_default(),
            kind: rule.kind,
            amount: amount_for_input(rule.amount, lang),
            currency: rule.currency.to_string(),
            account_id: rule.account_id,
            to_account_id: rule.to_account_id,
            category_id: rule.category_id,
            frequency: rule.frequency,
            interval: rule.interval_count.to_string(),
            last_day_of_month: rule.last_day_of_month,
            starts_on: rule.starts_on,
            ends_on: rule.ends_on,
            mode: rule.mode,
            active: rule.active,
            original: Some(rule.clone()),
        }
    }

    pub fn select_account(&mut self, data: &Data, account_id: Option<Uuid>) {
        self.account_id = account_id;
        if let Some(account) = account_id.and_then(|id| data.account(id)) {
            self.currency = account.currency.to_string();
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<RecurringRule, FormError> {
        let value = amount(&self.amount, lang, false)?;
        let interval: i32 = self
            .interval
            .trim()
            .parse()
            .ok()
            .filter(|n| (1..=365).contains(n))
            .ok_or(FormError::AmountInvalid)?;
        if self.kind == TransactionKind::Transfer {
            match (self.account_id, self.to_account_id) {
                (Some(from), Some(to)) if from == to => return Err(FormError::TransferSameAccount),
                (Some(_), Some(_)) => {}
                _ => return Err(FormError::TransferNeedsAccounts),
            }
        }
        Ok(RecurringRule {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            name: optional_text(&self.name),
            frequency: self.frequency,
            interval_count: interval,
            last_day_of_month: self.last_day_of_month && self.frequency == Frequency::Monthly,
            starts_on: self.starts_on,
            ends_on: self.ends_on.filter(|ends| *ends >= self.starts_on),
            mode: self.mode,
            active: self.active,
            kind: self.kind,
            amount: value,
            currency: currency(&self.currency)?,
            account_id: self.account_id,
            to_account_id: (self.kind == TransactionKind::Transfer)
                .then_some(self.to_account_id)
                .flatten(),
            category_id: (self.kind != TransactionKind::Transfer)
                .then_some(self.category_id)
                .flatten(),
            description: None,
        })
    }
}

pub fn rule_title(data: &Data, rule: &RecurringRule, lang: Lang) -> String {
    rule.name
        .clone()
        .or_else(|| rule.category_id.and_then(|id| data.category_path(id)))
        .unwrap_or_else(|| texts(lang).transaction_kind(rule.kind).to_owned())
}

fn signed(kind: TransactionKind, value: Decimal) -> (Decimal, Tone) {
    match kind {
        TransactionKind::Expense => (-value, Tone::Negative),
        TransactionKind::Income => (value, Tone::Positive),
        _ => (value, Tone::Neutral),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleView {
    pub id: Uuid,
    pub title: String,
    pub amount_text: String,
    pub tone: Tone,
    pub frequency_text: String,
    pub next_text: Option<String>,
    pub monthly_text: String,
    pub mode_label: &'static str,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecurringView {
    pub pending: Vec<TransactionView>,
    pub rules: Vec<RuleView>,
    /// Gasto periódico mensual equivalente, por moneda (ingresos restan).
    pub monthly_totals: Vec<String>,
}

pub fn recurring_view(data: &Data, today: NaiveDate, lang: Lang) -> RecurringView {
    let t = texts(lang);
    let mut pending: Vec<_> = data
        .transactions
        .iter()
        .filter(|tx| tx.status == TransactionStatus::Pending)
        .collect();
    pending.sort_by_key(|tx| tx.occurred_on);

    let mut monthly: BTreeMap<Currency, Decimal> = BTreeMap::new();
    let mut rules: Vec<RuleView> = data
        .rules
        .iter()
        .map(|rule| {
            let (value, tone) = signed(rule.kind, rule.amount);
            if rule.active && rule.kind == TransactionKind::Expense {
                *monthly.entry(rule.currency.clone()).or_default() += monthly_equivalent(rule);
            }
            let money =
                |value: Decimal| format_money(&Money::new(value, rule.currency.clone()), lang);
            RuleView {
                id: rule.meta.id,
                title: rule_title(data, rule, lang),
                amount_text: money(value),
                tone,
                frequency_text: t.frequency(
                    rule.frequency,
                    rule.interval_count,
                    rule.last_day_of_month,
                ),
                next_text: rule
                    .active
                    .then(|| next_occurrence(rule, today))
                    .flatten()
                    .map(|date| format!("{} {}", t.next, format_date(date, lang))),
                monthly_text: format!(
                    "{} {}",
                    crate::format::format_estimate(
                        &Money::new(monthly_equivalent(rule), rule.currency.clone()),
                        lang
                    ),
                    t.monthly_cost
                ),
                mode_label: match rule.mode {
                    RecurringMode::Auto => t.mode_auto,
                    RecurringMode::Confirm => t.mode_confirm,
                },
                active: rule.active,
            }
        })
        .collect();
    rules.sort_by_key(|rule| (!rule.active, rule.title.to_lowercase()));

    RecurringView {
        pending: pending
            .into_iter()
            .map(|tx| transaction_view(data, tx, lang))
            .collect(),
        rules,
        monthly_totals: monthly
            .into_iter()
            .map(|(currency, value)| format_money(&Money::new(value, currency), lang))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpcomingItem {
    pub title: String,
    pub detail: &'static str,
    pub amount_text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpcomingDay {
    pub date: NaiveDate,
    pub date_text: String,
    pub items: Vec<UpcomingItem>,
}

/// Periódicos y vencimientos de deudas desde hoy hasta `days` días después.
pub fn upcoming(data: &Data, today: NaiveDate, days: u64, lang: Lang) -> Vec<UpcomingDay> {
    let t = texts(lang);
    let end = today.checked_add_days(Days::new(days)).unwrap_or(today);
    let mut by_day: BTreeMap<NaiveDate, Vec<UpcomingItem>> = BTreeMap::new();

    for rule in data.rules.iter().filter(|rule| rule.active) {
        let (value, tone) = signed(rule.kind, rule.amount);
        for date in occurrences_between(rule, today, end) {
            let id = caudal_core::recurrence::occurrence_id(rule.meta.id, date);
            // Lo ya registrado o confirmado no se anuncia otra vez.
            if data
                .transaction(id)
                .is_some_and(|tx| tx.status == TransactionStatus::Confirmed)
            {
                continue;
            }
            by_day.entry(date).or_default().push(UpcomingItem {
                title: rule_title(data, rule, lang),
                detail: t.recurring,
                amount_text: format_money(&Money::new(value, rule.currency.clone()), lang),
                tone,
            });
        }
    }

    for debt in &data.debts {
        let Ok(state) = summary(data, debt, today) else {
            continue;
        };
        if state.settled {
            continue;
        }
        let (sign, tone) = match debt.direction {
            caudal_core::model::DebtDirection::OwedToMe => (Decimal::ONE, Tone::Positive),
            caudal_core::model::DebtDirection::IOwe => (Decimal::NEGATIVE_ONE, Tone::Negative),
        };
        let title = data
            .contact_name(debt.contact_id)
            .map(str::to_owned)
            .or_else(|| debt.description.clone())
            .unwrap_or_else(|| t.direction(debt.direction).to_owned());
        let dues: Vec<(NaiveDate, Decimal, &'static str)> = if state.installments.is_empty() {
            debt.due_on
                .map(|due| (due, state.outstanding, t.due_on))
                .into_iter()
                .collect()
        } else {
            state
                .installments
                .iter()
                .filter(|item| !item.settled)
                .map(|item| (item.due_on, item.amount - item.paid, t.installment_label))
                .collect()
        };
        for (date, value, detail) in dues {
            // Lo vencido aparece hoy, para no olvidarlo.
            let shown = date.max(today);
            if shown > end {
                continue;
            }
            by_day.entry(shown).or_default().push(UpcomingItem {
                title: title.clone(),
                detail: if date < today { t.overdue } else { detail },
                amount_text: format_money(&Money::new(value * sign, debt.currency.clone()), lang),
                tone,
            });
        }
    }

    by_day
        .into_iter()
        .map(|(date, items)| UpcomingDay {
            date,
            date_text: format_date(date, lang),
            items,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: Uuid = Uuid::from_u128(1);

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn data() -> Data {
        Data {
            user_id: USER,
            ..Data::default()
        }
    }

    fn rule(name: &str, amount: &str, frequency: Frequency, start: &str) -> RecurringRule {
        RecurringForm {
            name: name.into(),
            amount: amount.into(),
            currency: "CLP".into(),
            frequency,
            starts_on: day(start),
            ..RecurringForm::new(&data(), day(start))
        }
        .validate(&data(), Lang::Es)
        .unwrap()
    }

    #[test]
    fn suscripciones_y_costo_mensual() {
        let mut data = data();
        data.rules
            .push(rule("Streaming", "9.990", Frequency::Monthly, "2026-01-06"));
        data.rules
            .push(rule("Dominio", "12.000", Frequency::Yearly, "2026-03-01"));
        let view = recurring_view(&data, day("2026-11-09"), Lang::Es);
        assert_eq!(view.monthly_totals, vec!["10.990 CLP"]);
        let streaming = view.rules.iter().find(|r| r.title == "Streaming").unwrap();
        assert_eq!(streaming.frequency_text, "Cada mes");
        assert_eq!(streaming.next_text.as_deref(), Some("Próximo 06/12/2026"));
        assert_eq!(streaming.amount_text, "-9.990 CLP");
    }

    #[test]
    fn calendario_incluye_periodicos_y_cuotas_vencidas() {
        let mut data = data();
        data.rules.push(rule(
            "Arriendo",
            "450.000",
            Frequency::Monthly,
            "2026-01-05",
        ));
        let debt = crate::debts::DebtForm {
            new_contact: "Ana".into(),
            principal: "30000".into(),
            currency: "CLP".into(),
            installments: "3".into(),
            first_installment: day("2026-11-01"),
            ..crate::debts::DebtForm::new(&data, day("2026-10-01"))
        }
        .validate(&data, Lang::Es)
        .unwrap();
        data.contacts.push(debt.contact.unwrap());
        data.debts.push(debt.debt);
        data.installments.extend(debt.installments);

        let days = upcoming(&data, day("2026-11-09"), 30, Lang::Es);
        let summary: Vec<(String, Vec<&str>)> = days
            .iter()
            .map(|d| {
                (
                    d.date.to_string(),
                    d.items.iter().map(|i| i.detail).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("2026-11-09".to_owned(), vec!["Vencida"]),
                ("2026-12-01".to_owned(), vec!["Cuota"]),
                ("2026-12-05".to_owned(), vec!["Periódicos"]),
            ]
        );
    }

    #[test]
    fn regla_valida_intervalo_y_transferencias() {
        let mut form = RecurringForm::new(&data(), day("2026-01-01"));
        form.amount = "10".into();
        form.interval = "0".into();
        assert_eq!(
            form.validate(&data(), Lang::Es),
            Err(FormError::AmountInvalid)
        );
        form.interval = "2".into();
        form.kind = TransactionKind::Transfer;
        assert_eq!(
            form.validate(&data(), Lang::Es),
            Err(FormError::TransferNeedsAccounts)
        );
    }
}
