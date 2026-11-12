//! Informes, presupuestos, metas y simulador en la web.

use crate::state::{Dialog, Item, today};
use crate::ui::{close, dialog_buttons, parse_uuid, state, tone_class, uuid_options};
use caudal_app_state::analysis::{Period, period_containing, reports_view, shift};
use caudal_app_state::charts::Chart;
use caudal_app_state::model::{BudgetPeriod, Frequency, TransactionKind};
use caudal_app_state::planning::{
    BudgetForm, BudgetView, ContributionForm, GoalForm, budgets_view, goals_view, simulate,
};
use caudal_app_state::views::Tone;
use caudal_app_state::{Lang, Table, texts};
use leptos::prelude::*;

fn pct(value: f64) -> String {
    format!("{:.2}%", value * 100.0)
}

fn legend(chart: &Chart) -> impl IntoView + use<> {
    (chart.series.len() > 1).then(|| {
        view! {
            <div class="legend">
                {chart.series.iter().map(|series| view! {
                    <span><i class=format!("swatch s{}", series.slot + 1)></i>{series.name.clone()}</span>
                }).collect_view()}
            </div>
        }
    })
}

fn axis(chart: &Chart) -> impl IntoView + use<> {
    chart
        .ticks
        .iter()
        .map(|tick| {
            let bottom = pct(chart.fraction(tick.value));
            view! {
                <div class="grid" style=format!("bottom: {bottom}")></div>
                <span class="tick" style=format!("bottom: {bottom}")>{tick.label.clone()}</span>
            }
        })
        .collect_view()
}

fn x_labels(chart: &Chart) -> impl IntoView + use<> {
    let count = chart.labels.len();
    view! {
        <div class="xlabels">
            {chart.labels.iter().enumerate().map(|(index, label)| {
                // En pantallas estrechas se ocultan las alternas (CSS).
                let class = if (count - 1 - index) % 2 == 1 { "alt" } else { "" };
                view! { <span class=class>{label.clone()}</span> }
            }).collect_view()}
        </div>
    }
}

fn tooltip(chart: &Chart, index: usize) -> impl IntoView + use<> {
    let count = chart.labels.len().max(1);
    let left = pct((index as f64 + 0.5) / count as f64);
    view! {
        <div class="tip" style=format!("left: {left}")>
            <strong>{chart.labels.get(index).cloned().unwrap_or_default()}</strong>
            {chart.series.iter().map(|series| view! {
                <div><i class=format!("swatch s{}", series.slot + 1)></i>
                    {format!("{}: {}", series.name, series.texts.get(index).cloned().unwrap_or_default())}</div>
            }).collect_view()}
        </div>
    }
}

/// Columnas agrupadas: HTML y CSS para que el texto se vea nítido a cualquier ancho.
#[component]
fn BarChart(chart: Chart) -> impl IntoView {
    let hovered = RwSignal::new(None::<usize>);
    let tip_chart = chart.clone();
    view! {
        {legend(&chart)}
        <div class="chart" on:mouseleave=move |_| hovered.set(None)>
            <div class="plot">
                {axis(&chart)}
                <div class="bands">
                    {chart.labels.iter().enumerate().map(|(index, _)| view! {
                        <div class="band" class:hover=move || hovered.get() == Some(index)
                            on:mouseenter=move |_| hovered.set(Some(index)) on:click=move |_| hovered.set(Some(index))>
                            {chart.series.iter().map(|series| {
                                let value = series.values.get(index).copied().flatten().unwrap_or(0.0);
                                let height = pct(chart.fraction(value) - chart.fraction(0.0));
                                view! { <div class=format!("bar s{}", series.slot + 1) style=format!("height: {height}")></div> }
                            }).collect_view()}
                        </div>
                    }).collect_view()}
                </div>
                {move || hovered.get().map(|index| tooltip(&tip_chart, index))}
            </div>
            {x_labels(&chart)}
        </div>
    }
}

