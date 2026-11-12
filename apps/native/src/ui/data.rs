//! Datos del usuario: copia completa, CSV, reglas, plantillas, comprobantes,
//! actividad, avisos y privacidad.

use super::dialogs::{Dialog, Outcome, buttons, title};
use super::widgets::{
    self, amount_field, card, error_label, field_label, heading, muted, text_field, tone_color,
    uuid_combo, warning_color,
};
use crate::app::{CaudalApp, Page};
use crate::files::{self, Purpose};
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::csv_io::{
    CsvTable, DateOrder, ImportMapping, ImportPreview, RowStatus, preview,
};
use caudal_app_state::model::{Attachment, TransactionKind};
use caudal_app_state::notices::NoticeTarget;
use caudal_app_state::rules::{RuleForm, TemplateForm, rules_view, templates_view};
use caudal_app_state::views::Tone;
use caudal_app_state::{Data, Lang, Table, privacy, texts};
use eframe::egui::{self, Align, Layout, RichText, Ui};

/// Estado del asistente de importación.
#[derive(Clone)]
pub struct ImportState {
    pub table: CsvTable,
    pub mapping: ImportMapping,
    pub preview: ImportPreview,
    /// Configuración con la que se calculó la vista previa.
    pub computed: ImportMapping,
}

impl ImportState {
    pub fn new(data: &Data, table: CsvTable, lang: Lang) -> Self {
        let mapping = ImportMapping::guess(&table, data);
        let preview = preview(data, &table, &mapping, lang);
        Self {
            computed: mapping.clone(),
            table,
            mapping,
            preview,
        }
    }
}

