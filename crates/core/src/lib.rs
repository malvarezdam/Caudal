//! Dominio y cálculos puros. Sin IO; compila a nativo y WASM.

pub mod budgets;
pub mod debts;
pub mod fx;
pub mod hlc;
pub mod ledger;
pub mod model;
pub mod money;
pub mod portfolio;
pub mod recurrence;
pub mod series;
pub mod summary;

pub use fx::{FxRate, FxTable};
pub use hlc::HlcClock;
pub use money::{Currency, Money, MoneyError};
