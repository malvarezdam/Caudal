//! Acceso a la cuenta. Las UI usan esto en lugar de hablar con Supabase.

pub use caudal_supabase::{
    Session, SessionUser, SignUpOutcome, SupabaseClient, SupabaseConfig, SupabaseError,
};

use crate::i18n::Lang;

/// Contraseña mínima de Supabase por defecto.
pub const MIN_PASSWORD_LEN: usize = 6;

pub fn client() -> SupabaseClient {
    SupabaseClient::new(SupabaseConfig::development())
}

/// Mensaje comprensible para los errores de acceso más comunes.
pub fn error_message(error: &SupabaseError, lang: Lang) -> String {
    let es = lang == Lang::Es;
    let known = match error {
        SupabaseError::Http(_) => Some(if es {
            "Sin conexión. Revisa tu internet e inténtalo de nuevo."
        } else {
            "No connection. Check your internet and try again."
        }),
        SupabaseError::NotSignedIn => Some(if es {
            "Inicia sesión para continuar."
        } else {
            "Sign in to continue."
        }),
        SupabaseError::Api { code, .. } => match code.as_deref() {
            Some("invalid_credentials") => Some(if es {
                "Correo o contraseña incorrectos."
            } else {
                "Wrong email or password."
            }),
            Some("email_not_confirmed") => Some(if es {
                "Confirma tu correo antes de entrar."
            } else {
                "Confirm your email before signing in."
            }),
            Some("user_already_exists" | "email_exists") => Some(if es {
                "Ya existe una cuenta con ese correo."
            } else {
                "An account with that email already exists."
            }),
            Some("weak_password") => Some(if es {
                "La contraseña es demasiado débil."
            } else {
                "The password is too weak."
            }),
            Some("over_email_send_rate_limit" | "over_request_rate_limit") => Some(if es {
                "Demasiados intentos. Espera un momento."
            } else {
                "Too many attempts. Wait a moment."
            }),
            Some("otp_expired") => Some(if es {
                "El código expiró o no es válido."
            } else {
                "The code expired or is not valid."
            }),
            Some("validation_failed" | "email_address_invalid") => Some(if es {
                "Revisa el correo escrito."
            } else {
                "Check the email address."
            }),
            _ => None,
        },
        _ => None,
    };
    known.map_or_else(|| error.to_string(), str::to_owned)
}
