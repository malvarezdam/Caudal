//! Supabase Storage: comprobantes en el bucket privado `receipts`, cada uno en
//! la carpeta `{user_id}/` de su dueño (lo exigen las políticas del bucket).

use crate::{SupabaseClient, SupabaseError, read_json};

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
}
