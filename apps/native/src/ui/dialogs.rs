//! Ventanas modales: formularios y confirmaciones.

use super::pages::kind_options;
use super::widgets::{
    self, amount_field, date_field, error_label, field_label, text_field, uuid_combo,
};
use crate::app::CaudalApp;
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::forms::{
    AccountForm, CategoryForm, TagForm, TransactionForm, balance_adjustment,
};
use caudal_app_state::i18n::{ACCOUNT_KINDS, CATEGORY_KINDS};
use caudal_app_state::model::TransactionKind;
use caudal_app_state::{Data, Lang, Table, texts, views};
use eframe::egui::{self, RichText, Ui};
use uuid::Uuid;

pub enum Dialog {
    Transaction {
        form: Box<TransactionForm>,
        error: Option<String>,
    },
    Account {
        form: Box<AccountForm>,
        error: Option<String>,
    },
    Category {
        form: Box<CategoryForm>,
        error: Option<String>,
    },
    Tag {
        form: TagForm,
        error: Option<String>,
    },
    Adjust {
        account_id: Uuid,
        value: String,
        error: Option<String>,
    },
    ConfirmDelete {
        table: Table,
        id: Uuid,
        label: String,
    },
    ConfirmSignOut,
    Contact {
        form: Box<caudal_app_state::debts::ContactForm>,
        error: Option<String>,
    },
    Debt {
        form: Box<caudal_app_state::debts::DebtForm>,
        error: Option<String>,
    },
    Payment {
        form: Box<caudal_app_state::debts::PaymentForm>,
        error: Option<String>,
    },
    Shared {
        form: Box<caudal_app_state::debts::SharedExpenseForm>,
        error: Option<String>,
    },
    Recurring {
        form: Box<caudal_app_state::recurring::RecurringForm>,
        error: Option<String>,
    },
    Budget {
        form: Box<caudal_app_state::planning::BudgetForm>,
        error: Option<String>,
    },
    Goal {
        form: Box<caudal_app_state::planning::GoalForm>,
        error: Option<String>,
    },
    Contribution {
        form: Box<caudal_app_state::planning::ContributionForm>,
        error: Option<String>,
    },
    Asset {
        form: Box<caudal_app_state::investments::AssetForm>,
        query: String,
        error: Option<String>,
    },
    Trade {
        form: Box<caudal_app_state::investments::TradeForm>,
        error: Option<String>,
    },
    Alert {
        form: Box<caudal_app_state::investments::AlertForm>,
        error: Option<String>,
    },
    ConfirmDeleteAll,
    ConfirmDeleteAccount {
        final_step: bool,
    },
    SetPin {
        pin: String,
        error: Option<String>,
    },
    Rule {
        form: Box<caudal_app_state::rules::RuleForm>,
        error: Option<String>,
    },
    Template {
        form: Box<caudal_app_state::rules::TemplateForm>,
        error: Option<String>,
    },
    Import(Box<super::data::ImportState>),
    Attachment {
        attachment: Box<caudal_app_state::model::Attachment>,
        bytes: Option<Vec<u8>>,
        loaded: bool,
    },
    Simulate {
        debt: Box<caudal_app_state::model::Debt>,
        payment: String,
        result: Option<String>,
    },
}

pub(crate) enum Outcome {
    Keep,
    Close,
    Send(Command),
    /// Cierra este diálogo y abre otro.
    Replace(Dialog),
    /// Envía y abre otro diálogo (p. ej. pedir un archivo y mostrarlo).
    SendAndReplace(Command, Dialog),
    /// Envía sin cerrar el diálogo.
    SendAndKeep(Command),
    /// PIN nuevo para este dispositivo.
    Pin {
        hash: String,
        salt: String,
    },
}

