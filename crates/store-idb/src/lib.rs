//! `LocalStore` web sobre IndexedDB. Se completa en la Fase 2.
#![cfg(target_arch = "wasm32")]

use rexie::{ObjectStore, Rexie, TransactionMode};
use wasm_bindgen::JsValue;

const STORE: &str = "probe";

/// Guarda y relee un valor. Valida IndexedDB desde WASM.
pub async fn roundtrip(value: &str) -> Result<Option<String>, rexie::Error> {
    let db = Rexie::builder("caudal-probe")
        .version(1)
        .add_object_store(ObjectStore::new(STORE))
        .build()
        .await?;
    let key = JsValue::from_str("value");

    let tx = db.transaction(&[STORE], TransactionMode::ReadWrite)?;
    tx.store(STORE)?
        .put(&JsValue::from_str(value), Some(&key))
        .await?;
    tx.done().await?;

    let tx = db.transaction(&[STORE], TransactionMode::ReadOnly)?;
    let stored = tx.store(STORE)?.get(key).await?;
    Ok(stored.and_then(|v| v.as_string()))
}