/// Tarjetas "Tus datos" y "Privacidad" de Ajustes.
pub fn settings_cards(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    card(ui, |ui| {
        ui.label(RichText::new(t.data_section).strong());
        ui.horizontal_wrapped(|ui| {
            if ui.button(t.export_backup).clicked() {
                app.send(Command::ExportBackup);
            }
            if ui.button(t.restore_backup).clicked() {
                files::open(Purpose::RestoreBackup);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(t.export_csv).clicked() {
                let text = caudal_app_state::csv_io::export_transactions(&app.data, lang);
                let name = format!("caudal-movimientos-{}.csv", today());
                files::save(&name, "text/csv", text.as_bytes());
            }
            if ui.button(t.import_csv).clicked() {
                files::open(Purpose::ImportCsv);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(t.rules_title).clicked() {
                app.go(Page::Rules);
            }
            if ui.button(t.activity).clicked() {
                app.go(Page::Activity);
            }
        });
        if ui
            .button(RichText::new(t.delete_all).color(tone_color(ui, Tone::Negative)))
            .clicked()
        {
            app.dialog = Some(Dialog::ConfirmDeleteAll);
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.label(RichText::new(t.privacy).strong());
        let mut private = app.prefs.private;
        if ui.checkbox(&mut private, t.private_mode).changed() {
            app.set_private(private);
        }
        ui.label(muted(ui, t.pin_hint).small());
        ui.horizontal_wrapped(|ui| {
            if app.prefs.pin_hash.is_some() {
                if ui.button(t.lock_now).clicked() {
                    app.locked = true;
                }
                if ui.button(t.remove_pin).clicked() {
                    app.prefs.pin_hash = None;
                    app.prefs.pin_salt = None;
                    app.prefs.save(&app.platform);
                }
            } else if ui.button(t.set_pin).clicked() {
                app.dialog = Some(Dialog::SetPin {
                    pin: String::new(),
                    error: None,
                });
            }
        });
    });
    ui.add_space(8.0);
}

pub fn confirm_delete_all(ui: &mut Ui, lang: Lang) -> Outcome {
    let t = texts(lang);
    title(ui, &t.question(t.delete_all));
    ui.label(muted(ui, t.delete_all_confirm));
    ui.add_space(8.0);
    let mut outcome = Outcome::Keep;
    ui.horizontal(|ui| {
        if ui
            .button(
                RichText::new(t.delete_all)
                    .strong()
                    .color(tone_color(ui, Tone::Negative)),
            )
            .clicked()
        {
            outcome = Outcome::Send(Command::DeleteAllData);
        }
        if ui.button(t.cancel).clicked() {
            outcome = Outcome::Close;
        }
    });
    outcome
}

pub fn confirm_delete_account_dialog(ui: &mut Ui, final_step: bool, lang: Lang) -> Outcome {
    let t = texts(lang);
    title(ui, t.delete_account);
    if final_step {
        ui.label(muted(ui, t.delete_account_final));
        ui.add_space(8.0);
        let mut outcome = Outcome::Keep;
        ui.horizontal(|ui| {
            if ui
                .button(
                    RichText::new(t.delete_account_confirm)
                        .strong()
                        .color(tone_color(ui, Tone::Negative)),
                )
                .clicked()
            {
                outcome = Outcome::Send(Command::DeleteAccount);
            }
            if ui.button(t.cancel).clicked() {
                outcome = Outcome::Close;
            }
        });
        outcome
    } else {
        ui.label(muted(ui, t.delete_account_warning));
        ui.add_space(8.0);
        let mut outcome = Outcome::Keep;
        ui.horizontal_wrapped(|ui| {
            if ui.button(t.export_backup).clicked() {
                outcome = Outcome::SendAndKeep(Command::ExportBackup);
            }
            if ui
                .button(
                    RichText::new(t.delete_account_continue)
                        .strong()
                        .color(tone_color(ui, Tone::Negative)),
                )
                .clicked()
            {
                outcome = Outcome::Replace(Dialog::ConfirmDeleteAccount { final_step: true });
            }
            if ui.button(t.cancel).clicked() {
                outcome = Outcome::Close;
            }
        });
        outcome
    }
}

pub fn set_pin_dialog(
    ui: &mut Ui,
    pin: &mut String,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, t.set_pin);
    ui.label(muted(ui, t.pin_hint));
    let response = ui.add(
        egui::TextEdit::singleline(pin)
            .password(true)
            .hint_text("••••"),
    );
    if response.changed() {
        pin.retain(|c| c.is_ascii_digit());
    }
    error_label(ui, error);
    let (save, cancel, _) = buttons(ui, lang, false);
    if cancel {
        return Outcome::Close;
    }
    if save {
        if !privacy::valid_pin(pin) {
            *error = Some(t.pin_invalid.into());
            return Outcome::Keep;
        }
        let salt = privacy::new_salt();
        return Outcome::Pin {
            hash: privacy::hash_pin(pin, &salt),
            salt,
        };
    }
    Outcome::Keep
}

/// Pantalla de bloqueo: el resto de la app no se dibuja hasta el PIN correcto.
pub fn lock_screen(app: &mut CaudalApp, ui: &mut Ui) {
    let t = texts(app.lang);
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.25);
        ui.label(
            RichText::new(t.app_name)
                .strong()
                .size(26.0)
                .color(widgets::ACCENT),
        );
        ui.add_space(12.0);
        ui.label(t.enter_pin);
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.pin_entry)
                .password(true)
                .desired_width(160.0)
                .align(egui::Align2::CENTER_CENTER),
        );
        response.request_focus();
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button(t.unlock).clicked() || enter)
            && let (Some(hash), Some(salt)) = (&app.prefs.pin_hash, &app.prefs.pin_salt)
        {
            if privacy::verify_pin(&app.pin_entry, salt, hash) {
                app.locked = false;
                app.pin_error = false;
            } else {
                app.pin_error = true;
            }
            app.pin_entry.clear();
        }
        if app.pin_error {
            ui.label(RichText::new(t.wrong_pin).color(tone_color(ui, Tone::Negative)));
        }
        ui.add_space(16.0);
        ui.label(muted(ui, t.forgot_pin).small());
        if ui.small_button(t.sign_out).clicked() {
            app.send(Command::SignOut);
            app.prefs.pin_hash = None;
            app.prefs.pin_salt = None;
            app.prefs.save(&app.platform);
            app.locked = false;
        }
    });
}

/// Avisos del inicio; cada uno lleva a su pantalla.
pub fn notices_card(app: &mut CaudalApp, ui: &mut Ui) {
    let key = app.key();
    let lang = app.lang;
    let notices = app.cache.notices.get(key, || {
        caudal_app_state::notices::notices(&app.data, today(), lang)
    });
    if notices.is_empty() {
        return;
    }
    let t = texts(lang);
    card(ui, |ui| {
        ui.label(RichText::new(t.notices).strong().color(warning_color(ui)));
        for notice in &notices {
            let color = match notice.tone {
                Tone::Negative => tone_color(ui, Tone::Negative),
                _ => ui.visuals().text_color(),
            };
            let response = ui.add(
                egui::Label::new(RichText::new(&notice.text).color(color))
                    .sense(egui::Sense::click()),
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                app.go(match notice.target {
                    NoticeTarget::Debts => Page::Debts,
                    NoticeTarget::Plans => Page::Plans,
                    NoticeTarget::Investments => Page::Investments,
                });
            }
        }
    });
    ui.add_space(8.0);
}