pub fn show(app: &mut CaudalApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.dialog.take() else {
        return;
    };
    let lang = app.lang;
    let data = app.data.clone();
    let pending = app.pending_attachments.clone();
    let width = (ctx.content_rect().width() - 32.0).clamp(260.0, 460.0);
    // Arriba y sin pasar del teclado táctil, que en Android ocupa la parte inferior.
    let insets = crate::android::safe_area(ctx);
    let visible = (ctx.content_rect().height() - insets.top - insets.bottom).max(200.0);
    let id = egui::Id::unique("dialog");
    let area = egui::Modal::default_area(id)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, insets.top + 16.0));
    let response = egui::Modal::new(id).area(area).show(ctx, |ui| {
        ui.set_width(width);
        egui::ScrollArea::vertical()
            .max_height(visible - 60.0)
            // Sin esto, el alto se calcula con el tamaño del fotograma anterior y recorta el final.
            .min_scrolled_height(visible - 60.0)
            .show(ui, |ui| match &mut dialog {
                Dialog::Transaction { form, error } => {
                    transaction(ui, &data, form, error, &pending, lang)
                }
                Dialog::Account { form, error } => account(ui, &data, form, error, lang),
                Dialog::Category { form, error } => category(ui, &data, form, error, lang),
                Dialog::Tag { form, error } => tag(ui, &data, form, error, lang),
                Dialog::Adjust {
                    account_id,
                    value,
                    error,
                } => adjust(ui, &data, *account_id, value, error, lang),
                Dialog::ConfirmDelete { table, id, label } => {
                    confirm_delete(ui, *table, *id, label, lang)
                }
                Dialog::ConfirmSignOut => confirm_sign_out(ui, lang),
                Dialog::Contact { form, error } => {
                    super::debts::contact_dialog(ui, &data, form, error, lang)
                }
                Dialog::Debt { form, error } => {
                    super::debts::debt_dialog(ui, &data, form, error, lang)
                }
                Dialog::Payment { form, error } => {
                    super::debts::payment_dialog(ui, &data, form, error, lang)
                }
                Dialog::Shared { form, error } => {
                    super::debts::shared_dialog(ui, &data, form, error, lang)
                }
                Dialog::Recurring { form, error } => {
                    super::debts::recurring_dialog(ui, &data, form, error, lang)
                }
                Dialog::Budget { form, error } => {
                    super::reports::budget_dialog(ui, &data, form, error, lang)
                }
                Dialog::Goal { form, error } => {
                    super::reports::goal_dialog(ui, &data, form, error, lang)
                }
                Dialog::Contribution { form, error } => {
                    super::reports::contribution_dialog(ui, &data, form, error, lang)
                }
                Dialog::Asset { form, query, error } => {
                    super::invest::asset_dialog(ui, &data, form, query, error, lang)
                }
                Dialog::Trade { form, error } => {
                    super::invest::trade_dialog(ui, &data, form, error, lang)
                }
                Dialog::Alert { form, error } => {
                    super::invest::alert_dialog(ui, &data, form, error, lang)
                }
                Dialog::ConfirmDeleteAll => super::data::confirm_delete_all(ui, lang),
                Dialog::ConfirmDeleteAccount { final_step } => {
                    super::data::confirm_delete_account_dialog(ui, *final_step, lang)
                }
                Dialog::SetPin { pin, error } => super::data::set_pin_dialog(ui, pin, error, lang),
                Dialog::Rule { form, error } => {
                    super::data::rule_dialog(ui, &data, form, error, lang)
                }
                Dialog::Template { form, error } => {
                    super::data::template_dialog(ui, &data, form, error, lang)
                }
                Dialog::Import(state) => super::data::import_dialog(ui, &data, state, lang),
                Dialog::Attachment {
                    attachment,
                    bytes,
                    loaded,
                } => super::data::attachment_dialog(ui, attachment, bytes, *loaded, lang),
                Dialog::Simulate {
                    debt,
                    payment,
                    result,
                } => super::reports::simulate_dialog(ui, &data, debt, payment, result, lang),
            })
            .inner
    });
    let outcome = if response.should_close() {
        Outcome::Close
    } else {
        response.inner
    };
    match outcome {
        Outcome::Keep => app.dialog = Some(dialog),
        Outcome::Close => {}
        Outcome::Send(command) => app.send(command),
        Outcome::Replace(next) => app.dialog = Some(next),
        Outcome::SendAndReplace(command, next) => {
            app.send(command);
            app.dialog = Some(next);
        }
        Outcome::SendAndKeep(command) => {
            app.send(command);
            app.dialog = Some(dialog);
        }
        Outcome::Pin { hash, salt } => {
            app.prefs.pin_hash = Some(hash);
            app.prefs.pin_salt = Some(salt);
            app.prefs.save(&app.platform);
        }
    }
}

