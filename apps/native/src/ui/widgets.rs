//! Piezas comunes: colores, tarjetas, campos de monto y fecha, filas y selectores.

use caudal_app_state::format::{format_date, parse_date};
use caudal_app_state::views::{Tone, TransactionView};
use caudal_app_state::{Lang, texts};
use chrono::{Days, NaiveDate};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Response, RichText, Stroke,
    TextEdit, Ui,
};
use std::collections::HashSet;
use uuid::Uuid;

pub const ACCENT: Color32 = Color32::from_rgb(0x1f, 0x8a, 0x80);

pub fn tone_color(ui: &Ui, tone: Tone) -> Color32 {
    let dark = ui.visuals().dark_mode;
    match (tone, dark) {
        (Tone::Positive, true) => Color32::from_rgb(0x5f, 0xd0, 0x8a),
        (Tone::Positive, false) => Color32::from_rgb(0x1d, 0x7f, 0x45),
        (Tone::Negative, true) => Color32::from_rgb(0xf0, 0x7b, 0x7b),
        (Tone::Negative, false) => Color32::from_rgb(0xb3, 0x2d, 0x2d),
        (Tone::Neutral, _) => ui.visuals().text_color(),
    }
}

pub fn warning_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0xf2, 0xc0, 0x5c)
    } else {
        Color32::from_rgb(0x9a, 0x62, 0x00)
    }
}

pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

pub fn muted(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(ui.visuals().weak_text_color())
}

/// Identificadores de los campos numéricos de este fotograma: Android abre el
/// teclado numérico en ellos.
#[derive(Clone, Default)]
pub struct NumericFields(pub HashSet<Id>);

pub fn reset_numeric_fields(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(Id::NULL, NumericFields::default()));
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn numeric_fields(ctx: &egui::Context) -> NumericFields {
    ctx.data(|data| data.get_temp::<NumericFields>(Id::NULL).unwrap_or_default())
}

fn mark_numeric(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<NumericFields>(Id::NULL)
            .0
            .insert(id);
    });
}

/// Si el teclado táctil tapa el campo, lo desplaza a la vista.
fn keep_visible(response: &Response) {
    if response.gained_focus() {
        response.scroll_to_me(Some(Align::Center));
    }
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    let response = ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    keep_visible(&response);
    response
}

pub fn password_field(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    let response = ui.add(
        TextEdit::singleline(value)
            .hint_text(hint)
            .password(true)
            .desired_width(f32::INFINITY),
    );
    keep_visible(&response);
    response
}

pub fn amount_field(ui: &mut Ui, value: &mut String, hint: &str, large: bool) -> Response {
    let mut edit = TextEdit::singleline(value)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
        .align(egui::Align2::RIGHT_CENTER);
    if large {
        edit = edit.font(egui::TextStyle::Heading);
    }
    let response = ui.add(edit);
    mark_numeric(ui.ctx(), response.id);
    keep_visible(&response);
    response
}

/// Fecha editable a mano, con < > y "Hoy".
pub fn date_field(ui: &mut Ui, id_salt: &str, date: &mut NaiveDate, lang: Lang, today: NaiveDate) {
    let id = ui.scope_id().with(id_salt);
    ui.horizontal(|ui| {
        if ui.button("<").clicked()
            && let Some(previous) = date.checked_sub_days(Days::new(1))
        {
            *date = previous;
            ui.data_mut(|data| data.remove::<String>(id));
        }
        let mut text = ui.data_mut(|data| {
            data.get_temp::<String>(id)
                .unwrap_or_else(|| format_date(*date, lang))
        });
        let response = ui.add(TextEdit::singleline(&mut text).desired_width(110.0));
        keep_visible(&response);
        if response.changed() {
            if let Some(parsed) = parse_date(&text, lang) {
                *date = parsed;
            }
            ui.data_mut(|data| data.insert_temp(id, text.clone()));
        }
        if response.lost_focus() {
            ui.data_mut(|data| data.remove::<String>(id));
        }
        if ui.button(">").clicked()
            && let Some(next) = date.checked_add_days(Days::new(1))
        {
            *date = next;
            ui.data_mut(|data| data.remove::<String>(id));
        }
        let today_label = if lang == Lang::Es { "Hoy" } else { "Today" };
        if *date != today && ui.button(today_label).clicked() {
            *date = today;
            ui.data_mut(|data| data.remove::<String>(id));
        }
    });
}

/// Selector de una opción entre `options`, con una entrada vacía opcional.
pub fn uuid_combo(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut Option<Uuid>,
    empty_label: Option<&str>,
    options: &[(Uuid, String)],
) -> bool {
    let selected = value
        .and_then(|id| options.iter().find(|(option, _)| *option == id))
        .map(|(_, label)| label.clone())
        .or_else(|| empty_label.map(str::to_owned))
        .unwrap_or_default();
    let before = *value;
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected)
        .width(ui.available_width().min(320.0))
        .show_ui(ui, |ui| {
            if let Some(empty) = empty_label {
                ui.selectable_value(value, None, empty);
            }
            for (id, label) in options {
                ui.selectable_value(value, Some(*id), label);
            }
        });
    *value != before
}

pub fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(muted(ui, text).small());
}

pub fn error_label(ui: &mut Ui, error: &Option<String>) {
    if let Some(error) = error {
        let color = tone_color(ui, Tone::Negative);
        ui.label(RichText::new(error).color(color));
    }
}

/// Fila de movimiento. Devuelve la respuesta de toda la fila.
pub fn transaction_row(ui: &mut Ui, view: &TransactionView, lang: Lang) -> Response {
    let texts = texts(lang);
    let response = Frame::new()
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 130.0).max(120.0));
                    let mut title = RichText::new(&view.title).strong();
                    if view.pending {
                        title = title.italics();
                    }
                    ui.add(egui::Label::new(title).truncate());
                    let mut subtitle = view.subtitle.clone();
                    if view.pending {
                        subtitle = format!("{} · {subtitle}", texts.pending);
                    }
                    if !subtitle.is_empty() {
                        ui.add(egui::Label::new(muted(ui, subtitle).small()).truncate());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.vertical(|ui| {
                        ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
                            ui.label(
                                RichText::new(&view.amount_text)
                                    .strong()
                                    .color(tone_color(ui, view.tone)),
                            );
                            ui.label(muted(ui, &view.date_text).small());
                        });
                    });
                });
            });
        })
        .response;
    let response = response.interact(egui::Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.heading(RichText::new(text).strong());
    ui.add_space(6.0);
}
