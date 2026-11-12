//! Informes, presupuestos, metas y simulador de pagos.

use super::charts::{bar_chart, line_chart, ratio_bar, series_color};
use super::dialogs::{Dialog, Outcome, buttons, title};
use super::widgets::{
    amount_field, card, date_field, error_label, field_label, heading, muted, text_field,
    tone_color, uuid_combo, warning_color,
};
use crate::app::CaudalApp;
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::analysis::{Period, period_containing, reports_view, shift};
use caudal_app_state::model::{BudgetPeriod, Debt, TransactionKind};
use caudal_app_state::planning::{
    BudgetForm, ContributionForm, GoalForm, budgets_view, goals_view, simulate,
};
use caudal_app_state::views::Tone;
use caudal_app_state::{Data, Lang, Table, texts};
use eframe::egui::{self, Align, Grid, Layout, RichText, Ui};

pub fn reports(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    heading(ui, t.reports);

    ui.horizontal_wrapped(|ui| {
        let yearly = matches!(app.report_period, Period::Year(_));
        if ui.selectable_label(!yearly, t.month_view).clicked() && yearly {
            app.report_period = period_containing(today(), false);
        }
        if ui.selectable_label(yearly, t.year_view).clicked() && !yearly {
            app.report_period = period_containing(today(), true);
        }
        ui.add_space(12.0);
        if ui.button("<").on_hover_text(t.previous_period).clicked() {
            app.report_period = shift(app.report_period, -1);
        }
        if ui.button(">").on_hover_text(t.next_period).clicked() {
            app.report_period = shift(app.report_period, 1);
        }
    });
    let key = (app.key(), app.report_period);
    let period = app.report_period;
    let view = app
        .cache
        .reports
        .get(key, || reports_view(&app.data, period, today(), lang));
    ui.add_space(6.0);

    card(ui, |ui| {
        ui.label(RichText::new(&view.title).size(18.0).strong());
        ui.columns(3, |columns| {
            let items = [
                (t.income, &view.income, Tone::Positive, &view.changes[0]),
                (t.expense, &view.expense, Tone::Negative, &view.changes[1]),
                (t.savings, &view.net, view.net_tone, &view.changes[2]),
            ];
            for (column, (label, value, tone, change)) in columns.iter_mut().zip(items) {
                column.label(muted(column, label).small());
                column.label(
                    RichText::new(value)
                        .strong()
                        .color(tone_color(column, tone)),
                );
                column.label(
                    RichText::new(format!("{} {}", change.text, t.vs_previous))
                        .small()
                        .color(tone_color(column, change.tone)),
                );
            }
        });
        if view.has_unconverted {
            ui.label(
                RichText::new(t.unconverted_note)
                    .small()
                    .color(warning_color(ui)),
            );
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(t.flow_chart).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if app.report_table {
                    t.show_chart
                } else {
                    t.show_table
                };
                if ui.small_button(label).clicked() {
                    app.report_table = !app.report_table;
                }
            });
        });
        if app.report_table {
            Grid::new("flow-table")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for header in ["", t.income, t.expense, t.savings] {
                        ui.label(muted(ui, header).small());
                    }
                    ui.end_row();
                    for row in &view.flow_rows {
                        ui.label(&row.label);
                        ui.label(&row.income);
                        ui.label(&row.expense);
                        ui.label(&row.net);
                        ui.end_row();
                    }
                });
        } else if view.flow.is_empty() {
            ui.label(muted(ui, t.no_data));
        } else {
            bar_chart(ui, &view.flow, 200.0);
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.label(RichText::new(t.by_category).strong());
        if view.categories.is_empty() {
            ui.label(muted(ui, t.no_data));
        }
        for row in &view.categories {
            ui.horizontal(|ui| {
                ui.label(&row.name);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(muted(ui, &row.percent_text).small());
                    ui.label(&row.amount_text);
                });
            });
            ratio_bar(ui, row.bar, series_color(ui, 0));
            ui.add_space(4.0);
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.label(RichText::new(t.net_worth_chart).strong());
        if let Some(note) = &view.net_worth_note {
            ui.label(RichText::new(note).small().color(warning_color(ui)));
        }
        line_chart(ui, &view.net_worth, 180.0);
    });
}

