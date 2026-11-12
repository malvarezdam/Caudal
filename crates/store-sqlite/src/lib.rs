//! `LocalStore` nativo sobre SQLite.

use caudal_sync::{LocalStore, OutboxEntry, Record, RecordError, Row, Table};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

const SCHEMA_VERSION_KEY: &str = "schema_version";
const LOCAL_SCHEMA_VERSION: u32 = 2;

const SCHEMA_V1: &str = "
    create table if not exists records (
        tbl text not null,
        id text not null,
        updated_at text not null,
        row text not null,
        primary key (tbl, id)
    ) without rowid;
    create table if not exists outbox (
        seq integer primary key autoincrement,
        tbl text not null,
        id text not null
    );
    create table if not exists cursors (tbl text primary key, seq integer not null);
    create table if not exists meta (key text primary key, value text not null);
";

const MIGRATION_V2: &str = "
    create index if not exists outbox_tbl_id_idx on outbox (tbl, id);
";

#[derive(Debug, thiserror::Error)]
pub enum SqliteStoreError {
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Record(#[from] RecordError),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("dato local dañado: {0}")]
    Corrupt(String),
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteStoreError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, SqliteStoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, SqliteStoreError> {
        Self::migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn migrate(conn: &mut Connection) -> Result<(), SqliteStoreError> {
        conn.execute_batch(SCHEMA_V1)?;
        let current: u32 = match conn
            .query_row(
                "select value from meta where key = ?1",
                [SCHEMA_VERSION_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            Some(text) => text
                .parse::<u32>()
                .map_err(|_| SqliteStoreError::Corrupt(format!("{SCHEMA_VERSION_KEY}: {text}")))?,
            None => 1,
        };
        if current > LOCAL_SCHEMA_VERSION {
            return Err(SqliteStoreError::Corrupt(format!(
                "versión de esquema no soportada: {current} > {LOCAL_SCHEMA_VERSION}"
            )));
        }
        for target in (current + 1)..=LOCAL_SCHEMA_VERSION {
            let tx = conn.transaction()?;
            match target {
                2 => tx.execute_batch(MIGRATION_V2)?,
                _ => unreachable!("migración {target} no definida"),
            }
            tx.execute(
                "insert into meta (key, value) values (?1, ?2)
                 on conflict (key) do update set value = excluded.value",
                params![SCHEMA_VERSION_KEY, target.to_string()],
            )?;
            tx.pragma_update(None, "user_version", target)?;
            tx.commit()?;
        }
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("conexión sin envenenar")
    }
}

fn table(name: &str) -> Result<Table, SqliteStoreError> {
    Table::parse(name).ok_or_else(|| SqliteStoreError::Corrupt(format!("tabla {name}")))
}

fn uuid(text: &str) -> Result<Uuid, SqliteStoreError> {
    Uuid::parse_str(text).map_err(|_| SqliteStoreError::Corrupt(format!("id {text}")))
}

fn decode(table: Table, row: &str) -> Result<Record, SqliteStoreError> {
    let row: Row = serde_json::from_str(row)?;
    Ok(Record::from_row(table, row)?)
}

fn encode(record: &Record) -> Result<String, SqliteStoreError> {
    Ok(serde_json::to_string(&record.row)?)
}

impl LocalStore for SqliteStore {
    type Error = SqliteStoreError;

    async fn get(&self, table: Table, id: Uuid) -> Result<Option<Record>, SqliteStoreError> {
        let row: Option<String> = self
            .lock()
            .query_row(
                "select row from records where tbl = ?1 and id = ?2",
                params![table.as_str(), id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        row.map(|row| decode(table, &row)).transpose()
    }

    async fn list(&self, table: Table) -> Result<Vec<Record>, SqliteStoreError> {
        let conn = self.lock();
        let mut statement = conn.prepare("select row from records where tbl = ?1 order by id")?;
        let rows = statement.query_map([table.as_str()], |row| row.get::<_, String>(0))?;
        rows.map(|row| decode(table, &row?)).collect()
    }

    async fn write_local(&self, record: &Record) -> Result<(), SqliteStoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "insert into records (tbl, id, updated_at, row) values (?1, ?2, ?3, ?4)
             on conflict (tbl, id) do update set updated_at = excluded.updated_at, row = excluded.row",
            params![
                record.table.as_str(),
                record.id.to_string(),
                record.updated_at.0,
                encode(record)?
            ],
        )?;
        tx.execute(
            "insert into outbox (tbl, id) values (?1, ?2)",
            params![record.table.as_str(), record.id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    async fn outbox(&self, limit: usize) -> Result<Vec<OutboxEntry>, SqliteStoreError> {
        let conn = self.lock();
        let mut statement =
            conn.prepare("select seq, tbl, id from outbox order by seq limit ?1")?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = statement.query_map([limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (seq, tbl, id) = row?;
            Ok(OutboxEntry {
                seq,
                table: table(&tbl)?,
                id: uuid(&id)?,
            })
        })
        .collect()
    }

    async fn ack(&self, entries: &[OutboxEntry]) -> Result<(), SqliteStoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        for entry in entries {
            tx.execute("delete from outbox where seq = ?1", [entry.seq])?;
        }
        tx.commit()?;
        Ok(())
    }

    async fn apply_remote(&self, records: &[Record]) -> Result<usize, SqliteStoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let mut applied = 0;
        for record in records {
            // Comparación binaria de texto: el mismo orden que la collation "C" del servidor.
            applied += tx.execute(
                "insert into records (tbl, id, updated_at, row) values (?1, ?2, ?3, ?4)
                 on conflict (tbl, id) do update set updated_at = excluded.updated_at, row = excluded.row
                 where excluded.updated_at > records.updated_at",
                params![
                    record.table.as_str(),
                    record.id.to_string(),
                    record.updated_at.0,
                    encode(record)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(applied)
    }

    async fn cursor(&self, table: Table) -> Result<i64, SqliteStoreError> {
        let seq = self
            .lock()
            .query_row(
                "select seq from cursors where tbl = ?1",
                [table.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(seq.unwrap_or(0))
    }

    async fn set_cursor(&self, table: Table, seq: i64) -> Result<(), SqliteStoreError> {
        self.lock().execute(
            "insert into cursors (tbl, seq) values (?1, ?2)
             on conflict (tbl) do update set seq = excluded.seq",
            params![table.as_str(), seq],
        )?;
        Ok(())
    }

    async fn meta(&self, key: &str) -> Result<Option<String>, SqliteStoreError> {
        Ok(self
            .lock()
            .query_row("select value from meta where key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    async fn set_meta(&self, key: &str, value: &str) -> Result<(), SqliteStoreError> {
        self.lock().execute(
            "insert into meta (key, value) values (?1, ?2)
             on conflict (key) do update set value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudal_sync::memory::{MemoryRemote, MemoryStore};
    use caudal_sync::{Repository, sync};
    use pollster::block_on;
    use serde_json::json;

    const USER: Uuid = Uuid::from_u128(7);

    fn tag(id: u128, name: &str) -> Row {
        json!({"id": Uuid::from_u128(id).to_string(), "name": name, "color": null, "amount": "0.100000000000"})
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn guarda_y_encola_en_una_transaccion() {
        let repo = block_on(Repository::open(
            SqliteStore::open_in_memory().unwrap(),
            USER,
            || 1_000,
            || 1,
        ))
        .unwrap();
        block_on(repo.save(Table::Tags, tag(1, "Economía ñandú"))).unwrap();
        block_on(repo.save(Table::Tags, tag(1, "Economía ñandú 2"))).unwrap();

        let stored = block_on(repo.get(Table::Tags, Uuid::from_u128(1)))
            .unwrap()
            .unwrap();
        assert_eq!(stored.row["name"], "Economía ñandú 2");
        assert_eq!(stored.row["amount"], "0.100000000000");
        let outbox = block_on(repo.store().outbox(10)).unwrap();
        assert_eq!(outbox.len(), 2);
        block_on(repo.store().ack(&outbox[..1])).unwrap();
        assert_eq!(block_on(repo.store().outbox(10)).unwrap().len(), 1);
    }

    #[test]
    fn solo_aplica_lo_remoto_si_es_mas_nuevo() {
        let store = SqliteStore::open_in_memory().unwrap();
        let make = |stamp: &str, name: &str| {
            let mut row = tag(1, name);
            row.insert("updated_at".into(), json!(stamp));
            Record::from_row(Table::Tags, row).unwrap()
        };
        assert_eq!(
            block_on(store.apply_remote(&[make("0002", "nuevo")])).unwrap(),
            1
        );
        assert_eq!(
            block_on(store.apply_remote(&[make("0001", "viejo")])).unwrap(),
            0
        );
        assert_eq!(
            block_on(store.apply_remote(&[make("0002", "igual")])).unwrap(),
            0
        );
        let stored = block_on(store.get(Table::Tags, Uuid::from_u128(1)))
            .unwrap()
            .unwrap();
        assert_eq!(stored.row["name"], "nuevo");
        assert!(block_on(store.outbox(10)).unwrap().is_empty());
    }

    #[test]
    fn persiste_en_disco_cursor_reloj_y_cola() {
        let dir = std::env::temp_dir().join(format!("caudal-sqlite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("local.db");
        let _ = std::fs::remove_file(&path);
        let first = {
            let repo = block_on(Repository::open(
                SqliteStore::open(&path).unwrap(),
                USER,
                || 5_000,
                || 3,
            ))
            .unwrap();
            block_on(repo.store().set_cursor(Table::Tags, 42)).unwrap();
            block_on(repo.save(Table::Tags, tag(1, "a"))).unwrap()
        };
        let repo = block_on(Repository::open(
            SqliteStore::open(&path).unwrap(),
            USER,
            || 1,
            || 99,
        ))
        .unwrap();
        assert_eq!(block_on(repo.store().cursor(Table::Tags)).unwrap(), 42);
        assert_eq!(block_on(repo.store().outbox(10)).unwrap().len(), 1);
        let second = block_on(repo.save(Table::Tags, tag(1, "b"))).unwrap();
        assert!(second.updated_at > first.updated_at);
        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn converge_con_un_dispositivo_en_memoria() {
        let remote = MemoryRemote::default();
        let a = block_on(Repository::open(
            SqliteStore::open_in_memory().unwrap(),
            USER,
            || 1_000,
            || 0xA,
        ))
        .unwrap();
        let b = block_on(Repository::open(
            MemoryStore::default(),
            USER,
            || 2_000,
            || 0xB,
        ))
        .unwrap();

        block_on(a.save(Table::Tags, tag(1, "de A"))).unwrap();
        block_on(b.save(Table::Tags, tag(2, "de B"))).unwrap();
        block_on(sync(&a, &remote, 1)).unwrap();
        block_on(sync(&b, &remote, 1)).unwrap();
        block_on(b.save(Table::Tags, tag(1, "A editado en B"))).unwrap();
        block_on(b.delete(Table::Tags, Uuid::from_u128(2))).unwrap();
        block_on(sync(&b, &remote, 1)).unwrap();
        block_on(sync(&a, &remote, 1)).unwrap();

        let names = |records: Vec<Record>| {
            records
                .into_iter()
                .map(|record| (record.id, record.row["name"].clone(), record.is_deleted()))
                .collect::<Vec<_>>()
        };
        let in_a = names(block_on(a.store().list(Table::Tags)).unwrap());
        let in_b = names(block_on(b.store().list(Table::Tags)).unwrap());
        assert_eq!(in_a, in_b);
        assert_eq!(in_a[0].1, "A editado en B");
        assert!(in_a[1].2);
    }

    #[test]
    fn migra_una_base_version_1_creada_a_mano_y_la_deja_usable() {
        let dir = std::env::temp_dir().join(format!("caudal-sqlite-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v1.db");
        let _ = std::fs::remove_file(&path);

        let id = Uuid::from_u128(101);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "create table records (
                    tbl text not null,
                    id text not null,
                    updated_at text not null,
                    row text not null,
                    primary key (tbl, id)
                ) without rowid;
                create table outbox (
                    seq integer primary key autoincrement,
                    tbl text not null,
                    id text not null
                );
                create table cursors (tbl text primary key, seq integer not null);
                create table meta (key text primary key, value text not null);
                pragma user_version = 1;",
            )
            .unwrap();
            let mut row = tag(101, "Etiqueta de v1");
            row.insert("user_id".into(), json!(USER.to_string()));
            row.insert("updated_at".into(), json!("0000019000000000-0001-0001"));
            row.insert("deleted_at".into(), json!(null));
            conn.execute(
                "insert into records (tbl, id, updated_at, row) values (?1, ?2, ?3, ?4)",
                params![
                    "tags",
                    id.to_string(),
                    "0000019000000000-0001-0001",
                    serde_json::to_string(&row).unwrap()
                ],
            )
            .unwrap();
            conn.execute(
                "insert into outbox (tbl, id) values ('tags', ?1)",
                [id.to_string()],
            )
            .unwrap();
            conn.execute("insert into cursors (tbl, seq) values ('tags', 17)", [])
                .unwrap();
            conn.execute(
                "insert into meta (key, value) values ('hlc', '0000019000000000-0001-0001')",
                [],
            )
            .unwrap();
        }

        let repo = block_on(Repository::open(
            SqliteStore::open(&path).unwrap(),
            USER,
            || 2_000,
            || 2,
        ))
        .unwrap();

        assert_eq!(
            block_on(repo.store().meta(SCHEMA_VERSION_KEY)).unwrap(),
            Some(LOCAL_SCHEMA_VERSION.to_string())
        );
        let from_v1 = block_on(repo.get(Table::Tags, id)).unwrap().unwrap();
        assert_eq!(from_v1.row["name"], "Etiqueta de v1");
        assert_eq!(block_on(repo.store().cursor(Table::Tags)).unwrap(), 17);
        assert_eq!(block_on(repo.store().outbox(10)).unwrap().len(), 1);

        // El repositorio puede seguir escribiendo y leyendo tras migrar.
        block_on(repo.save(Table::Tags, tag(102, "Creada en v2"))).unwrap();
        assert_eq!(block_on(repo.list_active(Table::Tags)).unwrap().len(), 2);
        assert_eq!(block_on(repo.store().outbox(10)).unwrap().len(), 2);
        drop(repo);

        // Reabrir en v2 es idempotente y rechaza versiones futuras desconocidas.
        let store = SqliteStore::open(&path).unwrap();
        block_on(store.set_meta(SCHEMA_VERSION_KEY, "999")).unwrap();
        drop(store);
        assert!(matches!(
            SqliteStore::open(&path),
            Err(SqliteStoreError::Corrupt(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
