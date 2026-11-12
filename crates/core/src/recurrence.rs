//! Reglas periódicas: fechas de cada ocurrencia y el movimiento que generan.

use crate::model::{
    Frequency, Hlc, RecurringMode, RecurringRule, SyncMeta, Transaction, TransactionStatus,
};
use chrono::{Datelike, Days, Months, NaiveDate};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Tope de ocurrencias por consulta, para reglas diarias con años de historia.
pub const MAX_OCCURRENCES: usize = 400;

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let first_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    first_next
        .and_then(|date| date.pred_opt())
        .unwrap_or(NaiveDate::MAX)
}

/// Fecha de la ocurrencia `n` (la primera es 0). Un día 31 cae en el último
/// día de los meses cortos; `last_day_of_month` fija siempre el último día.
pub fn nth_occurrence(rule: &RecurringRule, n: u32) -> Option<NaiveDate> {
    let interval = u32::try_from(rule.interval_count.max(1)).ok()?;
    let step = n.checked_mul(interval)?;
    let start = rule.starts_on;
    match rule.frequency {
        Frequency::Daily => start.checked_add_days(Days::new(u64::from(step))),
        Frequency::Weekly => start.checked_add_days(Days::new(u64::from(step) * 7)),
        Frequency::Monthly | Frequency::Yearly => {
            let months = if rule.frequency == Frequency::Yearly {
                step.checked_mul(12)?
            } else {
                step
            };
            let first_of_month = start.with_day(1)?.checked_add_months(Months::new(months))?;
            let last = last_day_of_month(first_of_month.year(), first_of_month.month());
            if rule.last_day_of_month {
                Some(last)
            } else {
                Some(first_of_month.with_day(start.day()).unwrap_or(last))
            }
        }
    }
}

/// Ocurrencias entre `from` y `to`, ambos incluidos, respetando inicio y fin de
/// la regla. Si hay más de [`MAX_OCCURRENCES`], devuelve las más recientes.
pub fn occurrences_between(rule: &RecurringRule, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let end = rule.ends_on.map_or(to, |ends| ends.min(to));
    let mut dates = Vec::new();
    for n in 0.. {
        let Some(date) = nth_occurrence(rule, n) else {
            break;
        };
        if date > end {
            break;
        }
        if date >= from {
            dates.push(date);
            if dates.len() > MAX_OCCURRENCES {
                dates.remove(0);
            }
        }
    }
    dates
}

/// Primera ocurrencia en `after` o después.
pub fn next_occurrence(rule: &RecurringRule, after: NaiveDate) -> Option<NaiveDate> {
    (0..)
        .map_while(|n| nth_occurrence(rule, n))
        .take_while(|date| rule.ends_on.is_none_or(|ends| *date <= ends))
        .find(|date| *date >= after)
}

/// El mismo id en todos los dispositivos: generar dos veces no duplica.
pub fn occurrence_id(rule_id: Uuid, date: NaiveDate) -> Uuid {
    Uuid::new_v5(&rule_id, date.to_string().as_bytes())
}

/// Movimiento de una ocurrencia. Con modo `Confirm` queda pendiente.
pub fn occurrence_transaction(rule: &RecurringRule, date: NaiveDate) -> Transaction {
    Transaction {
        meta: SyncMeta {
            id: occurrence_id(rule.meta.id, date),
            user_id: rule.meta.user_id,
            updated_at: Hlc(String::new()),
            deleted_at: None,
            server_seq: None,
        },
        kind: rule.kind,
        status: match rule.mode {
            RecurringMode::Auto => TransactionStatus::Confirmed,
            RecurringMode::Confirm => TransactionStatus::Pending,
        },
        amount: rule.amount,
        currency: rule.currency.clone(),
        occurred_on: date,
        account_id: rule.account_id,
        to_account_id: rule.to_account_id,
        account_amount: None,
        to_amount: None,
        base_amount: None,
        base_currency: None,
        category_id: rule.category_id,
        tag_ids: Vec::new(),
        contact_id: None,
        recurring_rule_id: Some(rule.meta.id),
        occurrence_date: Some(date),
        description: rule.description.clone().or_else(|| rule.name.clone()),
        note: None,
    }
}

