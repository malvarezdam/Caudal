//! Deudas y periódicos en la web.

use crate::state::{Dialog, Item, today};
use crate::ui::{
    close, dialog_buttons, parse_uuid, state, tone_class, transaction_rows, uuid_options,
};
use caudal_app_state::debts::{ContactForm, DebtForm, PaymentForm, SharedExpenseForm, debts_view};
use caudal_app_state::forms::FormError;
use caudal_app_state::i18n::{DIRECTIONS, FORM_KINDS, FREQUENCIES};
use caudal_app_state::model::{Frequency, RecurringMode, TransactionKind};
use caudal_app_state::recurring::{RecurringForm, recurring_view, upcoming};
use caudal_app_state::{Data, Lang, Table, texts};
use leptos::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

fn account_options(data: &Data) -> Vec<(Uuid, String)> {
    data.selectable_accounts(None)
        .into_iter()
        .map(|account| {
            (
                account.meta.id,
                format!("{} ({})", account.name, account.currency),
            )
        })
        .collect()
}

fn report(state: crate::state::AppState, problem: FormError, lang: Lang) {
    state
        .dialog_error
        .set(Some(problem.message(texts(lang)).into()));
}

#[component]
pub fn Debts() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2 class="desktop-only">{t.debts}</h2>
        <div class="row wrap">
            <button on:click=move |_| state.open_dialog(Dialog::Debt(DebtForm::new(&state.data.get_untracked(), today()))) >"+ " {t.new_debt}</button>
            <button on:click=move |_| state.open_dialog(Dialog::Shared(SharedExpenseForm::new(&state.data.get_untracked(), today())))>"+ " {t.shared_expense}</button>
            <button on:click=move |_| state.open_dialog(Dialog::Contact(ContactForm::default()))>"+ " {t.new_contact}</button>
        </div>
        {move || {
            let data: Arc<Data> = state.data.get();
            let view = debts_view(&data, today(), lang);
            let totals = |items: Vec<String>, class: &'static str| {
                if items.is_empty() {
                    view! { <strong>"—"</strong> }.into_any()
                } else {
                    items.into_iter().map(|item| view! { <strong class=class>{item}</strong> }).collect_view().into_any()
                }
            };
            let empty = view.open.is_empty() && view.settled.is_empty();
            view! {
                <section class="card grid2">
                    <div><span class="muted small">{t.owed_to_me}</span>{totals(view.owed_to_me, "pos")}</div>
                    <div><span class="muted small">{t.i_owe}</span>{totals(view.i_owe, "neg")}</div>
                </section>
                {empty.then(|| view! { <p class="muted">{t.no_debts}</p> })}
                {view.open.into_iter().chain(view.settled).map(|debt| {
                    let id = debt.id;
                    let label = debt.title.clone();
                    let progress = format!("width: {}%", (debt.progress * 100.0).round());
                    let installments = debt.installments.clone();
                    view! {
                        <section class="card">
                            <div class="row between">
                                <div><strong>{debt.title.clone()}</strong><br/>
                                    <span class="muted small">{if debt.settled { format!("{} · {}", debt.direction_label, t.settled) } else { debt.direction_label.to_owned() }}</span></div>
                                <span class=format!("big-amount {}", tone_class(debt.tone))>{debt.outstanding_text}</span>
                            </div>
                            <div class="progress"><span style=progress></span></div>
                            <p class="muted small">{debt.detail}</p>
                            {debt.next_due_text.map(|next| view! {
                                <p class=if debt.overdue { "small neg" } else { "small" }>{if debt.overdue { format!("{} · {next}", t.overdue) } else { next }}</p>
                            })}
                            {(!installments.is_empty()).then(|| view! {
                                <details><summary>{format!("{} ({})", t.installments, installments.len())}</summary>
                                    {installments.into_iter().map(|item| {
                                        let (label, class) = if item.settled { (t.paid, "pos") } else if item.overdue { (t.overdue, "neg") } else { (t.outstanding, "muted") };
                                        view! { <div class="line"><span>{item.due_text}" · "{item.amount_text}</span><span class=format!("small {class}")>{label}</span></div> }
                                    }).collect_view()}
                                </details>
                            })}
                            <div class="row wrap">
                                {(!debt.settled).then(|| view! {
                                    <button class="small" on:click=move |_| {
                                        let data = state.data.get_untracked();
                                        if let Some(model) = data.debt(id) {
                                            state.open_dialog(Dialog::Payment(PaymentForm::new(&data, model, today(), lang)));
                                        }
                                    }>{t.register_payment}</button>
                                })}
                                {(!debt.settled).then(|| view! {
                                    <button class="small" on:click=move |_| {
                                        if let Some(model) = state.data.get_untracked().debt(id) {
                                            state.open_dialog(Dialog::Simulate(model.clone()));
                                        }
                                    }>{t.simulate}</button>
                                })}
                                <button class="small" on:click=move |_| {
                                    if let Some(model) = state.data.get_untracked().debt(id) {
                                        state.open_dialog(Dialog::Debt(DebtForm::edit(model, today(), lang)));
                                    }
                                }>{t.edit}</button>
                                <button class="small" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete { table: Table::Debts, id, label: label.clone() })>{t.delete}</button>
                            </div>
                        </section>
                    }
                }).collect_view()}
                <h2>{t.contacts}</h2>
                <section class="card">
                    {view.contacts.is_empty().then(|| view! { <p class="muted">{t.no_contacts}</p> })}
                    {view.contacts.into_iter().map(|contact| {
                        let id = contact.id;
                        let name = contact.name.clone();
                        view! {
                            <div class="line">
                                <span>{contact.name}" "<span class=format!("small {}", tone_class(contact.tone))>{contact.balance_text}</span></span>
                                <span class="row">
                                    <button class="small" on:click=move |_| {
                                        if let Some(model) = state.data.get_untracked().contact(id) {
                                            state.open_dialog(Dialog::Contact(ContactForm::edit(model)));
                                        }
                                    }>{t.edit}</button>
                                    <button class="small" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete { table: Table::Contacts, id, label: name.clone() })>{t.delete}</button>
                                </span>
                            </div>
                        }
                    }).collect_view()}
                </section>
            }
        }}
    }
}

