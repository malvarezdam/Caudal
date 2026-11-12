//! Lo que cambia entre Windows y Android: carpetas, sesión, reloj y márgenes.

use caudal_app_state::auth::{Session, SessionUser};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Platform {
    pub data_dir: PathBuf,
}

impl Platform {
    #[cfg(not(target_os = "android"))]
    pub fn desktop() -> Self {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let data_dir = base.join("Caudal");
        let _ = std::fs::create_dir_all(&data_dir);
        Self { data_dir }
    }

    #[cfg(target_os = "android")]
    pub fn android(app: &winit::platform::android::activity::AndroidApp) -> Self {
        let data_dir = app.internal_data_path().unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        Self { data_dir }
    }

    pub fn database_path(&self, name: &str) -> PathBuf {
        self.data_dir.join(format!("{name}.db"))
    }

    pub fn session(&self) -> SessionStorage {
        SessionStorage {
            #[cfg(not(windows))]
            file: self.data_dir.join("session.json"),
        }
    }

    pub fn prefs_path(&self) -> PathBuf {
        self.data_dir.join("prefs.json")
    }
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

pub fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn random_node() -> u64 {
    Uuid::new_v4().as_u64_pair().0
}

/// Solo se guarda lo necesario para renovar la sesión: el token de acceso se
/// pide de nuevo al abrir, así el secreto guardado es pequeño.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSession {
    refresh_token: String,
    user_id: Uuid,
    email: Option<String>,
}

/// Windows: Administrador de credenciales. Android: almacenamiento privado de la app.
pub struct SessionStorage {
    #[cfg(not(windows))]
    file: PathBuf,
}

impl SessionStorage {
    pub fn load(&self) -> Option<Session> {
        let stored: StoredSession = serde_json::from_str(&self.read()?).ok()?;
        Some(Session {
            access_token: String::new(),
            refresh_token: stored.refresh_token,
            expires_at: 0,
            user: SessionUser {
                id: stored.user_id,
                email: stored.email,
            },
        })
    }

    pub fn save(&self, session: &Session) {
        let stored = StoredSession {
            refresh_token: session.refresh_token.clone(),
            user_id: session.user.id,
            email: session.user.email.clone(),
        };
        if let Ok(text) = serde_json::to_string(&stored) {
            self.write(&text);
        }
    }

    /// Distinto de lo guardado: el servidor rotó el token de renovación.
    pub fn is_stale(&self, session: &Session) -> bool {
        self.load()
            .is_none_or(|stored| stored.refresh_token != session.refresh_token)
    }

    #[cfg(windows)]
    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new("Caudal", "session").ok()
    }

    #[cfg(windows)]
    fn read(&self) -> Option<String> {
        Self::entry()?.get_password().ok()
    }

    #[cfg(windows)]
    fn write(&self, text: &str) {
        if let Some(entry) = Self::entry() {
            let _ = entry.set_password(text);
        }
    }

    #[cfg(windows)]
    pub fn clear(&self) {
        if let Some(entry) = Self::entry() {
            let _ = entry.delete_credential();
        }
    }

    #[cfg(not(windows))]
    fn read(&self) -> Option<String> {
        std::fs::read_to_string(&self.file).ok()
    }

    #[cfg(not(windows))]
    fn write(&self, text: &str) {
        let _ = std::fs::write(&self.file, text);
    }

    #[cfg(not(windows))]
    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Preferencias de este dispositivo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    pub lang: Option<String>,
    pub theme: Option<String>,
    /// PIN de este dispositivo: hash con sal, nunca el número.
    #[serde(default)]
    pub pin_hash: Option<String>,
    #[serde(default)]
    pub pin_salt: Option<String>,
    #[serde(default)]
    pub private: bool,
}

impl Prefs {
    pub fn load(platform: &Platform) -> Self {
        std::fs::read_to_string(platform.prefs_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, platform: &Platform) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(platform.prefs_path(), text);
        }
    }
}