pub fn plans(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    let data = app.data.clone();
    heading(ui, t.budgets);
    if ui.button(format!("+ {}", t.new_budget)).clicked() {
        app.dialog = Some(Dialog::Budget {
            form: Box::new(BudgetForm::new(&data)),
            error: None,
        });
    }
    ui.add_space(6.0);
    let key = app.key();
    let budgets = app
        .cache
        .budgets
        .get(key, || budgets_view(&data, today(), lang));
    if budgets.is_empty() {
        ui.label(muted(ui, t.no_budgets));
    }
    budget_cards(app, ui, &budgets, usize::MAX);

    ui.add_space(12.0);
    heading(ui, t.goals);
    if ui.button(format!("+ {}", t.new_goal)).clicked() {
        app.dialog = Some(Dialog::Goal {
            form: Box::new(GoalForm::new(&data)),
            error: None,
        });
    }
    ui.add_space(6.0);
    let goals = app
        .cache
        .goals
        .get(key, || goals_view(&data, today(), lang));
    if goals.is_empty() {
        ui.label(muted(ui, t.no_goals));
    }
    for goal in &goals {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&goal.name).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(muted(ui, format!("/ {}", goal.target_text)).small());
                    ui.label(RichText::new(&goal.saved_text).strong());
                });
            });
            ratio_bar(ui, goal.ratio, series_color(ui, 0));
            if goal.reached {
                ui.label(
                    RichText::new(format!("✔ {}", t.goal_reached))
                        .color(tone_color(ui, Tone::Positive)),
                );
            }
            for line in [
                &goal.target_date_text,
                &goal.projection_text,
                &goal.needed_text,
            ]
            .into_iter()
            .flatten()
            {
                ui.label(muted(ui, line).small());
            }
            ui.horizontal_wrapped(|ui| {
                if !goal.uses_account
                    && ui.small_button(t.add_contribution).clicked()
                    && let Some(model) = data.goals.iter().find(|g| g.meta.id == goal.id)
                {
                    app.dialog = Some(Dialog::Contribution {
                        form: Box::new(ContributionForm::new(model, today())),
                        error: None,
                    });
                }
                if ui.small_button(t.edit).clicked()
                    && let Some(model) = data.goals.iter().find(|g| g.meta.id == goal.id)
                {
                    app.dialog = Some(Dialog::Goal {
                        form: Box::new(GoalForm::edit(model, lang)),
                        error: None,
                    });
                }
                if ui.small_button(t.delete).clicked() {
                    app.dialog = Some(Dialog::ConfirmDelete {
                        table: Table::Goals,
                        id: goal.id,
                        label: goal.name.clone(),
                    });
                }
            });
        });
        ui.add_space(6.0);
    }
}

/// Tarjetas de presupuesto; en el inicio se muestran solo las más usadas.
pub fn budget_cards(
    app: &mut CaudalApp,
    ui: &mut Ui,
    budgets: &[caudal_app_state::planning::BudgetView],
    max: usize,
) {
    let lang = app.lang;
    let t = texts(lang);
    for budget in budgets.iter().take(max) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&budget.title).strong());
                    let mut detail = budget.period_label.to_owned();
                    if budget.inactive {
                        detail = format!("{detail} · {}", t.inactive);
                    }
                    ui.label(muted(ui, detail).small());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(muted(ui, format!("/ {}", budget.limit_text)).small());
                    ui.label(RichText::new(&budget.spent_text).strong());
                });
            });
            let color = if budget.over {
                tone_color(ui, Tone::Negative)
            } else {
                series_color(ui, 0)
            };
            ratio_bar(ui, budget.ratio, color);
            ui.horizontal(|ui| {
                if budget.over {
                    ui.label(
                        RichText::new(format!("⚠ {}: {}", t.over_budget, budget.remaining_text))
                            .small()
                            .color(tone_color(ui, Tone::Negative)),
                    );
                } else {
                    ui.label(
                        muted(ui, format!("{}: {}", t.available, budget.remaining_text)).small(),
                    );
                }
                if let Some(carried) = &budget.carried_text {
                    ui.label(muted(ui, carried).small());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t.edit).clicked()
                        && let Some(model) =
                            app.data.budgets.iter().find(|b| b.meta.id == budget.id)
                    {
                        app.dialog = Some(Dialog::Budget {
                            form: Box::new(BudgetForm::edit(model, lang)),
                            error: None,
                        });
                    }
                });
            });
            if budget.has_unconverted {
                ui.label(
                    RichText::new(t.unconverted_note)
                        .small()
                        .color(warning_color(ui)),
                );
            }
        });
        ui.add_space(6.0);
    }
}

