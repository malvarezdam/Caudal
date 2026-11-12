//! Implementaciones en memoria: referencia del comportamiento esperado y base
//! de las pruebas de convergencia. `MemoryRemote` imita el trigger del servidor.

use crate::record::{Record, RecordError, Row};
use crate::store::{LocalStore, OutboxEntry, Remote};
use crate::table::Table;
use serde_json::Value;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: Mutex<StoreInner>,
}

#[derive(Debug, Default)]
struct StoreInner {
    records: BTreeMap<(Table, Uuid), Record>,
    outbox: BTreeMap<i64, (Table, Uuid)>,
    next_seq: i64,
    cursors: BTreeMap<Table, i64>,
    meta: BTreeMap<String, String>,
}

impl MemoryStore {
    fn lock(&self) -> MutexGuard<'_, StoreInner> {
        self.inner.lock().expect("almacén sin envenenar")
    }
}

impl LocalStore for MemoryStore {
    type Error = Infallible;

    async fn get(&self, table: Table, id: Uuid) -> Result<Option<Record>, Infallible> {
        Ok(self.lock().records.get(&(table, id)).cloned())
    }

    async fn list(&self, table: Table) -> Result<Vec<Record>, Infallible> {
        Ok(self
            .lock()
            .records
            .values()
            .filter(|record| record.table == table)
            .cloned()
            .collect())
    }

    async fn write_local(&self, record: &Record) -> Result<(), Infallible> {
        let mut inner = self.lock();
        inner.next_seq += 1;
        let seq = inner.next_seq;
        inner.outbox.insert(seq, (record.table, record.id));
        inner
            .records
            .insert((record.table, record.id), record.clone());
        Ok(())
    }

    async fn outbox(&self, limit: usize) -> Result<Vec<OutboxEntry>, Infallible> {
        Ok(self
            .lock()
            .outbox
            .iter()
            .take(limit)
            .map(|(seq, (table, id))| OutboxEntry {
                seq: *seq,
                table: *table,
                id: *id,
            })
            .collect())
    }

    async fn ack(&self, entries: &[OutboxEntry]) -> Result<(), Infallible> {
        let mut inner = self.lock();
        for entry in entries {
            inner.outbox.remove(&entry.seq);
        }
        Ok(())
    }

    async fn apply_remote(&self, records: &[Record]) -> Result<usize, Infallible> {
        let mut inner = self.lock();
        let mut applied = 0;
        for record in records {
            let key = (record.table, record.id);
            if record.wins_over(inner.records.get(&key)) {
                inner.records.insert(key, record.clone());
                applied += 1;
            }
        }
        Ok(applied)
    }

    async fn cursor(&self, table: Table) -> Result<i64, Infallible> {
        Ok(self.lock().cursors.get(&table).copied().unwrap_or(0))
    }

    async fn set_cursor(&self, table: Table, seq: i64) -> Result<(), Infallible> {
        self.lock().cursors.insert(table, seq);
        Ok(())
    }

    async fn meta(&self, key: &str) -> Result<Option<String>, Infallible> {
        Ok(self.lock().meta.get(key).cloned())
    }

    async fn set_meta(&self, key: &str, value: &str) -> Result<(), Infallible> {
        self.lock().meta.insert(key.to_owned(), value.to_owned());
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryRemoteError {
    #[error("sin conexión")]
    Offline,
    #[error(transparent)]
    Record(#[from] RecordError),
}

#[derive(Debug, Default)]
pub struct MemoryRemote {
    inner: Mutex<RemoteInner>,
}

#[derive(Debug, Default)]
struct RemoteInner {
    rows: BTreeMap<(Table, Uuid), Record>,
    seq: i64,
    offline: bool,
    fail_pulls: bool,
}

impl MemoryRemote {
    fn lock(&self) -> MutexGuard<'_, RemoteInner> {
        self.inner.lock().expect("servidor sin envenenar")
    }

    pub fn set_offline(&self, offline: bool) {
        self.lock().offline = offline;
    }

    /// Acepta subidas pero falla al bajar: simula un corte a mitad.
    pub fn set_fail_pulls(&self, fail: bool) {
        self.lock().fail_pulls = fail;
    }

    pub fn records(&self) -> Vec<Record> {
        self.lock().rows.values().cloned().collect()
    }
}

impl Remote for MemoryRemote {
    type Error = MemoryRemoteError;

    async fn push(&self, table: Table, rows: Vec<Row>) -> Result<(), MemoryRemoteError> {
        let mut inner = self.lock();
        if inner.offline {
            return Err(MemoryRemoteError::Offline);
        }
        for row in rows {
            let mut record = Record::from_row(table, row)?;
            let key = (table, record.id);
            if record.wins_over(inner.rows.get(&key)) {
                inner.seq += 1;
                record
                    .row
                    .insert("server_seq".into(), Value::from(inner.seq));
                inner.rows.insert(key, record);
            }
        }
        Ok(())
    }

    async fn pull(
        &self,
        table: Table,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Record>, MemoryRemoteError> {
        let inner = self.lock();
        if inner.offline || inner.fail_pulls {
            return Err(MemoryRemoteError::Offline);
        }
        let mut records: Vec<Record> = inner
            .rows
            .values()
            .filter(|record| {
                record.table == table && record.server_seq().is_some_and(|seq| seq > after)
            })
            .cloned()
            .collect();
        records.sort_by_key(|record| record.server_seq());
        records.truncate(limit);
        Ok(records)
    }
}
