//! Dominio y cálculos puros. Sin IO; compila a nativo y WASM.

pub mod fx;
pub mod ledger;
pub mod model;
pub mod money;
pub mod summary;

pub use fx::{FxRate, FxTable};
pub use money::{Currency, Money, MoneyError};