#[component]
pub fn PendingCard() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    move || {
        let pending = recurring_view(&state.data.get(), today(), lang).pending;
        (!pending.is_empty()).then(|| {
            let count = pending.len();
            view! {
                <section class="card">
                    <strong class="warn">{format!("{} ({count})", t.to_confirm)}</strong>
                    {pending.into_iter().map(|tx| {
                        let id = tx.id;
                        view! {
                            <div class="pending-row">
                                {transaction_rows(vec![tx], lang)}
                                <button class="small" on:click=move |_| state.save(Item::Confirm(id))>{t.confirm}</button>
                                <button class="small" on:click=move |_| state.delete(Table::Transactions, id)>{t.skip}</button>
                            </div>
                        }
                    }).collect_view()}
                </section>
            }
        })
    }
}

#[component]
pub fn UpcomingCard(days: u64, max_items: usize) -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    move || {
        let calendar = upcoming(&state.data.get(), today(), days, lang);
        let mut remaining = max_items;
        let empty = calendar.is_empty();
        view! {
            <section class="card">
                <strong>{t.upcoming}</strong>
                {empty.then(|| view! { <p class="muted">{t.nothing_upcoming}</p> })}
                {calendar.into_iter().filter_map(|day| {
                    if remaining == 0 {
                        return None;
                    }
                    let items: Vec<_> = day.items.into_iter().take(remaining).collect();
                    remaining -= items.len();
                    Some(view! {
                        <p class="muted small day">{day.date_text}</p>
                        {items.into_iter().map(|item| {
                            let detail_class = if item.detail == t.overdue { "small neg" } else { "small muted" };
                            view! {
                                <div class="line"><span>{item.title}" "<span class=detail_class>{item.detail}</span></span>
                                    <span class=tone_class(item.tone)>{item.amount_text}</span></div>
                            }
                        }).collect_view()}
                    })
                }).collect_view()}
            </section>
        }
    }
}

