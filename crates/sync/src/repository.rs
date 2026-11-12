use crate::record::{Record, RecordError, Row};
use crate::store::{LocalStore, Remote};
use crate::table::Table;
use caudal_core::hlc::HlcClock;
use caudal_core::model::Hlc;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;
use uuid::Uuid;

const NODE_KEY: &str = "hlc_node";
const LAST_KEY: &str = "hlc_last";

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError<E: std::error::Error + 'static> {
    #[error("almacén local: {0}")]
    Store(#[source] E),
    #[error(transparent)]
    Record(#[from] RecordError),
}

/// Lectura y escritura local para un usuario. Nunca espera a la red.
pub struct Repository<S> {
    store: S,
    user_id: Uuid,
    clock: Mutex<HlcClock>,
    now_millis: Box<dyn Fn() -> u64>,
}

impl<S: LocalStore> Repository<S> {
    /// `new_node` solo se usa la primera vez: identifica al dispositivo en las marcas.
    pub async fn open(
        store: S,
        user_id: Uuid,
        now_millis: impl Fn() -> u64 + 'static,
        new_node: impl FnOnce() -> u64,
    ) -> Result<Self, RepositoryError<S::Error>> {
        let saved_node = store
            .meta(NODE_KEY)
            .await
            .map_err(RepositoryError::Store)?
            .and_then(|value| u64::from_str_radix(&value, 16).ok());
        let node = match saved_node {
            Some(node) => node,
            None => {
                let node = new_node();
                store
                    .set_meta(NODE_KEY, &format!("{node:016x}"))
                    .await
                    .map_err(RepositoryError::Store)?;
                node
            }
        };
        let last = store
            .meta(LAST_KEY)
            .await
            .map_err(RepositoryError::Store)?
            .map(Hlc);
        Ok(Self {
            store,
            user_id,
            clock: Mutex::new(HlcClock::new(node, last.as_ref())),
            now_millis: Box::new(now_millis),
        })
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Crea o reemplaza el registro completo. `row` debe traer `id`.
    pub async fn save(
        &self,
        table: Table,
        mut row: Row,
    ) -> Result<Record, RepositoryError<S::Error>> {
        let stamp = self.next_stamp().await?;
        row.insert("user_id".into(), Value::String(self.user_id.to_string()));
        row.insert("updated_at".into(), Value::String(stamp.0));
        row.remove("server_seq");
        let record = Record::from_row(table, row)?;
        self.store
            .write_local(&record)
            .await
            .map_err(RepositoryError::Store)?;
        Ok(record)
    }

    /// Guarda un registro generado automáticamente (p. ej. la ocurrencia de una
    /// regla periódica) con la marca más baja posible: cualquier cambio del
    /// usuario, en este u otro dispositivo, gana siempre a la generación.
    /// No reemplaza un registro que ya exista localmente.
    pub async fn save_generated(
        &self,
        table: Table,
        mut row: Row,
    ) -> Result<Option<Record>, RepositoryError<S::Error>> {
        let node = self.clock.lock().expect("reloj sin envenenar").node();
        let stamp = caudal_core::hlc::HlcParts {
            millis: 0,
            counter: 0,
            node,
        }
        .encode();
        row.insert("user_id".into(), Value::String(self.user_id.to_string()));
        row.insert("updated_at".into(), Value::String(stamp.0));
        row.remove("server_seq");
        let record = Record::from_row(table, row)?;
        if self
            .store
            .get(table, record.id)
            .await
            .map_err(RepositoryError::Store)?
            .is_some()
        {
            return Ok(None);
        }
        self.store
            .write_local(&record)
            .await
            .map_err(RepositoryError::Store)?;
        Ok(Some(record))
    }

    pub async fn save_model<T: Serialize>(
        &self,
        table: Table,
        model: &T,
    ) -> Result<Record, RepositoryError<S::Error>> {
        let row = Record::from_model(table, model)?.row;
        self.save(table, row).await
    }

    /// Envía el registro a la papelera. `None` si no existe.
    pub async fn delete(
        &self,
        table: Table,
        id: Uuid,
    ) -> Result<Option<Record>, RepositoryError<S::Error>> {
        self.set_deleted(table, id, true).await
    }

    /// Recupera un registro de la papelera.
    pub async fn restore(
        &self,
        table: Table,
        id: Uuid,
    ) -> Result<Option<Record>, RepositoryError<S::Error>> {
        self.set_deleted(table, id, false).await
    }

    pub async fn get(
        &self,
        table: Table,
        id: Uuid,
    ) -> Result<Option<Record>, RepositoryError<S::Error>> {
        self.store
            .get(table, id)
            .await
            .map_err(RepositoryError::Store)
    }

    pub async fn list_active(
        &self,
        table: Table,
    ) -> Result<Vec<Record>, RepositoryError<S::Error>> {
        self.list_where(table, false).await
    }

    pub async fn list_trash(&self, table: Table) -> Result<Vec<Record>, RepositoryError<S::Error>> {
        self.list_where(table, true).await
    }

    async fn list_where(
        &self,
        table: Table,
        deleted: bool,
    ) -> Result<Vec<Record>, RepositoryError<S::Error>> {
        let records = self
            .store
            .list(table)
            .await
            .map_err(RepositoryError::Store)?;
        Ok(records
            .into_iter()
            .filter(|record| record.is_deleted() == deleted)
            .collect())
    }

    async fn set_deleted(
        &self,
        table: Table,
        id: Uuid,
        deleted: bool,
    ) -> Result<Option<Record>, RepositoryError<S::Error>> {
        let Some(existing) = self.get(table, id).await? else {
            return Ok(None);
        };
        if existing.is_deleted() == deleted {
            return Ok(Some(existing));
        }
        let mut row = existing.row;
        let stamp = self.next_stamp().await?;
        let deleted_at = if deleted {
            Value::String(stamp.0.clone())
        } else {
            Value::Null
        };
        row.insert("deleted_at".into(), deleted_at);
        row.insert("updated_at".into(), Value::String(stamp.0));
        row.remove("server_seq");
        let record = Record::from_row(table, row)?;
        self.store
            .write_local(&record)
            .await
            .map_err(RepositoryError::Store)?;
        Ok(Some(record))
    }

    async fn next_stamp(&self) -> Result<Hlc, RepositoryError<S::Error>> {
        let stamp = self
            .clock
            .lock()
            .expect("reloj sin envenenar")
            .tick((self.now_millis)());
        self.store
            .set_meta(LAST_KEY, &stamp.0)
            .await
            .map_err(RepositoryError::Store)?;
        Ok(stamp)
    }

    fn observe(&self, stamp: &Hlc) {
        self.clock
            .lock()
            .expect("reloj sin envenenar")
            .observe(stamp);
    }

    fn last_stamp(&self) -> Hlc {
        self.clock.lock().expect("reloj sin envenenar").last()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    /// Registros subidos; el servidor descarta los que ya tenía en una versión igual o más nueva.
    pub pushed: usize,
    /// Registros del servidor aplicados localmente.
    pub pulled: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError<SE: std::error::Error + 'static, RE: std::error::Error + 'static> {
    #[error("almacén local: {0}")]
    Store(#[source] SE),
    #[error("servidor: {0}")]
    Remote(#[source] RE),
}

/// Sube la cola local y baja lo nuevo del servidor. Si falla a mitad, se
/// puede repetir: nada se pierde ni se duplica.
pub async fn sync<S: LocalStore, R: Remote>(
    repo: &Repository<S>,
    remote: &R,
    batch_size: usize,
) -> Result<SyncReport, SyncError<S::Error, R::Error>> {
    let batch_size = batch_size.max(1);
    let store = repo.store();
    let mut report = SyncReport::default();

    loop {
        let entries = store.outbox(batch_size).await.map_err(SyncError::Store)?;
        if entries.is_empty() {
            break;
        }
        // Se sube la versión actual de cada registro, una vez por lote.
        let mut by_table: BTreeMap<Table, BTreeMap<Uuid, Row>> = BTreeMap::new();
        for entry in &entries {
            if let Some(record) = store
                .get(entry.table, entry.id)
                .await
                .map_err(SyncError::Store)?
            {
                by_table
                    .entry(entry.table)
                    .or_default()
                    .insert(entry.id, record.upload_row());
            }
        }
        for (table, rows) in by_table {
            report.pushed += rows.len();
            remote
                .push(table, rows.into_values().collect())
                .await
                .map_err(SyncError::Remote)?;
        }
        store.ack(&entries).await.map_err(SyncError::Store)?;
    }

    for table in Table::ALL {
        let mut cursor = store.cursor(table).await.map_err(SyncError::Store)?;
        loop {
            let records = remote
                .pull(table, cursor, batch_size)
                .await
                .map_err(SyncError::Remote)?;
            let Some(max_seq) = records.iter().filter_map(Record::server_seq).max() else {
                break;
            };
            for record in &records {
                repo.observe(&record.updated_at);
            }
            report.pulled += store
                .apply_remote(&records)
                .await
                .map_err(SyncError::Store)?;
            if max_seq <= cursor {
                break;
            }
            cursor = max_seq;
            store
                .set_cursor(table, cursor)
                .await
                .map_err(SyncError::Store)?;
            if records.len() < batch_size {
                break;
            }
        }
    }

    store
        .set_meta(LAST_KEY, &repo.last_stamp().0)
        .await
        .map_err(SyncError::Store)?;
    Ok(report)
}
