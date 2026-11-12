//! Cliente de Supabase sobre `reqwest`: Auth y PostgREST.

mod auth;
mod market;
mod remote;
mod storage;

pub use auth::{Session, SessionUser, SignUpOutcome};
pub use remote::normalize_numbers;
pub use storage::RECEIPTS_BUCKET;

use serde_json::Value;
use std::sync::Mutex;

/// URL del proyecto de desarrollo.
pub const SUPABASE_URL: &str = "https://vmrlfftmkgyfgmqqjexm.supabase.co";

/// Clave pública: puede ir en los clientes porque la seguridad la dan las reglas RLS.
pub const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable_0T9qoX5bNXxKk0rnGqSyGQ_RCEP57RU";

/// Margen antes de que caduque el token para renovarlo.
const REFRESH_MARGIN_SECS: i64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum SupabaseError {
    #[error("sin conexión o error de red: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{message}")]
    Api {
        status: u16,
        code: Option<String>,
        message: String,
    },
    #[error("no hay sesión iniciada")]
    NotSignedIn,
    #[error("respuesta inesperada: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Record(#[from] caudal_sync::RecordError),
}

impl SupabaseError {
    /// La sesión ya no sirve y hay que volver a iniciarla.
    pub fn requires_sign_in(&self) -> bool {
        match self {
            SupabaseError::NotSignedIn => true,
            SupabaseError::Api { status, code, .. } => {
                *status == 401
                    || code.as_deref().is_some_and(|code| {
                        matches!(
                            code,
                            "refresh_token_not_found"
                                | "refresh_token_already_used"
                                | "session_not_found"
                                | "session_expired"
                        )
                    })
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupabaseConfig {
    pub url: String,
    pub publishable_key: String,
}

impl SupabaseConfig {
    pub fn development() -> Self {
        Self {
            url: SUPABASE_URL.into(),
            publishable_key: SUPABASE_PUBLISHABLE_KEY.into(),
        }
    }
}

/// La app guarda la sesión con [`SupabaseClient::session`] y la repone con
/// [`SupabaseClient::set_session`]; el cliente la renueva solo al usarla.
pub struct SupabaseClient {
    http: reqwest::Client,
    config: SupabaseConfig,
    session: Mutex<Option<Session>>,
}

impl SupabaseClient {
    pub fn new(config: SupabaseConfig) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        #[cfg(target_arch = "wasm32")]
        let http = reqwest::Client::new();
        Self {
            http,
            config,
            session: Mutex::new(None),
        }
    }

    pub fn session(&self) -> Option<Session> {
        self.session.lock().expect("sesión sin envenenar").clone()
    }

    pub fn set_session(&self, session: Option<Session>) {
        *self.session.lock().expect("sesión sin envenenar") = session;
    }

    /// Comprueba que el servidor responde y acepta la clave pública.
    pub async fn check_connection(&self) -> Result<(), SupabaseError> {
        let response = self
            .http
            .get(self.endpoint("/auth/v1/settings"))
            .header("apikey", &self.config.publishable_key)
            .send()
            .await?;
        read_json(response).await.map(|_| ())
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.config.url.trim_end_matches('/'))
    }

    /// Token válido, renovándolo si está por caducar.
    async fn access_token(&self) -> Result<String, SupabaseError> {
        let session = self.session().ok_or(SupabaseError::NotSignedIn)?;
        if session.expires_at - REFRESH_MARGIN_SECS > now_secs() {
            return Ok(session.access_token);
        }
        Ok(self.refresh().await?.access_token)
    }
}

/// Segundos desde 1970 según el reloj del dispositivo.
pub(crate) fn now_secs() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64)
    }
}

/// Devuelve el cuerpo si la respuesta es correcta y, si no, el mensaje del servidor.
pub(crate) async fn read_json(response: reqwest::Response) -> Result<Value, SupabaseError> {
    let status = response.status();
    let body = response.text().await?;
    if status.is_success() {
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return Ok(serde_json::from_str(&body)?);
    }
    let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let field = |name: &str| parsed.get(name).and_then(Value::as_str).map(str::to_owned);
    let message = field("msg")
        .or_else(|| field("error_description"))
        .or_else(|| field("message"))
        .or_else(|| field("error"))
        .unwrap_or_else(|| format!("error {status}"));
    let code = field("error_code").or_else(|| field("code"));
    Err(SupabaseError::Api {
        status: status.as_u16(),
        code,
        message,
    })
}
