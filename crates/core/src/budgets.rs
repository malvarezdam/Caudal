//! Presupuestos por categoría y metas de ahorro.

use crate::fx::FxTable;
use crate::model::{
    Budget, BudgetPeriod, Category, Goal, GoalContribution, Transaction, TransactionSplit,
};
use crate::money::MoneyError;
use crate::summary::{Period, summarize};
use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Meses o años hacia atrás que se miran para arrastrar sobrantes.
const MAX_ROLLOVER_PERIODS: u32 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetStatus {
    pub budget_id: Uuid,
    pub period: Period,
    /// Sobrante (o exceso, si es negativo) arrastrado de periodos anteriores.
    pub carried: Decimal,
    /// Monto del periodo más lo arrastrado.
    pub limit: Decimal,
    pub spent: Decimal,
    pub remaining: Decimal,
    /// Gastos sin tipo de cambio hacia la moneda del presupuesto: no se cuentan.
    pub unconverted: Vec<Uuid>,
    /// El presupuesto no aplica hoy (antes de su inicio o tras su fin).
    pub inactive: bool,
}

fn period_of(budget: &Budget, date: NaiveDate) -> Period {
    match budget.period {
        BudgetPeriod::Monthly => Period::Month {
            year: date.year(),
            month: date.month(),
        },
        BudgetPeriod::Yearly => Period::Year(date.year()),
    }
}

fn previous(budget: &Budget, date: NaiveDate) -> Option<NaiveDate> {
    let months = match budget.period {
        BudgetPeriod::Monthly => 1,
        BudgetPeriod::Yearly => 12,
    };
    date.with_day(1)?.checked_sub_months(Months::new(months))
}

fn period_start(period: Period) -> Option<NaiveDate> {
    match period {
        Period::Month { year, month } => NaiveDate::from_ymd_opt(year, month, 1),
        Period::Year(year) => NaiveDate::from_ymd_opt(year, 1, 1),
    }
}

/// Gasto del periodo en la categoría (y sus subcategorías), en la moneda del presupuesto.
fn spent_in(
    budget: &Budget,
    categories: &[Category],
    transactions: &[Transaction],
    splits: &[TransactionSplit],
    fx: &FxTable,
    period: Period,
) -> Result<(Decimal, Vec<Uuid>), MoneyError> {
    let summary = summarize(transactions, splits, period, &budget.currency, fx)?;
    let spent = match budget.category_id {
        None => summary.expense,
        Some(category) => summary
            .expense_by_category
            .iter()
            .filter(|(id, _)| {
                id.is_some_and(|id| {
                    id == category
                        || categories
                            .iter()
                            .any(|c| c.meta.id == id && c.parent_id == Some(category))
                })
            })
            .try_fold(Decimal::ZERO, |sum, (_, value)| {
                sum.checked_add(*value).ok_or(MoneyError::Overflow)
            })?,
    };
    Ok((spent, summary.unconverted))
}

