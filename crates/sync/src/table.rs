/// Tablas sincronizables; el nombre coincide con el de Supabase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Table {
    Profiles,
    Accounts,
    Categories,
    Tags,
    Transactions,
    TransactionSplits,
    Contacts,
    Debts,
    DebtInstallments,
    DebtPayments,
    RecurringRules,
    Budgets,
    Goals,
    GoalContributions,
    Assets,
    Trades,
    PriceAlerts,
    CategoryRules,
    TransactionTemplates,
    Attachments,
}

impl Table {
    pub const ALL: [Table; 20] = [
        Table::Profiles,
        Table::Accounts,
        Table::Categories,
        Table::Tags,
        Table::Transactions,
        Table::TransactionSplits,
        Table::Contacts,
        Table::Debts,
        Table::DebtInstallments,
        Table::DebtPayments,
        Table::RecurringRules,
        Table::Budgets,
        Table::Goals,
        Table::GoalContributions,
        Table::Assets,
        Table::Trades,
        Table::PriceAlerts,
        Table::CategoryRules,
        Table::TransactionTemplates,
        Table::Attachments,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Table::Profiles => "profiles",
            Table::Accounts => "accounts",
            Table::Categories => "categories",
            Table::Tags => "tags",
            Table::Transactions => "transactions",
            Table::TransactionSplits => "transaction_splits",
            Table::Contacts => "contacts",
            Table::Debts => "debts",
            Table::DebtInstallments => "debt_installments",
            Table::DebtPayments => "debt_payments",
            Table::RecurringRules => "recurring_rules",
            Table::Budgets => "budgets",
            Table::Goals => "goals",
            Table::GoalContributions => "goal_contributions",
            Table::Assets => "assets",
            Table::Trades => "trades",
            Table::PriceAlerts => "price_alerts",
            Table::CategoryRules => "category_rules",
            Table::TransactionTemplates => "transaction_templates",
            Table::Attachments => "attachments",
        }
    }

    pub fn parse(name: &str) -> Option<Table> {
        Table::ALL.into_iter().find(|table| table.as_str() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_unicos_y_reversibles() {
        for table in Table::ALL {
            assert_eq!(Table::parse(table.as_str()), Some(table));
        }
        let mut names: Vec<_> = Table::ALL.iter().map(|table| table.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Table::ALL.len());
    }

    #[test]
    fn coincide_con_la_migracion() {
        let migration = [
            include_str!("../../../supabase/migrations/0001_schema.sql"),
            include_str!("../../../supabase/migrations/0002_prices.sql"),
            include_str!("../../../supabase/migrations/0003_data.sql"),
        ]
        .concat();
        for table in Table::ALL {
            assert!(
                migration.contains(&format!("create table public.{} (", table.as_str())),
                "{} no está en la migración",
                table.as_str()
            );
        }
    }
}