fn result<T>(
    value: Result<T, caudal_app_state::forms::FormError>,
    error: &mut Option<String>,
    lang: Lang,
    item: impl FnOnce(T) -> Item,
) -> Outcome {
    match value {
        Ok(value) => Outcome::Send(Command::Save(Box::new(item(value)))),
        Err(problem) => {
            *error = Some(problem.message(texts(lang)).into());
            Outcome::Keep
        }
    }
}

pub fn budget_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut BudgetForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_budget
        } else {
            t.new_budget
        },
    );
    field_label(ui, t.category);
    uuid_combo(
        ui,
        "budget-category",
        &mut form.category_id,
        Some(t.all_expenses),
        &data.categories_for(TransactionKind::Expense),
    );
    field_label(ui, t.amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.amount, "0", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    field_label(ui, t.period);
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut form.period,
            BudgetPeriod::Monthly,
            t.frequency_unit(caudal_app_state::model::Frequency::Monthly),
        );
        ui.selectable_value(
            &mut form.period,
            BudgetPeriod::Yearly,
            t.frequency_unit(caudal_app_state::model::Frequency::Yearly),
        );
    });
    ui.checkbox(&mut form.rollover, t.rollover);
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        let label = original
            .category_id
            .and_then(|id| data.category_path(id))
            .unwrap_or_else(|| t.all_expenses.to_owned());
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::Budgets,
            id: original.meta.id,
            label,
        });
    }
    if save {
        return result(
            form.validate(data, today(), lang),
            error,
            lang,
            Item::Budget,
        );
    }
    Outcome::Keep
}

pub fn goal_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut GoalForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(
        ui,
        if form.original.is_some() {
            t.edit_goal
        } else {
            t.new_goal
        },
    );
    field_label(ui, t.name);
    text_field(ui, &mut form.name, "");
    field_label(ui, t.target_amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.target_amount, "0", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    let mut has_date = form.target_date.is_some();
    if ui.checkbox(&mut has_date, t.target_date).changed() {
        form.target_date = has_date.then(today);
    }
    if let Some(date) = form.target_date.as_mut() {
        date_field(ui, "goal-date", date, lang, today());
    }
    field_label(ui, t.linked_account);
    uuid_combo(
        ui,
        "goal-account",
        &mut form.account_id,
        Some(t.none),
        &data.accounts_in(&form.currency),
    );
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return result(form.validate(data, lang), error, lang, Item::Goal);
    }
    Outcome::Keep
}

pub fn contribution_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut ContributionForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, t.add_contribution);
    ui.label(&form.goal.name);
    field_label(ui, &format!("{} ({})", t.amount, form.goal.currency));
    amount_field(ui, &mut form.amount, "0", true);
    field_label(ui, t.date);
    date_field(ui, "contribution-date", &mut form.date, lang, today());
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return result(form.validate(data, lang), error, lang, Item::Contribution);
    }
    Outcome::Keep
}

pub fn simulate_dialog(
    ui: &mut Ui,
    data: &Data,
    debt: &Debt,
    payment: &mut String,
    outcome: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, t.simulate);
    let who = data
        .contact_name(debt.contact_id)
        .map(str::to_owned)
        .or_else(|| debt.description.clone())
        .unwrap_or_default();
    ui.label(who);
    field_label(ui, &format!("{} ({})", t.monthly_payment, debt.currency));
    let response = amount_field(ui, payment, "0", true);
    if response.changed() {
        *outcome = simulate(data, debt, payment, today(), lang).ok();
    }
    if let Some(text) = outcome {
        ui.add_space(6.0);
        ui.label(RichText::new(text.as_str()).strong());
    }
    ui.label(muted(ui, t.simulator_note).small());
    ui.add_space(8.0);
    if ui.button(t.back).clicked() {
        return Outcome::Close;
    }
    Outcome::Keep
}