#[component]
pub fn Recurring() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2 class="desktop-only">{t.recurring}</h2>
        <button on:click=move |_| state.open_dialog(Dialog::Recurring(RecurringForm::new(&state.data.get_untracked(), today())))>"+ " {t.new_recurring}</button>
        <PendingCard/>
        {move || {
            let view = recurring_view(&state.data.get(), today(), lang);
            let empty = view.rules.is_empty();
            view! {
                <section class="card">
                    <div class="row between"><strong>{t.fixed_monthly}</strong>
                        <span class="neg">{view.monthly_totals.join(" · ")}</span></div>
                    {empty.then(|| view! { <p class="muted">{t.no_recurring}</p> })}
                    {view.rules.into_iter().map(|rule| {
                        let id = rule.id;
                        let detail = if rule.active { format!("{} · {}", rule.frequency_text, rule.mode_label) } else { format!("{} · {} · {}", t.paused, rule.frequency_text, rule.mode_label) };
                        view! {
                            <button class="tx" class:pending=!rule.active on:click=move |_| {
                                if let Some(model) = state.data.get_untracked().rule(id) {
                                    state.open_dialog(Dialog::Recurring(RecurringForm::edit(model, lang)));
                                }
                            }>
                                <span class="tx-main"><span class="tx-title">{rule.title}</span>
                                    <span class="tx-sub muted">{detail}</span>
                                    <span class="tx-sub muted">{rule.next_text.unwrap_or_default()}</span></span>
                                <span class="tx-side"><span class=format!("tx-amount {}", tone_class(rule.tone))>{rule.amount_text}</span>
                                    <span class="tx-sub muted">{rule.monthly_text}</span></span>
                            </button>
                        }
                    }).collect_view()}
                </section>
            }
        }}
        <UpcomingCard days=30 max_items=usize::MAX/>
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

