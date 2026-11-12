//! Sincronización local primero.
//!
//! Toda escritura va al almacén local y a su cola de salida en la misma
//! transacción. [`sync`] sube la cola y después baja los cambios del servidor
//! desde el último cursor de cada tabla. Los conflictos se resuelven por
//! registro: gana la marca `updated_at` mayor, tanto en el cliente como en el
//! servidor, así que reintentar es siempre seguro.

pub mod backoff;
pub mod memory;
mod record;
mod repository;
mod store;
mod table;

pub use record::{Record, RecordError, Row};
pub use repository::{Repository, RepositoryError, SyncError, SyncReport, sync};
pub use store::{LocalStore, OutboxEntry, Remote};
pub use table::Table;
