use crate::record::{Record, Row};
use crate::table::Table;
use uuid::Uuid;

/// Cambio local pendiente de subir. Varias entradas pueden apuntar al mismo registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxEntry {
    pub seq: i64,
    pub table: Table,
    pub id: Uuid,
}

/// Almacén local: SQLite en nativo, IndexedDB en web.
#[allow(async_fn_in_trait)]
pub trait LocalStore {
    type Error: std::error::Error + 'static;

    /// Incluye registros borrados (papelera).
    async fn get(&self, table: Table, id: Uuid) -> Result<Option<Record>, Self::Error>;
    async fn list(&self, table: Table) -> Result<Vec<Record>, Self::Error>;
    /// Guarda el registro y lo encola, en la misma transacción.
    async fn write_local(&self, record: &Record) -> Result<(), Self::Error>;
    /// Entradas más antiguas primero.
    async fn outbox(&self, limit: usize) -> Result<Vec<OutboxEntry>, Self::Error>;
    async fn ack(&self, entries: &[OutboxEntry]) -> Result<(), Self::Error>;
    /// Aplica registros del servidor si ganan al local. No los encola.
    /// Devuelve cuántos se aplicaron.
    async fn apply_remote(&self, records: &[Record]) -> Result<usize, Self::Error>;
    async fn cursor(&self, table: Table) -> Result<i64, Self::Error>;
    async fn set_cursor(&self, table: Table, seq: i64) -> Result<(), Self::Error>;
    async fn meta(&self, key: &str) -> Result<Option<String>, Self::Error>;
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), Self::Error>;
}

/// Servidor: Supabase en producción, memoria en las pruebas.
#[allow(async_fn_in_trait)]
pub trait Remote {
    type Error: std::error::Error + 'static;

    /// Inserta o actualiza; el servidor ignora las filas que no ganan.
    async fn push(&self, table: Table, rows: Vec<Row>) -> Result<(), Self::Error>;
    /// Filas con `server_seq > after`, en orden creciente.
    async fn pull(
        &self,
        table: Table,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Record>, Self::Error>;
}
