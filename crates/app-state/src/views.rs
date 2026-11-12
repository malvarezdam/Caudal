//! Datos listos para dibujar: textos formateados, sin lógica en las UI.

use crate::data::Data;
use crate::format::{format_date, format_money};
use crate::i18n::{Lang, texts};
use caudal_core::fx::FxTable;
use caudal_core::ledger::account_balances;
use caudal_core::model::{AccountKind, Transaction, TransactionKind, TransactionStatus};
use caudal_core::money::{Currency, Money};
use caudal_core::summary::{Period, summarize};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Cómo pintar un importe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Positive,
    Negative,
    Neutral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountView {
    pub id: Uuid,
    pub name: String,
    pub kind: AccountKind,
    pub kind_label: &'static str,
    pub balance: Decimal,
    pub currency: Currency,
    pub balance_text: String,
    pub tone: Tone,
    pub archived: bool,
    /// Movimientos en otra moneda sin equivalente en la cuenta.
    pub unconverted: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionView {
    pub id: Uuid,
    pub date: NaiveDate,
    pub date_text: String,
    pub title: String,
    pub subtitle: String,
    pub amount_text: String,
    pub tone: Tone,
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonthView {
    pub income: String,
    pub expense: String,
    pub savings: String,
    pub savings_tone: Tone,
    pub has_unconverted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardView {
    /// Un total por moneda; si todo es convertible, uno solo en moneda base.
    pub net_worth: Vec<String>,
    pub net_worth_incomplete: bool,
    /// "Incluye inversiones: 5.808.200 CLP".
    pub investments: Option<String>,
    pub accounts: Vec<AccountView>,
    pub month: MonthView,
    pub recent: Vec<TransactionView>,
}

fn tone(value: Decimal) -> Tone {
    if value.is_sign_negative() && !value.is_zero() {
        Tone::Negative
    } else if value.is_zero() {
        Tone::Neutral
    } else {
        Tone::Positive
    }
}

pub fn base_currency(data: &Data) -> Currency {
    data.profile
        .as_ref()
        .map(|profile| profile.base_currency.clone())
        .unwrap_or_else(|| Currency::new("USD").expect("USD es válido"))
}

pub fn accounts(data: &Data, lang: Lang, include_archived: bool) -> Vec<AccountView> {
    let balances = account_balances(&data.accounts, &data.transactions).unwrap_or_default();
    let texts = texts(lang);
    let mut views: Vec<AccountView> = data
        .accounts
        .iter()
        .filter(|account| include_archived || !account.archived)
        .map(|account| {
            let (balance, unconverted) = balances
                .get(&account.meta.id)
                .map_or((account.opening_balance, 0), |entry| {
                    (entry.balance.amount, entry.unconverted.len())
                });
            AccountView {
                id: account.meta.id,
                name: account.name.clone(),
                kind: account.kind,
                kind_label: texts.account_kind(account.kind),
                balance,
                currency: account.currency.clone(),
                balance_text: format_money(&Money::new(balance, account.currency.clone()), lang),
                tone: tone(balance),
                archived: account.archived,
                unconverted,
            }
        })
        .collect();
    views.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    views
}

pub fn transaction_view(data: &Data, tx: &Transaction, lang: Lang) -> TransactionView {
    let texts = texts(lang);
    let category = tx.category_id.and_then(|id| data.category_path(id));
    let title = tx
        .description
        .clone()
        .or_else(|| category.clone())
        .unwrap_or_else(|| texts.transaction_kind(tx.kind).to_owned());
    let from = data.account_name(tx.account_id);
    let to = data.account_name(tx.to_account_id);
    let mut parts: Vec<String> = Vec::new();
    match (tx.kind, from, to) {
        (TransactionKind::Transfer, from, to) => parts.push(format!(
            "{} » {}",
            from.unwrap_or(texts.no_account),
            to.unwrap_or(texts.no_account)
        )),
        (_, Some(from), _) => parts.push(from.to_owned()),
        _ => {}
    }
    if tx.description.is_some()
        && let Some(category) = category
    {
        parts.push(category);
    }
    let tag_names: Vec<&str> = tx
        .tag_ids
        .iter()
        .filter_map(|id| data.tag(*id).map(|tag| tag.name.as_str()))
        .collect();
    if !tag_names.is_empty() {
        parts.push(
            tag_names
                .iter()
                .map(|name| format!("#{name}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let signed = match tx.kind {
        TransactionKind::Expense => -tx.amount,
        _ => tx.amount,
    };
    let tone = match tx.kind {
        TransactionKind::Transfer => Tone::Neutral,
        _ => tone(signed),
    };
    let money = Money::new(signed, tx.currency.clone());
    let amount_text = match tx.kind {
        TransactionKind::Income => format!("+{}", format_money(&money, lang)),
        _ => format_money(&money, lang),
    };
    TransactionView {
        id: tx.meta.id,
        date: tx.occurred_on,
        date_text: format_date(tx.occurred_on, lang),
        title,
        subtitle: parts.join(" · "),
        amount_text,
        tone,
        pending: tx.status == TransactionStatus::Pending,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransactionFilter {
    pub text: String,
    pub account_id: Option<Uuid>,
    /// `Some(None)` filtra los movimientos sin categoría.
    pub category_id: Option<Option<Uuid>>,
    pub kind: Option<TransactionKind>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

impl TransactionFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn matches(&self, data: &Data, tx: &Transaction) -> bool {
        if self.kind.is_some_and(|kind| kind != tx.kind)
            || self.from.is_some_and(|from| tx.occurred_on < from)
            || self.to.is_some_and(|to| tx.occurred_on > to)
        {
            return false;
        }
        if let Some(account) = self.account_id
            && tx.account_id != Some(account)
            && tx.to_account_id != Some(account)
        {
            return false;
        }
        if let Some(category) = self.category_id {
            // Incluye las subcategorías de la elegida.
            let parent = tx
                .category_id
                .and_then(|id| data.category(id))
                .and_then(|c| c.parent_id);
            if tx.category_id != category && (category.is_none() || parent != category) {
                return false;
            }
        }
        let needle = self.text.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let haystack = [
            tx.description.as_deref(),
            tx.note.as_deref(),
            data.account_name(tx.account_id),
            data.account_name(tx.to_account_id),
            tx.category_id
                .and_then(|id| data.category(id))
                .map(|c| c.name.as_str()),
        ];
        haystack
            .iter()
            .flatten()
            .any(|value| value.to_lowercase().contains(&needle))
            || tx.tag_ids.iter().any(|id| {
                data.tag(*id)
                    .is_some_and(|tag| tag.name.to_lowercase().contains(&needle))
            })
            || tx.amount.to_string().contains(&needle.replace(',', "."))
    }
}

/// Movimientos filtrados, del más reciente al más antiguo. Devuelve también el total.
pub fn transactions(
    data: &Data,
    filter: &TransactionFilter,
    limit: usize,
    lang: Lang,
) -> (Vec<TransactionView>, usize) {
    let mut matching: Vec<&Transaction> = data
        .transactions
        .iter()
        .filter(|tx| filter.matches(data, tx))
        .collect();
    matching.sort_by(|a, b| {
        b.occurred_on
            .cmp(&a.occurred_on)
            .then(b.meta.updated_at.cmp(&a.meta.updated_at))
    });
    let total = matching.len();
    let views = matching
        .into_iter()
        .take(limit)
        .map(|tx| transaction_view(data, tx, lang))
        .collect();
    (views, total)
}

pub fn dashboard(data: &Data, today: NaiveDate, lang: Lang) -> DashboardView {
    let accounts = accounts(data, lang, true);
    let base = base_currency(data);
    let fx = FxTable::new(data.fx_rates.iter().cloned());

    let mut by_currency: BTreeMap<Currency, Decimal> = BTreeMap::new();
    for account in &accounts {
        *by_currency.entry(account.currency.clone()).or_default() += account.balance;
    }
    let mut incomplete = accounts.iter().any(|account| account.unconverted > 0);
    // El patrimonio incluye las inversiones a su valor de hoy.
    let mut investments: Option<(Decimal, Currency)> = None;
    for asset in data.assets.iter().filter(|asset| !asset.meta.is_deleted()) {
        let Ok(position) =
            caudal_core::portfolio::position(asset, &data.trades, &data.prices, &fx, today)
        else {
            continue;
        };
        if position.quantity <= Decimal::ZERO {
            continue;
        }
        match position.market_value {
            Some(value) => {
                let value = value.round_dp(crate::format::currency_decimals(&asset.currency));
                *by_currency.entry(asset.currency.clone()).or_default() += value;
                if let Some(value) = fx
                    .convert(&Money::new(value, asset.currency.clone()), &base, today)
                    .map(|money| money.amount)
                {
                    let sum = investments.get_or_insert((Decimal::ZERO, base.clone()));
                    sum.0 += value;
                }
            }
            None => incomplete = true,
        }
    }
    let in_base: Option<Decimal> = by_currency
        .iter()
        .map(|(currency, amount)| {
            fx.convert(&Money::new(*amount, currency.clone()), &base, today)
                .map(|money| money.amount)
        })
        .sum();
    let net_worth = match in_base {
        Some(total) => vec![format_money(&Money::new(total, base.clone()), lang)],
        None => by_currency
            .iter()
            .map(|(currency, amount)| format_money(&Money::new(*amount, currency.clone()), lang))
            .collect(),
    };
    if by_currency.is_empty() {
        incomplete = false;
    }

    let month = summarize(
        &data.transactions,
        &data.splits,
        Period::Month {
            year: today.year(),
            month: today.month(),
        },
        &base,
        &fx,
    )
    .ok();
    let money = |amount: Decimal| format_money(&Money::new(amount, base.clone()), lang);
    let month = match month {
        Some(summary) => {
            let net = summary.net().unwrap_or_default();
            MonthView {
                income: money(summary.income),
                expense: money(summary.expense),
                savings: money(net),
                savings_tone: tone(net),
                has_unconverted: !summary.unconverted.is_empty(),
            }
        }
        None => MonthView {
            income: money(Decimal::ZERO),
            expense: money(Decimal::ZERO),
            savings: money(Decimal::ZERO),
            savings_tone: Tone::Neutral,
            has_unconverted: false,
        },
    };

    let (recent, _) = transactions(data, &TransactionFilter::default(), 6, lang);
    DashboardView {
        net_worth,
        net_worth_incomplete: incomplete,
        investments: investments.map(|(value, currency)| {
            format!(
                "{} {}",
                texts(lang).includes_investments,
                crate::format::format_estimate(&Money::new(value, currency), lang)
            )
        }),
        accounts: accounts
            .into_iter()
            .filter(|account| !account.archived)
            .collect(),
        month,
        recent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{AccountForm, CategoryForm, ProfileForm, TransactionForm};

    const USER: Uuid = Uuid::from_u128(1);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    fn sample() -> Data {
        let mut data = Data {
            profile: Some(
                ProfileForm {
                    base_currency: "USD".into(),
                    ..ProfileForm::new(Lang::Es)
                }
                .validate(USER)
                .unwrap(),
            ),
            ..Data::default()
        };
        let bank = AccountForm {
            name: "Banco".into(),
            opening_balance: "100".into(),
            ..AccountForm::new(&data)
        }
        .validate(USER, Lang::Es)
        .unwrap();
        data.accounts.push(bank.clone());
        let food = CategoryForm {
            name: "Comida".into(),
            ..CategoryForm::new()
        }
        .validate(USER)
        .unwrap();
        data.categories.push(food.clone());
        let mut form = TransactionForm::new(&data, today());
        form.amount = "25,5".into();
        form.category_id = Some(food.meta.id);
        data.transactions
            .push(form.validate(&data, USER, Lang::Es).unwrap());
        let mut form = TransactionForm::new(&data, today());
        form.kind = TransactionKind::Income;
        form.amount = "1000".into();
        form.description = "Sueldo noviembre".into();
        data.transactions
            .push(form.validate(&data, USER, Lang::Es).unwrap());
        data
    }

    #[test]
    fn panel_con_saldos_y_mes() {
        let view = dashboard(&sample(), today(), Lang::Es);
        assert_eq!(view.net_worth, vec!["1.074,50 USD"]);
        assert_eq!(view.accounts[0].balance_text, "1.074,50 USD");
        assert_eq!(view.month.income, "1.000,00 USD");
        assert_eq!(view.month.expense, "25,50 USD");
        assert_eq!(view.month.savings, "974,50 USD");
        assert_eq!(view.recent.len(), 2);
    }

    #[test]
    fn titulos_y_signos() {
        let data = sample();
        let (list, total) = transactions(&data, &TransactionFilter::default(), 10, Lang::Es);
        assert_eq!(total, 2);
        let expense = list.iter().find(|tx| tx.title == "Comida").unwrap();
        assert_eq!(expense.amount_text, "-25,50 USD");
        assert_eq!(expense.tone, Tone::Negative);
        assert_eq!(expense.subtitle, "Banco");
        let income = list
            .iter()
            .find(|tx| tx.title == "Sueldo noviembre")
            .unwrap();
        assert_eq!(income.amount_text, "+1.000,00 USD");
    }

    #[test]
    fn filtros() {
        let data = sample();
        let search = |text: &str| {
            transactions(
                &data,
                &TransactionFilter {
                    text: text.into(),
                    ..TransactionFilter::default()
                },
                10,
                Lang::Es,
            )
            .1
        };
        assert_eq!(search("comi"), 1);
        assert_eq!(search("banco"), 2);
        assert_eq!(search("25,5"), 1);
        assert_eq!(search("nada"), 0);
        let only_uncategorized = TransactionFilter {
            category_id: Some(None),
            ..TransactionFilter::default()
        };
        assert_eq!(transactions(&data, &only_uncategorized, 10, Lang::Es).1, 1);
        let only_income = TransactionFilter {
            kind: Some(TransactionKind::Income),
            ..TransactionFilter::default()
        };
        assert_eq!(transactions(&data, &only_income, 10, Lang::Es).1, 1);
    }

    #[test]
    fn varias_monedas_sin_tasa_se_muestran_por_separado() {
        let mut data = sample();
        let clp = AccountForm {
            name: "Pesos".into(),
            currency: "CLP".into(),
            opening_balance: "15000".into(),
            ..AccountForm::new(&data)
        }
        .validate(USER, Lang::Es)
        .unwrap();
        data.accounts.push(clp);
        let view = dashboard(&data, today(), Lang::Es);
        assert_eq!(view.net_worth, vec!["15.000 CLP", "1.074,50 USD"]);
    }
}

/// Elemento de la papelera con su texto ya formateado.
#[derive(Debug, Clone, PartialEq)]
pub struct TrashView {
    pub table: caudal_sync::Table,
    pub id: Uuid,
    pub label: String,
    pub kind_label: &'static str,
}

pub fn trash(data: &Data, lang: Lang) -> Vec<TrashView> {
    use caudal_sync::Table;
    let texts = texts(lang);
    data.trash
        .iter()
        .map(|item| {
            let record = &item.record;
            let label = match record.table {
                Table::Transactions => record
                    .to_model::<Transaction>()
                    .map(|tx| {
                        let view = transaction_view(data, &tx, lang);
                        format!("{} · {} · {}", view.title, view.amount_text, view.date_text)
                    })
                    .unwrap_or_else(|_| item.label.clone()),
                Table::Trades => record
                    .to_model::<caudal_core::model::Trade>()
                    .map(|trade| {
                        let asset = data
                            .asset(trade.asset_id)
                            .map(|asset| asset.name.clone())
                            .unwrap_or_default();
                        format!(
                            "{}: {asset} · {}",
                            texts.trade_kind(trade.kind),
                            format_date(trade.traded_on, lang)
                        )
                    })
                    .unwrap_or_else(|_| item.label.clone()),
                _ => item.label.clone(),
            };
            let kind_label = match record.table {
                Table::Transactions => texts.transactions,
                Table::Accounts => texts.accounts,
                Table::Assets | Table::Trades => texts.investments,
                Table::Categories => texts.categories,
                Table::Tags => texts.tags,
                _ => "",
            };
            TrashView {
                table: record.table,
                id: record.id,
                label,
                kind_label,
            }
        })
        .collect()
}
