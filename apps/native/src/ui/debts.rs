//! Deudas y periódicos: páginas y diálogos.

use super::dialogs::{Dialog, Outcome, buttons, title};
use super::widgets::{
    self, amount_field, card, date_field, error_label, field_label, heading, muted, text_field,
    tone_color, transaction_row, uuid_combo,
};
use crate::app::CaudalApp;
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::debts::{ContactForm, DebtForm, PaymentForm, SharedExpenseForm, debts_view};
use caudal_app_state::i18n::{DIRECTIONS, FORM_KINDS, FREQUENCIES};
use caudal_app_state::model::{Frequency, RecurringMode, TransactionKind};
use caudal_app_state::recurring::{RecurringForm, recurring_view, upcoming};
use caudal_app_state::views::Tone;
use caudal_app_state::{Data, Lang, Table, texts};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use uuid::Uuid;

fn account_options(data: &Data, keep: Option<Uuid>) -> Vec<(Uuid, String)> {
    data.selectable_accounts(keep)
        .into_iter()
        .map(|account| {
            (
                account.meta.id,
                format!("{} ({})", account.name, account.currency),
            )
        })
        .collect()
}

fn contact_options(data: &Data) -> Vec<(Uuid, String)> {
    data.contacts
        .iter()
        .map(|contact| (contact.meta.id, contact.name.clone()))
        .collect()
}

pub fn debts(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    let data = app.data.clone();
    let key = app.key();
    let view = app
        .cache
        .debts
        .get(key, || debts_view(&data, today(), lang));
    heading(ui, t.debts);
    ui.horizontal_wrapped(|ui| {
        if ui.button(format!("+ {}", t.new_debt)).clicked() {
            app.dialog = Some(Dialog::Debt {
                form: Box::new(DebtForm::new(&data, today())),
                error: None,
            });
        }
        if ui.button(format!("+ {}", t.shared_expense)).clicked() {
            app.dialog = Some(Dialog::Shared {
                form: Box::new(SharedExpenseForm::new(&data, today())),
                error: None,
            });
        }
        if ui.button(format!("+ {}", t.new_contact)).clicked() {
            app.dialog = Some(Dialog::Contact {
                form: Box::new(ContactForm::default()),
                error: None,
            });
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.columns(2, |columns| {
            for (column, label, totals, tone) in [
                (0, t.owed_to_me, &view.owed_to_me, Tone::Positive),
                (1, t.i_owe, &view.i_owe, Tone::Negative),
            ] {
                let ui = &mut columns[column];
                ui.label(muted(ui, label).small());
                if totals.is_empty() {
                    ui.label("—");
                }
                for total in totals {
                    ui.label(RichText::new(total).strong().color(tone_color(ui, tone)));
                }
            }
        });
    });
    ui.add_space(8.0);

    if view.open.is_empty() && view.settled.is_empty() {
        ui.label(muted(ui, t.no_debts));
    }
    for debt in view.open.iter().chain(view.settled.iter()) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&debt.title).strong());
                    let mut detail = debt.direction_label.to_owned();
                    if debt.settled {
                        detail = format!("{detail} · {}", t.settled);
                    }
                    ui.label(muted(ui, detail).small());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&debt.outstanding_text)
                            .size(17.0)
                            .strong()
                            .color(tone_color(ui, debt.tone)),
                    );
                });
            });
            ui.add(egui::ProgressBar::new(debt.progress).desired_height(6.0));
            ui.label(muted(ui, &debt.detail).small());
            if let Some(next) = &debt.next_due_text {
                let color = if debt.overdue {
                    tone_color(ui, Tone::Negative)
                } else {
                    ui.visuals().text_color()
                };
                let label = if debt.overdue {
                    format!("{} · {next}", t.overdue)
                } else {
                    next.clone()
                };
                ui.label(RichText::new(label).small().color(color));
            }
            if !debt.installments.is_empty() {
                egui::CollapsingHeader::new(format!(
                    "{} ({})",
                    t.installments,
                    debt.installments.len()
                ))
                .id_salt(debt.id)
                .show(ui, |ui| {
                    for item in &debt.installments {
                        ui.horizontal(|ui| {
                            ui.label(&item.due_text);
                            ui.label(&item.amount_text);
                            let (label, tone) = if item.settled {
                                (t.paid, Tone::Positive)
                            } else if item.overdue {
                                (t.overdue, Tone::Negative)
                            } else {
                                (t.outstanding, Tone::Neutral)
                            };
                            ui.label(RichText::new(label).small().color(tone_color(ui, tone)));
                        });
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if !debt.settled
                    && ui.small_button(t.register_payment).clicked()
                    && let Some(model) = data.debt(debt.id)
                {
                    app.dialog = Some(Dialog::Payment {
                        form: Box::new(PaymentForm::new(&data, model, today(), lang)),
                        error: None,
                    });
                }
                if !debt.settled
                    && ui.small_button(t.simulate).clicked()
                    && let Some(model) = data.debt(debt.id)
                {
                    app.dialog = Some(Dialog::Simulate {
                        debt: Box::new(model.clone()),
                        payment: String::new(),
                        result: None,
                    });
                }
                if ui.small_button(t.edit).clicked()
                    && let Some(model) = data.debt(debt.id)
                {
                    app.dialog = Some(Dialog::Debt {
                        form: Box::new(DebtForm::edit(model, today(), lang)),
                        error: None,
                    });
                }
                if ui.small_button(t.delete).clicked() {
                    app.dialog = Some(Dialog::ConfirmDelete {
                        table: Table::Debts,
                        id: debt.id,
                        label: debt.title.clone(),
                    });
                }
            });
        });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    heading(ui, t.contacts);
    card(ui, |ui| {
        if view.contacts.is_empty() {
            ui.label(muted(ui, t.no_contacts));
        }
        for contact in &view.contacts {
            ui.horizontal(|ui| {
                ui.label(&contact.name);
                ui.label(
                    RichText::new(&contact.balance_text)
                        .small()
                        .color(tone_color(ui, contact.tone)),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t.delete).clicked() {
                        app.dialog = Some(Dialog::ConfirmDelete {
                            table: Table::Contacts,
                            id: contact.id,
                            label: contact.name.clone(),
                        });
                    }
                    if ui.small_button(t.edit).clicked()
                        && let Some(model) = data.contact(contact.id)
                    {
                        app.dialog = Some(Dialog::Contact {
                            form: Box::new(ContactForm::edit(model)),
                            error: None,
                        });
                    }
                });
            });
        }
    });
}

