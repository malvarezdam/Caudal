//! Primera vez: moneda principal, idioma y categorías sugeridas.

use super::widgets::{self, error_label, text_field};
use crate::app::CaudalApp;
use crate::worker::Command;
use caudal_app_state::{COMMON_CURRENCIES, Lang, texts};
use eframe::egui::{self, RichText, Ui};

pub fn show(app: &mut CaudalApp, ui: &mut Ui) {
    let user_id = app.data.user_id;
    let mut send = None;
    let mut new_lang = None;
    let Some(onboarding) = app.onboarding.as_mut() else {
        return;
    };
    let lang = onboarding.form.lang;
    let texts = texts(lang);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(32.0);
            ui.set_max_width(420.0);
            widgets::card(ui, |ui| {
                ui.label(RichText::new(texts.welcome).size(22.0).strong());
                ui.label(widgets::muted(ui, texts.welcome_hint));
                ui.add_space(10.0);

                widgets::field_label(ui, texts.base_currency);
                ui.horizontal_wrapped(|ui| {
                    for code in COMMON_CURRENCIES {
                        let selected = onboarding.form.base_currency == code;
                        if ui.selectable_label(selected, code).clicked() {
                            onboarding.form.base_currency = code.to_owned();
                        }
                    }
                });
                text_field(ui, &mut onboarding.form.base_currency, "USD");

                widgets::field_label(ui, texts.language);
                ui.horizontal(|ui| {
                    for option in Lang::ALL {
                        if ui.selectable_label(lang == option, option.name()).clicked() {
                            onboarding.form.lang = option;
                            new_lang = Some(option);
                        }
                    }
                });

                widgets::field_label(ui, texts.display_name);
                text_field(ui, &mut onboarding.form.display_name, "");

                ui.add_space(6.0);
                ui.checkbox(&mut onboarding.suggested, texts.suggested_categories);
                ui.add_space(6.0);
                error_label(ui, &onboarding.error);
                if ui
                    .add(
                        egui::Button::new(RichText::new(texts.start).strong())
                            .min_size(egui::vec2(140.0, 34.0)),
                    )
                    .clicked()
                {
                    match onboarding.form.validate(user_id) {
                        Ok(profile) => {
                            send = Some(Command::Onboard {
                                profile: Box::new(profile),
                                suggested: onboarding.suggested,
                                lang,
                            });
                        }
                        Err(error) => onboarding.error = Some(error.message(texts).into()),
                    }
                }
            });
        });
    });
    if let Some(lang) = new_lang {
        app.set_lang(lang);
    }
    if let Some(command) = send {
        app.send(command);
    }
}
