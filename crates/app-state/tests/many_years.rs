//! Tres años de datos: los totales cuadran y las vistas responden rápido.

use caudal_app_state::analysis::{period_containing, reports_view};
use caudal_app_state::csv_io::{CsvTable, ImportMapping, preview};
use caudal_app_state::defaults::demo::sample;
use caudal_app_state::investments::portfolio_view;
use caudal_app_state::model::{Hlc, TransactionKind, TransactionStatus};
use caudal_app_state::notices::{activity, notices};
use caudal_app_state::planning::budgets_view;
use caudal_app_state::views::{TransactionFilter, dashboard, transactions};
use caudal_app_state::{AppCore, Data, Lang, Table};
use caudal_sync::memory::MemoryStore;
use chrono::{Datelike, Days, NaiveDate};
use pollster::block_on;
use rust_decimal::Decimal;
use std::fmt::Write as _;
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
        contacts: demo.debt.contact.into_iter().collect(),
        debts: vec![demo.debt.debt],
        installments: demo.debt.installments,
        rules: demo.rules,
        budgets: vec![demo.budget],
        goals: vec![demo.goal],
        contributions: demo.contributions,
        assets: demo.assets,
        trades: demo.trades.into_iter().map(|d| d.trade).collect(),
        prices: demo.market.prices,
        fx_rates: demo.market.fx_rates,
        alerts: demo.alerts,
        category_rules: demo.category_rules,
        templates: demo.templates,
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
            tx.meta.updated_at = Hlc(format!("{counter:016x}-0001-0001"));
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
fn tiempos_con_cien_mil_movimientos() {
    let (mut data, today) = big_data(91);
    while data.transactions.len() < 100_000 {
        let mut tx = data.transactions.last().unwrap().clone();
        let next = data.transactions.len() as u128 + 10_000;
        tx.meta.id = Uuid::from_u128(next);
        tx.meta.updated_at = Hlc(format!("{next:016x}-0001-0001"));
        data.transactions.push(tx);
    }
    data.transactions.truncate(100_000);
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
    measure("portfolio_view", &|| {
        portfolio_view(&data, today, Lang::Es);
    });
    measure("notices", &|| {
        notices(&data, today, Lang::Es);
    });
    measure("activity", &|| {
        activity(&data, 30, -180, Lang::Es);
    });

    let mut csv_text = String::from("Fecha;Descripción;Monto;Categoría\n");
    for i in 0..5_000u32 {
        let d = today
            .checked_sub_days(Days::new(u64::from(i % 900)))
            .unwrap();
        let _ = writeln!(
            csv_text,
            "{};Supermercado compra {i};-{};Comida",
            d.format("%d/%m/%Y"),
            1_500 + (i % 80) * 100
        );
    }
    measure("vista previa CSV (5.000 filas)", &|| {
        let table = CsvTable::parse(csv_text.as_bytes()).unwrap();
        let mapping = ImportMapping::guess(&table, &data);
        let prev = preview(&data, &table, &mapping, Lang::Es);
        assert_eq!(prev.rows.len(), 5_000);
    });

    block_on(async {
        let core = AppCore::open(MemoryStore::default(), data.user_id, || 5_000, || 1)
            .await
            .unwrap();
        if let Some(profile) = &data.profile {
            core.save(Table::Profiles, profile).await.unwrap();
        }
        for acc in &data.accounts {
            core.save(Table::Accounts, acc).await.unwrap();
        }
        for cat in &data.categories {
            core.save(Table::Categories, cat).await.unwrap();
        }
        for tx in &data.transactions {
            core.save(Table::Transactions, tx).await.unwrap();
        }

        let start = Instant::now();
        let backup = core.export_backup("2026-11-09").await.unwrap();
        println!("export_backup: {:?}", start.elapsed());

        let target = AppCore::open(MemoryStore::default(), data.user_id, || 6_000, || 2)
            .await
            .unwrap();
        let start = Instant::now();
        let report = target.restore_backup(&backup).await.unwrap();
        println!("restore_backup: {:?}", start.elapsed());
        assert_eq!(report.skipped, 0);
        assert!(report.restored >= 100_000);
    });
}
