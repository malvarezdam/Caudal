//! Estado de una deuda: total, pagado, pendiente, cuotas y vencimientos.

use crate::model::{Debt, DebtInstallment, DebtPayment};
use crate::money::MoneyError;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallmentStatus {
    pub id: Uuid,
    pub due_on: NaiveDate,
    pub amount: Decimal,
    pub paid: Decimal,
    pub settled: bool,
    pub overdue: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtSummary {
    pub debt_id: Uuid,
    /// Interés simple acumulado hasta `as_of`; solo si no hay cuotas.
    pub interest: Decimal,
    pub total: Decimal,
    pub paid: Decimal,
    pub outstanding: Decimal,
    pub settled: bool,
    pub overdue: bool,
    /// Próximo pago esperado: fecha y monto pendiente de esa cuota o de la deuda.
    pub next_due: Option<(NaiveDate, Decimal)>,
    pub installments: Vec<InstallmentStatus>,
}

/// Con cuotas, el total es la suma de las cuotas (el usuario ya incluyó lo que
/// corresponda). Sin cuotas, capital más interés simple anual desde `started_on`.
/// Los pagos cubren las cuotas en orden de vencimiento.
pub fn summarize_debt(
    debt: &Debt,
    installments: &[DebtInstallment],
    payments: &[DebtPayment],
    as_of: NaiveDate,
) -> Result<DebtSummary, MoneyError> {
    let add = |a: Decimal, b: Decimal| a.checked_add(b).ok_or(MoneyError::Overflow);
    let mut plan: Vec<&DebtInstallment> = installments
        .iter()
        .filter(|item| item.debt_id == debt.meta.id && !item.meta.is_deleted())
        .collect();
    plan.sort_by_key(|item| (item.due_on, item.meta.id));

    let mut paid = Decimal::ZERO;
    for payment in payments
        .iter()
        .filter(|payment| payment.debt_id == debt.meta.id && !payment.meta.is_deleted())
    {
        paid = add(paid, payment.amount)?;
    }

    let interest = if plan.is_empty() {
        simple_interest(debt, as_of)?
    } else {
        Decimal::ZERO
    };
    let total = if plan.is_empty() {
        add(debt.principal, interest)?
    } else {
        plan.iter()
            .try_fold(Decimal::ZERO, |sum, item| add(sum, item.amount))?
    };
    let outstanding = total
        .checked_sub(paid)
        .ok_or(MoneyError::Overflow)?
        .max(Decimal::ZERO);
    let settled = debt.closed || outstanding.is_zero();

    let mut remaining = paid;
    let installments: Vec<InstallmentStatus> = plan
        .iter()
        .map(|item| {
            let covered = remaining.min(item.amount).max(Decimal::ZERO);
            remaining -= covered;
            let item_settled = settled || covered >= item.amount;
            InstallmentStatus {
                id: item.meta.id,
                due_on: item.due_on,
                amount: item.amount,
                paid: covered,
                settled: item_settled,
                overdue: !item_settled && item.due_on < as_of,
            }
        })
        .collect();

    let next_due = if settled {
        None
    } else if installments.is_empty() {
        debt.due_on.map(|due| (due, outstanding))
    } else {
        installments
            .iter()
            .find(|item| !item.settled)
            .map(|item| (item.due_on, item.amount - item.paid))
    };
    let overdue = !settled
        && if installments.is_empty() {
            debt.due_on.is_some_and(|due| due < as_of)
        } else {
            installments.iter().any(|item| item.overdue)
        };

    Ok(DebtSummary {
        debt_id: debt.meta.id,
        interest,
        total,
        paid,
        outstanding,
        settled,
        overdue,
        next_due,
        installments,
    })
}

fn simple_interest(debt: &Debt, as_of: NaiveDate) -> Result<Decimal, MoneyError> {
    let (Some(rate), Some(start)) = (debt.interest_rate, debt.started_on) else {
        return Ok(Decimal::ZERO);
    };
    let days = (as_of - start).num_days().max(0);
    if days == 0 || rate <= Decimal::ZERO {
        return Ok(Decimal::ZERO);
    }
    debt.principal
        .checked_mul(rate)
        .and_then(|value| value.checked_mul(Decimal::from(days)))
        .and_then(|value| value.checked_div(Decimal::from(36_500)))
        .map(|value| value.round_dp(2))
        .ok_or(MoneyError::Overflow)
}

/// Reparte `total` en `count` cuotas iguales; la última absorbe el redondeo.
pub fn split_installments(total: Decimal, count: u32, decimals: u32) -> Vec<Decimal> {
    if count == 0 {
        return Vec::new();
    }
    let each = (total / Decimal::from(count))
        .round_dp_with_strategy(decimals, rust_decimal::RoundingStrategy::ToZero);
    let mut parts = vec![each; count as usize];
    let assigned = each * Decimal::from(count - 1);
    parts[count as usize - 1] = total - assigned;
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::test_support::*;
    use crate::model::DebtDirection;
    use proptest::prelude::*;

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn debt(principal: &str) -> Debt {
        Debt {
            meta: meta(1),
            contact_id: None,
            direction: DebtDirection::OwedToMe,
            principal: dec(principal),
            currency: cur("CLP"),
            interest_rate: None,
            started_on: None,
            due_on: None,
            closed: false,
            description: None,
        }
    }

    fn installment(value: u128, due: &str, amount: &str) -> DebtInstallment {
        DebtInstallment {
            meta: meta(value),
            debt_id: id(1),
            due_on: day(due),
            amount: dec(amount),
            note: None,
        }
    }

    fn payment(value: u128, amount: &str) -> DebtPayment {
        DebtPayment {
            meta: meta(value),
            debt_id: id(1),
            installment_id: None,
            transaction_id: None,
            amount: dec(amount),
            paid_on: day("2026-01-10"),
            note: None,
        }
    }

    #[test]
    fn pago_parcial_reduce_lo_pendiente() {
        let mut loan = debt("100000");
        loan.due_on = Some(day("2026-02-01"));
        let summary =
            summarize_debt(&loan, &[], &[payment(10, "30000")], day("2026-01-15")).unwrap();
        assert_eq!(summary.paid, dec("30000"));
        assert_eq!(summary.outstanding, dec("70000"));
        assert!(!summary.settled && !summary.overdue);
        assert_eq!(summary.next_due, Some((day("2026-02-01"), dec("70000"))));

        let later = summarize_debt(&loan, &[], &[payment(10, "30000")], day("2026-02-02")).unwrap();
        assert!(later.overdue);
    }

    #[test]
    fn pagos_cubren_cuotas_en_orden() {
        let plan = [
            installment(21, "2026-03-01", "40000"),
            installment(20, "2026-02-01", "40000"),
            installment(22, "2026-04-01", "40000"),
        ];
        let payments = [payment(10, "50000")];
        let summary = summarize_debt(&debt("100000"), &plan, &payments, day("2026-03-15")).unwrap();
        assert_eq!(summary.total, dec("120000"));
        assert_eq!(summary.outstanding, dec("70000"));
        let status: Vec<_> = summary
            .installments
            .iter()
            .map(|i| (i.paid, i.settled, i.overdue))
            .collect();
        assert_eq!(
            status,
            vec![
                (dec("40000"), true, false),
                (dec("10000"), false, true),
                (dec("0"), false, false),
            ]
        );
        assert_eq!(summary.next_due, Some((day("2026-03-01"), dec("30000"))));
        assert!(summary.overdue);
    }

    #[test]
    fn sobrepago_queda_saldado_sin_negativos() {
        let summary = summarize_debt(
            &debt("1000"),
            &[],
            &[payment(10, "1500")],
            day("2026-01-01"),
        )
        .unwrap();
        assert_eq!(summary.outstanding, Decimal::ZERO);
        assert!(summary.settled);
        assert!(summary.next_due.is_none());
    }

    #[test]
    fn interes_simple_anual() {
        let mut loan = debt("100000");
        loan.interest_rate = Some(dec("12"));
        loan.started_on = Some(day("2026-01-01"));
        let summary = summarize_debt(&loan, &[], &[], day("2026-12-31")).unwrap();
        // 100000 × 12 % × 364/365
        assert_eq!(summary.interest, dec("11967.12"));
        assert_eq!(summary.total, dec("111967.12"));
    }

    #[test]
    fn cerrada_a_mano_o_pagos_borrados() {
        let mut loan = debt("1000");
        loan.closed = true;
        assert!(
            summarize_debt(&loan, &[], &[], day("2026-01-01"))
                .unwrap()
                .settled
        );

        let mut deleted = payment(10, "1000");
        deleted.meta.deleted_at = Some(crate::model::Hlc("0002".into()));
        let open = summarize_debt(&debt("1000"), &[], &[deleted], day("2026-01-01")).unwrap();
        assert_eq!(open.outstanding, dec("1000"));
    }

    proptest! {
        #[test]
        fn las_cuotas_suman_el_total(cents in 1i64..10_000_000_000, count in 1u32..60) {
            let total = Decimal::new(cents, 2);
            let parts = split_installments(total, count, 2);
            prop_assert_eq!(parts.len(), count as usize);
            prop_assert_eq!(parts.iter().copied().sum::<Decimal>(), total);
            prop_assert!(parts.iter().all(|part| *part >= Decimal::ZERO));
        }
    }
}

/// Resultado de simular pagos mensuales fijos sobre un saldo con interés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayoffPlan {
    pub months: u32,
    pub total_interest: Decimal,
    pub total_paid: Decimal,
    /// `false` si el pago no alcanza a cubrir el interés: la deuda no termina.
    pub finishes: bool,
}

