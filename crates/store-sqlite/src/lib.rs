//! `LocalStore` nativo sobre SQLite.

use caudal_sync::{LocalStore, OutboxEntry, Record, RecordError, Row, Table};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

const SCHEMA: &str = "
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
    pragma user_version = 1;
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

    fn init(conn: Connection) -> Result<Self, SqliteStoreError> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
}
