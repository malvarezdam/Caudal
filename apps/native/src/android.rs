//! Integración con Android. En otras plataformas solo quedan versiones vacías.

use caudal_app_state::{Data, Lang, notices::NoticeTarget, texts};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Espacio ocupado por la barra de estado, la de navegación y el teclado, en puntos.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Un aviso programado para una fecha concreta (`YYYY-MM-DD`), ya traducido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledNotice {
    pub date: String,
    pub text: String,
}

/// Contenido de `files/notices.json` que lee `NoticeWorker` en Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticesFile {
    pub title: String,
    pub items: Vec<ScheduledNotice>,
}

/// Construye los avisos de los próximos 7 días (cuotas vencidas o por vencer y
/// alertas de precio cumplidas hoy) desde `notices`, `recurring::upcoming` y
/// `investments::triggered_alerts`.
pub fn build_notices(data: &Data, today: NaiveDate, lang: Lang) -> NoticesFile {
    let t = texts(lang);
    let mut items = Vec::new();
    let mut push = |date: NaiveDate, text: String| {
        let date = date.format("%Y-%m-%d").to_string();
        if !items
            .iter()
            .any(|item: &ScheduledNotice| item.date == date && item.text == text)
        {
            items.push(ScheduledNotice { date, text });
        }
    };

    for notice in caudal_app_state::notices::notices(data, today, lang) {
        if notice.target == NoticeTarget::Debts {
            push(today, notice.text);
        }
    }
    for alert in caudal_app_state::investments::triggered_alerts(data, today, lang) {
        push(today, alert.text);
    }

    for day in caudal_app_state::recurring::upcoming(data, today, 7, lang) {
        for item in day
            .items
            .into_iter()
            .filter(|item| item.detail != t.recurring)
        {
            let when = if item.detail == t.overdue {
                t.overdue.to_lowercase()
            } else {
                format!("{} {}", t.due_soon, day.date_text)
            };
            let text = format!("{}: {} · {when}", item.title, item.amount_text);
            if let Some(remind_on) = day.date.checked_sub_days(Days::new(3))
                && remind_on > today
            {
                push(remind_on, text.clone());
            }
            push(day.date, text);
        }
    }

    NoticesFile {
        title: format!("{} · {}", t.app_name, t.notices),
        items,
    }
}

/// Escribe `notices.json` en la carpeta de datos y avisa a `MainActivity` si hay
/// avisos para que pida el permiso la primera vez.
pub fn write_notices(data_dir: &Path, data: &Data, today: NaiveDate, lang: Lang) {
    // Solo lo lee el `NoticeWorker` de Android.
    if !cfg!(target_os = "android") {
        return;
    }
    let file = build_notices(data, today, lang);
    let has_notices = !file.items.is_empty();
    if let Ok(json) = serde_json::to_string_pretty(&file) {
        let _ = std::fs::write(data_dir.join("notices.json"), json);
    }
    #[cfg(target_os = "android")]
    imp::on_notices_updated(has_notices);
    #[cfg(not(target_os = "android"))]
    let _ = has_notices;
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
    use jni::objects::{JObject, JValue};
    use jni::sys::jboolean;
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

    pub fn on_notices_updated(has_notices: bool) {
        let Some(app) = APP.get() else { return };
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else {
            return;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return;
        };
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        if env
            .call_method(
                &activity,
                "onNoticesUpdated",
                "(Z)V",
                &[JValue::Bool(jboolean::from(has_notices))],
            )
            .is_err()
        {
            let _ = env.exception_clear();
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
    use super::{build_notices, text_diff};
    use caudal_app_state::{Data, Lang, defaults::demo};
    use chrono::NaiveDate;
    use uuid::Uuid;

    #[test]
    fn diff_de_autocorreccion() {
        assert_eq!(text_diff("hola", "hola m"), (0, " m".to_string()));
        assert_eq!(text_diff("cafe", "café"), (1, "é".to_string()));
        assert_eq!(text_diff("ñu", ""), (2, String::new()));
    }

    #[test]
    fn avisos_de_demo_incluyen_cuota_vencida_y_alerta_de_precio() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let sample = demo::sample(Uuid::from_u128(1), today, Lang::Es);
        let mut data = Data {
            user_id: Uuid::from_u128(1),
            profile: Some(sample.profile),
            accounts: sample.accounts,
            categories: sample.categories,
            transactions: sample.transactions,
            assets: sample.assets,
            trades: sample.trades.into_iter().map(|d| d.trade).collect(),
            prices: sample.market.prices,
            fx_rates: sample.market.fx_rates,
            alerts: sample.alerts,
            ..Data::default()
        };
        if let Some(contact) = sample.debt.contact {
            data.contacts.push(contact);
        }
        data.debts.push(sample.debt.debt);
        data.installments.extend(sample.debt.installments);

        let file = build_notices(&data, today, Lang::Es);
        assert_eq!(file.title, "Caudal · Avisos");
        assert!(
            file.items.iter().any(|item| item.date == "2026-11-12"
                && item.text.contains("Ana")
                && item.text.contains("vencida")),
            "debe incluir la cuota vencida de Ana hoy: {:?}",
            file.items
        );
        assert!(
            file.items
                .iter()
                .any(|item| item.date == "2026-11-12" && item.text.contains("Apple")),
            "debe incluir la alerta de precio cumplida de Apple hoy: {:?}",
            file.items
        );
    }
}
