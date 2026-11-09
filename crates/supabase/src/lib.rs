//! Cliente de Supabase sobre `reqwest`. Se completa en la Fase 2.

/// URL del proyecto de desarrollo.
pub const SUPABASE_URL: &str = "https://vmrlfftmkgyfgmqqjexm.supabase.co";

/// Clave pública: puede ir en los clientes porque la seguridad la dan las reglas RLS.
pub const SUPABASE_PUBLISHABLE_KEY: &str = "sb_publishable_0T9qoX5bNXxKk0rnGqSyGQ_RCEP57RU";

/// Descarga un recurso como texto. Valida `reqwest` en cada target.
pub async fn fetch_text(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.error_for_status()?.text().await
}
