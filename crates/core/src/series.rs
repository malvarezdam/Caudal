//! Series para gráficos: flujo mensual, patrimonio en el tiempo y gasto por
//! categoría. Todo en la moneda base y calculado desde los movimientos.

use crate::fx::FxTable;
use crate::ledger::account_balances;
use crate::model::{Account, Category, Transaction, TransactionSplit};
use crate::money::{Currency, Money, MoneyError};
use crate::summary::{Period, Summary, summarize};
use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthFlow {
    pub year: i32,
    pub month: u32,
    pub income: Decimal,
    pub expense: Decimal,
    pub unconverted: usize,
}

/// Primer día de cada uno de los `count` meses que terminan en el mes de `end`.
pub fn month_starts(end: NaiveDate, count: u32) -> Vec<NaiveDate> {
    let last = end.with_day(1).unwrap_or(end);
    (0..count)
        .rev()
        .filter_map(|back| last.checked_sub_months(Months::new(back)))
        .collect()
}

/// Ingresos y gastos de cada mes. Recorre los movimientos una sola vez por mes
/// solo dentro de su rango, para que años de datos no bloqueen la interfaz.
pub fn monthly_flow(
    transactions: &[Transaction],
    splits: &[TransactionSplit],
    base: &Currency,
    fx: &FxTable,
    end: NaiveDate,
    count: u32,
) -> Result<Vec<MonthFlow>, MoneyError> {
    let starts = month_starts(end, count);
    let Some(first) = starts.first().copied() else {
        return Ok(Vec::new());
    };
    let relevant: Vec<Transaction> = transactions
        .iter()
        .filter(|tx| tx.occurred_on >= first && tx.occurred_on <= end)
        .cloned()
        .collect();
    let mut by_month: BTreeMap<(i32, u32), Vec<Transaction>> = BTreeMap::new();
    for tx in relevant {
        by_month
            .entry((tx.occurred_on.year(), tx.occurred_on.month()))
            .or_default()
            .push(tx);
    }
    starts
        .into_iter()
        .map(|start| {
            let key = (start.year(), start.month());
            let month_txs = by_month.get(&key).map_or(&[][..], Vec::as_slice);
            let summary = summarize(
                month_txs,
                splits,
                Period::Month {
                    year: key.0,
                    month: key.1,
                },
                base,
                fx,
            )?;
            Ok(MonthFlow {
                year: key.0,
                month: key.1,
                income: summary.income,
                expense: summary.expense,
                unconverted: summary.unconverted.len(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetWorthPoint {
    pub date: NaiveDate,
    /// Suma de las cuentas que se pudieron convertir a la moneda base.
    pub value: Decimal,
    /// Cuentas sin tipo de cambio ese día: no están en `value`.
    pub excluded: Vec<Uuid>,
}

/// Patrimonio al cierre de cada mes (o hoy, para el mes en curso).
pub fn net_worth_series(
    accounts: &[Account],
    transactions: &[Transaction],
    base: &Currency,
    fx: &FxTable,
    end: NaiveDate,
    count: u32,
) -> Result<Vec<NetWorthPoint>, MoneyError> {
    let mut sorted: Vec<&Transaction> = transactions.iter().collect();
    sorted.sort_by_key(|tx| tx.occurred_on);
    month_starts(end, count)
        .into_iter()
        .map(|start| {
            let close = start
                .checked_add_months(Months::new(1))
                .and_then(|next| next.pred_opt())
                .unwrap_or(start)
                .min(end);
            let upto = sorted.partition_point(|tx| tx.occurred_on <= close);
            let balances = account_balances(accounts, sorted[..upto].iter().copied())?;
            let mut total = Decimal::ZERO;
            let mut excluded = Vec::new();
            for (account_id, balance) in &balances {
                match fx
                    .convert(&balance.balance, base, close)
                    .map(|money: Money| money.amount)
                {
                    Some(value) => total = total.checked_add(value).ok_or(MoneyError::Overflow)?,
                    None => excluded.push(*account_id),
                }
            }
            Ok(NetWorthPoint {
                date: close,
                value: total,
                excluded,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryShare {
    /// Categoría principal; `None` agrupa lo que no tiene categoría.
    pub category_id: Option<Uuid>,
    pub amount: Decimal,
}

/// Gasto por categoría principal (las subcategorías suman a su madre), de mayor a menor.
pub fn expense_by_parent(summary: &Summary, categories: &[Category]) -> Vec<CategoryShare> {
    let parent_of = |id: Uuid| {
        categories
            .iter()
            .find(|category| category.meta.id == id)
            .and_then(|category| category.parent_id)
            .filter(|parent| categories.iter().any(|c| c.meta.id == *parent))
            .unwrap_or(id)
    };
    let mut totals: BTreeMap<Option<Uuid>, Decimal> = BTreeMap::new();
    for (category, amount) in &summary.expense_by_category {
        *totals.entry(category.map(parent_of)).or_default() += *amount;
    }
    let mut shares: Vec<CategoryShare> = totals
        .into_iter()
        .filter(|(_, amount)| !amount.is_zero())
        .map(|(category_id, amount)| CategoryShare {
            category_id,
            amount,
        })
        .collect();
    shares.sort_by_key(|share| std::cmp::Reverse(share.amount));
    shares
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::test_support::*;
    use crate::model::{CategoryKind, TransactionKind};

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn at(mut item: Transaction, date: &str, account: u128) -> Transaction {
        item.occurred_on = day(date);
        item.account_id = Some(id(account));
        item
    }

    #[test]
    fn flujo_de_doce_meses_suma_lo_mismo_que_el_resumen_anual() {
        let mut txs = Vec::new();
        for month in 1..=12u32 {
            let date = format!("2026-{month:02}-10");
            txs.push(at(
                tx(month as u128, TransactionKind::Income, dec("1000"), "USD"),
                &date,
                1,
            ));
            txs.push(at(
                tx(
                    100 + month as u128,
                    TransactionKind::Expense,
                    Decimal::from(month * 10),
                    "USD",
                ),
                &date,
                1,
            ));
        }
        let usd = cur("USD");
        let flow =
            monthly_flow(&txs, &[], &usd, &FxTable::default(), day("2026-12-31"), 12).unwrap();
        assert_eq!(flow.len(), 12);
        assert_eq!((flow[0].year, flow[0].month), (2026, 1));
        let year = summarize(&txs, &[], Period::Year(2026), &usd, &FxTable::default()).unwrap();
        assert_eq!(flow.iter().map(|m| m.income).sum::<Decimal>(), year.income);
        assert_eq!(
            flow.iter().map(|m| m.expense).sum::<Decimal>(),
            year.expense
        );
    }

    #[test]
    fn patrimonio_al_cierre_de_cada_mes() {
        let accounts = [account(1, "USD", "100")];
        let txs = [
            at(
                tx(1, TransactionKind::Income, dec("50"), "USD"),
                "2026-09-15",
                1,
            ),
            at(
                tx(2, TransactionKind::Expense, dec("20"), "USD"),
                "2026-10-31",
                1,
            ),
            at(
                tx(3, TransactionKind::Expense, dec("5"), "USD"),
                "2026-11-20",
                1,
            ),
        ];
        let points = net_worth_series(
            &accounts,
            &txs,
            &cur("USD"),
            &FxTable::default(),
            day("2026-11-09"),
            3,
        )
        .unwrap();
        let values: Vec<_> = points
            .iter()
            .map(|p| (p.date.to_string(), p.value))
            .collect();
        assert_eq!(
            values,
            vec![
                ("2026-09-30".into(), dec("150")),
                ("2026-10-31".into(), dec("130")),
                ("2026-11-09".into(), dec("130")),
            ]
        );
    }

    #[test]
    fn patrimonio_sin_tasa_excluye_la_cuenta() {
        let accounts = [account(1, "USD", "100"), account(2, "CLP", "5000")];
        let points = net_worth_series(
            &accounts,
            &[],
            &cur("USD"),
            &FxTable::default(),
            day("2026-11-09"),
            1,
        )
        .unwrap();
        assert_eq!(points[0].value, dec("100"));
        assert_eq!(points[0].excluded, vec![id(2)]);
    }

    #[test]
    fn subcategorias_suman_a_la_principal() {
        let food = Category {
            meta: meta(10),
            name: "Comida".into(),
            parent_id: None,
            kind: CategoryKind::Expense,
            color: None,
            icon: None,
        };
        let market = Category {
            parent_id: Some(id(10)),
            meta: meta(11),
            ..food.clone()
        };
        let mut a = tx(1, TransactionKind::Expense, dec("30"), "USD");
        a.category_id = Some(id(10));
        let mut b = tx(2, TransactionKind::Expense, dec("50"), "USD");
        b.category_id = Some(id(11));
        let c = tx(3, TransactionKind::Expense, dec("5"), "USD");
        let summary = summarize(
            &[a, b, c],
            &[],
            Period::Month {
                year: 2026,
                month: 3,
            },
            &cur("USD"),
            &FxTable::default(),
        )
        .unwrap();
        let shares = expense_by_parent(&summary, &[food, market]);
        assert_eq!(
            shares[0],
            CategoryShare {
                category_id: Some(id(10)),
                amount: dec("80")
            }
        );
        assert_eq!(
            shares[1],
            CategoryShare {
                category_id: None,
                amount: dec("5")
            }
        );
    }
}
