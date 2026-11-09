//! UI egui. Por ahora contiene el formulario de prueba de la Fase 0.

use eframe::egui;

pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native(
        "Caudal",
        options,
        Box::new(|_cc| Ok(Box::<ProbeApp>::default())),
    )
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let _ = android_ime::APP.set(app.clone());
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(error) = run(options) {
        eprintln!("error al iniciar la app: {error}");
    }
}

/// Cuántos caracteres borrar y qué texto añadir para pasar de `previous` a `current`.
#[cfg(any(target_os = "android", test))]
fn text_diff(previous: &str, current: &str) -> (usize, String) {
    let common = previous
        .chars()
        .zip(current.chars())
        .take_while(|(a, b)| a == b)
        .count();
    (
        previous.chars().count() - common,
        current.chars().skip(common).collect(),
    )
}

/// winit 0.30 ignora el texto del teclado táctil: se lee de GameActivity y se entrega a egui.
#[cfg(target_os = "android")]
mod android_ime {
    use super::{egui, text_diff};
    use std::sync::OnceLock;
    use std::time::Duration;
    use winit::platform::android::activity::AndroidApp;
    use winit::platform::android::activity::input::{TextInputState, TextSpan};

    pub static APP: OnceLock<AndroidApp> = OnceLock::new();

    #[derive(Default)]
    pub struct Bridge {
        focused: Option<egui::Id>,
        previous: String,
    }

    impl Bridge {
        pub fn pump(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
            let Some(app) = APP.get() else { return };
            let focused = ctx.memory(|memory| memory.focused());
            if focused != self.focused {
                self.focused = focused;
                self.previous.clear();
                app.set_text_input_state(TextInputState {
                    text: String::new(),
                    selection: TextSpan { start: 0, end: 0 },
                    compose_region: None,
                });
                return;
            }
            if focused.is_none() {
                return;
            }
            // El bucle de eventos no despierta con el texto del teclado.
            ctx.request_repaint_after(Duration::from_millis(50));

            let current = app.text_input_state().text;
            let (backspaces, added) = text_diff(&self.previous, &current);
            for _ in 0..backspaces {
                raw_input.events.push(egui::Event::Key {
                    key: egui::Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if !added.is_empty() {
                raw_input.events.push(egui::Event::Text(added));
            }
            self.previous = current;
        }
    }
}

#[derive(Default)]
struct ProbeApp {
    description: String,
    amount: String,
    note: String,
    saved: Vec<String>,
    #[cfg(target_os = "android")]
    ime: android_ime::Bridge,
}

impl eframe::App for ProbeApp {
    #[cfg(target_os = "android")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.ime.pump(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Prueba de formulario");
            ui.label("Descripción (probar tildes y ñ)");
            ui.text_edit_singleline(&mut self.description);
            ui.label("Monto");
            ui.text_edit_singleline(&mut self.amount);
            ui.label("Nota");
            ui.text_edit_multiline(&mut self.note);
            if ui.button("Guardar").clicked() {
                self.saved.push(format!(
                    "{} | {} | {}",
                    self.description, self.amount, self.note
                ));
                self.description.clear();
                self.amount.clear();
                self.note.clear();
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for row in &self.saved {
                    ui.label(row);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::text_diff;

    #[test]
    fn diff_de_autocorreccion() {
        assert_eq!(text_diff("hola", "hola m"), (0, " m".to_string()));
        assert_eq!(text_diff("cafe", "café"), (1, "é".to_string()));
        assert_eq!(text_diff("ñu", ""), (2, String::new()));
    }
}
