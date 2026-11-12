//! `LocalStore` web sobre IndexedDB.
#![cfg(target_arch = "wasm32")]

use caudal_sync::{LocalStore, OutboxEntry, Record, RecordError, Row, Table};
use rexie::{ObjectStore, Rexie, TransactionMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen::JsValue;

const SCHEMA_VERSION_KEY: &str = "schema_version";
const LOCAL_SCHEMA_VERSION: u32 = 2;

const RECORDS: &str = "records";
const OUTBOX: &str = "outbox";
const META: &str = "meta";
const OUTBOX_NEXT: &str = "outbox_next";

#[derive(Debug, thiserror::Error)]
pub enum IdbError {
    #[error("IndexedDB: {0}")]
    Rexie(#[from] rexie::Error),
    #[error(transparent)]
    Record(#[from] RecordError),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("dato local dañado")]
    Corrupt,
}

#[derive(Serialize, Deserialize)]
struct StoredRecord {
    table: String,
    row: Row,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    seq: i64,
    table: String,
    id: Uuid,
}

pub struct IdbStore {
    db: Rexie,
}

impl IdbStore {
    /// Una base por usuario evita mezclar datos al cambiar de cuenta.
    pub async fn open(name: &str) -> Result<Self, IdbError> {
        let db = Rexie::builder(name)
            .version(LOCAL_SCHEMA_VERSION)
            .add_object_store(ObjectStore::new(RECORDS))
            .add_object_store(ObjectStore::new(OUTBOX))
            .add_object_store(ObjectStore::new(META))
            .build()
            .await?;
        let store = Self { db };
        store.migrate().await?;
        Ok(store)
    }

    /// Elimina por completo la base de datos IndexedDB del usuario.
    pub async fn delete_database(name: &str) -> Result<(), IdbError> {
        Rexie::delete(name).await?;
        Ok(())
    }

    async fn migrate(&self) -> Result<(), IdbError> {
        let current: u32 = match self.meta(SCHEMA_VERSION_KEY).await? {
            Some(text) => text.parse::<u32>().map_err(|_| IdbError::Corrupt)?,
            None => 1,
        };
        if current > LOCAL_SCHEMA_VERSION {
            return Err(IdbError::Corrupt);
        }
        for target in (current + 1)..=LOCAL_SCHEMA_VERSION {
            match target {
                2 => {}
                _ => unreachable!("migración {target} no definida"),
            }
            self.set_meta(SCHEMA_VERSION_KEY, &target.to_string())
                .await?;
        }
        Ok(())
    }
}

/// Elimina por completo la base de datos IndexedDB del usuario.
pub async fn delete_database(name: &str) -> Result<(), IdbError> {
    IdbStore::delete_database(name).await
}

fn record_key(table: Table, id: Uuid) -> JsValue {
    JsValue::from_str(&format!("{}/{id}", table.as_str()))
}

fn text(value: &JsValue) -> Result<String, IdbError> {
    value.as_string().ok_or(IdbError::Corrupt)
}

fn decode_record(value: &JsValue) -> Result<Record, IdbError> {
    let stored: StoredRecord = serde_json::from_str(&text(value)?)?;
    let table = Table::parse(&stored.table).ok_or(IdbError::Corrupt)?;
    Ok(Record::from_row(table, stored.row)?)
}

fn encode_record(record: &Record) -> Result<JsValue, IdbError> {
    let stored = StoredRecord {
        table: record.table.as_str().to_owned(),
        row: record.row.clone(),
    };
    Ok(JsValue::from_str(&serde_json::to_string(&stored)?))
}

impl LocalStore for IdbStore {
    type Error = IdbError;

    async fn get(&self, table: Table, id: Uuid) -> Result<Option<Record>, IdbError> {
        let tx = self.db.transaction(&[RECORDS], TransactionMode::ReadOnly)?;
        let value = tx.store(RECORDS)?.get(record_key(table, id)).await?;
        tx.done().await?;
        value.as_ref().map(decode_record).transpose()
    }

    async fn list(&self, table: Table) -> Result<Vec<Record>, IdbError> {
        let tx = self.db.transaction(&[RECORDS], TransactionMode::ReadOnly)?;
        let values = tx.store(RECORDS)?.get_all(None, None).await?;
        tx.done().await?;
        let mut records = Vec::new();
        for value in &values {
            let record = decode_record(value)?;
            if record.table == table {
                records.push(record);
            }
        }
        Ok(records)
    }

    async fn write_local(&self, record: &Record) -> Result<(), IdbError> {
        let tx = self
            .db
            .transaction(&[RECORDS, OUTBOX, META], TransactionMode::ReadWrite)?;
        let meta = tx.store(META)?;
        let next_key = JsValue::from_str(OUTBOX_NEXT);
        let seq = match meta.get(next_key.clone()).await? {
            Some(value) => text(&value)?
                .parse::<i64>()
                .map_err(|_| IdbError::Corrupt)?,
            None => 1,
        };
        tx.store(RECORDS)?
            .put(
                &encode_record(record)?,
                Some(&record_key(record.table, record.id)),
            )
            .await?;
        let entry = StoredEntry {
            seq,
            table: record.table.as_str().to_owned(),
            id: record.id,
        };
        tx.store(OUTBOX)?
            .put(
                &JsValue::from_str(&serde_json::to_string(&entry)?),
                Some(&JsValue::from_f64(seq as f64)),
            )
            .await?;
        meta.put(&JsValue::from_str(&(seq + 1).to_string()), Some(&next_key))
            .await?;
        tx.done().await?;
        Ok(())
    }

    async fn outbox(&self, limit: usize) -> Result<Vec<OutboxEntry>, IdbError> {
        let tx = self.db.transaction(&[OUTBOX], TransactionMode::ReadOnly)?;
        let values = tx.store(OUTBOX)?.get_all(None, None).await?;
        tx.done().await?;
        let mut entries = values
            .iter()
            .map(|value| {
                let stored: StoredEntry = serde_json::from_str(&text(value)?)?;
                Ok(OutboxEntry {
                    seq: stored.seq,
                    table: Table::parse(&stored.table).ok_or(IdbError::Corrupt)?,
                    id: stored.id,
                })
            })
            .collect::<Result<Vec<_>, IdbError>>()?;
        entries.sort_by_key(|entry| entry.seq);
        entries.truncate(limit);
        Ok(entries)
    }

    async fn ack(&self, entries: &[OutboxEntry]) -> Result<(), IdbError> {
        let tx = self.db.transaction(&[OUTBOX], TransactionMode::ReadWrite)?;
        let store = tx.store(OUTBOX)?;
        for entry in entries {
            store.delete(JsValue::from_f64(entry.seq as f64)).await?;
        }
        tx.done().await?;
        Ok(())
    }

    async fn apply_remote(&self, records: &[Record]) -> Result<usize, IdbError> {
        let tx = self
            .db
            .transaction(&[RECORDS], TransactionMode::ReadWrite)?;
        let store = tx.store(RECORDS)?;
        let mut applied = 0;
        for record in records {
            let key = record_key(record.table, record.id);
            let existing = store
                .get(key.clone())
                .await?
                .as_ref()
                .map(decode_record)
                .transpose()?;
            if record.wins_over(existing.as_ref()) {
                store.put(&encode_record(record)?, Some(&key)).await?;
                applied += 1;
            }
        }
        tx.done().await?;
        Ok(applied)
    }

    async fn cursor(&self, table: Table) -> Result<i64, IdbError> {
        let value = self.meta(&format!("cursor:{}", table.as_str())).await?;
        value.map_or(Ok(0), |value| value.parse().map_err(|_| IdbError::Corrupt))
    }

    async fn set_cursor(&self, table: Table, seq: i64) -> Result<(), IdbError> {
        self.set_meta(&format!("cursor:{}", table.as_str()), &seq.to_string())
            .await
    }

    async fn meta(&self, key: &str) -> Result<Option<String>, IdbError> {
        let tx = self.db.transaction(&[META], TransactionMode::ReadOnly)?;
        let value = tx.store(META)?.get(JsValue::from_str(key)).await?;
        tx.done().await?;
        value.as_ref().map(text).transpose()
    }

    async fn set_meta(&self, key: &str, value: &str) -> Result<(), IdbError> {
        let tx = self.db.transaction(&[META], TransactionMode::ReadWrite)?;
        tx.store(META)?
            .put(&JsValue::from_str(value), Some(&JsValue::from_str(key)))
            .await?;
        tx.done().await?;
        Ok(())
    }
}
