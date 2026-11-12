//! egui redibuja en cada fotograma: las vistas se recalculan solo cuando
//! cambian los datos, el idioma, el día o sus parámetros.

use caudal_app_state::Lang;
use caudal_app_state::analysis::Period;
use caudal_app_state::analysis::ReportsView;
use caudal_app_state::debts::DebtsView;
use caudal_app_state::planning::{BudgetView, GoalView};
use caudal_app_state::recurring::{RecurringView, UpcomingDay};
use caudal_app_state::views::{AccountView, DashboardView, TransactionFilter, TransactionView};
use chrono::NaiveDate;

pub struct Memo<K, V> {
    slot: Option<(K, V)>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self { slot: None }
    }
}

impl<K: PartialEq, V: Clone> Memo<K, V> {
    pub fn get(&mut self, key: K, compute: impl FnOnce() -> V) -> V {
        if let Some((cached, value)) = &self.slot
            && *cached == key
        {
            return value.clone();
        }
        let value = compute();
        self.slot = Some((key, value.clone()));
        value
    }
}

/// Identifica una versión concreta de los datos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub data: usize,
    pub lang: Lang,
    pub today: NaiveDate,
    pub private: bool,
}

#[derive(Default)]
pub struct ViewCache {
    pub dashboard: Memo<Key, DashboardView>,
    pub pending: Memo<Key, Vec<TransactionView>>,
    pub upcoming: Memo<(Key, u64), Vec<UpcomingDay>>,
    pub transactions: Memo<(Key, TransactionFilter, usize), (Vec<TransactionView>, usize)>,
    pub accounts: Memo<(Key, bool), Vec<AccountView>>,
    pub debts: Memo<Key, DebtsView>,
    pub recurring: Memo<Key, RecurringView>,
    pub reports: Memo<(Key, Period), ReportsView>,
    pub budgets: Memo<Key, Vec<BudgetView>>,
    pub goals: Memo<Key, Vec<GoalView>>,
    pub portfolio: Memo<Key, caudal_app_state::investments::PortfolioView>,
    pub alerts: Memo<Key, Vec<caudal_app_state::investments::AlertView>>,
    pub notices: Memo<Key, Vec<caudal_app_state::notices::Notice>>,
    pub activity: Memo<Key, Vec<caudal_app_state::notices::ActivityView>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn solo_recalcula_si_cambia_la_clave() {
        let calls = Cell::new(0);
        let mut memo: Memo<u32, u32> = Memo::default();
        let compute = || {
            calls.set(calls.get() + 1);
            7
        };
        assert_eq!(memo.get(1, compute), 7);
        assert_eq!(memo.get(1, compute), 7);
        assert_eq!(calls.get(), 1);
        memo.get(2, compute);
        assert_eq!(calls.get(), 2);
    }
}