/// Línea SVG de 2 px que no se deforma al escalar, con punto final.
#[component]
pub fn LineChart(chart: Chart) -> impl IntoView {
    let hovered = RwSignal::new(None::<usize>);
    let count = chart.labels.len().max(1);
    let x = move |index: usize| (index as f64 + 0.5) / count as f64;
    let series = chart.series.first().cloned();
    let mut segments: Vec<Vec<String>> = vec![Vec::new()];
    let mut last = None;
    if let Some(series) = &series {
        for (index, value) in series.values.iter().enumerate() {
            match value {
                Some(value) => {
                    let y = 100.0 - chart.fraction(*value) * 100.0;
                    segments
                        .last_mut()
                        .unwrap()
                        .push(format!("{:.3},{:.3}", x(index) * 100.0, y));
                    last = Some((index, *value));
                }
                None => segments.push(Vec::new()),
            }
        }
    }
    let dot = move |index: usize, value: f64, chart: &Chart| {
        view! { <span class="dot s1" style=format!("left: {}; bottom: {}", pct(x(index)), pct(chart.fraction(value)))></span> }
    };
    let hover_chart = chart.clone();
    let tip_chart = chart.clone();
    view! {
        <div class="chart" on:mouseleave=move |_| hovered.set(None)>
            <div class="plot">
                {axis(&chart)}
                <svg class="line" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
                    {segments.into_iter().filter(|points| points.len() > 1).map(|points| view! {
                        <polyline points=points.join(" ") vector-effect="non-scaling-stroke"></polyline>
                    }).collect_view()}
                </svg>
                {last.map(|(index, value)| dot(index, value, &chart))}
                <div class="bands overlay">
                    {(0..count).map(|index| view! {
                        <div class="band" on:mouseenter=move |_| hovered.set(Some(index)) on:click=move |_| hovered.set(Some(index))></div>
                    }).collect_view()}
                </div>
                {move || hovered.get().map(|index| {
                    let value = hover_chart.series.first().and_then(|s| s.values.get(index).copied().flatten());
                    view! {
                        <div class="crosshair" style=format!("left: {}", pct(x(index)))></div>
                        {value.map(|value| dot(index, value, &hover_chart))}
                        {tooltip(&tip_chart, index)}
                    }
                })}
            </div>
            {x_labels(&chart)}
        </div>
    }
}