pub fn rules_page(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    let data = app.data.clone();
    heading(ui, t.rules_title);
    ui.label(muted(ui, t.rule_hint).small());
    if ui.button(format!("+ {}", t.new_rule)).clicked() {
        app.dialog = Some(Dialog::Rule {
            form: Box::new(RuleForm::new()),
            error: None,
        });
    }
    ui.add_space(6.0);
    for rule in rules_view(&data) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let pattern = RichText::new(format!("\"{}\"", rule.pattern));
                ui.label(if rule.active {
                    pattern.strong()
                } else {
                    pattern.weak()
                });
                ui.label(format!("» {}", rule.category));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t.edit).clicked()
                        && let Some(model) =
                            data.category_rules.iter().find(|r| r.meta.id == rule.id)
                    {
                        app.dialog = Some(Dialog::Rule {
                            form: Box::new(RuleForm::edit(model)),
                            error: None,
                        });
                    }
                });
            });
        });
        ui.add_space(4.0);
    }

    ui.add_space(12.0);
    heading(ui, t.templates_title);
    if ui.button(format!("+ {}", t.new_template)).clicked() {
        let base = caudal_app_state::forms::TransactionForm::new(&data, today());
        app.dialog = Some(Dialog::Template {
            form: Box::new(TemplateForm::from_transaction(&base)),
            error: None,
        });
    }
    ui.add_space(6.0);
    for template in templates_view(&data, lang) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&template.name).strong());
                ui.label(muted(ui, &template.detail).small());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t.edit).clicked()
                        && let Some(model) =
                            data.templates.iter().find(|m| m.meta.id == template.id)
                    {
                        app.dialog = Some(Dialog::Template {
                            form: Box::new(TemplateForm::edit(model, lang)),
                            error: None,
                        });
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
}

pub fn activity_page(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    heading(ui, t.activity);
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    let key = app.key();
    let items = app.cache.activity.get(key, || {
        caudal_app_state::notices::activity(&app.data, 100, offset, lang)
    });
    card(ui, |ui| {
        for item in &items {
            ui.horizontal_wrapped(|ui| {
                ui.label(muted(ui, &item.when).small());
                let color = match item.action {
                    caudal_app_state::notices::Action::Deleted => tone_color(ui, Tone::Negative),
                    _ => ui.visuals().text_color(),
                };
                ui.label(
                    RichText::new(item.action_label)
                        .small()
                        .strong()
                        .color(color),
                );
                ui.label(muted(ui, item.kind_label).small());
                ui.label(RichText::new(&item.label).small());
            });
        }
    });
}