#[component]
pub fn ContactDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let form = read(|d| match d {
        Dialog::Contact(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut ContactForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Contact(f)) = d {
                change(f)
            }
        })
    };
    let editing = form().is_some_and(|f| f.original.is_some());
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(state.data.get_untracked().user_id) {
            Ok(model) => {
                close(state);
                state.save(Item::Contact(model));
            }
            Err(problem) => report(state, problem, lang),
        }
    };
    view! {
        <h2>{if editing { t.edit } else { t.new_contact }}</h2>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default() on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <label>{t.phone}<input inputmode="tel" prop:value=move || form().map(|f| f.phone).unwrap_or_default() on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.phone = v.clone()) }/></label>
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default() on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
pub fn DebtDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Debt(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut DebtForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Debt(f)) = d {
                change(f)
            }
        })
    };
    let creating = form().is_some_and(|f| f.original.is_none());
    let contacts: Vec<(Uuid, String)> = data
        .contacts
        .iter()
        .map(|c| (c.meta.id, c.name.clone()))
        .collect();
    let account_data = data.clone();
    let data_for_save = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data_for_save, lang) {
            Ok(draft) => {
                close(state);
                state.save(Item::Debt(draft));
            }
            Err(problem) => report(state, problem, lang),
        }
    };
    let text_input = move |get: fn(&DebtForm) -> String,
                           set: fn(&mut DebtForm, String),
                           mode: &'static str| {
        view! {
            <input inputmode=mode prop:value=move || form().map(|f| get(&f)).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| set(f, v.clone())) }/>
        }
    };
    view! {
        <h2>{if creating { t.new_debt } else { t.edit_debt }}</h2>
        <div class="chips">
            {DIRECTIONS.into_iter().map(|direction| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.direction == direction) on:click=move |_| edit(&|f| f.direction = direction)>{t.direction(direction)}</button>
            }).collect_view()}
        </div>
        <label>{t.contact}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.contact_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.contact_id.is_none())>{t.none}</option>
                {uuid_options(contacts, move || form().and_then(|f| f.contact_id))}
            </select>
        </label>
        <Show when=move || form().is_some_and(|f| f.contact_id.is_none())>
            <input placeholder=t.new_contact_name prop:value=move || form().map(|f| f.new_contact).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.new_contact = v.clone()) }/>
        </Show>
        <label>{t.principal}</label>
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || form().map(|f| f.principal).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.principal = v.clone()) }/>
            <input class="currency" prop:value=move || form().map(|f| f.currency).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.description}{text_input(|f| f.description.clone(), |f, v| f.description = v, "text")}</label>
        <label>{t.started_on}<input type="date" prop:value=move || form().map(|f| f.started_on.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.started_on = date) }/></label>
        {creating.then(|| view! {
            <label>{t.installments_count}{text_input(|f| f.installments.clone(), |f, v| f.installments = v, "numeric")}</label>
            <Show when=move || form().is_some_and(|f| !f.installments.trim().is_empty())>
                <label>{t.first_installment}<input type="date" prop:value=move || form().map(|f| f.first_installment.to_string()).unwrap_or_default()
                    on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.first_installment = date) }/></label>
            </Show>
        })}
        <Show when=move || form().is_some_and(|f| f.installments.trim().is_empty())>
            <label>{t.due_on}<input type="date" prop:value=move || form().and_then(|f| f.due_on).map(|d| d.to_string()).unwrap_or_default()
                on:change=move |ev| { let value = event_target_value(&ev).parse().ok(); edit(&|f| f.due_on = value) }/></label>
            <label>{t.interest_rate}{text_input(|f| f.interest_rate.clone(), |f, v| f.interest_rate = v, "decimal")}</label>
        </Show>
        {if creating {
            view! {
                <label>{t.record_movement}
                    <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                        <option value="">{t.no_account}</option>
                        {move || uuid_options(account_data.accounts_in(&form().map(|f| f.currency).unwrap_or_default()), move || form().and_then(|f| f.account_id))}
                    </select>
                </label>
            }.into_any()
        } else {
            view! {
                <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.closed)
                    on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.closed = v) }/>{t.close_debt}</label>
            }.into_any()
        }}
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
pub fn PaymentDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Payment(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut PaymentForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Payment(f)) = d {
                change(f)
            }
        })
    };
    let Some(initial) = form() else {
        return ().into_any();
    };
    let who = data
        .contact_name(initial.debt.contact_id)
        .map(str::to_owned)
        .or_else(|| initial.debt.description.clone())
        .unwrap_or_default();
    let accounts = data.accounts_in(initial.debt.currency.as_str());
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok((payment, movement)) => {
                close(state);
                state.save(Item::Payment(payment, movement));
            }
            Err(problem) => report(state, problem, lang),
        }
    };
    view! {
        <h2>{t.register_payment}</h2>
        <p>{format!("{who} · {}", t.direction(initial.debt.direction))}</p>
        <label>{format!("{} ({})", t.amount, initial.debt.currency)}
            <input class="amount" inputmode="decimal" prop:value=move || form().map(|f| f.amount).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/></label>
        <label>{t.date}<input type="date" prop:value=move || form().map(|f| f.date.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.date = date) }/></label>
        <label>{t.record_movement}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                <option value="">{t.no_account}</option>
                {uuid_options(accounts, move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, None)}
    }
    .into_any()
}