#[component]
pub fn Reports() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let period = RwSignal::new(period_containing(today(), false));
    let table = RwSignal::new(false);
    view! {
        <h2 class="desktop-only">{t.reports}</h2>
        <div class="row wrap">
            <button class="chip" class:active=move || matches!(period.get(), Period::Month { .. })
                on:click=move |_| period.set(period_containing(today(), false))>{t.month_view}</button>
            <button class="chip" class:active=move || matches!(period.get(), Period::Year(_))
                on:click=move |_| period.set(period_containing(today(), true))>{t.year_view}</button>
            <button class="small" aria-label=t.previous_period on:click=move |_| period.update(|p| *p = shift(*p, -1))>"<"</button>
            <button class="small" aria-label=t.next_period on:click=move |_| period.update(|p| *p = shift(*p, 1))>">"</button>
        </div>
        {move || {
            let view = reports_view(&state.data.get(), period.get(), today(), lang);
            let summary = [
                (t.income, view.income.clone(), Tone::Positive, view.changes[0].clone()),
                (t.expense, view.expense.clone(), Tone::Negative, view.changes[1].clone()),
                (t.savings, view.net.clone(), view.net_tone, view.changes[2].clone()),
            ];
            let flow = view.flow.clone();
            let rows = view.flow_rows.clone();
            view! {
                <section class="card">
                    <h3>{view.title.clone()}</h3>
                    <div class="grid3">
                        {summary.into_iter().map(|(label, value, tone, change)| view! {
                            <div><span class="muted small">{label}</span>
                                <strong class=tone_class(tone)>{value}</strong>
                                <span class=format!("small {}", tone_class(change.tone))>{format!("{} {}", change.text, t.vs_previous)}</span></div>
                        }).collect_view()}
                    </div>
                    {view.has_unconverted.then(|| view! { <p class="warn small">{t.unconverted_note}</p> })}
                </section>
                <section class="card">
                    <div class="row between"><strong>{t.flow_chart}</strong>
                        <button class="small" on:click=move |_| table.update(|v| *v = !*v)>
                            {move || if table.get() { t.show_chart } else { t.show_table }}</button></div>
                    {move || if table.get() {
                        view! {
                            <table class="data">
                                <thead><tr><th></th><th>{t.income}</th><th>{t.expense}</th><th>{t.savings}</th></tr></thead>
                                <tbody>{rows.iter().map(|row| view! {
                                    <tr><td>{row.label.clone()}</td><td>{row.income.clone()}</td><td>{row.expense.clone()}</td><td>{row.net.clone()}</td></tr>
                                }).collect_view()}</tbody>
                            </table>
                        }.into_any()
                    } else if flow.is_empty() {
                        view! { <p class="muted">{t.no_data}</p> }.into_any()
                    } else {
                        view! { <BarChart chart=flow.clone()/> }.into_any()
                    }}
                </section>
                <section class="card">
                    <strong>{t.by_category}</strong>
                    {view.categories.is_empty().then(|| view! { <p class="muted">{t.no_data}</p> })}
                    {view.categories.iter().map(|row| view! {
                        <div class="line"><span>{row.name.clone()}</span>
                            <span>{row.amount_text.clone()}" "<span class="muted small">{row.percent_text.clone()}</span></span></div>
                        <div class="meter"><span class="s1" style=format!("width: {}", pct(f64::from(row.bar)))></span></div>
                    }).collect_view()}
                </section>
                <section class="card">
                    <strong>{t.net_worth_chart}</strong>
                    {view.net_worth_note.clone().map(|note| view! { <p class="warn small">{note}</p> })}
                    <LineChart chart=view.net_worth.clone()/>
                </section>
            }
        }}
    }
}

pub fn budget_cards(budgets: Vec<BudgetView>, lang: Lang) -> impl IntoView {
    let state = state();
    let t = texts(lang);
    budgets
        .into_iter()
        .map(|budget| {
            let id = budget.id;
            let width = pct(f64::from(budget.ratio.clamp(0.0, 1.0)));
            let detail = if budget.inactive { format!("{} · {}", budget.period_label, t.inactive) } else { budget.period_label.to_owned() };
            view! {
                <section class="card">
                    <div class="row between">
                        <div><strong>{budget.title.clone()}</strong><br/><span class="muted small">{detail}</span></div>
                        <span><strong>{budget.spent_text.clone()}</strong>" "<span class="muted small">{format!("/ {}", budget.limit_text)}</span></span>
                    </div>
                    <div class="meter"><span class=if budget.over { "over" } else { "s1" } style=format!("width: {width}")></span></div>
                    <div class="row between">
                        <span class=if budget.over { "small neg" } else { "small muted" }>
                            {if budget.over { format!("⚠ {}: {}", t.over_budget, budget.remaining_text) } else { format!("{}: {}", t.available, budget.remaining_text) }}
                            {budget.carried_text.clone().map(|text| format!(" · {text}"))}
                        </span>
                        <button class="small" on:click=move |_| {
                            if let Some(model) = state.data.get_untracked().budgets.iter().find(|b| b.meta.id == id) {
                                state.open_dialog(Dialog::Budget(BudgetForm::edit(model, lang)));
                            }
                        }>{t.edit}</button>
                    </div>
                    {budget.has_unconverted.then(|| view! { <p class="warn small">{t.unconverted_note}</p> })}
                </section>
            }
        })
        .collect_view()
}

#[component]
pub fn HomeBudgets() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    move || {
        let budgets: Vec<BudgetView> = budgets_view(&state.data.get(), today(), lang)
            .into_iter()
            .take(3)
            .collect();
        (!budgets.is_empty()).then(|| view! { <h3>{t.budgets}</h3>{budget_cards(budgets, lang)} })
    }
}

