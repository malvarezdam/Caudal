//! Categorías sugeridas. Sus ids derivan del usuario, así dos dispositivos que
//! las crean sin conexión producen los mismos registros y no se duplican.

use crate::i18n::Lang;
use caudal_core::model::{Category, CategoryKind, Hlc, SyncMeta};
use uuid::Uuid;

const SUGGESTED: [(&str, &str, &str, CategoryKind); 10] = [
    ("food", "Comida", "Food", CategoryKind::Expense),
    (
        "transport",
        "Transporte",
        "Transport",
        CategoryKind::Expense,
    ),
    ("home", "Casa", "Home", CategoryKind::Expense),
    ("bills", "Servicios", "Bills", CategoryKind::Expense),
    ("health", "Salud", "Health", CategoryKind::Expense),
    ("leisure", "Ocio", "Leisure", CategoryKind::Expense),
    ("shopping", "Compras", "Shopping", CategoryKind::Expense),
    ("education", "Educación", "Education", CategoryKind::Expense),
    ("salary", "Sueldo", "Salary", CategoryKind::Income),
    (
        "other_income",
        "Otros ingresos",
        "Other income",
        CategoryKind::Income,
    ),
];

pub fn default_category_id(user_id: Uuid, key: &str) -> Uuid {
    Uuid::new_v5(
        &user_id,
        format!("caudal:default-category:{key}").as_bytes(),
    )
}

