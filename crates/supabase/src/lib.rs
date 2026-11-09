//! Cliente de Supabase sobre `reqwest`. Se completa en la Fase 2.

/// Descarga un recurso como texto. Valida `reqwest` en cada target.
pub async fn fetch_text(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.error_for_status()?.text().await
}