#[component]
pub fn SharedDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Shared(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut SharedExpenseForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Shared(f)) = d {
                change(f)
            }
        })
    };
    let account_data = data.clone();
    let categories = data.categories_for(TransactionKind::Expense);
    let contacts = data.contacts.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(draft) => {
                close(state);
                state.save(Item::Shared(draft));
            }
            Err(FormError::NameRequired) => state.dialog_error.set(Some(t.participants.into())),
            Err(problem) => report(state, problem, lang),
        }
    };
    view! {
        <h2>{t.shared_expense}</h2>
        <label>{t.amount}</label>
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || form().map(|f| f.amount).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/>
            <input class="currency" prop:value=move || form().map(|f| f.currency).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.description}<input prop:value=move || form().map(|f| f.description).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.description = v.clone()) }/></label>
        <label>{t.date}<input type="date" prop:value=move || form().map(|f| f.date.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.date = date) }/></label>
        <label>{t.account}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.account_id.is_none())>{t.no_account}</option>
                {move || uuid_options(account_data.accounts_in(&form().map(|f| f.currency).unwrap_or_default()), move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        <label>{t.category}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                <option value="">{t.no_category}</option>
                {uuid_options(categories, move || form().and_then(|f| f.category_id))}
            </select>
        </label>
        <label>{t.participants}</label>
        {contacts.is_empty().then(|| view! { <p class="muted small">{t.no_contacts}</p> })}
        <div class="chips">
            {contacts.into_iter().map(|contact| {
                let id = contact.meta.id;
                view! {
                    <button class="chip" class:active=move || form().is_some_and(|f| f.contacts.contains(&id))
                        on:click=move |_| edit(&|f| if f.contacts.contains(&id) { f.contacts.retain(|c| *c != id) } else { f.contacts.push(id) })>
                        {contact.name.clone()}
                    </button>
                }
            }).collect_view()}
        </div>
        <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.include_me)
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.include_me = v) }/>{t.include_me}</label>
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
pub fn RecurringDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Recurring(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut RecurringForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Recurring(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let kind = Memo::new(move |_| form().map_or(TransactionKind::Expense, |f| f.kind));
    let accounts = account_options(&data);
    let to_accounts = accounts.clone();
    let category_data = data.clone();
    let select_data = data.clone();
    let save_data = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&save_data, lang) {
            Ok(rule) => {
                close(state);
                state.save(Item::Rule(rule));
            }
            Err(problem) => report(state, problem, lang),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|rule| {
        let label = caudal_app_state::recurring::rule_title(&data, &rule, lang);
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::RecurringRules,
                id: rule.meta.id,
                label: label.clone(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit_recurring } else { t.new_recurring }}</h2>
        <div class="chips">
            {FORM_KINDS.into_iter().map(|option| view! {
                <button class="chip" class:active=move || kind.get() == option on:click=move |_| edit(&|f| f.kind = option)>{t.transaction_kind(option)}</button>
            }).collect_view()}
        </div>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <label>{t.amount}</label>
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || form().map(|f| f.amount).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/>
            <input class="currency" prop:value=move || form().map(|f| f.currency).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.account}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); let data = select_data.clone(); edit(&|f| f.select_account(&data, value)) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.account_id.is_none())>{t.no_account}</option>
                {uuid_options(accounts, move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        {move || if kind.get() == TransactionKind::Transfer {
            let options = to_accounts.clone();
            view! {
                <label>{t.to_account}
                    <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.to_account_id = value) }>
                        <option value="">{t.no_account}</option>
                        {uuid_options(options, move || form().and_then(|f| f.to_account_id))}
                    </select>
                </label>
            }.into_any()
        } else {
            let options = category_data.categories_for(kind.get());
            view! {
                <label>{t.category}
                    <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                        <option value="" prop:selected=move || form().is_some_and(|f| f.category_id.is_none())>{t.no_category}</option>
                        {uuid_options(options, move || form().and_then(|f| f.category_id))}
                    </select>
                </label>
            }.into_any()
        }}
        <label>{t.frequency}</label>
        <div class="chips">
            {FREQUENCIES.into_iter().map(|frequency| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.frequency == frequency)
                    on:click=move |_| edit(&|f| f.frequency = frequency)>{t.frequency_unit(frequency)}</button>
            }).collect_view()}
        </div>
        <label class="inline">{t.interval}<input inputmode="numeric" class="short" prop:value=move || form().map(|f| f.interval).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.interval = v.clone()) }/></label>
        <Show when=move || form().is_some_and(|f| f.frequency == Frequency::Monthly)>
            <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.last_day_of_month)
                on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.last_day_of_month = v) }/>{t.last_day}</label>
        </Show>
        <label>{t.started_on}<input type="date" prop:value=move || form().map(|f| f.starts_on.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.starts_on = date) }/></label>
        <label>{t.ends_on}<input type="date" prop:value=move || form().and_then(|f| f.ends_on).map(|d| d.to_string()).unwrap_or_default()
            on:change=move |ev| { let value = event_target_value(&ev).parse().ok(); edit(&|f| f.ends_on = value) }/></label>
        <div class="chips">
            {[(RecurringMode::Confirm, t.mode_confirm), (RecurringMode::Auto, t.mode_auto)].into_iter().map(|(mode, label)| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.mode == mode) on:click=move |_| edit(&|f| f.mode = mode)>{label}</button>
            }).collect_view()}
        </div>
        <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.active)
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.active = v) }/>{t.active}</label>
        {dialog_buttons(state, lang, save, delete)}
    }
}