pub fn recurring(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    let data = app.data.clone();
    let key = app.key();
    let view = app
        .cache
        .recurring
        .get(key, || recurring_view(&data, today(), lang));
    heading(ui, t.recurring);
    if ui.button(format!("+ {}", t.new_recurring)).clicked() {
        app.dialog = Some(Dialog::Recurring {
            form: Box::new(RecurringForm::new(&data, today())),
            error: None,
        });
    }
    ui.add_space(8.0);

    pending_card(app, ui, &view.pending);

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(t.fixed_monthly).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for total in view.monthly_totals.iter().rev() {
                    ui.label(
                        RichText::new(total)
                            .strong()
                            .color(tone_color(ui, Tone::Negative)),
                    );
                }
            });
        });
        ui.separator();
        if view.rules.is_empty() {
            ui.label(muted(ui, t.no_recurring));
        }
        for rule in &view.rules {
            let response = ui
                .horizontal(|ui| {
                    ui.vertical(|ui| {
                        let mut name = RichText::new(&rule.title).strong();
                        if !rule.active {
                            name = name.italics();
                        }
                        ui.label(name);
                        let mut detail = format!("{} · {}", rule.frequency_text, rule.mode_label);
                        if !rule.active {
                            detail = format!("{} · {detail}", t.paused);
                        }
                        ui.label(muted(ui, detail).small());
                        if let Some(next) = &rule.next_text {
                            ui.label(muted(ui, next).small());
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                                ui.label(
                                    RichText::new(&rule.amount_text)
                                        .strong()
                                        .color(tone_color(ui, rule.tone)),
                                );
                                ui.label(muted(ui, &rule.monthly_text).small());
                            });
                        });
                    });
                })
                .response
                .interact(egui::Sense::click());
            if response.clicked()
                && let Some(model) = data.rule(rule.id)
            {
                app.dialog = Some(Dialog::Recurring {
                    form: Box::new(RecurringForm::edit(model, lang)),
                    error: None,
                });
            }
            ui.separator();
        }
    });
    ui.add_space(8.0);
    upcoming_card(app, ui, 30, usize::MAX);
}

/// Movimientos generados que esperan confirmación.
pub fn pending_card(
    app: &mut CaudalApp,
    ui: &mut Ui,
    pending: &[caudal_app_state::views::TransactionView],
) {
    if pending.is_empty() {
        return;
    }
    let t = texts(app.lang);
    let lang = app.lang;
    card(ui, |ui| {
        ui.label(
            RichText::new(format!("{} ({})", t.to_confirm, pending.len()))
                .strong()
                .color(widgets::warning_color(ui)),
        );
        for tx in pending {
            // Botones debajo de la fila: al lado no caben en un teléfono.
            if transaction_row(ui, tx, lang).clicked()
                && let Some(model) = app.data.transaction(tx.id)
            {
                app.dialog = Some(Dialog::Transaction {
                    form: Box::new(caudal_app_state::forms::TransactionForm::edit(model, lang)),
                    error: None,
                });
            }
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                if ui.small_button(t.confirm).clicked() {
                    app.send(Command::Save(Box::new(Item::Confirm(tx.id))));
                }
                if ui.small_button(t.skip).clicked() {
                    app.send(Command::Delete {
                        table: Table::Transactions,
                        id: tx.id,
                    });
                }
            });
        }
    });
    ui.add_space(8.0);
}