pub(crate) fn buttons(ui: &mut Ui, lang: Lang, delete: bool) -> (bool, bool, bool) {
    let texts = texts(lang);
    let mut result = (false, false, false);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        result.0 = ui
            .add(
                egui::Button::new(RichText::new(texts.save).strong())
                    .fill(widgets::ACCENT.linear_multiply(0.6))
                    .min_size(egui::vec2(110.0, 32.0)),
            )
            .clicked();
        result.1 = ui.button(texts.cancel).clicked();
        if delete {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                result.2 = ui
                    .button(
                        RichText::new(texts.delete)
                            .color(widgets::tone_color(ui, views::Tone::Negative)),
                    )
                    .clicked();
            });
        }
    });
    ui.add_space(10.0);
    result
}

pub(crate) fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(18.0).strong());
    ui.add_space(4.0);
}

fn transaction(
    ui: &mut Ui,
    data: &Data,
    form: &mut TransactionForm,
    error: &mut Option<String>,
    pending: &std::collections::BTreeSet<Uuid>,
    lang: Lang,
) -> Outcome {
    let texts = texts(lang);
    let editing = form.original.is_some();
    title(
        ui,
        if editing {
            texts.edit_transaction
        } else {
            texts.new_transaction
        },
    );

    // Plantillas: un toque llena el formulario.
    if !editing {
        let templates = caudal_app_state::rules::templates_view(data, lang);
        if !templates.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for template in templates {
                    if ui
                        .small_button(&template.name)
                        .on_hover_text(&template.detail)
                        .clicked()
                        && let Some(model) =
                            data.templates.iter().find(|m| m.meta.id == template.id)
                    {
                        *form = TransactionForm::from_template(model, data, today(), lang);
                    }
                }
            });
        }
    }

    if form.kind == TransactionKind::Adjustment {
        ui.label(widgets::muted(ui, texts.transaction_kind(form.kind)));
    } else {
        ui.horizontal(|ui| {
            for (kind, label) in kind_options(lang) {
                ui.selectable_value(&mut form.kind, kind, label);
            }
        });
    }

    field_label(ui, texts.amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 36.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.amount, "0", true)
        });
        ui.add_sized(
            [80.0, 36.0],
            egui::TextEdit::singleline(&mut form.currency).hint_text(texts.currency),
        );
    });

    field_label(ui, texts.date);
    date_field(ui, "tx-date", &mut form.date, lang, today());

    let accounts: Vec<(Uuid, String)> = data
        .selectable_accounts(form.account_id.or(form.to_account_id))
        .into_iter()
        .map(|account| {
            (
                account.meta.id,
                format!("{} ({})", account.name, account.currency),
            )
        })
        .collect();
    field_label(ui, texts.account);
    let mut account_id = form.account_id;
    if uuid_combo(
        ui,
        "tx-account",
        &mut account_id,
        Some(texts.no_account),
        &accounts,
    ) {
        form.select_account(data, account_id);
    }
    if form.kind == TransactionKind::Transfer {
        field_label(ui, texts.to_account);
        uuid_combo(
            ui,
            "tx-to-account",
            &mut form.to_account_id,
            Some(texts.no_account),
            &accounts,
        );
        if form.needs_received_amount(data) {
            field_label(ui, texts.received_amount);
            amount_field(ui, &mut form.received_amount, "0", false);
        }
    } else if form.kind != TransactionKind::Adjustment {
        field_label(ui, texts.category);
        let categories = data.categories_for(form.kind);
        uuid_combo(
            ui,
            "tx-category",
            &mut form.category_id,
            Some(texts.no_category),
            &categories,
        );
    }

    let has_details =
        !form.description.is_empty() || !form.note.is_empty() || !form.tag_ids.is_empty();
    egui::CollapsingHeader::new(texts.optional_details)
        .default_open(has_details)
        .show(ui, |ui| {
            field_label(ui, texts.description);
            if text_field(ui, &mut form.description, "").changed() {
                form.apply_rules(data);
            }
            if !data.tags.is_empty() {
                field_label(ui, texts.tags);
                ui.horizontal_wrapped(|ui| {
                    for tag in &data.tags {
                        let mut selected = form.tag_ids.contains(&tag.meta.id);
                        if ui
                            .toggle_value(&mut selected, format!("#{}", tag.name))
                            .changed()
                        {
                            if selected {
                                form.tag_ids.push(tag.meta.id);
                            } else {
                                form.tag_ids.retain(|id| *id != tag.meta.id);
                            }
                        }
                    }
                });
            }
            field_label(ui, texts.note);
            ui.add(
                egui::TextEdit::multiline(&mut form.note)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
        });

    let mut extra = None;
    if let Some(original) = &form.original {
        extra = super::data::attachments_section(ui, data, original.meta.id, pending, lang);
    }
    if !form.description.trim().is_empty() && ui.small_button(texts.save_as_template).clicked() {
        match caudal_app_state::rules::TemplateForm::from_transaction(form).validate(data, lang) {
            Ok(template) => {
                extra = Some(Outcome::SendAndKeep(Command::Save(Box::new(
                    Item::Template(template),
                ))));
            }
            Err(problem) => *error = Some(problem.message(texts).into()),
        }
    }
    if let Some(outcome) = extra {
        return outcome;
    }

    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing);
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = &form.original {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::Transactions,
            id: original.meta.id,
            label: views::transaction_view(data, original, lang).title,
        });
    }
    if save || ui.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.ctrl) {
        return match form.validate(data, data.user_id, lang) {
            Ok(tx) => Outcome::Send(Command::Save(Box::new(Item::Transaction(tx)))),
            Err(problem) => {
                *error = Some(problem.message(texts).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn account(
    ui: &mut Ui,
    data: &Data,
    form: &mut AccountForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let texts = texts(lang);
    let editing = form.original.is_some();
    title(
        ui,
        if editing {
            texts.edit
        } else {
            texts.new_account
        },
    );
    field_label(ui, texts.name);
    text_field(ui, &mut form.name, "");
    field_label(ui, texts.kind);
    ui.horizontal_wrapped(|ui| {
        for kind in ACCOUNT_KINDS {
            ui.selectable_value(&mut form.kind, kind, texts.account_kind(kind));
        }
    });
    field_label(ui, texts.currency);
    text_field(ui, &mut form.currency, "USD");
    field_label(ui, texts.opening_balance);
    amount_field(ui, &mut form.opening_balance, "0", false);
    field_label(ui, texts.note);
    text_field(ui, &mut form.note, "");
    if editing {
        ui.checkbox(&mut form.archived, texts.archived);
    }
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return match form.validate(data.user_id, lang) {
            Ok(model) => Outcome::Send(Command::Save(Box::new(Item::Account(model)))),
            Err(problem) => {
                *error = Some(problem.message(texts).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn category(
    ui: &mut Ui,
    data: &Data,
    form: &mut CategoryForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let texts = texts(lang);
    let editing = form.original.is_some();
    title(
        ui,
        if editing {
            texts.edit
        } else {
            texts.new_category
        },
    );
    field_label(ui, texts.name);
    text_field(ui, &mut form.name, "");
    field_label(ui, texts.kind);
    ui.horizontal(|ui| {
        for kind in CATEGORY_KINDS {
            ui.selectable_value(&mut form.kind, kind, texts.category_kind(kind));
        }
    });
    field_label(ui, texts.parent_category);
    let own = form.original.as_ref().map(|original| original.meta.id);
    let parents: Vec<(Uuid, String)> = data
        .categories
        .iter()
        .filter(|category| category.parent_id.is_none() && Some(category.meta.id) != own)
        .map(|category| (category.meta.id, category.name.clone()))
        .collect();
    uuid_combo(
        ui,
        "category-parent",
        &mut form.parent_id,
        Some(texts.none),
        &parents,
    );
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return match form.validate(data.user_id) {
            Ok(model) => Outcome::Send(Command::Save(Box::new(Item::Category(model)))),
            Err(problem) => {
                *error = Some(problem.message(texts).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn tag(
    ui: &mut Ui,
    data: &Data,
    form: &mut TagForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let texts = texts(lang);
    let editing = form.original.is_some();
    title(ui, if editing { texts.edit } else { texts.new_tag });
    field_label(ui, texts.name);
    text_field(ui, &mut form.name, "");
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing);
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = &form.original {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::Tags,
            id: original.meta.id,
            label: original.name.clone(),
        });
    }
    if save {
        return match form.validate(data.user_id) {
            Ok(model) => Outcome::Send(Command::Save(Box::new(Item::Tag(model)))),
            Err(problem) => {
                *error = Some(problem.message(texts).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn adjust(
    ui: &mut Ui,
    data: &Data,
    account_id: Uuid,
    value: &mut String,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let texts = texts(lang);
    let Some(account) = data.account(account_id) else {
        return Outcome::Close;
    };
    title(ui, texts.adjust_balance);
    ui.label(&account.name);
    let current = views::accounts(data, lang, true)
        .into_iter()
        .find(|view| view.id == account_id)
        .map(|view| view.balance)
        .unwrap_or_default();
    field_label(
        ui,
        &format!("{} ({})", texts.real_balance, account.currency),
    );
    amount_field(ui, value, "0", true);
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        return match balance_adjustment(account, current, value, data.user_id, today(), lang) {
            Ok(Some(tx)) => Outcome::Send(Command::Save(Box::new(Item::Transaction(tx)))),
            Ok(None) => Outcome::Close,
            Err(problem) => {
                *error = Some(problem.message(texts).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn confirm_delete(ui: &mut Ui, table: Table, id: Uuid, label: &str, lang: Lang) -> Outcome {
    let texts = texts(lang);
    title(ui, &texts.question(texts.delete));
    ui.label(RichText::new(label).strong());
    ui.label(widgets::muted(ui, texts.trash_hint));
    ui.add_space(8.0);
    let mut outcome = Outcome::Keep;
    ui.horizontal(|ui| {
        if ui
            .button(
                RichText::new(texts.delete)
                    .strong()
                    .color(widgets::tone_color(ui, views::Tone::Negative)),
            )
            .clicked()
        {
            outcome = Outcome::Send(Command::Delete { table, id });
        }
        if ui.button(texts.cancel).clicked() {
            outcome = Outcome::Close;
        }
    });
    outcome
}

fn confirm_sign_out(ui: &mut Ui, lang: Lang) -> Outcome {
    let texts = texts(lang);
    title(ui, &texts.question(texts.sign_out));
    let note = if lang == Lang::Es {
        "Los cambios que aún no se subieron quedan guardados en este dispositivo y se subirán cuando vuelvas a entrar con esta cuenta."
    } else {
        "Changes not yet uploaded stay on this device and will upload when you sign in again with this account."
    };
    ui.label(widgets::muted(ui, note));
    ui.add_space(8.0);
    let mut outcome = Outcome::Keep;
    ui.horizontal(|ui| {
        if ui.button(RichText::new(texts.sign_out).strong()).clicked() {
            outcome = Outcome::Send(Command::SignOut);
        }
        if ui.button(texts.cancel).clicked() {
            outcome = Outcome::Close;
        }
    });
    outcome
}
