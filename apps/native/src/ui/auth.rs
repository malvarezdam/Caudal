//! Iniciar sesión, crear cuenta y recuperar la contraseña con un código.

use super::widgets::{self, error_label, password_field, text_field};
use crate::app::CaudalApp;
use crate::worker::Command;
use caudal_app_state::auth::MIN_PASSWORD_LEN;
use caudal_app_state::{Lang, texts};
use eframe::egui::{self, RichText, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    SignIn,
    SignUp,
    RecoverEmail,
    RecoverCode,
    NewPassword,
}

#[derive(Default)]
pub struct AuthUi {
    pub mode: Mode,
    pub email: String,
    pub password: String,
    pub repeat: String,
    pub code: String,
    pub info: Option<String>,
    pub error: Option<String>,
}

impl AuthUi {
    pub fn recovery_verified(&mut self) {
        self.mode = Mode::NewPassword;
        self.password.clear();
        self.repeat.clear();
        self.info = None;
        self.error = None;
    }

    fn switch(&mut self, mode: Mode) {
        self.mode = mode;
        self.error = None;
        self.info = None;
        self.password.clear();
        self.repeat.clear();
    }

    /// Validación previa a enviar; los demás errores los da el servidor.
    fn check_new_password(&mut self, lang: Lang) -> bool {
        let texts = texts(lang);
        if self.password.chars().count() < MIN_PASSWORD_LEN {
            self.error = Some(texts.password_too_short.into());
            false
        } else if self.password != self.repeat {
            self.error = Some(texts.passwords_differ.into());
            false
        } else {
            true
        }
    }
}

pub fn show(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).max(24.0));
            ui.set_max_width(380.0);
            ui.label(
                RichText::new(texts.app_name)
                    .size(34.0)
                    .strong()
                    .color(widgets::ACCENT),
            );
            ui.add_space(18.0);
            widgets::card(ui, |ui| form(app, ui));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for option in Lang::ALL {
                    if ui
                        .selectable_label(app.lang == option, option.name())
                        .clicked()
                    {
                        app.set_lang(option);
                    }
                }
            });
        });
    });
}

fn form(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    let busy = app.busy;
    let auth = &mut app.auth;
    let mut command = None;

    let title = match auth.mode {
        Mode::SignIn => texts.sign_in,
        Mode::SignUp => texts.sign_up,
        Mode::RecoverEmail | Mode::RecoverCode => texts.forgot_password,
        Mode::NewPassword => texts.new_password,
    };
    ui.label(RichText::new(title).size(18.0).strong());
    ui.add_space(8.0);

    if auth.mode != Mode::NewPassword {
        widgets::field_label(ui, texts.email);
        let response = text_field(ui, &mut auth.email, "nombre@correo.com");
        if auth.mode == Mode::RecoverCode {
            response.surrender_focus();
        }
    }
    let submit = match auth.mode {
        Mode::SignIn => {
            widgets::field_label(ui, texts.password);
            let response = password_field(ui, &mut auth.password, "");
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
        }
        Mode::SignUp | Mode::NewPassword => {
            widgets::field_label(
                ui,
                if auth.mode == Mode::SignUp {
                    texts.password
                } else {
                    texts.new_password
                },
            );
            password_field(ui, &mut auth.password, "");
            widgets::field_label(ui, texts.password_repeat);
            let response = password_field(ui, &mut auth.repeat, "");
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
        }
        Mode::RecoverEmail => false,
        Mode::RecoverCode => {
            widgets::field_label(ui, texts.code);
            let response = widgets::amount_field(ui, &mut auth.code, "123456", false);
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
        }
    };

    ui.add_space(6.0);
    error_label(ui, &auth.error);
    if let Some(info) = &auth.info {
        ui.label(RichText::new(info).color(widgets::ACCENT));
    }
    ui.add_space(6.0);

    let action = match auth.mode {
        Mode::SignIn => texts.sign_in,
        Mode::SignUp => texts.sign_up,
        Mode::RecoverEmail => texts.send_code,
        Mode::RecoverCode => texts.confirm,
        Mode::NewPassword => texts.save_password,
    };
    ui.horizontal(|ui| {
        let clicked = ui
            .add_enabled(
                !busy,
                egui::Button::new(RichText::new(action).strong()).min_size(egui::vec2(140.0, 34.0)),
            )
            .clicked();
        if busy {
            ui.spinner();
        }
        if !(clicked || submit) || busy {
            return;
        }
        auth.error = None;
        if auth.mode != Mode::NewPassword && auth.email.trim().is_empty() {
            auth.error = Some(texts.email_required.into());
            return;
        }
        command = match auth.mode {
            Mode::SignIn => Some(Command::SignIn {
                email: auth.email.clone(),
                password: auth.password.clone(),
            }),
            Mode::SignUp => auth.check_new_password(lang).then(|| Command::SignUp {
                email: auth.email.clone(),
                password: auth.password.clone(),
            }),
            Mode::RecoverEmail => {
                auth.mode = Mode::RecoverCode;
                Some(Command::SendRecovery {
                    email: auth.email.clone(),
                })
            }
            Mode::RecoverCode => Some(Command::VerifyRecovery {
                email: auth.email.clone(),
                code: auth.code.clone(),
            }),
            Mode::NewPassword => auth
                .check_new_password(lang)
                .then(|| Command::UpdatePassword {
                    password: auth.password.clone(),
                }),
        };
    });

    ui.add_space(10.0);
    match auth.mode {
        Mode::SignIn => {
            if ui.link(texts.no_account_yet).clicked() {
                auth.switch(Mode::SignUp);
            }
            if ui.link(texts.forgot_password).clicked() {
                auth.switch(Mode::RecoverEmail);
            }
        }
        Mode::SignUp => {
            if ui.link(texts.have_account).clicked() {
                auth.switch(Mode::SignIn);
            }
        }
        _ => {
            if ui.link(texts.back).clicked() {
                auth.switch(Mode::SignIn);
            }
        }
    }
    if let Some(command) = command {
        app.send(command);
    }
}