pub fn rule_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut RuleForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_rule
        } else {
            t.new_rule
        },
    );
    field_label(ui, t.rule_pattern);
    text_field(ui, &mut form.pattern, "uber");
    field_label(ui, t.category);
    uuid_combo(
        ui,
        "rule-category",
        &mut form.category_id,
        None,
        &data
            .categories_for(TransactionKind::Expense)
            .into_iter()
            .chain(data.categories_for(TransactionKind::Income))
            .fold(Vec::new(), |mut list: Vec<_>, item| {
                if !list.iter().any(|(id, _)| *id == item.0) {
                    list.push(item);
                }
                list
            }),
    );
    ui.checkbox(&mut form.active, t.alert_active);
    ui.label(muted(ui, t.rule_hint).small());
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::CategoryRules,
            id: original.meta.id,
            label: original.pattern,
        });
    }
    if save {
        return match form.validate(data) {
            Ok(rule) => Outcome::Send(Command::Save(Box::new(Item::CategoryRule(rule)))),
            Err(problem) => {
                *error = Some(problem.message(t).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

pub fn template_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut TemplateForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_template
        } else {
            t.new_template
        },
    );
    field_label(ui, t.name);
    text_field(ui, &mut form.name, "");
    ui.horizontal(|ui| {
        for (kind, label) in super::pages::kind_options(lang) {
            ui.selectable_value(&mut form.kind, kind, label);
        }
    });
    field_label(ui, t.amount);
    ui.horizontal(|ui| {
        let width = ui.available_width();
        ui.add_sized([width - 90.0, 34.0], |ui: &mut Ui| {
            amount_field(ui, &mut form.amount, "", true)
        });
        ui.add_sized([80.0, 34.0], egui::TextEdit::singleline(&mut form.currency));
    });
    let accounts: Vec<(uuid::Uuid, String)> = data
        .selectable_accounts(form.account_id)
        .into_iter()
        .map(|a| (a.meta.id, a.name.clone()))
        .collect();
    field_label(ui, t.account);
    uuid_combo(
        ui,
        "template-account",
        &mut form.account_id,
        Some(t.no_account),
        &accounts,
    );
    if form.kind == TransactionKind::Transfer {
        field_label(ui, t.to_account);
        uuid_combo(
            ui,
            "template-to",
            &mut form.to_account_id,
            Some(t.no_account),
            &accounts,
        );
    } else {
        field_label(ui, t.category);
        uuid_combo(
            ui,
            "template-category",
            &mut form.category_id,
            Some(t.no_category),
            &data.categories_for(form.kind),
        );
    }
    field_label(ui, t.description);
    text_field(ui, &mut form.description, "");
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::TransactionTemplates,
            id: original.meta.id,
            label: original.name,
        });
    }
    if save {
        return match form.validate(data, lang) {
            Ok(template) => Outcome::Send(Command::Save(Box::new(Item::Template(template)))),
            Err(problem) => {
                *error = Some(problem.message(t).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

fn column_combo(ui: &mut Ui, id: &str, headers: &[String], value: &mut Option<usize>, none: &str) {
    let selected = value
        .and_then(|i| headers.get(i).cloned())
        .unwrap_or_else(|| none.to_owned());
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(ui.available_width().min(260.0))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, none);
            for (index, header) in headers.iter().enumerate() {
                let label = if header.is_empty() {
                    format!("#{}", index + 1)
                } else {
                    header.clone()
                };
                ui.selectable_value(value, Some(index), label);
            }
        });
}

pub fn import_dialog(ui: &mut Ui, data: &Data, state: &mut ImportState, lang: Lang) -> Outcome {
    let t = texts(lang);
    title(ui, t.import_csv);
    let headers = state.table.headers.clone();
    let m = &mut state.mapping;
    egui::Grid::new("import-columns")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label(muted(ui, t.column_date).small());
            column_combo(ui, "col-date", &headers, &mut m.date, t.none);
            ui.end_row();
            ui.label(muted(ui, t.column_amount).small());
            column_combo(ui, "col-amount", &headers, &mut m.amount, t.none);
            ui.end_row();
            ui.label(muted(ui, t.column_debit).small());
            column_combo(ui, "col-debit", &headers, &mut m.debit, t.none);
            ui.end_row();
            ui.label(muted(ui, t.column_credit).small());
            column_combo(ui, "col-credit", &headers, &mut m.credit, t.none);
            ui.end_row();
            ui.label(muted(ui, t.column_description).small());
            column_combo(ui, "col-description", &headers, &mut m.description, t.none);
            ui.end_row();
            ui.label(muted(ui, t.column_category).small());
            column_combo(ui, "col-category", &headers, &mut m.category, t.none);
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut m.date_order, DateOrder::DayFirst, t.day_first);
        ui.selectable_value(&mut m.date_order, DateOrder::MonthFirst, t.month_first);
    });
    ui.checkbox(&mut m.invert, t.invert_sign);
    let accounts: Vec<(uuid::Uuid, String)> = data
        .selectable_accounts(m.account_id)
        .into_iter()
        .map(|a| (a.meta.id, format!("{} ({})", a.name, a.currency)))
        .collect();
    field_label(ui, t.target_account);
    let mut account = m.account_id;
    if uuid_combo(
        ui,
        "import-account",
        &mut account,
        Some(t.no_account),
        &accounts,
    ) {
        m.account_id = account;
        if let Some(found) = account.and_then(|id| data.account(id)) {
            m.currency = found.currency.to_string();
        }
    }
    ui.horizontal(|ui| {
        ui.label(muted(ui, t.currency).small());
        ui.add_sized([80.0, 28.0], egui::TextEdit::singleline(&mut m.currency));
    });
    ui.checkbox(&mut m.include_duplicates, t.include_duplicates);

    if state.mapping != state.computed {
        state.preview = preview(data, &state.table, &state.mapping, lang);
        state.computed = state.mapping.clone();
    }
    let p = &state.preview;
    ui.add_space(6.0);
    ui.label(format!(
        "{} {} · {} {} · {} {}",
        p.new, t.import_new, p.duplicates, t.import_duplicates, p.invalid, t.import_invalid
    ));
    {
        {
            for row in p.rows.iter().take(40) {
                let color = match row.status {
                    RowStatus::New => ui.visuals().text_color(),
                    RowStatus::Duplicate => ui.visuals().weak_text_color(),
                    RowStatus::Invalid => tone_color(ui, Tone::Negative),
                };
                let mut text = row.description.clone();
                if let Some(category) = &row.category {
                    text = format!("{text} · {category}");
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&row.date).small().color(color));
                    ui.label(RichText::new(&row.amount).small().color(color));
                    ui.add(egui::Label::new(RichText::new(text).small().color(color)).truncate());
                });
            }
        }
    }
    if p.rows.len() > 40 {
        ui.label(muted(ui, format!("… {}", p.rows.len() - 40)).small());
    }
    ui.add_space(8.0);
    let mut outcome = Outcome::Keep;
    ui.horizontal(|ui| {
        let count = p.transactions.len();
        if ui
            .add_enabled(
                count > 0,
                egui::Button::new(RichText::new(format!("{} {count}", t.import_action)).strong())
                    .fill(widgets::ACCENT.linear_multiply(0.6)),
            )
            .clicked()
        {
            outcome = Outcome::Send(Command::Import(p.transactions.clone()));
        }
        if ui.button(t.cancel).clicked() {
            outcome = Outcome::Close;
        }
    });
    outcome
}

