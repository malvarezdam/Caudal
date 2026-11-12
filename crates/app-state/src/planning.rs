//! Presupuestos, metas de ahorro y simulador de pago de deudas.

use crate::charts::to_f64;
use crate::data::Data;
use crate::debts::summary;
use crate::format::{amount_for_input, format_date, format_money};
use crate::forms::{FormError, amount, currency, meta, optional_text, required_name};
use crate::i18n::{Lang, texts};
use crate::views::base_currency;
use caudal_core::budgets::{budget_status, goal_status};
use caudal_core::debts::simulate_payoff;
use caudal_core::fx::FxTable;
use caudal_core::ledger::account_balances;
use caudal_core::model::{Budget, BudgetPeriod, Debt, Goal, GoalContribution};
use caudal_core::money::Money;
use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetForm {
    pub category_id: Option<Uuid>,
    pub amount: String,
    pub currency: String,
    pub period: BudgetPeriod,
    pub rollover: bool,
    pub original: Option<Budget>,
}

impl BudgetForm {
    pub fn new(data: &Data) -> Self {
        Self {
            category_id: None,
            amount: String::new(),
            currency: base_currency(data).to_string(),
            period: BudgetPeriod::Monthly,
            rollover: false,
            original: None,
        }
    }

    pub fn edit(budget: &Budget, lang: Lang) -> Self {
        Self {
            category_id: budget.category_id,
            amount: amount_for_input(budget.amount, lang),
            currency: budget.currency.to_string(),
            period: budget.period,
            rollover: budget.rollover,
            original: Some(budget.clone()),
        }
    }