/// Costo mensual promedio, para comparar suscripciones de distinta frecuencia.
pub fn monthly_equivalent(rule: &RecurringRule) -> Decimal {
    let interval = Decimal::from(rule.interval_count.max(1));
    let per_month = match rule.frequency {
        Frequency::Daily => Decimal::new(365, 0) / Decimal::from(12),
        Frequency::Weekly => Decimal::new(52, 0) / Decimal::from(12),
        Frequency::Monthly => Decimal::ONE,
        Frequency::Yearly => Decimal::ONE / Decimal::from(12),
    };
    (rule.amount * per_month / interval).round_dp(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::test_support::*;
    use crate::model::TransactionKind;

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn rule(frequency: Frequency, start: &str) -> RecurringRule {
        RecurringRule {
            meta: meta(7),
            name: Some("Arriendo".into()),
            frequency,
            interval_count: 1,
            last_day_of_month: false,
            starts_on: day(start),
            ends_on: None,
            mode: RecurringMode::Confirm,
            active: true,
            kind: TransactionKind::Expense,
            amount: dec("450000"),
            currency: cur("CLP"),
            account_id: None,
            to_account_id: None,
            category_id: None,
            description: None,
        }
    }

    fn dates(rule: &RecurringRule, from: &str, to: &str) -> Vec<String> {
        occurrences_between(rule, day(from), day(to))
            .into_iter()
            .map(|date| date.to_string())
            .collect()
    }

    #[test]
    fn mensual_el_31_se_ajusta_a_meses_cortos() {
        let monthly = rule(Frequency::Monthly, "2026-01-31");
        assert_eq!(
            dates(&monthly, "2026-01-01", "2026-05-31"),
            [
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30",
                "2026-05-31"
            ]
        );
    }

    #[test]
    fn ultimo_dia_de_cada_mes() {
        let mut monthly = rule(Frequency::Monthly, "2028-01-15");
        monthly.last_day_of_month = true;
        assert_eq!(
            dates(&monthly, "2028-01-01", "2028-03-31"),
            ["2028-01-31", "2028-02-29", "2028-03-31"]
        );
    }

    #[test]
    fn semanal_cada_dos_y_fin_de_regla() {
        let mut weekly = rule(Frequency::Weekly, "2026-11-02");
        weekly.interval_count = 2;
        weekly.ends_on = Some(day("2026-12-01"));
        assert_eq!(
            dates(&weekly, "2026-11-01", "2027-01-01"),
            ["2026-11-02", "2026-11-16", "2026-11-30"]
        );
    }

    #[test]
    fn anual_29_de_febrero() {
        let yearly = rule(Frequency::Yearly, "2028-02-29");
        assert_eq!(
            dates(&yearly, "2028-01-01", "2032-12-31"),
            [
                "2028-02-29",
                "2029-02-28",
                "2030-02-28",
                "2031-02-28",
                "2032-02-29"
            ]
        );
    }

    #[test]
    fn diaria_con_tope_de_ocurrencias() {
        let daily = rule(Frequency::Daily, "2020-01-01");
        let all = occurrences_between(&daily, day("2020-01-01"), day("2026-11-09"));
        assert_eq!(all.len(), MAX_OCCURRENCES);
        assert_eq!(all.last().copied(), Some(day("2026-11-09")));
    }

    #[test]
    fn siguiente_ocurrencia() {
        let monthly = rule(Frequency::Monthly, "2026-01-05");
        assert_eq!(
            next_occurrence(&monthly, day("2026-11-06")),
            Some(day("2026-12-05"))
        );
        assert_eq!(
            next_occurrence(&monthly, day("2026-11-05")),
            Some(day("2026-11-05"))
        );
        let mut ended = monthly.clone();
        ended.ends_on = Some(day("2026-06-30"));
        assert_eq!(next_occurrence(&ended, day("2026-11-06")), None);
    }

    #[test]
    fn misma_ocurrencia_mismo_id() {
        let monthly = rule(Frequency::Monthly, "2026-01-05");
        let a = occurrence_transaction(&monthly, day("2026-02-05"));
        let b = occurrence_transaction(&monthly, day("2026-02-05"));
        let c = occurrence_transaction(&monthly, day("2026-03-05"));
        assert_eq!(a.meta.id, b.meta.id);
        assert_ne!(a.meta.id, c.meta.id);
        assert_eq!(a.status, TransactionStatus::Pending);
        assert_eq!(a.description.as_deref(), Some("Arriendo"));
        assert_eq!(a.recurring_rule_id, Some(monthly.meta.id));
    }

    #[test]
    fn equivalente_mensual() {
        let mut yearly = rule(Frequency::Yearly, "2026-01-01");
        yearly.amount = dec("120000");
        assert_eq!(monthly_equivalent(&yearly), dec("10000"));
        let mut weekly = rule(Frequency::Weekly, "2026-01-01");
        weekly.amount = dec("3000");
        assert_eq!(monthly_equivalent(&weekly), dec("13000"));
    }
}
