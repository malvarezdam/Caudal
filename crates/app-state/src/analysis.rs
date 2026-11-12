//! Informes: resumen de un mes o un año, comparación con el anterior, gasto por
//! categoría y gráficos de flujo y patrimonio.

use crate::charts::{Chart, ChartSeries, to_f64};
use crate::data::Data;
use crate::format::format_money;
use crate::i18n::{Lang, texts};
use crate::views::{Tone, base_currency};
use caudal_core::fx::FxTable;
use caudal_core::money::Money;
use caudal_core::series::{expense_by_parent, monthly_flow, net_worth_series};
pub use caudal_core::summary::Period;
use caudal_core::summary::summarize;
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;

pub fn period_containing(date: NaiveDate, yearly: bool) -> Period {
    if yearly {
        Period::Year(date.year())
    } else {
        Period::Month {
            year: date.year(),
            month: date.month(),
        }
    }
}

pub fn shift(period: Period, steps: i32) -> Period {
    match period {
        Period::Year(year) => Period::Year(year + steps),
        Period::Month { year, month } => {
            let index = year * 12 + month as i32 - 1 + steps;
            Period::Month {
                year: index.div_euclid(12),
                month: index.rem_euclid(12) as u32 + 1,
            }
        }
    }
}

pub fn period_title(period: Period, lang: Lang) -> String {
    match period {
        Period::Year(year) => year.to_string(),
        Period::Month { year, month } => {
            format!("{} {year}", texts(lang).month_long(month))
        }
    }
}