    /// Un presupuesto nuevo empieza a contar este mes; no arrastra el pasado.
    pub fn validate(&self, data: &Data, today: NaiveDate, lang: Lang) -> Result<Budget, FormError> {
        let starts_on = match &self.original {
            Some(original) => original.starts_on,
            None => today.with_day(1),
        };
        Ok(Budget {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            category_id: self.category_id,
            amount: amount(&self.amount, lang, false)?,
            currency: currency(&self.currency)?,
            period: self.period,
            starts_on,
            ends_on: self.original.as_ref().and_then(|original| original.ends_on),
            rollover: self.rollover,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetView {
    pub id: Uuid,
    pub title: String,
    pub period_label: &'static str,
    pub spent_text: String,
    pub limit_text: String,
    pub remaining_text: String,
    /// Gastado / límite, puede pasar de 1.
    pub ratio: f32,
    pub over: bool,
    pub carried_text: Option<String>,
    pub inactive: bool,
    pub has_unconverted: bool,
}

/// De más usado a menos usado, los inactivos al final.
pub fn budgets_view(data: &Data, today: NaiveDate, lang: Lang) -> Vec<BudgetView> {
    let t = texts(lang);
    let fx = FxTable::new(data.fx_rates.iter().cloned());
    let mut views: Vec<BudgetView> = data
        .budgets
        .iter()
        .filter_map(|budget| {
            let status = budget_status(
                budget,
                &data.categories,
                &data.transactions,
                &data.splits,
                &fx,
                today,
            )
            .ok()?;
            let money =
                |value: Decimal| format_money(&Money::new(value, budget.currency.clone()), lang);
            let ratio = if status.limit <= Decimal::ZERO {
                if status.spent > Decimal::ZERO {
                    2.0
                } else {
                    0.0
                }
            } else {
                to_f64(status.spent / status.limit) as f32
            };
            Some(BudgetView {
                id: budget.meta.id,
                title: budget
                    .category_id
                    .and_then(|id| data.category_path(id))
                    .unwrap_or_else(|| t.all_expenses.to_owned()),
                period_label: match budget.period {
                    BudgetPeriod::Monthly => {
                        t.frequency_unit(caudal_core::model::Frequency::Monthly)
                    }
                    BudgetPeriod::Yearly => t.frequency_unit(caudal_core::model::Frequency::Yearly),
                },
                spent_text: money(status.spent),
                limit_text: money(status.limit),
                remaining_text: money(status.remaining),
                ratio,
                over: status.remaining < Decimal::ZERO,
                carried_text: (!status.carried.is_zero())
                    .then(|| format!("{} {}", t.carried, money(status.carried))),
                inactive: status.inactive,
                has_unconverted: !status.unconverted.is_empty(),
            })
        })
        .collect();
    views.sort_by(|a, b| {
        a.inactive
            .cmp(&b.inactive)
            .then(b.ratio.total_cmp(&a.ratio))
    });
    views
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalForm {
    pub name: String,
    pub target_amount: String,
    pub currency: String,
    pub target_date: Option<NaiveDate>,
    pub account_id: Option<Uuid>,
    pub note: String,
    pub original: Option<Goal>,
}

impl GoalForm {
    pub fn new(data: &Data) -> Self {
        Self {
            name: String::new(),
            target_amount: String::new(),
            currency: base_currency(data).to_string(),
            target_date: None,
            account_id: None,
            note: String::new(),
            original: None,
        }
    }

    pub fn edit(goal: &Goal, lang: Lang) -> Self {
        Self {
            name: goal.name.clone(),
            target_amount: amount_for_input(goal.target_amount, lang),
            currency: goal.currency.to_string(),
            target_date: goal.target_date,
            account_id: goal.account_id,
            note: goal.note.clone().unwrap_or_default(),
            original: Some(goal.clone()),
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<Goal, FormError> {
        let currency = currency(&self.currency)?;
        if let Some(account) = self.account_id.and_then(|id| data.account(id))
            && account.currency != currency
        {
            return Err(FormError::AccountCurrencyMismatch);
        }
        Ok(Goal {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            name: required_name(&self.name)?,
            target_amount: amount(&self.target_amount, lang, false)?,
            currency,
            target_date: self.target_date,
            account_id: self.account_id,
            note: optional_text(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContributionForm {
    pub goal: Goal,
    pub amount: String,
    pub date: NaiveDate,
    pub note: String,
}

impl ContributionForm {
    pub fn new(goal: &Goal, today: NaiveDate) -> Self {
        Self {
            goal: goal.clone(),
            amount: String::new(),
            date: today,
            note: String::new(),
        }
    }

    /// Admite negativos: retirar dinero de la meta también es libre.
    pub fn validate(&self, data: &Data, lang: Lang) -> Result<GoalContribution, FormError> {
        Ok(GoalContribution {
            meta: meta(None, data.user_id),
            goal_id: self.goal.meta.id,
            transaction_id: None,
            amount: amount(&self.amount, lang, true)?,
            contributed_on: self.date,
            note: optional_text(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalView {
    pub id: Uuid,
    pub name: String,
    pub saved_text: String,
    pub target_text: String,
    pub ratio: f32,
    pub reached: bool,
    pub uses_account: bool,
    pub target_date_text: Option<String>,
    pub projection_text: Option<String>,
    pub needed_text: Option<String>,
}

pub fn goals_view(data: &Data, today: NaiveDate, lang: Lang) -> Vec<GoalView> {
    let t = texts(lang);
    let balances = account_balances(&data.accounts, &data.transactions).unwrap_or_default();
    data.goals
        .iter()
        .map(|goal| {
            let account_balance = goal
                .account_id
                .and_then(|id| balances.get(&id))
                .filter(|balance| balance.balance.currency == goal.currency)
                .map(|balance| balance.balance.amount);
            let status = goal_status(goal, &data.contributions, account_balance, today);
            let money =
                |value: Decimal| format_money(&Money::new(value, goal.currency.clone()), lang);
            GoalView {
                id: goal.meta.id,
                name: goal.name.clone(),
                saved_text: money(status.saved),
                target_text: money(goal.target_amount),
                ratio: if goal.target_amount.is_zero() {
                    1.0
                } else {
                    to_f64(status.saved / goal.target_amount).clamp(0.0, 1.0) as f32
                },
                reached: status.reached,
                uses_account: account_balance.is_some(),
                target_date_text: goal
                    .target_date
                    .map(|date| format!("{} {}", t.target_date, format_date(date, lang))),
                projection_text: status
                    .projected_date
                    .filter(|_| !status.reached)
                    .map(|date| format!("{} {}", t.projection, format_date(date, lang))),
                needed_text: status.needed_per_month.map(|value| {
                    let estimate = crate::format::format_estimate(
                        &Money::new(value, goal.currency.clone()),
                        lang,
                    );
                    format!("{}: {estimate}", t.needed_monthly)
                }),
            }
        })
        .collect()
}

/// Simula pagar `payment` cada mes la deuda con su saldo pendiente e interés.
pub fn simulate(
    data: &Data,
    debt: &Debt,
    payment: &str,
    today: NaiveDate,
    lang: Lang,
) -> Result<String, FormError> {
    let t = texts(lang);
    let payment = amount(payment, lang, false)?;
    let outstanding = summary(data, debt, today)
        .map(|state| state.outstanding)
        .unwrap_or(debt.principal);
    let plan = simulate_payoff(outstanding, debt.interest_rate.unwrap_or_default(), payment);
    if !plan.finishes {
        return Ok(t.payoff_never.to_owned());
    }
    let finish = today
        .checked_add_months(Months::new(plan.months))
        .map(|date| format!("{}/{}", t.month_short(date.month()), date.year()))
        .unwrap_or_default();
    Ok(t.payoff_result(
        plan.months,
        &finish,
        &crate::format::format_estimate(
            &Money::new(plan.total_interest, debt.currency.clone()),
            lang,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{CategoryForm, TransactionForm};

    const USER: Uuid = Uuid::from_u128(1);

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    #[test]
    fn presupuesto_excedido_y_orden() {
        let mut data = Data {
            user_id: USER,
            ..Data::default()
        };
        let food = CategoryForm {
            name: "Comida".into(),
            ..CategoryForm::new()
        }
        .validate(USER)
        .unwrap();
        data.categories.push(food.clone());
        let mut form = TransactionForm::new(&data, day("2026-11-05"));
        form.amount = "120".into();
        form.category_id = Some(food.meta.id);
        data.transactions
            .push(form.validate(&data, USER, Lang::Es).unwrap());

        let tight = BudgetForm {
            category_id: Some(food.meta.id),
            amount: "100".into(),
            ..BudgetForm::new(&data)
        }
        .validate(&data, day("2026-11-01"), Lang::Es)
        .unwrap();
        let wide = BudgetForm {
            amount: "1000".into(),
            ..BudgetForm::new(&data)
        }
        .validate(&data, day("2026-11-01"), Lang::Es)
        .unwrap();
        data.budgets = vec![wide, tight];
        let views = budgets_view(&data, day("2026-11-09"), Lang::Es);
        assert_eq!(views[0].title, "Comida");
        assert!(views[0].over);
        assert_eq!(views[0].remaining_text, "-20,00 USD");
        assert_eq!(views[1].title, "Todos los gastos");
        assert!(!views[1].over);
    }

    #[test]
    fn meta_con_aportes_y_simulador() {
        let mut data = Data {
            user_id: USER,
            ..Data::default()
        };
        let goal = GoalForm {
            name: "Viaje".into(),
            target_amount: "1000".into(),
            target_date: Some(day("2027-05-01")),
            ..GoalForm::new(&data)
        }
        .validate(&data, Lang::Es)
        .unwrap();
        data.goals.push(goal.clone());
        for (amount, date) in [
            ("100", "2026-09-20"),
            ("100", "2026-10-20"),
            ("100", "2026-11-05"),
        ] {
            let item = ContributionForm {
                amount: amount.into(),
                date: day(date),
                ..ContributionForm::new(&goal, day(date))
            };
            data.contributions
                .push(item.validate(&data, Lang::Es).unwrap());
        }
        let view = &goals_view(&data, day("2026-11-09"), Lang::Es)[0];
        assert_eq!(view.saved_text, "300,00 USD");
        assert!((view.ratio - 0.3).abs() < 0.001);
        assert_eq!(
            view.projection_text.as_deref(),
            Some("A este ritmo, la alcanzas el 09/06/2027")
        );
        assert!(view.needed_text.as_deref().unwrap().ends_with("116,67 USD"));

        let debt = crate::debts::DebtForm {
            principal: "1000".into(),
            interest_rate: "24".into(),
            ..crate::debts::DebtForm::new(&data, day("2026-11-09"))
        }
        .validate(&data, Lang::Es)
        .unwrap()
        .debt;
        let text = simulate(&data, &debt, "100", day("2026-11-09"), Lang::Es).unwrap();
        assert!(
            text.starts_with("Terminas en 12 meses (nov/2027)"),
            "{text}"
        );
        assert_eq!(
            simulate(&data, &debt, "10", day("2026-11-09"), Lang::Es).unwrap(),
            texts(Lang::Es).payoff_never
        );
    }
}
