//! Comprobantes adjuntos. Sin red, el archivo espera en el dispositivo y se
//! sube en la próxima sincronización; sus datos viajan como cualquier registro.

use crate::auth::SupabaseClient;
use crate::core::{AppCore, AppError};
use crate::data::Data;
use crate::forms::meta;
use crate::i18n::{Lang, texts};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use caudal_core::model::Attachment;
use caudal_sync::{LocalStore, Table};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const MAX_BYTES: usize = 10 * 1024 * 1024;
const PENDING: &str = "attachments_pending";

fn bytes_key(id: Uuid) -> String {
    format!("attachment:{id}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentError {
    TooBig,
    UnsupportedType,
}

impl AttachmentError {
    pub fn message(self, lang: Lang) -> &'static str {
        let t = texts(lang);
        match self {
            AttachmentError::TooBig => t.attachment_too_big,
            AttachmentError::UnsupportedType => t.attachment_type,
        }
    }
}

/// Tipo del archivo por su contenido, o por la extensión si no se reconoce.
pub fn mime_type(file_name: &str, bytes: &[u8]) -> Option<&'static str> {
    let by_content = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        Some("image/png")
    } else if bytes.starts_with(b"%PDF") {
        Some("application/pdf")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.len() >= 12 && &bytes[4..12] == b"ftypheic" {
        Some("image/heic")
    } else {
        None
    };
    by_content.or_else(|| {
        let extension = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "jpg" | "jpeg" => Some("image/jpeg"),
            "png" => Some("image/png"),
            "webp" => Some("image/webp"),
            "heic" | "heif" => Some("image/heic"),
            "pdf" => Some("application/pdf"),
            _ => None,
        }
    })
}

/// Datos de un comprobante nuevo, en la carpeta del usuario.
pub fn new_attachment(
    data: &Data,
    transaction_id: Uuid,
    file_name: &str,
    bytes: &[u8],
) -> Result<Attachment, AttachmentError> {
    if bytes.len() > MAX_BYTES {
        return Err(AttachmentError::TooBig);
    }
    let mime = mime_type(file_name, bytes).ok_or(AttachmentError::UnsupportedType)?;
    let meta = meta(None, data.user_id);
    let name = file_name
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("comprobante");
    Ok(Attachment {
        storage_path: format!("{}/{}", data.user_id, meta.id),
        meta,
        transaction_id,
        file_name: name.to_owned(),
        mime_type: mime.to_owned(),
        size_bytes: bytes.len() as i64,
    })
}

impl Data {
    pub fn attachments_for(&self, transaction_id: Uuid) -> Vec<&Attachment> {
        self.attachments
            .iter()
            .filter(|a| a.transaction_id == transaction_id && !a.meta.is_deleted())
            .collect()
    }
}

impl<S: LocalStore> AppCore<S> {
    async fn pending_set(&self) -> Result<BTreeSet<Uuid>, AppError> {
        let text = self
            .repo()
            .store()
            .meta(PENDING)
            .await
            .map_err(AppError::storage)?;
        Ok(text
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    async fn set_pending(&self, set: &BTreeSet<Uuid>) -> Result<(), AppError> {
        let text = serde_json::to_string(set).map_err(AppError::storage)?;
        self.repo()
            .store()
            .set_meta(PENDING, &text)
            .await
            .map_err(AppError::storage)
    }

    /// Comprobantes que aún esperan en este dispositivo.
    pub async fn pending_attachments(&self) -> Result<BTreeSet<Uuid>, AppError> {
        self.pending_set().await
    }

    /// Guarda los datos del comprobante y su archivo localmente; se sube al sincronizar.
    pub async fn add_attachment(
        &self,
        attachment: &Attachment,
        bytes: &[u8],
    ) -> Result<(), AppError> {
        self.repo()
            .store()
            .set_meta(&bytes_key(attachment.meta.id), &STANDARD.encode(bytes))
            .await
            .map_err(AppError::storage)?;
        let mut pending = self.pending_set().await?;
        pending.insert(attachment.meta.id);
        self.set_pending(&pending).await?;
        self.save(Table::Attachments, attachment).await
    }

    async fn local_bytes(&self, id: Uuid) -> Result<Option<Vec<u8>>, AppError> {
        let text = self
            .repo()
            .store()
            .meta(&bytes_key(id))
            .await
            .map_err(AppError::storage)?;
        Ok(text
            .filter(|text| !text.is_empty())
            .and_then(|text| STANDARD.decode(text).ok()))
    }

    /// El archivo: el que espera en el dispositivo o, si ya se subió, desde Storage.
    /// `None` si todavía no está en ninguna parte (otro dispositivo no lo ha subido).
    pub async fn attachment_bytes(
        &self,
        client: Option<&SupabaseClient>,
        attachment: &Attachment,
    ) -> Result<Option<Vec<u8>>, AppError> {
        if let Some(bytes) = self.local_bytes(attachment.meta.id).await? {
            return Ok(Some(bytes));
        }
        let Some(client) = client else {
            return Ok(None);
        };
        match client.download_receipt(&attachment.storage_path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(caudal_supabase::SupabaseError::Api {
                status: 400 | 404, ..
            }) => Ok(None),
            Err(error) => Err(AppError::from(&error)),
        }
    }

    /// Sube lo pendiente. Un fallo de red deja el archivo esperando para la próxima vez.
    pub async fn upload_pending_attachments(
        &self,
        client: &SupabaseClient,
    ) -> Result<usize, AppError> {
        let mut pending = self.pending_set().await?;
        let mut uploaded = 0;
        for id in pending.clone() {
            let record = self
                .repo()
                .get(Table::Attachments, id)
                .await
                .map_err(AppError::storage)?;
            let attachment: Option<Attachment> = record.and_then(|record| record.to_model().ok());
            let bytes = self.local_bytes(id).await?;
            // Sin datos o sin archivo no hay nada que subir.
            if let (Some(attachment), Some(bytes)) = (attachment, bytes) {
                client
                    .upload_receipt(&attachment.storage_path, bytes, &attachment.mime_type)
                    .await
                    .map_err(|error| AppError::from(&error))?;
                uploaded += 1;
            }
            self.repo()
                .store()
                .set_meta(&bytes_key(id), "")
                .await
                .map_err(AppError::storage)?;
            pending.remove(&id);
            self.set_pending(&pending).await?;
        }
        Ok(uploaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipo_por_contenido_o_extension() {
        assert_eq!(
            mime_type("x.bin", &[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            mime_type("boleta.PDF", b"no es pdf"),
            Some("application/pdf")
        );
        assert_eq!(mime_type("foto.heic", &[]), Some("image/heic"));
        assert_eq!(mime_type("virus.exe", b"MZ"), None);
    }

    #[test]
    fn valida_tamano_y_carpeta() {
        let data = Data {
            user_id: Uuid::from_u128(7),
            ..Data::default()
        };
        let tx = Uuid::from_u128(9);
        let ok = new_attachment(
            &data,
            tx,
            "C:\\Fotos\\boleta.png",
            &[0x89, 0x50, 0x4E, 0x47],
        )
        .unwrap();
        assert_eq!(ok.file_name, "boleta.png");
        assert_eq!(ok.storage_path, format!("{}/{}", data.user_id, ok.meta.id));
        assert_eq!(
            new_attachment(&data, tx, "a.txt", b"hola"),
            Err(AttachmentError::UnsupportedType)
        );
        let big = vec![0xFF; MAX_BYTES + 1];
        assert_eq!(
            new_attachment(&data, tx, "a.jpg", &big),
            Err(AttachmentError::TooBig)
        );
    }
}
