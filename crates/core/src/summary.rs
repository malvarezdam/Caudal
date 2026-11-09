//! Resúmenes por periodo en la moneda base del usuario.

use crate::fx::FxTable;
use crate::model::{Transaction, TransactionKind, TransactionSplit, TransactionStatus};
use crate::money::{AMOUNT_SCALE, Currency, Money, MoneyError};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Month { year: i32, month: u32 },
    Year(i32),
}

impl Period {
    pub fn contains(self, date: NaiveDate) -> bool {
        match self {
            Period::Month { year, month } => date.year() == year && date.month() == month,
            Period::Year(year) => date.year() == year,
        }
    }
}

/// Totales de un periodo. La clave `None` agrupa lo que no tiene categoría.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub base_currency: Currency,
    pub income: Decimal,
    pub expense: Decimal,
    pub income_by_category: BTreeMap<Option<Uuid>, Decimal>,
    pub expense_by_category: BTreeMap<Option<Uuid>, Decimal>,
    /// Movimientos sin tipo de cambio disponible: no están en los totales.
    pub unconverted: Vec<Uuid>,
}

impl Summary {
    fn empty(base_currency: Currency) -> Self {
        Self {
            base_currency,
            income: Decimal::ZERO,
            expense: Decimal::ZERO,
            income_by_category: BTreeMap::new(),
            expense_by_category: BTreeMap::new(),
            unconverted: Vec::new(),
        }
    }

    /// Ahorro del periodo.
    pub fn net(&self) -> Result<Decimal, MoneyError> {
        self.income
            .checked_sub(self.expense)
            .ok_or(MoneyError::Overflow)
    }
}

/// Diferencia de `current` respecto de `previous`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodChange {
    pub income: Decimal,
    pub expense: Decimal,
    pub net: Decimal,
}

pub fn compare(current: &Summary, previous: &Summary) -> Result<PeriodChange, MoneyError> {
    if current.base_currency != previous.base_currency {
        return Err(MoneyError::CurrencyMismatch(
            current.base_currency.clone(),
            previous.base_currency.clone(),
        ));
    }
    let difference = |a: Decimal, b: Decimal| a.checked_sub(b).ok_or(MoneyError::Overflow);
    Ok(PeriodChange {
        income: difference(current.income, previous.income)?,
        expense: difference(current.expense, previous.expense)?,
        net: difference(current.net()?, previous.net()?)?,
    })
}

/// Ingresos y gastos confirmados. Transferencias y ajustes no son ni lo uno ni lo otro.
pub fn summarize(
    transactions: &[Transaction],
    splits: &[TransactionSplit],
    period: Period,
    base_currency: &Currency,
    fx: &FxTable,
) -> Result<Summary, MoneyError> {
    let mut splits_by_transaction: HashMap<Uuid, Vec<&TransactionSplit>> = HashMap::new();
    for split in splits.iter().filter(|split| !split.meta.is_deleted()) {
        splits_by_transaction
            .entry(split.transaction_id)
            .or_default()
            .push(split);
    }

    let mut summary = Summary::empty(base_currency.clone());
    for tx in transactions.iter().filter(|tx| {
        !tx.meta.is_deleted()
            && tx.status == TransactionStatus::Confirmed
            && period.contains(tx.occurred_on)
            && matches!(tx.kind, TransactionKind::Income | TransactionKind::Expense)
    }) {
        let Some(amount) = amount_in_base(tx, base_currency, fx) else {
            summary.unconverted.push(tx.meta.id);
            continue;
        };
        let parts = allocate(
            tx,
            amount,
            splits_by_transaction
                .get(&tx.meta.id)
                .map_or(&[], Vec::as_slice),
        )?;
        let (total, by_category) = if tx.kind == TransactionKind::Income {
            (&mut summary.income, &mut summary.income_by_category)
        } else {
            (&mut summary.expense, &mut summary.expense_by_category)
        };
        *total = total.checked_add(amount).ok_or(MoneyError::Overflow)?;
        for (category, part) in parts {
            let entry = by_category.entry(category).or_default();
            *entry = entry.checked_add(part).ok_or(MoneyError::Overflow)?;
        }
    }
    Ok(summary)
}

/// Un resumen por cada mes del año, de enero a diciembre.
pub fn months_of_year(
    transactions: &[Transaction],
    splits: &[TransactionSplit],
    year: i32,
    base_currency: &Currency,
    fx: &FxTable,
) -> Result<Vec<Summary>, MoneyError> {
    (1..=12)
        .map(|month| {
            summarize(
                transactions,
                splits,
                Period::Month { year, month },
                base_currency,
                fx,
            )
        })
        .collect()
}