pub fn upcoming_card(app: &mut CaudalApp, ui: &mut Ui, days: u64, max_items: usize) {
    let t = texts(app.lang);
    let key = (app.key(), days);
    let lang = app.lang;
    let calendar = app
        .cache
        .upcoming
        .get(key, || upcoming(&app.data, today(), days, lang));
    card(ui, |ui| {
        ui.label(RichText::new(t.upcoming).strong());
        if calendar.is_empty() {
            ui.label(muted(ui, t.nothing_upcoming));
        }
        let mut shown = 0;
        'days: for day in &calendar {
            ui.add_space(4.0);
            ui.label(muted(ui, &day.date_text).small());
            for item in &day.items {
                if shown == max_items {
                    break 'days;
                }
                shown += 1;
                ui.horizontal(|ui| {
                    ui.label(&item.title);
                    let color = if item.detail == t.overdue {
                        tone_color(ui, Tone::Negative)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.label(RichText::new(item.detail).small().color(color));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(&item.amount_text).color(tone_color(ui, item.tone)));
                    });
                });
            }
        }
    });
}

fn save_or_error<T>(
    result: Result<T, caudal_app_state::forms::FormError>,
    error: &mut Option<String>,
    lang: Lang,
    item: impl FnOnce(T) -> Item,
) -> Outcome {
    match result {
        Ok(value) => Outcome::Send(Command::Save(Box::new(item(value)))),
        Err(problem) => {
            *error = Some(problem.message(texts(lang)).into());
            Outcome::Keep
        }
    }
}

pub fn contact_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut ContactForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(
        ui,
        if form.original.is_some() {
            t.edit
        } else {
            t.new_contact
        },
    );
    field_label(ui, t.name);
    text_field(ui, &mut form.name, "");
    field_label(ui, t.phone);
    text_field(ui, &mut form.phone, "");
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return save_or_error(form.validate(data.user_id), error, lang, Item::Contact);
    }
    Outcome::Keep
}

pub fn debt_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut DebtForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let creating = form.original.is_none();
    title(ui, if creating { t.new_debt } else { t.edit_debt });
    ui.horizontal(|ui| {
        for direction in DIRECTIONS {
            ui.selectable_value(&mut form.direction, direction, t.direction(direction));
        }
    });
    field_label(ui, t.contact);
    uuid_combo(
        ui,
        "debt-contact",
        &mut form.contact_id,
        Some(t.none),
        &contact_options(data),
    );
    if form.contact_id.is_none() {
        text_field(ui, &mut form.new_contact, t.new_contact_name);
    }
    field_label(ui, t.principal);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.principal, "0", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    field_label(ui, t.description);
    text_field(ui, &mut form.description, "");
    field_label(ui, t.started_on);
    date_field(ui, "debt-start", &mut form.started_on, lang, today());
    if creating {
        field_label(ui, t.installments_count);
        amount_field(ui, &mut form.installments, "", false);
        if !form.installments.trim().is_empty() {
            field_label(ui, t.first_installment);
            date_field(ui, "debt-first", &mut form.first_installment, lang, today());
        }
    }
    if form.installments.trim().is_empty() {
        let mut has_due = form.due_on.is_some();
        if ui.checkbox(&mut has_due, t.due_on).changed() {
            form.due_on = has_due.then(today);
        }
        if let Some(due) = form.due_on.as_mut() {
            date_field(ui, "debt-due", due, lang, today());
        }
        field_label(ui, t.interest_rate);
        amount_field(ui, &mut form.interest_rate, "0", false);
    }
    if creating {
        field_label(ui, t.record_movement);
        uuid_combo(
            ui,
            "debt-account",
            &mut form.account_id,
            Some(t.no_account),
            &data.accounts_in(&form.currency),
        );
    } else {
        ui.checkbox(&mut form.closed, t.close_debt);
    }
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return save_or_error(form.validate(data, lang), error, lang, Item::Debt);
    }
    Outcome::Keep
}

pub fn payment_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut PaymentForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, t.register_payment);
    let who = data
        .contact_name(form.debt.contact_id)
        .map(str::to_owned)
        .or_else(|| form.debt.description.clone())
        .unwrap_or_default();
    ui.label(format!("{who} · {}", t.direction(form.debt.direction)));
    field_label(ui, &format!("{} ({})", t.amount, form.debt.currency));
    amount_field(ui, &mut form.amount, "0", true);
    field_label(ui, t.date);
    date_field(ui, "payment-date", &mut form.date, lang, today());
    field_label(ui, t.record_movement);
    uuid_combo(
        ui,
        "payment-account",
        &mut form.account_id,
        Some(t.no_account),
        &data.accounts_in(form.debt.currency.as_str()),
    );
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return save_or_error(
            form.validate(data, lang),
            error,
            lang,
            |(payment, movement)| Item::Payment(payment, movement),
        );
    }
    Outcome::Keep
}