pub fn suggested_categories(user_id: Uuid, lang: Lang) -> Vec<Category> {
    SUGGESTED
        .iter()
        .map(|(key, es, en, kind)| Category {
            meta: SyncMeta {
                id: default_category_id(user_id, key),
                user_id,
                updated_at: Hlc(String::new()),
                deleted_at: None,
                server_seq: None,
            },
            name: match lang {
                Lang::Es => (*es).to_owned(),
                Lang::En => (*en).to_owned(),
            },
            parent_id: None,
            kind: *kind,
            color: None,
            icon: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismos_ids_en_cualquier_dispositivo_y_distintos_por_usuario() {
        let user = Uuid::from_u128(1);
        let first = suggested_categories(user, Lang::Es);
        let second = suggested_categories(user, Lang::En);
        assert_eq!(first[0].meta.id, second[0].meta.id);
        let other = suggested_categories(Uuid::from_u128(2), Lang::Es);
        assert_ne!(first[0].meta.id, other[0].meta.id);
    }
}

/// Datos de ejemplo para el modo demostración.
pub mod demo {
    use super::suggested_categories;
    use crate::data::Data;
    use crate::forms::{AccountForm, ProfileForm, TransactionForm};
    use crate::i18n::Lang;
    use caudal_core::model::{
        Account, AccountKind, Category, Profile, Transaction, TransactionKind,
    };
    use chrono::{Datelike, NaiveDate};
    use uuid::Uuid;

    pub struct DemoData {
        pub profile: Profile,
        pub accounts: Vec<Account>,
        pub categories: Vec<Category>,
        pub transactions: Vec<Transaction>,
        pub debt: crate::debts::DebtDraft,
        pub rules: Vec<caudal_core::model::RecurringRule>,
        pub history: Vec<Transaction>,
        pub budget: caudal_core::model::Budget,
        pub goal: caudal_core::model::Goal,
        pub contributions: Vec<caudal_core::model::GoalContribution>,
        pub assets: Vec<caudal_core::model::Asset>,
        pub trades: Vec<crate::investments::TradeDraft>,
        pub market: crate::Market,
        pub alerts: Vec<caudal_core::model::PriceAlert>,
        pub category_rules: Vec<caudal_core::model::CategoryRule>,
        pub templates: Vec<caudal_core::model::TransactionTemplate>,
    }

    /// Cartera de ejemplo: Apple pagada desde la cuenta en dólares, un ETF, una
    /// cripto y un depósito con precio manual. Los precios simulan el cierre de
    /// cada mes; en uso real los escribe el worker.
    fn investments(
        data: &Data,
        savings: &Account,
        today: NaiveDate,
    ) -> (
        Vec<caudal_core::model::Asset>,
        Vec<crate::investments::TradeDraft>,
        crate::Market,
    ) {
        use crate::investments::{AssetForm, TradeForm};
        use caudal_core::model::{AssetPrice, PriceProviderId, TradeKind};
        use rust_decimal::Decimal;
        let first = today.with_day(1).unwrap_or(today);
        let months_ago = |n: u32| {
            first
                .checked_sub_months(chrono::Months::new(n))
                .unwrap_or(first)
                .with_day(10)
                .unwrap_or(first)
        };
        let mut data = data.clone();
        let mut assets = Vec::new();
        for query in ["AAPL", "VOO", "BTC"] {
            let mut form = AssetForm::new(&data, today);
            form.fill_from(&crate::catalog::search(query)[0]);
            let asset = form
                .validate(&data, Lang::Es)
                .expect("activo de ejemplo válido");
            data.assets.push(asset.clone());
            assets.push(asset);
        }
        let mut deposit = AssetForm::new(&data, today);
        deposit.name = "Depósito a plazo".into();
        deposit.kind = caudal_core::model::AssetKind::Bond;
        deposit.currency = "CLP".into();
        deposit.manual_price = "2050000".into();
        deposit.manual_price_on = first;
        let deposit = deposit.validate(&data, Lang::Es).expect("depósito válido");
        data.assets.push(deposit.clone());
        assets.push(deposit);

        // (activo, tipo, hace meses, cantidad, precio, comisión, desde la cuenta)
        let plan: [(usize, TradeKind, u32, &str, &str, &str, bool); 7] = [
            (0, TradeKind::Buy, 8, "4", "180", "1", true),
            (0, TradeKind::Buy, 4, "2", "195", "1", true),
            (0, TradeKind::Dividend, 2, "", "1,5", "", false),
            (1, TradeKind::Buy, 6, "3", "480", "", false),
            (2, TradeKind::Buy, 10, "0,015", "60000", "", false),
            (2, TradeKind::Sell, 3, "0,005", "95000", "", false),
            (3, TradeKind::TransferIn, 5, "1", "2000000", "", false),
        ];
        let trades = plan
            .iter()
            .map(|(asset, kind, back, quantity, price, fee, paid)| {
                let form = TradeForm {
                    kind: *kind,
                    quantity: (*quantity).into(),
                    price: (*price).into(),
                    fee: (*fee).into(),
                    account_id: paid.then_some(savings.meta.id),
                    ..TradeForm::new(Some(assets[*asset].meta.id), months_ago(*back))
                };
                form.validate(&data, Lang::Es)
                    .expect("operación de ejemplo válida")
            })
            .collect();

        // Cierre simulado de los últimos 12 meses y de hoy.
        let curves: [(&str, PriceProviderId, &str, [u32; 13]); 3] = [
            (
                "AAPL.US",
                PriceProviderId::Eodhd,
                "USD",
                [
                    172, 168, 175, 181, 178, 186, 192, 198, 195, 204, 211, 218, 224,
                ],
            ),
            (
                "VOO.US",
                PriceProviderId::Eodhd,
                "USD",
                [
                    455, 462, 470, 468, 476, 489, 495, 502, 498, 510, 521, 528, 534,
                ],
            ),
            (
                "bitcoin",
                PriceProviderId::Coingecko,
                "USD",
                [
                    52000, 58000, 61000, 67000, 63000, 70000, 74000, 69000, 82000, 91000, 88000,
                    96000, 101000,
                ],
            ),
        ];
        let mut prices = Vec::new();
        for (symbol, provider, currency, values) in curves {
            for (index, value) in values.iter().enumerate() {
                let price_on = if index == 12 {
                    today
                } else {
                    first
                        .checked_sub_months(chrono::Months::new(11 - index as u32))
                        .and_then(|month| month.checked_sub_days(chrono::Days::new(1)))
                        .unwrap_or(first)
                };
                prices.push(AssetPrice {
                    provider,
                    symbol: symbol.into(),
                    price_on,
                    price: Decimal::from(*value),
                    currency: caudal_core::Currency::new(currency).expect("moneda válida"),
                });
            }
        }
        let rate = |quote: &str, value: &str| caudal_core::fx::FxRate {
            base: caudal_core::Currency::new("USD").expect("moneda válida"),
            quote: caudal_core::Currency::new(quote).expect("moneda válida"),
            rate_on: first
                .checked_sub_months(chrono::Months::new(24))
                .unwrap_or(first),
            rate: value.parse().expect("tipo de cambio válido"),
        };
        let market = crate::Market {
            prices,
            fx_rates: vec![rate("CLP", "950"), rate("EUR", "0.92")],
        };
        (assets, trades, market)
    }

    pub fn sample(user_id: Uuid, today: NaiveDate, lang: Lang) -> DemoData {
        let profile = ProfileForm {
            base_currency: "CLP".into(),
            lang,
            display_name: String::new(),
        }
        .validate(user_id)
        .expect("perfil de ejemplo válido");
        let categories = suggested_categories(user_id, lang);
        let category = |index: usize| categories.get(index).map(|c| c.meta.id);
        let mut data = Data {
            user_id,
            profile: Some(profile.clone()),
            categories: categories.clone(),
            ..Data::default()
        };
        let account = |name: &str, kind: AccountKind, currency: &str, opening: &str| {
            AccountForm {
                name: name.into(),
                kind,
                currency: currency.into(),
                opening_balance: opening.into(),
                ..AccountForm::new(&data)
            }
            .validate(user_id, Lang::En)
            .expect("cuenta de ejemplo válida")
        };
        let checking = account("Cuenta corriente", AccountKind::Bank, "CLP", "420000");
        let cash = account("Efectivo", AccountKind::Cash, "CLP", "60000");
        let savings = account("Ahorro en dólares", AccountKind::Bank, "USD", "1200");
        data.accounts = vec![checking.clone(), cash.clone(), savings.clone()];

        let first = today.with_day(1).unwrap_or(today);
        let day = |n: u32| first.with_day(n.min(today.day())).unwrap_or(today);
        // (tipo, monto, categoría sugerida, cuenta, día, descripción)
        type Entry<'a> = (
            TransactionKind,
            &'a str,
            Option<usize>,
            &'a Account,
            u32,
            &'a str,
        );
        let entries: [Entry; 7] = [
            (
                TransactionKind::Income,
                "1500000",
                Some(8),
                &checking,
                1,
                "Sueldo",
            ),
            (
                TransactionKind::Expense,
                "450000",
                Some(2),
                &checking,
                2,
                "Arriendo",
            ),
            (
                TransactionKind::Expense,
                "85430",
                Some(0),
                &checking,
                4,
                "Supermercado",
            ),
            (
                TransactionKind::Expense,
                "25000",
                Some(1),
                &cash,
                5,
                "Carga tarjeta de metro",
            ),
            (
                TransactionKind::Expense,
                "9990",
                Some(3),
                &checking,
                6,
                "Streaming",
            ),
            (TransactionKind::Expense, "12500", Some(5), &cash, 7, ""),
            (
                TransactionKind::Transfer,
                "190000",
                None,
                &checking,
                8,
                "Ahorro del mes",
            ),
        ];
        let mut transactions = Vec::new();
        for (kind, amount, category_index, from, n, description) in entries {
            let mut form = TransactionForm::new(&data, day(n));
            form.kind = kind;
            form.amount = amount.into();
            form.select_account(&data, Some(from.meta.id));
            form.category_id = category_index.and_then(category);
            form.description = description.into();
            if kind == TransactionKind::Transfer {
                form.to_account_id = Some(savings.meta.id);
                form.received_amount = "200".into();
            }
            if let Ok(tx) = form.validate(&data, user_id, Lang::En) {
                transactions.push(tx);
            }
        }
        data.transactions = transactions.clone();
        let debt = crate::debts::DebtForm {
            new_contact: "Ana".into(),
            principal: "90000".into(),
            currency: "CLP".into(),
            description: "Préstamo para el viaje".into(),
            installments: "3".into(),
            first_installment: first,
            ..crate::debts::DebtForm::new(&data, first)
        }
        .validate(&data, Lang::Es)
        .expect("deuda de ejemplo válida");
        let rule = |name: &str,
                    amount: &str,
                    kind: TransactionKind,
                    category: Option<usize>,
                    mode: caudal_core::model::RecurringMode| {
            crate::recurring::RecurringForm {
                name: name.into(),
                kind,
                amount: amount.into(),
                currency: "CLP".into(),
                account_id: Some(checking.meta.id),
                category_id: category.and_then(|index| categories.get(index).map(|c| c.meta.id)),
                starts_on: first.with_day(6).unwrap_or(first),
                mode,
                ..crate::recurring::RecurringForm::new(&data, first)
            }
            .validate(&data, Lang::Es)
            .expect("regla de ejemplo válida")
        };
        let rules = vec![
            rule(
                "Música",
                "5990",
                TransactionKind::Expense,
                Some(3),
                caudal_core::model::RecurringMode::Confirm,
            ),
            rule(
                "Gimnasio",
                "25000",
                TransactionKind::Expense,
                Some(4),
                caudal_core::model::RecurringMode::Auto,
            ),
        ];
        // Cinco meses anteriores, para que los gráficos tengan historia.
        let mut history = Vec::new();
        for back in 1..=5u32 {
            let Some(month) = first.checked_sub_months(chrono::Months::new(back)) else {
                continue;
            };
            let swing = i64::from(back) * 23_000;
            for (kind, amount, category_index, day_of_month, description) in [
                (TransactionKind::Income, 1_500_000, Some(8), 1, "Sueldo"),
                (TransactionKind::Expense, 450_000, Some(2), 2, "Arriendo"),
                (
                    TransactionKind::Expense,
                    95_000 + swing,
                    Some(0),
                    9,
                    "Supermercado",
                ),
                (
                    TransactionKind::Expense,
                    40_000 + swing / 2,
                    Some(5),
                    18,
                    "Salidas",
                ),
                (TransactionKind::Expense, 30_000, Some(1), 20, "Transporte"),
            ] {
                let mut form =
                    TransactionForm::new(&data, month.with_day(day_of_month).unwrap_or(month));
                form.kind = kind;
                form.amount = amount.to_string();
                form.select_account(&data, Some(checking.meta.id));
                form.category_id = category_index.and_then(category);
                form.description = description.into();
                if let Ok(tx) = form.validate(&data, user_id, Lang::En) {
                    history.push(tx);
                }
            }
        }
        let budget = crate::planning::BudgetForm {
            category_id: category(0),
            amount: "150000".into(),
            currency: "CLP".into(),
            rollover: true,
            ..crate::planning::BudgetForm::new(&data)
        }
        .validate(
            &data,
            first
                .checked_sub_months(chrono::Months::new(2))
                .unwrap_or(first),
            Lang::Es,
        )
        .expect("presupuesto de ejemplo válido");
        let goal = crate::planning::GoalForm {
            name: "Viaje a Japón".into(),
            target_amount: "3000000".into(),
            currency: "CLP".into(),
            target_date: first.checked_add_months(chrono::Months::new(14)),
            ..crate::planning::GoalForm::new(&data)
        }
        .validate(&data, Lang::Es)
        .expect("meta de ejemplo válida");
        let contributions = (0..3u32)
            .filter_map(|back| first.checked_sub_months(chrono::Months::new(back)))
            .map(|date| {
                crate::planning::ContributionForm {
                    amount: "200000".into(),
                    date: date.with_day(5).unwrap_or(date),
                    ..crate::planning::ContributionForm::new(&goal, date)
                }
                .validate(&data, Lang::Es)
                .expect("aporte de ejemplo válido")
            })
            .collect();
        let (assets, trades, market) = investments(&data, &savings, today);
        let category_rules = [
            ("uber", 1),
            ("metro", 1),
            ("supermercado", 0),
            ("netflix", 3),
            ("remuneración", 8),
        ]
        .into_iter()
        .map(|(pattern, index)| {
            crate::rules::RuleForm {
                pattern: pattern.into(),
                category_id: category(index),
                ..crate::rules::RuleForm::new()
            }
            .validate(&data)
            .expect("regla de ejemplo válida")
        })
        .collect();
        let templates = [
            ("Café", "3500", 0usize, &cash),
            ("Metro", "800", 1, &cash),
            ("Almuerzo", "", 0, &checking),
        ]
        .into_iter()
        .map(|(name, amount, index, account)| {
            crate::rules::TemplateForm {
                name: name.into(),
                kind: TransactionKind::Expense,
                amount: amount.into(),
                currency: "CLP".into(),
                account_id: Some(account.meta.id),
                to_account_id: None,
                category_id: category(index),
                tag_ids: Vec::new(),
                description: name.into(),
                original: None,
            }
            .validate(&data, Lang::Es)
            .expect("plantilla de ejemplo válida")
        })
        .collect();
        let alerts = {
            let mut with_assets = data.clone();
            with_assets.assets = assets.clone();
            let alert = |index: usize, direction, target: &str| {
                crate::investments::AlertForm {
                    direction,
                    target_price: target.into(),
                    ..crate::investments::AlertForm::new(assets[index].meta.id)
                }
                .validate(&with_assets, Lang::Es)
                .expect("alerta de ejemplo válida")
            };
            vec![
                alert(0, caudal_core::model::AlertDirection::Above, "220"),
                alert(1, caudal_core::model::AlertDirection::Below, "450"),
            ]
        };
        DemoData {
            profile,
            accounts: data.accounts,
            categories,
            transactions,
            debt,
            rules,
            history,
            budget,
            goal,
            contributions,
            assets,
            trades,
            market,
            alerts,
            category_rules,
            templates,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn datos_de_ejemplo_validos() {
            let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
            let demo = sample(Uuid::from_u128(1), today, Lang::Es);
            assert_eq!(demo.accounts.len(), 3);
            assert_eq!(demo.transactions.len(), 7);
        }
        #[test]
        fn cartera_de_ejemplo_cuadra_a_mano() {
            let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
            let demo = sample(Uuid::from_u128(1), today, Lang::Es);
            let mut data = Data {
                user_id: Uuid::from_u128(1),
                profile: Some(demo.profile.clone()),
                accounts: demo.accounts.clone(),
                assets: demo.assets.clone(),
                trades: demo.trades.iter().map(|d| d.trade.clone()).collect(),
                prices: demo.market.prices.clone(),
                fx_rates: demo.market.fx_rates.clone(),
                ..Data::default()
            };
            data.transactions
                .extend(demo.trades.iter().filter_map(|d| d.movement.clone()));
            let view = crate::investments::portfolio_view(&data, today, Lang::Es);
            // (6 × 224 + 3 × 534 + 0,01 × 101.000) × 950 + 2.050.000
            assert_eq!(view.total, "5.808.200 CLP");
            // BTC vendido: 475 - 300 de costo = 175; más 1,5 de dividendo.
            assert_eq!(view.realized, "167.675 CLP");
            let apple = view.positions.iter().find(|p| p.name == "Apple").unwrap();
            assert_eq!(apple.cost, "1.112,00 USD");
            assert_eq!(apple.average_cost.as_deref(), Some("185,33 USD"));
            assert!(view.note.is_none());
            // La compra de Apple salió de la cuenta en dólares: 1.200 - 721 - 391.
            let balances =
                caudal_core::ledger::account_balances(&data.accounts, &data.transactions).unwrap();
            let savings = data
                .accounts
                .iter()
                .find(|a| a.currency.as_str() == "USD")
                .unwrap();
            assert_eq!(
                balances[&savings.meta.id]
                    .balance
                    .amount
                    .normalize()
                    .to_string(),
                "88"
            );
            assert_eq!(view.chart.labels.len(), 12);
        }
    }
}
