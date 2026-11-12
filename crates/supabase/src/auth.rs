//! Supabase Auth: registro, inicio de sesión, renovación y recuperación con código.

use crate::{SupabaseClient, SupabaseError, now_secs, read_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: Uuid,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Segundos desde 1970.
    pub expires_at: i64,
    pub user: SessionUser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignUpOutcome {
    SignedIn(Session),
    /// Supabase envió un correo de confirmación.
    ConfirmationRequired {
        email: String,
    },
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: Option<i64>,
    expires_in: Option<i64>,
    user: SessionUser,
}

impl TokenResponse {
    fn into_session(self) -> Session {
        let expires_at = self
            .expires_at
            .unwrap_or_else(|| now_secs() + self.expires_in.unwrap_or(3_600));
        Session {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at,
            user: self.user,
        }
    }
}

impl SupabaseClient {
    pub async fn sign_up(
        &self,
        email: &str,
        password: &str,
    ) -> Result<SignUpOutcome, SupabaseError> {
        let body = self
            .auth_post(
                "/auth/v1/signup",
                json!({"email": email, "password": password}),
            )
            .await?;
        if body.get("access_token").is_some() {
            let session = self.store(serde_json::from_value::<TokenResponse>(body)?);
            return Ok(SignUpOutcome::SignedIn(session));
        }
        Ok(SignUpOutcome::ConfirmationRequired {
            email: email.to_owned(),
        })
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<Session, SupabaseError> {
        let body = self
            .auth_post(
                "/auth/v1/token?grant_type=password",
                json!({"email": email, "password": password}),
            )
            .await?;
        Ok(self.store(serde_json::from_value(body)?))
    }

    /// Renueva la sesión actual. Si el servidor la rechaza, se descarta.
    pub async fn refresh(&self) -> Result<Session, SupabaseError> {
        let current = self.session().ok_or(SupabaseError::NotSignedIn)?;
        let result = self
            .auth_post(
                "/auth/v1/token?grant_type=refresh_token",
                json!({"refresh_token": current.refresh_token}),
            )
            .await;
        match result {
            Ok(body) => Ok(self.store(serde_json::from_value(body)?)),
            Err(error) => {
                if error.requires_sign_in()
                    || matches!(error, SupabaseError::Api { status: 400, .. })
                {
                    self.set_session(None);
                }
                Err(error)
            }
        }
    }

    /// Envía un correo con un código para restablecer la contraseña.
    pub async fn send_password_recovery(&self, email: &str) -> Result<(), SupabaseError> {
        self.auth_post("/auth/v1/recover", json!({"email": email}))
            .await
            .map(|_| ())
    }

    /// Inicia sesión con el código del correo; después se llama a [`Self::update_password`].
    pub async fn verify_recovery_code(
        &self,
        email: &str,
        code: &str,
    ) -> Result<Session, SupabaseError> {
        let body = self
            .auth_post(
                "/auth/v1/verify",
                json!({"type": "recovery", "email": email, "token": code.trim()}),
            )
            .await?;
        Ok(self.store(serde_json::from_value(body)?))
    }

    pub async fn update_password(&self, new_password: &str) -> Result<(), SupabaseError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .put(self.endpoint("/auth/v1/user"))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .json(&json!({"password": new_password}))
            .send()
            .await?;
        read_json(response).await.map(|_| ())
    }

    /// Cierra la sesión en el servidor si puede; en local siempre.
    pub async fn sign_out(&self) -> Result<(), SupabaseError> {
        let Some(session) = self.session() else {
            return Ok(());
        };
        self.set_session(None);
        let response = self
            .http
            .post(self.endpoint("/auth/v1/logout"))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(session.access_token)
            .send()
            .await?;
        read_json(response).await.map(|_| ())
    }

    async fn auth_post(&self, path: &str, body: Value) -> Result<Value, SupabaseError> {
        let response = self
            .http
            .post(self.endpoint(path))
            .header("apikey", &self.config.publishable_key)
            .json(&body)
            .send()
            .await?;
        read_json(response).await
    }

    fn store(&self, token: TokenResponse) -> Session {
        let session = token.into_session();
        self.set_session(Some(session.clone()));
        session
    }
}
