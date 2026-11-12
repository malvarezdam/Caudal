//! Integración con Android. En otras plataformas solo quedan versiones vacías.

/// Espacio ocupado por la barra de estado, la de navegación y el teclado, en puntos.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Cuántos caracteres borrar y qué texto añadir para pasar de `previous` a `current`.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub fn text_diff(previous: &str, current: &str) -> (usize, String) {
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

#[cfg(not(target_os = "android"))]
pub fn safe_area(_ctx: &eframe::egui::Context) -> Insets {
    Insets::default()
}

#[cfg(target_os = "android")]
pub use imp::{APP, Bridge, safe_area};

#[cfg(target_os = "android")]
mod imp {
    use super::{Insets, text_diff};
    use crate::ui::widgets::numeric_fields;
    use eframe::egui;
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::time::Duration;
    use winit::platform::android::activity::AndroidApp;
    use winit::platform::android::activity::input::{
        ImeOptions, InputType, TextInputAction, TextInputState, TextSpan,
    };

    pub static APP: OnceLock<AndroidApp> = OnceLock::new();

    /// Zonas ocupadas por el sistema, en píxeles; las informa `MainActivity`.
    static INSETS: [AtomicI32; 4] = [const { AtomicI32::new(0) }; 4];

    #[unsafe(no_mangle)]
    extern "system" fn Java_app_caudal_MainActivity_nativeSetInsets(
        _env: *mut c_void,
        _class: *mut c_void,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) {
        for (slot, value) in INSETS.iter().zip([left, top, right, bottom]) {
            slot.store(value.max(0), Ordering::Relaxed);
        }
    }

    pub fn safe_area(ctx: &egui::Context) -> Insets {
        let ppp = ctx.pixels_per_point();
        let [left, top, right, bottom] = INSETS
            .each_ref()
            .map(|slot| slot.load(Ordering::Relaxed) as f32 / ppp);
        Insets {
            left,
            top,
            right,
            bottom,
        }
    }

    /// winit 0.30 ignora el texto del teclado táctil: se lee de GameActivity y se entrega a egui.
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
                if let Some(id) = focused {
                    // Teclado numérico en los montos; texto normal en lo demás.
                    let input_type = if numeric_fields(ctx).0.contains(&id) {
                        InputType::TYPE_CLASS_NUMBER
                            | InputType::TYPE_NUMBER_FLAG_DECIMAL
                            | InputType::TYPE_NUMBER_FLAG_SIGNED
                    } else {
                        InputType::TYPE_CLASS_TEXT
                    };
                    app.set_ime_editor_info(
                        input_type,
                        TextInputAction::Done,
                        ImeOptions::IME_FLAG_NO_FULLSCREEN,
                    );
                    app.show_soft_input(true);
                }
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