/// Tope de la simulación: 50 años.
const MAX_PAYOFF_MONTHS: u32 = 600;

/// Interés compuesto mensual (tasa anual / 12). Solo calcula el escenario que
/// el usuario plantea; no recomienda nada.
pub fn simulate_payoff(balance: Decimal, annual_rate: Decimal, payment: Decimal) -> PayoffPlan {
    let monthly_rate = annual_rate.max(Decimal::ZERO) / Decimal::from(1_200);
    let mut remaining = balance.max(Decimal::ZERO);
    let mut months = 0;
    let mut total_interest = Decimal::ZERO;
    let mut total_paid = Decimal::ZERO;
    while remaining > Decimal::ZERO && months < MAX_PAYOFF_MONTHS {
        let interest = (remaining * monthly_rate).round_dp(2);
        if payment <= interest {
            return PayoffPlan {
                months,
                total_interest,
                total_paid,
                finishes: false,
            };
        }
        remaining += interest;
        total_interest += interest;
        let paid = payment.min(remaining);
        remaining -= paid;
        total_paid += paid;
        months += 1;
    }
    PayoffPlan {
        months,
        total_interest,
        total_paid,
        finishes: remaining.is_zero(),
    }
}

#[cfg(test)]
mod payoff_tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().unwrap()
    }

    #[test]
    fn sin_interes_es_division_simple() {
        let plan = simulate_payoff(dec("1000"), Decimal::ZERO, dec("300"));
        assert_eq!(
            (plan.months, plan.total_paid, plan.finishes),
            (4, dec("1000"), true)
        );
    }

    #[test]
    fn con_interes_tarda_mas_y_cuesta_mas() {
        let plan = simulate_payoff(dec("1000"), dec("24"), dec("100"));
        assert!(plan.finishes);
        assert_eq!(plan.months, 12);
        assert!(plan.total_interest > dec("100") && plan.total_interest < dec("150"));
        assert_eq!(plan.total_paid, dec("1000") + plan.total_interest);
    }

    #[test]
    fn pago_que_no_cubre_el_interes_nunca_termina() {
        let plan = simulate_payoff(dec("10000"), dec("36"), dec("300"));
        assert!(!plan.finishes);
    }
}