#[component]
pub fn Plans() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2>{t.budgets}</h2>
        <button on:click=move |_| state.open_dialog(Dialog::Budget(BudgetForm::new(&state.data.get_untracked())))>"+ " {t.new_budget}</button>
        {move || {
            let budgets = budgets_view(&state.data.get(), today(), lang);
            if budgets.is_empty() { view! { <p class="muted">{t.no_budgets}</p> }.into_any() } else { budget_cards(budgets, lang).into_any() }
        }}
        <h2>{t.goals}</h2>
        <button on:click=move |_| state.open_dialog(Dialog::Goal(GoalForm::new(&state.data.get_untracked())))>"+ " {t.new_goal}</button>
        {move || {
            let data = state.data.get();
            let goals = goals_view(&data, today(), lang);
            let empty = goals.is_empty();
            view! {
                {empty.then(|| view! { <p class="muted">{t.no_goals}</p> })}
                {goals.into_iter().map(|goal| {
                    let id = goal.id;
                    let name = goal.name.clone();
                    let notes: Vec<String> = [goal.target_date_text.clone(), goal.projection_text.clone(), goal.needed_text.clone()].into_iter().flatten().collect();
                    view! {
                        <section class="card">
                            <div class="row between"><strong>{goal.name.clone()}</strong>
                                <span><strong>{goal.saved_text.clone()}</strong>" "<span class="muted small">{format!("/ {}", goal.target_text)}</span></span></div>
                            <div class="meter"><span class="s1" style=format!("width: {}", pct(f64::from(goal.ratio)))></span></div>
                            {goal.reached.then(|| view! { <p class="pos">{format!("✔ {}", t.goal_reached)}</p> })}
                            {notes.into_iter().map(|line| view! { <p class="muted small">{line}</p> }).collect_view()}
                            <div class="row wrap">
                                {(!goal.uses_account).then(|| view! {
                                    <button class="small" on:click=move |_| {
                                        if let Some(model) = state.data.get_untracked().goals.iter().find(|g| g.meta.id == id) {
                                            state.open_dialog(Dialog::Contribution(ContributionForm::new(model, today())));
                                        }
                                    }>{t.add_contribution}</button>
                                })}
                                <button class="small" on:click=move |_| {
                                    if let Some(model) = state.data.get_untracked().goals.iter().find(|g| g.meta.id == id) {
                                        state.open_dialog(Dialog::Goal(GoalForm::edit(model, lang)));
                                    }
                                }>{t.edit}</button>
                                <button class="small" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete { table: Table::Goals, id, label: name.clone() })>{t.delete}</button>
                            </div>
                        </section>
                    }
                }).collect_view()}
            }
        }}
    }
}

fn read<T: Clone + 'static>(pick: fn(&Dialog) -> Option<&T>) -> impl Fn() -> Option<T> + Copy {
    let state = state();
    move || {
        state
            .dialog
            .with(|dialog| dialog.as_ref().and_then(pick).cloned())
    }
}

fn money_row(
    amount: impl Fn() -> String + Send + Sync + Copy + 'static,
    set_amount: impl Fn(String) + Copy + 'static,
    currency: impl Fn() -> String + Send + Sync + Copy + 'static,
    set_currency: impl Fn(String) + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=amount on:input=move |ev| set_amount(event_target_value(&ev))/>
            <input class="currency" prop:value=currency on:input=move |ev| set_currency(event_target_value(&ev))/>
        </div>
    }
}