pub fn shared_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut SharedExpenseForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, t.shared_expense);
    field_label(ui, t.amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.amount, "0", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    field_label(ui, t.description);
    text_field(ui, &mut form.description, "");
    field_label(ui, t.date);
    date_field(ui, "shared-date", &mut form.date, lang, today());
    field_label(ui, t.account);
    uuid_combo(
        ui,
        "shared-account",
        &mut form.account_id,
        Some(t.no_account),
        &data.accounts_in(&form.currency),
    );
    field_label(ui, t.category);
    uuid_combo(
        ui,
        "shared-category",
        &mut form.category_id,
        Some(t.no_category),
        &data.categories_for(TransactionKind::Expense),
    );
    field_label(ui, t.participants);
    if data.contacts.is_empty() {
        ui.label(muted(ui, t.no_contacts));
    }
    ui.horizontal_wrapped(|ui| {
        for contact in &data.contacts {
            let mut selected = form.contacts.contains(&contact.meta.id);
            if ui.toggle_value(&mut selected, &contact.name).changed() {
                if selected {
                    form.contacts.push(contact.meta.id);
                } else {
                    form.contacts.retain(|id| *id != contact.meta.id);
                }
            }
        }
    });
    ui.checkbox(&mut form.include_me, t.include_me);
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        let result = form.validate(data, lang).map_err(|problem| match problem {
            caudal_app_state::forms::FormError::NameRequired => {
                *error = Some(t.participants.into());
                problem
            }
            other => other,
        });
        if result.is_err() && error.is_some() {
            return Outcome::Keep;
        }
        return save_or_error(result, error, lang, Item::Shared);
    }
    Outcome::Keep
}

pub fn recurring_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut RecurringForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_recurring
        } else {
            t.new_recurring
        },
    );
    ui.horizontal(|ui| {
        for kind in FORM_KINDS {
            ui.selectable_value(&mut form.kind, kind, t.transaction_kind(kind));
        }
    });
    field_label(ui, t.name);
    text_field(ui, &mut form.name, "");
    field_label(ui, t.amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.amount, "0", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    field_label(ui, t.account);
    let accounts = account_options(data, form.account_id.or(form.to_account_id));
    let mut account = form.account_id;
    if uuid_combo(
        ui,
        "rule-account",
        &mut account,
        Some(t.no_account),
        &accounts,
    ) {
        form.select_account(data, account);
    }
    if form.kind == TransactionKind::Transfer {
        field_label(ui, t.to_account);
        uuid_combo(
            ui,
            "rule-to",
            &mut form.to_account_id,
            Some(t.no_account),
            &accounts,
        );
    } else {
        field_label(ui, t.category);
        uuid_combo(
            ui,
            "rule-category",
            &mut form.category_id,
            Some(t.no_category),
            &data.categories_for(form.kind),
        );
    }
    field_label(ui, t.frequency);
    ui.horizontal_wrapped(|ui| {
        for frequency in FREQUENCIES {
            ui.selectable_value(&mut form.frequency, frequency, t.frequency_unit(frequency));
        }
    });
    ui.horizontal(|ui| {
        ui.label(muted(ui, t.interval).small());
        ui.add_sized([60.0, 28.0], egui::TextEdit::singleline(&mut form.interval));
    });
    if form.frequency == Frequency::Monthly {
        ui.checkbox(&mut form.last_day_of_month, t.last_day);
    }
    field_label(ui, t.started_on);
    date_field(ui, "rule-start", &mut form.starts_on, lang, today());
    let mut has_end = form.ends_on.is_some();
    if ui.checkbox(&mut has_end, t.has_end).changed() {
        form.ends_on = has_end.then_some(form.starts_on);
    }
    if let Some(ends) = form.ends_on.as_mut() {
        date_field(ui, "rule-end", ends, lang, today());
    }
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut form.mode, RecurringMode::Confirm, t.mode_confirm);
        ui.selectable_value(&mut form.mode, RecurringMode::Auto, t.mode_auto);
    });
    ui.checkbox(&mut form.active, t.active);
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::RecurringRules,
            id: original.meta.id,
            label: caudal_app_state::recurring::rule_title(data, &original, lang),
        });
    }
    if save {
        return save_or_error(form.validate(data, lang), error, lang, Item::Rule);
    }
    Outcome::Keep
}
