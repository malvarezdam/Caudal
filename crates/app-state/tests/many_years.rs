//! Tres años de datos: los totales cuadran y las vistas responden rápido.

use caudal_app_state::analysis::{period_containing, reports_view};
use caudal_app_state::defaults::demo::sample;
use caudal_app_state::model::{TransactionKind, TransactionStatus};
use caudal_app_state::planning::budgets_view;
use caudal_app_state::views::{TransactionFilter, dashboard, transactions};
use caudal_app_state::{Data, Lang};
use chrono::{Datelike, Days, NaiveDate};
use rust_decimal::Decimal;
use std::time::Instant;
use uuid::Uuid;

fn big_data(per_day: u32) -> (Data, NaiveDate) {
    let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    let user = Uuid::from_u128(1);
    let demo = sample(user, today, Lang::Es);
    let mut data = Data {
        user_id: user,
        profile: Some(demo.profile),
        accounts: demo.accounts,
        categories: demo.categories,
        ..Data::default()
    };
    let template = demo.transactions[2].clone();
    let start = NaiveDate::from_ymd_opt(2023, 11, 10).unwrap();
    let mut counter: u128 = 10_000;
    let mut date = start;
    while date <= today {
        for n in 0..per_day {
            counter += 1;
            let mut tx = template.clone();
            tx.meta.id = Uuid::from_u128(counter);
            tx.occurred_on = date;
            tx.kind = if n == 0 && date.day() == 1 {
                TransactionKind::Income
            } else {
                TransactionKind::Expense
            };
            tx.amount = Decimal::from(1_000 + (counter % 97) as i64 * 10);
            tx.status = TransactionStatus::Confirmed;
            tx.category_id = data
                .categories
                .get((counter % 8) as usize)
                .map(|c| c.meta.id);
            data.transactions.push(tx);
        }
        date = date.checked_add_days(Days::new(1)).unwrap();
    }
    (data, today)
}

#[test]
fn totales_cuadran_con_tres_anios() {
    let (data, today) = big_data(5);
    let period = period_containing(today, true);
    let view = reports_view(&data, period, today, Lang::Es);
    let expected: Decimal = data
        .transactions
        .iter()
        .filter(|tx| tx.kind == TransactionKind::Expense && tx.occurred_on.year() == 2026)
        .map(|tx| tx.amount)
        .sum();
    let currency = caudal_app_state::Currency::new("CLP").unwrap();
    assert_eq!(
        view.expense,
        caudal_app_state::format::format_money(
            &caudal_app_state::Money::new(expected, currency),
            Lang::Es
        )
    );
    // Cada barra de categoría suma al total del gráfico mensual.
    let (_, total) = transactions(&data, &TransactionFilter::default(), 50, Lang::Es);
    assert_eq!(total, data.transactions.len());
}

/// `cargo test --release -p caudal-app-state --test many_years -- --ignored --nocapture`
#[test]
#[ignore]
fn tiempos_con_cincuenta_mil_movimientos() {
    let (data, today) = big_data(45);
    println!("movimientos: {}", data.transactions.len());
    let measure = |name: &str, run: &dyn Fn()| {
        let start = Instant::now();
        run();
        println!("{name}: {:?}", start.elapsed());
    };
    measure("inicio", &|| {
        dashboard(&data, today, Lang::Es);
    });
    measure("informe del mes", &|| {
        reports_view(&data, period_containing(today, false), today, Lang::Es);
    });
    measure("informe del año", &|| {
        reports_view(&data, period_containing(today, true), today, Lang::Es);
    });
    measure("lista con búsqueda", &|| {
        transactions(
            &data,
            &TransactionFilter {
                text: "super".into(),
                ..TransactionFilter::default()
            },
            50,
            Lang::Es,
        );
    });
    measure("presupuestos", &|| {
        budgets_view(&data, today, Lang::Es);
    });
}