/// Comprobantes dentro del formulario de un movimiento ya guardado.
pub fn attachments_section(
    ui: &mut Ui,
    data: &Data,
    transaction_id: uuid::Uuid,
    pending: &std::collections::BTreeSet<uuid::Uuid>,
    lang: Lang,
) -> Option<Outcome> {
    let t = texts(lang);
    let mut outcome = None;
    field_label(ui, t.attachments_title);
    for attachment in data.attachments_for(transaction_id) {
        ui.horizontal(|ui| {
            let mut name = format!(
                "{} ({} KB)",
                attachment.file_name,
                (attachment.size_bytes + 1023) / 1024
            );
            if pending.contains(&attachment.meta.id) {
                name = format!("{name} · {}", t.attachment_pending);
            }
            if ui.link(name).clicked() {
                outcome = Some(Outcome::SendAndReplace(
                    Command::LoadAttachment(Box::new(attachment.clone())),
                    Dialog::Attachment {
                        attachment: Box::new(attachment.clone()),
                        bytes: None,
                        loaded: false,
                    },
                ));
            }
        });
    }
    if ui.small_button(format!("+ {}", t.attach)).clicked() {
        files::open(Purpose::Attach(transaction_id));
    }
    outcome
}

pub fn attachment_dialog(
    ui: &mut Ui,
    attachment: &Attachment,
    bytes: &Option<Vec<u8>>,
    loaded: bool,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    title(ui, &attachment.file_name);
    match (bytes, loaded) {
        (_, false) => {
            ui.spinner();
        }
        (None, true) => {
            ui.label(muted(ui, t.attachment_unavailable));
        }
        (Some(bytes), true) => {
            if attachment.mime_type.starts_with("image/") && attachment.mime_type != "image/heic" {
                let uri = format!("bytes://{}", attachment.meta.id);
                ui.add(
                    egui::Image::from_bytes(uri, bytes.clone())
                        .max_width(ui.available_width())
                        .max_height(420.0),
                );
            }
            ui.horizontal(|ui| {
                if ui.button(t.open_file).clicked() {
                    files::open_externally(&attachment.file_name, bytes);
                }
                if ui.button(t.save_copy).clicked() {
                    files::save(&attachment.file_name, &attachment.mime_type, bytes);
                }
            });
        }
    }
    ui.add_space(8.0);
    let mut outcome = Outcome::Keep;
    ui.horizontal(|ui| {
        if ui.button(t.back).clicked() {
            outcome = Outcome::Close;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button(RichText::new(t.delete).color(tone_color(ui, Tone::Negative)))
                .clicked()
            {
                outcome = Outcome::Replace(Dialog::ConfirmDelete {
                    table: Table::Attachments,
                    id: attachment.meta.id,
                    label: attachment.file_name.clone(),
                });
            }
        });
    });
    outcome
}