/// La copia guardada tiene prioridad para que el histórico no cambie.
fn amount_in_base(tx: &Transaction, base: &Currency, fx: &FxTable) -> Option<Decimal> {
    if tx.base_currency.as_ref() == Some(base)
        && let Some(amount) = tx.base_amount
    {
        return Some(amount);
    }
    if &tx.currency == base {
        return Some(tx.amount);
    }
    let money = Money::new(tx.amount, tx.currency.clone());
    fx.convert(&money, base, tx.occurred_on)
        .map(|converted| converted.amount)
}

/// Reparte `amount` entre las divisiones; lo no asignado va a la categoría del
/// movimiento. Las partes siempre suman exactamente `amount`.
fn allocate(
    tx: &Transaction,
    amount: Decimal,
    splits: &[&TransactionSplit],
) -> Result<Vec<(Option<Uuid>, Decimal)>, MoneyError> {
    if splits.is_empty() || tx.amount.is_zero() {
        return Ok(vec![(tx.category_id, amount)]);
    }
    let mut parts = Vec::with_capacity(splits.len() + 1);
    let mut allocated = Decimal::ZERO;
    let mut split_total = Decimal::ZERO;
    for split in splits {
        let part = split
            .amount
            .checked_div(tx.amount)
            .and_then(|ratio| ratio.checked_mul(amount))
            .ok_or(MoneyError::Overflow)?
            .round_dp(AMOUNT_SCALE);
        allocated = allocated.checked_add(part).ok_or(MoneyError::Overflow)?;
        split_total = split_total
            .checked_add(split.amount)
            .ok_or(MoneyError::Overflow)?;
        parts.push((split.category_id, part));
    }
    let remainder = amount.checked_sub(allocated).ok_or(MoneyError::Overflow)?;
    if split_total == tx.amount {
        // Las divisiones cubren todo: el resto solo puede ser redondeo.
        let last = parts.last_mut().expect("hay al menos una división");
        last.1 = last.1.checked_add(remainder).ok_or(MoneyError::Overflow)?;
    } else if !remainder.is_zero() {
        parts.push((tx.category_id, remainder));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxRate;
    use crate::ledger::test_support::*;
    use crate::model::Hlc;
    use proptest::prelude::*;

    const MARCH: Period = Period::Month {
        year: 2026,
        month: 3,
    };

    fn none() -> FxTable {
        FxTable::default()
    }

    fn split(value: u128, transaction: u128, category: u128, amount: &str) -> TransactionSplit {
        TransactionSplit {
            meta: meta(value),
            transaction_id: id(transaction),
            category_id: Some(id(category)),
            amount: dec(amount),
            note: None,
        }
    }

    #[test]
    fn calcula_ingresos_gastos_y_ahorro() {
        let txs = [
            tx(1, TransactionKind::Income, dec("1000"), "USD"),
            tx(2, TransactionKind::Expense, dec("300.50"), "USD"),
        ];
        let summary = summarize(&txs, &[], MARCH, &cur("USD"), &none()).unwrap();
        assert_eq!(summary.income, dec("1000"));
        assert_eq!(summary.expense, dec("300.50"));
        assert_eq!(summary.net().unwrap(), dec("699.50"));
    }

    #[test]
    fn excluye_transferencias_ajustes_pendientes_borrados_y_otros_periodos() {
        let transfer = tx(1, TransactionKind::Transfer, dec("1"), "USD");
        let adjustment = tx(2, TransactionKind::Adjustment, dec("1"), "USD");
        let mut pending = tx(3, TransactionKind::Expense, dec("1"), "USD");
        pending.status = TransactionStatus::Pending;
        let mut deleted = tx(4, TransactionKind::Expense, dec("1"), "USD");
        deleted.meta.deleted_at = Some(Hlc("0002".into()));
        let mut april = tx(5, TransactionKind::Expense, dec("1"), "USD");
        april.occurred_on = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let txs = [transfer, adjustment, pending, deleted, april];
        let summary = summarize(&txs, &[], MARCH, &cur("USD"), &none()).unwrap();
        assert_eq!(summary.expense, Decimal::ZERO);
        assert_eq!(summary.income, Decimal::ZERO);
    }

    #[test]
    fn la_copia_guardada_no_cambia_con_tasas_nuevas() {
        let mut foreign = tx(1, TransactionKind::Expense, dec("10"), "USD");
        foreign.base_amount = Some(dec("9000"));
        foreign.base_currency = Some(cur("CLP"));
        let fx = FxTable::new([FxRate {
            base: cur("USD"),
            quote: cur("CLP"),
            rate_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            rate: dec("1000"),
        }]);
        let summary = summarize(&[foreign], &[], MARCH, &cur("CLP"), &fx).unwrap();
        assert_eq!(summary.expense, dec("9000"));
    }

    #[test]
    fn convierte_con_la_tasa_de_la_fecha() {
        let foreign = tx(1, TransactionKind::Expense, dec("10"), "USD");
        let fx = FxTable::new([FxRate {
            base: cur("USD"),
            quote: cur("CLP"),
            rate_on: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            rate: dec("950"),
        }]);
        let summary = summarize(&[foreign], &[], MARCH, &cur("CLP"), &fx).unwrap();
        assert_eq!(summary.expense, dec("9500"));
    }

    #[test]
    fn avisa_de_lo_que_no_pudo_convertir() {
        let foreign = tx(1, TransactionKind::Expense, dec("10"), "USD");
        let summary = summarize(&[foreign], &[], MARCH, &cur("CLP"), &none()).unwrap();
        assert_eq!(summary.expense, Decimal::ZERO);
        assert_eq!(summary.unconverted, vec![id(1)]);
    }

    #[test]
    fn reparte_divisiones_y_el_resto_va_a_la_categoria() {
        let mut purchase = tx(1, TransactionKind::Expense, dec("100"), "USD");
        purchase.category_id = Some(id(70));
        let parts = [split(2, 1, 71, "30"), split(3, 1, 72, "20")];
        let summary = summarize(&[purchase], &parts, MARCH, &cur("USD"), &none()).unwrap();
        assert_eq!(summary.expense_by_category[&Some(id(71))], dec("30"));
        assert_eq!(summary.expense_by_category[&Some(id(72))], dec("20"));
        assert_eq!(summary.expense_by_category[&Some(id(70))], dec("50"));
    }

    #[test]
    fn division_borrada_no_cuenta() {
        let purchase = tx(1, TransactionKind::Expense, dec("100"), "USD");
        let mut part = split(2, 1, 71, "30");
        part.meta.deleted_at = Some(Hlc("0002".into()));
        let summary = summarize(&[purchase], &[part], MARCH, &cur("USD"), &none()).unwrap();
        assert_eq!(summary.expense_by_category[&None], dec("100"));
    }

    #[test]
    fn devuelve_doce_meses_y_compara() {
        let txs = [tx(1, TransactionKind::Expense, dec("10"), "USD")];
        let months = months_of_year(&txs, &[], 2026, &cur("USD"), &none()).unwrap();
        assert_eq!(months.len(), 12);
        assert_eq!(months[2].expense, dec("10"));
        let change = compare(&months[2], &months[1]).unwrap();
        assert_eq!(change.expense, dec("10"));
        assert_eq!(change.net, dec("-10"));
    }

    fn category_total(summary: &Summary) -> Decimal {
        summary.expense_by_category.values().copied().sum()
    }

    proptest! {
        /// Las categorías siempre suman el total, incluso con repartos inexactos.
        #[test]
        fn categorias_suman_el_total(
            total_cents in 1i64..1_000_000_000,
            rate in 1i64..100_000_000,
            shares in proptest::collection::vec(1u32..1000, 1..12),
        ) {
            let total = Decimal::new(total_cents, 2);
            let share_sum: u32 = shares.iter().sum();
            let mut assigned = Decimal::ZERO;
            let parts: Vec<TransactionSplit> = shares
                .iter()
                .enumerate()
                .map(|(index, share)| {
                    let amount = if index + 1 == shares.len() {
                        total - assigned
                    } else {
                        (total * Decimal::from(*share) / Decimal::from(share_sum)).round_dp(2)
                    };
                    assigned += amount;
                    TransactionSplit { amount, ..split(100 + index as u128, 1, 200 + index as u128, "0") }
                })
                .collect();
            let mut purchase = tx(1, TransactionKind::Expense, total, "USD");
            purchase.base_amount = Some((total * Decimal::new(rate, 4)).round_dp(AMOUNT_SCALE));
            purchase.base_currency = Some(cur("CLP"));
            let summary = summarize(&[purchase], &parts, MARCH, &cur("CLP"), &none()).unwrap();
            prop_assert_eq!(category_total(&summary), summary.expense);
        }
    }
}