pub fn budget_status(
    budget: &Budget,
    categories: &[Category],
    transactions: &[Transaction],
    splits: &[TransactionSplit],
    fx: &FxTable,
    today: NaiveDate,
) -> Result<BudgetStatus, MoneyError> {
    let period = period_of(budget, today);
    let (spent, unconverted) = spent_in(budget, categories, transactions, splits, fx, period)?;

    let mut carried = Decimal::ZERO;
    if budget.rollover {
        // Del periodo más antiguo al más reciente: cada uno hereda el sobrante del anterior.
        let mut starts = Vec::new();
        let mut cursor = previous(budget, today);
        while let Some(date) = cursor {
            if starts.len() as u32 == MAX_ROLLOVER_PERIODS
                || budget.starts_on.is_some_and(|start| {
                    period_start(period_of(budget, date))
                        .is_some_and(|s| s < start.with_day(1).unwrap_or(start))
                })
            {
                break;
            }
            starts.push(date);
            cursor = previous(budget, date);
        }
        for date in starts.into_iter().rev() {
            let (used, _) = spent_in(
                budget,
                categories,
                transactions,
                splits,
                fx,
                period_of(budget, date),
            )?;
            carried = carried
                .checked_add(budget.amount)
                .and_then(|value| value.checked_sub(used))
                .ok_or(MoneyError::Overflow)?;
        }
    }
    let limit = budget
        .amount
        .checked_add(carried)
        .ok_or(MoneyError::Overflow)?;
    let inactive = budget.starts_on.is_some_and(|start| today < start)
        || budget.ends_on.is_some_and(|end| today > end);
    Ok(BudgetStatus {
        budget_id: budget.meta.id,
        period,
        carried,
        limit,
        spent,
        remaining: limit - spent,
        unconverted,
        inactive,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalStatus {
    pub saved: Decimal,
    pub remaining: Decimal,
    pub reached: bool,
    /// Aporte mensual promedio de los últimos tres meses.
    pub monthly_average: Option<Decimal>,
    /// Cuándo se llegaría al ritmo actual.
    pub projected_date: Option<NaiveDate>,
    /// Cuánto aportar al mes para llegar a la fecha objetivo.
    pub needed_per_month: Option<Decimal>,
}

/// Con cuenta asociada (de la misma moneda), lo ahorrado es su saldo; si no,
/// la suma de los aportes.
pub fn goal_status(
    goal: &Goal,
    contributions: &[GoalContribution],
    account_balance: Option<Decimal>,
    today: NaiveDate,
) -> GoalStatus {
    let own: Vec<&GoalContribution> = contributions
        .iter()
        .filter(|item| item.goal_id == goal.meta.id && !item.meta.is_deleted())
        .collect();
    let saved = account_balance.unwrap_or_else(|| own.iter().map(|item| item.amount).sum());
    let remaining = (goal.target_amount - saved).max(Decimal::ZERO);
    let reached = remaining.is_zero();

    let since = today.checked_sub_months(Months::new(3)).unwrap_or(today);
    let recent: Decimal = own
        .iter()
        .filter(|item| item.contributed_on > since && item.contributed_on <= today)
        .map(|item| item.amount)
        .sum();
    let monthly_average = (recent > Decimal::ZERO).then(|| (recent / Decimal::from(3)).round_dp(2));

    let projected_date = match (reached, monthly_average) {
        (true, _) => Some(today),
        (false, Some(average)) => {
            let months = (remaining / average).ceil();
            u32::try_from(months.mantissa())
                .ok()
                .filter(|_| months.scale() == 0 && months < Decimal::from(1_200))
                .and_then(|months| today.checked_add_months(Months::new(months)))
        }
        (false, None) => None,
    };
    let needed_per_month = goal.target_date.filter(|_| !reached).map(|target| {
        let months = ((target.year() - today.year()) * 12 + target.month() as i32
            - today.month() as i32)
            .max(1);
        (remaining / Decimal::from(months)).round_dp(2)
    });

    GoalStatus {
        saved,
        remaining,
        reached,
        monthly_average,
        projected_date,
        needed_per_month,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::test_support::*;
    use crate::model::{CategoryKind, Hlc, TransactionKind};

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn category(value: u128, parent: Option<u128>) -> Category {
        Category {
            meta: meta(value),
            name: "c".into(),
            parent_id: parent.map(id),
            kind: CategoryKind::Expense,
            color: None,
            icon: None,
        }
    }

    fn expense(value: u128, amount: &str, date: &str, category: u128) -> Transaction {
        let mut item = tx(value, TransactionKind::Expense, dec(amount), "CLP");
        item.occurred_on = day(date);
        item.category_id = Some(id(category));
        item
    }

    fn budget(amount: &str, rollover: bool) -> Budget {
        Budget {
            meta: meta(500),
            category_id: Some(id(10)),
            amount: dec(amount),
            currency: cur("CLP"),
            period: BudgetPeriod::Monthly,
            starts_on: Some(day("2026-09-01")),
            ends_on: None,
            rollover,
        }
    }

    #[test]
    fn cuenta_subcategorias_y_solo_el_periodo_actual() {
        let categories = [
            category(10, None),
            category(11, Some(10)),
            category(12, None),
        ];
        let txs = [
            expense(1, "30000", "2026-11-03", 10),
            expense(2, "20000", "2026-11-05", 11),
            expense(3, "99999", "2026-11-05", 12),
            expense(4, "50000", "2026-10-05", 10),
        ];
        let status = budget_status(
            &budget("100000", false),
            &categories,
            &txs,
            &[],
            &FxTable::default(),
            day("2026-11-09"),
        )
        .unwrap();
        assert_eq!(status.spent, dec("50000"));
        assert_eq!(status.remaining, dec("50000"));
        assert_eq!(status.carried, Decimal::ZERO);
    }

    #[test]
    fn arrastra_sobrantes_y_excesos_desde_el_inicio() {
        let categories = [category(10, None)];
        let txs = [
            expense(1, "120000", "2026-08-10", 10), // antes del inicio: no cuenta
            expense(2, "70000", "2026-09-10", 10),  // sobran 30.000
            expense(3, "110000", "2026-10-10", 10), // se pasa 10.000
            expense(4, "40000", "2026-11-02", 10),
        ];
        let status = budget_status(
            &budget("100000", true),
            &categories,
            &txs,
            &[],
            &FxTable::default(),
            day("2026-11-09"),
        )
        .unwrap();
        assert_eq!(status.carried, dec("20000"));
        assert_eq!(status.limit, dec("120000"));
        assert_eq!(status.remaining, dec("80000"));
    }

    #[test]
    fn presupuesto_general_e_inactivo() {
        let txs = [
            expense(1, "1000", "2026-11-03", 10),
            expense(2, "500", "2026-11-03", 99),
        ];
        let mut all = budget("10000", false);
        all.category_id = None;
        all.ends_on = Some(day("2026-10-31"));
        let status =
            budget_status(&all, &[], &txs, &[], &FxTable::default(), day("2026-11-09")).unwrap();
        assert_eq!(status.spent, dec("1500"));
        assert!(status.inactive);
    }

    fn goal(target: &str, date: Option<&str>) -> Goal {
        Goal {
            meta: meta(700),
            name: "Viaje".into(),
            target_amount: dec(target),
            currency: cur("CLP"),
            target_date: date.map(day),
            account_id: None,
            note: None,
        }
    }

    fn contribution(value: u128, amount: &str, date: &str) -> GoalContribution {
        GoalContribution {
            meta: meta(value),
            goal_id: id(700),
            transaction_id: None,
            amount: dec(amount),
            contributed_on: day(date),
            note: None,
        }
    }

    #[test]
    fn meta_proyecta_con_el_ritmo_reciente() {
        let items = [
            contribution(1, "100000", "2026-01-15"),
            contribution(2, "50000", "2026-09-15"),
            contribution(3, "50000", "2026-10-15"),
            contribution(4, "50000", "2026-11-05"),
        ];
        let status = goal_status(
            &goal("1000000", Some("2027-11-01")),
            &items,
            None,
            day("2026-11-09"),
        );
        assert_eq!(status.saved, dec("250000"));
        assert_eq!(status.monthly_average, Some(dec("50000")));
        // Faltan 750.000 a 50.000 al mes: 15 meses.
        assert_eq!(status.projected_date, Some(day("2028-02-09")));
        // Para llegar en 12 meses hacen falta 62.500 al mes.
        assert_eq!(status.needed_per_month, Some(dec("62500")));
    }

    #[test]
    fn meta_con_cuenta_y_alcanzada() {
        let mut removed = contribution(1, "999999", "2026-11-01");
        removed.meta.deleted_at = Some(Hlc("2".into()));
        let status = goal_status(
            &goal("100000", None),
            &[removed],
            Some(dec("120000")),
            day("2026-11-09"),
        );
        assert!(status.reached);
        assert_eq!(status.remaining, Decimal::ZERO);
        assert!(status.needed_per_month.is_none());
    }
}
