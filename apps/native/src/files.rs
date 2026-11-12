//! Abrir y guardar archivos del usuario. En Windows, con los diálogos del
//! sistema; en Android, con el selector de documentos (`MainActivity`), que
//! responde más tarde. Las respuestas se recogen con `poll` en cada fotograma.

use std::sync::Mutex;
use uuid::Uuid;

/// Para qué se pidió el archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    ImportCsv,
    RestoreBackup,
    Attach(Uuid),
}

#[derive(Debug, Clone)]
pub enum FileEvent {
    Opened {
        purpose: Purpose,
        name: String,
        bytes: Vec<u8>,
    },
    Saved(bool),
}

static EVENTS: Mutex<Vec<FileEvent>> = Mutex::new(Vec::new());
static CONTEXT: std::sync::OnceLock<eframe::egui::Context> = std::sync::OnceLock::new();

pub fn init(ctx: &eframe::egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

fn push(event: FileEvent) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(event);
    }
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

pub fn poll() -> Vec<FileEvent> {
    EVENTS
        .lock()
        .map(|mut events| std::mem::take(&mut *events))
        .unwrap_or_default()
}

/// Tipos que se aceptan para cada propósito.
fn accepted(
    purpose: Purpose,
) -> (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
) {
    match purpose {
        Purpose::ImportCsv => (
            "CSV",
            &["csv", "txt"],
            &[
                "text/csv",
                "text/comma-separated-values",
                "text/plain",
                "application/vnd.ms-excel",
            ],
        ),
        Purpose::RestoreBackup => (
            "JSON",
            &["json"],
            &["application/json", "application/octet-stream", "text/plain"],
        ),
        Purpose::Attach(_) => (
            "Comprobante",
            &["jpg", "jpeg", "png", "webp", "heic", "pdf"],
            &["image/*", "application/pdf"],
        ),
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use super::*;

    #[cfg(windows)]
    pub fn open(purpose: Purpose) {
        let (label, extensions, _) = accepted(purpose);
        let Some(path) = rfd::FileDialog::new()
            .add_filter(label, extensions)
            .pick_file()
        else {
            return;
        };
        if let Ok(bytes) = std::fs::read(&path) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            push(FileEvent::Opened {
                purpose,
                name,
                bytes,
            });
        }
    }

    #[cfg(windows)]
    pub fn save(name: &str, _mime: &str, bytes: &[u8]) {
        let extension = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_owned())
            .unwrap_or_default();
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter(extension.to_uppercase(), &[extension.as_str()])
            .save_file()
        else {
            return;
        };
        push(FileEvent::Saved(std::fs::write(path, bytes).is_ok()));
    }

    /// Abre una copia temporal con el programa predeterminado del sistema.
    #[cfg(windows)]
    pub fn open_externally(name: &str, bytes: &[u8]) -> bool {
        let path = std::env::temp_dir().join(format!("caudal-{}-{name}", Uuid::now_v7().simple()));
        std::fs::write(&path, bytes).is_ok()
            && std::process::Command::new("explorer")
                .arg(&path)
                .spawn()
                .is_ok()
    }

    // Otros sistemas de escritorio no son objetivo; se compilan sin diálogos.
    #[cfg(not(windows))]
    pub fn open(_purpose: Purpose) {}
    #[cfg(not(windows))]
    pub fn save(_name: &str, _mime: &str, _bytes: &[u8]) {
        push(FileEvent::Saved(false));
    }
    #[cfg(not(windows))]
    pub fn open_externally(_name: &str, _bytes: &[u8]) -> bool {
        false
    }
}

#[cfg(target_os = "android")]
mod imp {
    use super::*;
    use jni::JNIEnv;
    use jni::objects::{JByteArray, JClass, JObject, JString, JValue};
    use jni::sys::{jboolean, jint};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI32, Ordering};

    static NEXT: AtomicI32 = AtomicI32::new(1);
    static REQUESTS: Mutex<Option<HashMap<i32, Purpose>>> = Mutex::new(None);

    /// Llama a un método de `MainActivity` en el hilo de Java.
    fn with_activity(call: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<()>) {
        let Some(app) = crate::android::APP.get() else {
            return;
        };
        let vm = match unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut _) } {
            Ok(vm) => vm,
            Err(_) => return,
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return;
        };
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        if call(&mut env, &activity).is_err() {
            let _ = env.exception_clear();
        }
    }

    pub fn open(purpose: Purpose) {
        let code = NEXT.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut requests) = REQUESTS.lock() {
            requests
                .get_or_insert_with(HashMap::new)
                .insert(code, purpose);
        }
        let (_, _, mimes) = accepted(purpose);
        with_activity(|env, activity| {
            let array =
                env.new_object_array(mimes.len() as i32, "java/lang/String", JObject::null())?;
            for (index, mime) in mimes.iter().enumerate() {
                let value = env.new_string(mime)?;
                env.set_object_array_element(&array, index as i32, value)?;
            }
            env.call_method(
                activity,
                "openDocument",
                "(I[Ljava/lang/String;)V",
                &[JValue::Int(code), JValue::Object(&array)],
            )?;
            Ok(())
        });
    }

    pub fn save(name: &str, mime: &str, bytes: &[u8]) {
        with_activity(|env, activity| {
            let name = env.new_string(name)?;
            let mime = env.new_string(mime)?;
            let data = env.byte_array_from_slice(bytes)?;
            env.call_method(
                activity,
                "createDocument",
                "(Ljava/lang/String;Ljava/lang/String;[B)V",
                &[
                    JValue::Object(&name),
                    JValue::Object(&mime),
                    JValue::Object(&data),
                ],
            )?;
            Ok(())
        });
    }

    /// En Android se guarda una copia y el usuario la abre con su visor.
    pub fn open_externally(name: &str, bytes: &[u8]) -> bool {
        let mime = caudal_app_state::attachments::mime_type(name, bytes)
            .unwrap_or("application/octet-stream");
        save(name, mime, bytes);
        true
    }

    #[unsafe(no_mangle)]
    extern "system" fn Java_app_caudal_MainActivity_nativeFileOpened<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        code: jint,
        name: JString<'local>,
        data: JByteArray<'local>,
    ) {
        let purpose = REQUESTS
            .lock()
            .ok()
            .and_then(|mut requests| requests.as_mut()?.remove(&code));
        let Some(purpose) = purpose else { return };
        if data.is_null() {
            return;
        }
        let name: String = env
            .get_string(&name)
            .map(|text| text.into())
            .unwrap_or_default();
        if let Ok(bytes) = env.convert_byte_array(&data) {
            push(FileEvent::Opened {
                purpose,
                name,
                bytes,
            });
        }
    }

    #[unsafe(no_mangle)]
    extern "system" fn Java_app_caudal_MainActivity_nativeFileSaved<'local>(
        _env: JNIEnv<'local>,
        _class: JClass<'local>,
        ok: jboolean,
    ) {
        push(FileEvent::Saved(ok != 0));
    }
}

pub use imp::{open, open_externally, save};