fn last_day(period: Period) -> NaiveDate {
    let (year, month) = match period {
        Period::Year(year) => (year, 12),
        Period::Month { year, month } => (year, month),
    };
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.and_then(|date| date.pred_opt()).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub label: &'static str,
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRow {
    pub name: String,
    pub amount_text: String,
    pub percent_text: String,
    /// Fracción del gasto mayor, para el largo de la barra.
    pub bar: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowRow {
    pub label: String,
    pub income: String,
    pub expense: String,
    pub net: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportsView {
    pub title: String,
    pub income: String,
    pub expense: String,
    pub net: String,
    pub net_tone: Tone,
    pub changes: Vec<Change>,
    pub categories: Vec<CategoryRow>,
    pub flow: Chart,
    pub flow_rows: Vec<FlowRow>,
    pub net_worth: Chart,
    /// "Sin tipo de cambio, no incluye: Ahorro en dólares".
    pub net_worth_note: Option<String>,
    pub has_unconverted: bool,
}

/// Variación respecto del periodo anterior. Para gastos, subir es malo.
fn change(
    label: &'static str,
    now: Decimal,
    before: Decimal,
    more_is_good: bool,
    lang: Lang,
) -> Change {
    let diff = now - before;
    let tone = if diff.is_zero() {
        Tone::Neutral
    } else if diff.is_sign_positive() == more_is_good {
        Tone::Positive
    } else {
        Tone::Negative
    };
    let text = if before.is_zero() {
        if diff.is_zero() {
            "=".to_owned()
        } else {
            "—".to_owned()
        }
    } else {
        let percent = (diff / before.abs() * Decimal::from(100)).round_dp(0);
        let sign = if percent.is_sign_positive() && !percent.is_zero() {
            "+"
        } else {
            ""
        };
        if lang == Lang::Es {
            format!("{sign}{percent} %")
        } else {
            format!("{sign}{percent}%")
        }
    };
    Change { label, text, tone }
}

/// Informe del periodo. Los gráficos cubren los 12 meses que terminan en él.
pub fn reports_view(data: &Data, period: Period, today: NaiveDate, lang: Lang) -> ReportsView {
    let t = texts(lang);
    let base = base_currency(data);
    let fx = FxTable::new(data.fx_rates.iter().cloned());
    let money = |value: Decimal| format_money(&Money::new(value, base.clone()), lang);

    let current = summarize(&data.transactions, &data.splits, period, &base, &fx).ok();
    let previous = summarize(
        &data.transactions,
        &data.splits,
        shift(period, -1),
        &base,
        &fx,
    )
    .ok();
    let (income, expense) = current
        .as_ref()
        .map_or((Decimal::ZERO, Decimal::ZERO), |s| (s.income, s.expense));
    let (prev_income, prev_expense) = previous
        .as_ref()
        .map_or((Decimal::ZERO, Decimal::ZERO), |s| (s.income, s.expense));
    let net = income - expense;
    let changes = vec![
        change(t.income, income, prev_income, true, lang),
        change(t.expense, expense, prev_expense, false, lang),
        change(t.savings, net, prev_income - prev_expense, true, lang),
    ];

    let shares = current
        .as_ref()
        .map(|summary| expense_by_parent(summary, &data.categories))
        .unwrap_or_default();
    let top = shares.first().map_or(Decimal::ONE, |share| share.amount);
    let mut categories: Vec<CategoryRow> = Vec::new();
    let mut other = Decimal::ZERO;
    for (index, share) in shares.iter().enumerate() {
        // Más de ocho categorías se agrupan en "Otras".
        if index >= 8 {
            other += share.amount;
            continue;
        }
        let name = share
            .category_id
            .and_then(|id| data.category(id).map(|c| c.name.clone()))
            .unwrap_or_else(|| t.no_category.to_owned());
        categories.push(CategoryRow {
            name,
            amount_text: money(share.amount),
            percent_text: percent(share.amount, expense, lang),
            bar: ratio(share.amount, top),
        });
    }
    if !other.is_zero() {
        categories.push(CategoryRow {
            name: t.others.to_owned(),
            amount_text: money(other),
            percent_text: percent(other, expense, lang),
            bar: ratio(other, top),
        });
    }

    let flow = monthly_flow(
        &data.transactions,
        &data.splits,
        &base,
        &fx,
        last_day(period),
        12,
    )
    .unwrap_or_default();
    let labels: Vec<String> = flow
        .iter()
        .map(|month| {
            if month.month == 1 {
                format!("{} {}", t.month_short(month.month), month.year % 100)
            } else {
                t.month_short(month.month).to_owned()
            }
        })
        .collect();
    let flow_rows = flow
        .iter()
        .zip(&labels)
        .map(|(month, label)| FlowRow {
            label: format!(
                "{label} {}",
                if month.month == 1 {
                    String::new()
                } else {
                    month.year.to_string()
                }
            )
            .trim()
            .to_owned(),
            income: money(month.income),
            expense: money(month.expense),
            net: money(month.income - month.expense),
        })
        .collect();
    let has_unconverted = current.as_ref().is_some_and(|s| !s.unconverted.is_empty())
        || flow.iter().any(|month| month.unconverted > 0);
    let flow_chart = Chart::new(
        labels.clone(),
        vec![
            ChartSeries {
                name: t.income.to_owned(),
                slot: 0,
                values: flow.iter().map(|m| Some(to_f64(m.income))).collect(),
                texts: flow.iter().map(|m| money(m.income)).collect(),
            },
            ChartSeries {
                name: t.expense.to_owned(),
                slot: 1,
                values: flow.iter().map(|m| Some(to_f64(m.expense))).collect(),
                texts: flow.iter().map(|m| money(m.expense)).collect(),
            },
        ],
        lang,
    );

    let points = net_worth_series(
        &data.accounts,
        &data.transactions,
        &base,
        &fx,
        last_day(period).min(today),
        12,
    )
    .unwrap_or_default();
    // Más las inversiones valoradas a cada cierre.
    let points: Vec<_> = points
        .into_iter()
        .map(|mut point| {
            if let Ok((value, _)) = caudal_core::portfolio::portfolio_value(
                &data.assets,
                &data.trades,
                &data.prices,
                &fx,
                &base,
                point.date,
            ) {
                point.value += value;
            }
            point
        })
        .collect();
    let mut excluded: Vec<uuid::Uuid> = points
        .iter()
        .flat_map(|point| point.excluded.iter().copied())
        .collect();
    excluded.sort();
    excluded.dedup();
    let net_worth_note = (!excluded.is_empty()).then(|| {
        let names: Vec<&str> = excluded
            .iter()
            .filter_map(|id| data.account_name(Some(*id)))
            .collect();
        format!("{} {}", t.net_worth_unavailable, names.join(", "))
    });
    let net_worth = Chart::new(
        labels,
        vec![ChartSeries {
            name: t.net_worth.to_owned(),
            slot: 0,
            values: points.iter().map(|p| Some(to_f64(p.value))).collect(),
            texts: points.iter().map(|p| money(p.value)).collect(),
        }],
        lang,
    );

    ReportsView {
        title: period_title(period, lang),
        income: money(income),
        expense: money(expense),
        net: money(net),
        net_tone: if net.is_sign_negative() && !net.is_zero() {
            Tone::Negative
        } else {
            Tone::Positive
        },
        changes,
        categories,
        flow: flow_chart,
        flow_rows,
        net_worth,
        net_worth_note,
        has_unconverted,
    }
}

fn ratio(part: Decimal, whole: Decimal) -> f32 {
    if whole.is_zero() {
        0.0
    } else {
        to_f64(part / whole).clamp(0.0, 1.0) as f32
    }
}

fn percent(part: Decimal, whole: Decimal, lang: Lang) -> String {
    if whole.is_zero() {
        return String::new();
    }
    let value = (part / whole * Decimal::from(100)).round_dp(0);
    if lang == Lang::Es {
        format!("{value} %")
    } else {
        format!("{value}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::demo::sample;
    use caudal_core::model::TransactionKind;

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    #[test]
    fn periodos_se_desplazan_entre_anios() {
        assert_eq!(
            shift(
                Period::Month {
                    year: 2026,
                    month: 1
                },
                -1
            ),
            Period::Month {
                year: 2025,
                month: 12
            }
        );
        assert_eq!(
            shift(
                Period::Month {
                    year: 2026,
                    month: 12
                },
                1
            ),
            Period::Month {
                year: 2027,
                month: 1
            }
        );
        assert_eq!(
            period_title(
                Period::Month {
                    year: 2026,
                    month: 11
                },
                Lang::Es
            ),
            "noviembre 2026"
        );
    }

    #[test]
    fn el_informe_cuadra_con_los_movimientos() {
        let today = day("2026-11-09");
        let demo = sample(uuid::Uuid::from_u128(1), today, Lang::Es);
        let data = Data {
            user_id: uuid::Uuid::from_u128(1),
            profile: Some(demo.profile),
            accounts: demo.accounts,
            categories: demo.categories,
            transactions: demo.transactions,
            ..Data::default()
        };
        let view = reports_view(&data, period_containing(today, false), today, Lang::Es);
        assert_eq!(view.title, "noviembre 2026");
        assert_eq!(view.income, "1.500.000 CLP");
        assert_eq!(view.expense, "582.920 CLP");
        // La suma de las categorías es el gasto total.
        let expenses: Decimal = data
            .transactions
            .iter()
            .filter(|tx| tx.kind == TransactionKind::Expense)
            .map(|tx| tx.amount)
            .sum();
        assert_eq!(expenses, Decimal::from(582_920));
        assert_eq!(view.categories[0].name, "Casa");
        assert_eq!(view.categories[0].percent_text, "77 %");
        // El último mes del gráfico es el del informe.
        assert_eq!(view.flow.labels.last().map(String::as_str), Some("nov"));
        assert_eq!(
            view.flow.series[1].texts.last().map(String::as_str),
            Some("582.920 CLP")
        );
        assert_eq!(view.changes[0].text, "—");
    }
}