#[component]
pub fn BudgetDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Budget(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut BudgetForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Budget(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let categories = data.categories_for(TransactionKind::Expense);
    let save_data = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&save_data, today(), lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Budget(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|budget| {
        let label = budget
            .category_id
            .and_then(|id| data.category_path(id))
            .unwrap_or_else(|| t.all_expenses.to_owned());
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::Budgets,
                id: budget.meta.id,
                label: label.clone(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit_budget } else { t.new_budget }}</h2>
        <label>{t.category}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.category_id.is_none())>{t.all_expenses}</option>
                {uuid_options(categories, move || form().and_then(|f| f.category_id))}
            </select>
        </label>
        <label>{t.amount}</label>
        {money_row(
            move || form().map(|f| f.amount).unwrap_or_default(),
            move |v| edit(&|f| f.amount = v.clone()),
            move || form().map(|f| f.currency).unwrap_or_default(),
            move |v| edit(&|f| f.currency = v.clone()),
        )}
        <label>{t.period}</label>
        <div class="chips">
            {[(BudgetPeriod::Monthly, Frequency::Monthly), (BudgetPeriod::Yearly, Frequency::Yearly)].into_iter().map(|(period, frequency)| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.period == period) on:click=move |_| edit(&|f| f.period = period)>{t.frequency_unit(frequency)}</button>
            }).collect_view()}
        </div>
        <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.rollover)
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.rollover = v) }/>{t.rollover}</label>
        {dialog_buttons(state, lang, save, delete)}
    }
}

#[component]
pub fn GoalDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Goal(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut GoalForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Goal(f)) = d {
                change(f)
            }
        })
    };
    let editing = form().is_some_and(|f| f.original.is_some());
    let account_data = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Goal(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <h2>{if editing { t.edit_goal } else { t.new_goal }}</h2>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <label>{t.target_amount}</label>
        {money_row(
            move || form().map(|f| f.target_amount).unwrap_or_default(),
            move |v| edit(&|f| f.target_amount = v.clone()),
            move || form().map(|f| f.currency).unwrap_or_default(),
            move |v| edit(&|f| f.currency = v.clone()),
        )}
        <label>{t.target_date}<input type="date" prop:value=move || form().and_then(|f| f.target_date).map(|d| d.to_string()).unwrap_or_default()
            on:change=move |ev| { let value = event_target_value(&ev).parse().ok(); edit(&|f| f.target_date = value) }/></label>
        <label>{t.linked_account}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                <option value="">{t.none}</option>
                {move || uuid_options(account_data.accounts_in(&form().map(|f| f.currency).unwrap_or_default()), move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
pub fn ContributionDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Contribution(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut ContributionForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Contribution(f)) = d {
                change(f)
            }
        })
    };
    let Some(initial) = form() else {
        return ().into_any();
    };
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Contribution(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <h2>{t.add_contribution}</h2>
        <p>{initial.goal.name.clone()}</p>
        <label>{format!("{} ({})", t.amount, initial.goal.currency)}
            <input class="amount" inputmode="decimal" prop:value=move || form().map(|f| f.amount).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/></label>
        <label>{t.date}<input type="date" prop:value=move || form().map(|f| f.date.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.date = date) }/></label>
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, None)}
    }
    .into_any()
}

#[component]
pub fn SimulateDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let Some(debt) = state.dialog.with_untracked(|d| match d {
        Some(Dialog::Simulate(debt)) => Some(debt.clone()),
        _ => None,
    }) else {
        return ().into_any();
    };
    let payment = RwSignal::new(String::new());
    let who = data
        .contact_name(debt.contact_id)
        .map(str::to_owned)
        .or_else(|| debt.description.clone())
        .unwrap_or_default();
    let currency = debt.currency.to_string();
    let result = move || {
        let text = payment.get();
        (!text.trim().is_empty())
            .then(|| simulate(&data, &debt, &text, today(), lang).ok())
            .flatten()
    };
    view! {
        <h2>{t.simulate}</h2>
        <p>{who}</p>
        <label>{format!("{} ({currency})", t.monthly_payment)}
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || payment.get()
                on:input=move |ev| payment.set(event_target_value(&ev))/></label>
        {move || result().map(|text| view! { <p><strong>{text}</strong></p> })}
        <p class="muted small">{t.simulator_note}</p>
        <div class="row actions"><button on:click=move |_| close(state)>{t.back}</button></div>
    }
    .into_any()
}
