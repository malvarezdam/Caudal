//! Supabase Storage: comprobantes en el bucket privado `receipts`, cada uno en
//! la carpeta `{user_id}/` de su dueño (lo exigen las políticas del bucket).

use crate::{SupabaseClient, SupabaseError, read_json};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const RECEIPTS_BUCKET: &str = "receipts";

impl SupabaseClient {
    fn object_url(&self, path: &str) -> String {
        self.endpoint(&format!("/storage/v1/object/{RECEIPTS_BUCKET}/{path}"))
    }

    /// Sube o reemplaza el archivo.
    pub async fn upload_receipt(
        &self,
        path: &str,
        bytes: Vec<u8>,
        mime_type: &str,
    ) -> Result<(), SupabaseError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .post(self.object_url(path))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .header("content-type", mime_type)
            .header("x-upsert", "true")
            .body(bytes)
            .send()
            .await?;
        read_json(response).await.map(|_| ())
    }

    pub async fn download_receipt(&self, path: &str) -> Result<Vec<u8>, SupabaseError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .get(self.object_url(path))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .send()
            .await?;
        if !response.status().is_success() {
            return read_json(response).await.map(|_| Vec::new());
        }
        Ok(response.bytes().await?.to_vec())
    }

    /// Lista los archivos en `receipts/{user_id}/` del usuario actual.
    pub async fn list_user_receipts(&self) -> Result<Vec<String>, SupabaseError> {
        let user_id = self.session().ok_or(SupabaseError::NotSignedIn)?.user.id;
        let token = self.access_token().await?;
        let prefix = format!("{user_id}/");
        let response = self
            .http
            .post(self.endpoint(&format!("/storage/v1/object/list/{RECEIPTS_BUCKET}")))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .json(&json!({
                "prefix": prefix,
                "limit": 1000,
                "offset": 0,
            }))
            .send()
            .await?;
        match read_json(response).await {
            Ok(Value::Array(items)) => Ok(items
                .into_iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.trim_matches('/');
                    (!name.is_empty()).then(|| format!("{user_id}/{name}"))
                })
                .collect()),
            Ok(_) => Ok(Vec::new()),
            Err(SupabaseError::Api {
                status: 400 | 404, ..
            }) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// Borra un archivo de `receipts`. Si ya no existe, no falla.
    pub async fn delete_receipt(&self, path: &str) -> Result<(), SupabaseError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .delete(self.object_url(path))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .send()
            .await?;
        match read_json(response).await {
            Ok(_)
            | Err(SupabaseError::Api {
                status: 400 | 404, ..
            }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Borra todos los archivos de `receipts/{user_id}/`, tanto los listados en
    /// Storage como las rutas locales conocidas de la cuenta.
    pub async fn delete_user_receipts(
        &self,
        known_paths: &[String],
    ) -> Result<usize, SupabaseError> {
        let user_id = self.session().ok_or(SupabaseError::NotSignedIn)?.user.id;
        let prefix = format!("{user_id}/");
        let mut paths: BTreeSet<String> = self.list_user_receipts().await?.into_iter().collect();
        for path in known_paths {
            if path.starts_with(&prefix) {
                paths.insert(path.clone());
            }
        }
        let mut deleted = 0;
        for path in paths {
            self.delete_receipt(&path).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}
