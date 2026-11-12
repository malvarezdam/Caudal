//! Casos de uso compartidos por las UI de egui y Leptos. Las UI solo dibujan:
//! los formularios se validan, los datos se preparan y las escrituras se hacen aquí.

pub mod analysis;
pub mod attachments;
pub mod auth;
pub mod backup;
pub mod catalog;
pub mod charts;
mod core;
pub mod csv_io;
mod data;
pub mod debts;
pub mod defaults;
pub mod format;
pub mod forms;
pub mod i18n;
pub mod investments;
pub mod notices;
pub mod planning;
pub mod privacy;
pub mod recurring;
pub mod rules;
pub mod views;

pub use crate::core::{AppCore, AppError, Market, SyncState};
pub use crate::data::Data;
pub use caudal_core::model;
pub use caudal_core::{Currency, Money};
pub use caudal_sync::{LocalStore, Table};
pub use i18n::{Lang, Texts, texts};

/// Monedas que se ofrecen primero; se puede escribir cualquier otra.
pub const COMMON_CURRENCIES: [&str; 14] = [
    "USD", "EUR", "CLP", "ARS", "MXN", "COP", "PEN", "BRL", "UYU", "GBP", "JPY", "CAD", "BTC",
    "ETH",
];
