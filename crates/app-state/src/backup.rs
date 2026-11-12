//! Copia completa de los datos del usuario: exportar, borrar y restaurar.
//! El archivo es JSON legible, con todas las tablas y la papelera.

use crate::core::{AppCore, AppError};
use caudal_core::model::{
    Account, Asset, Attachment, Budget, Category, CategoryRule, Contact, Debt, DebtInstallment,
    DebtPayment, Goal, GoalContribution, PriceAlert, Profile, RecurringRule, Tag, Trade,
    Transaction, TransactionSplit, TransactionTemplate,
};
use caudal_sync::{LocalStore, Record, Row, Table};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use uuid::Uuid;

pub const BACKUP_FORMAT: &str = "caudal-backup";
pub const BACKUP_VERSION: u64 = 1;

/// Columnas que dependen de la cuenta o del servidor: no viajan en la copia.
const LOCAL_COLUMNS: [&str; 3] = ["user_id", "updated_at", "server_seq"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: usize,
    /// Filas que no se entienden o que no pueden ir en esta cuenta.
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupError {
    NotABackup,
    NewerVersion,
}

/// Fila → modelo → fila: descarta columnas desconocidas y rechaza datos
/// inválidos antes de que lleguen a la cola de subida.
fn normalize<T: DeserializeOwned + Serialize>(row: &Row) -> Option<Row> {
    let model: T = serde_json::from_value(Value::Object(row.clone())).ok()?;
    match serde_json::to_value(model).ok()? {
        Value::Object(row) => Some(row),
        _ => None,
    }
}

fn normalize_row(table: Table, row: &Row) -> Option<Row> {
    match table {
        Table::Profiles => normalize::<Profile>(row),
        Table::Accounts => normalize::<Account>(row),
        Table::Categories => normalize::<Category>(row),
        Table::Tags => normalize::<Tag>(row),
        Table::Transactions => normalize::<Transaction>(row),
        Table::TransactionSplits => normalize::<TransactionSplit>(row),
        Table::Contacts => normalize::<Contact>(row),
        Table::Debts => normalize::<Debt>(row),
        Table::DebtInstallments => normalize::<DebtInstallment>(row),
        Table::DebtPayments => normalize::<DebtPayment>(row),
        Table::RecurringRules => normalize::<RecurringRule>(row),
        Table::Budgets => normalize::<Budget>(row),
        Table::Goals => normalize::<Goal>(row),
        Table::GoalContributions => normalize::<GoalContribution>(row),
        Table::Assets => normalize::<Asset>(row),
        Table::Trades => normalize::<Trade>(row),
        Table::PriceAlerts => normalize::<PriceAlert>(row),
        Table::CategoryRules => normalize::<CategoryRule>(row),
        Table::TransactionTemplates => normalize::<TransactionTemplate>(row),
        Table::Attachments => normalize::<Attachment>(row),
    }
}

impl<S: LocalStore> AppCore<S> {
    /// Todas las tablas, incluida la papelera, como JSON.
    pub async fn export_backup(&self, exported_on: &str) -> Result<String, AppError> {
        let mut tables = Map::new();
        for table in Table::ALL {
            let mut records: Vec<Record> = self
                .repo()
                .store()
                .list(table)
                .await
                .map_err(AppError::storage)?;
            records.sort_by_key(|record| record.id);
            let rows: Vec<Value> = records
                .into_iter()
                .map(|record| {
                    let mut row = record.row;
                    for column in LOCAL_COLUMNS {
                        row.remove(column);
                    }
                    Value::Object(row)
                })
                .collect();
            tables.insert(table.as_str().to_owned(), Value::Array(rows));
        }
        serde_json::to_string_pretty(&json!({
            "format": BACKUP_FORMAT,
            "version": BACKUP_VERSION,
            "exported_on": exported_on,
            "tables": tables,
        }))
        .map_err(AppError::storage)
    }

    /// Manda a la papelera todo salvo el perfil. Se puede deshacer restaurando
    /// una copia o recuperando desde la papelera.
    pub async fn delete_all_data(&self) -> Result<usize, AppError> {
        let mut count = 0;
        for table in Table::ALL {
            if table == Table::Profiles {
                continue;
            }
            for record in self
                .repo()
                .list_active(table)
                .await
                .map_err(AppError::storage)?
            {
                self.repo()
                    .delete(table, record.id)
                    .await
                    .map_err(AppError::storage)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Vuelve a guardar cada fila de la copia en la cuenta actual: lo que está
    /// en la copia gana a lo que haya (también a lo borrado). Lo que no está
    /// en la copia no se toca.
    pub async fn restore_backup(&self, text: &str) -> Result<RestoreReport, RestoreError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| RestoreError::Format(BackupError::NotABackup))?;
        if value.get("format").and_then(Value::as_str) != Some(BACKUP_FORMAT) {
            return Err(RestoreError::Format(BackupError::NotABackup));
        }
        if value.get("version").and_then(Value::as_u64).unwrap_or(0) > BACKUP_VERSION {
            return Err(RestoreError::Format(BackupError::NewerVersion));
        }
        let Some(Value::Object(tables)) = value.get("tables") else {
            return Err(RestoreError::Format(BackupError::NotABackup));
        };
        let user = self.user_id();
        let mut report = RestoreReport::default();
        for table in Table::ALL {
            let Some(Value::Array(rows)) = tables.get(table.as_str()) else {
                continue;
            };
            for row in rows {
                let Value::Object(row) = row else {
                    report.skipped += 1;
                    continue;
                };
                let mut row = row.clone();
                // Columnas que se rellenan aquí, para validar con el modelo.
                row.insert("user_id".into(), Value::String(user.to_string()));
                row.insert("updated_at".into(), Value::String(String::new()));
                if table == Table::Profiles {
                    // El perfil es uno por cuenta y lleva su id.
                    row.insert("id".into(), Value::String(user.to_string()));
                }
                if table == Table::Attachments {
                    // El archivo vive en la carpeta de la cuenta que lo subió.
                    let id = row
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| Uuid::parse_str(id).ok());
                    let path = row.get("storage_path").and_then(Value::as_str);
                    if id.is_none_or(|id| path != Some(format!("{user}/{id}").as_str())) {
                        report.skipped += 1;
                        continue;
                    }
                }
                let Some(row) = normalize_row(table, &row) else {
                    report.skipped += 1;
                    continue;
                };
                self.repo()
                    .save(table, row)
                    .await
                    .map_err(|error| RestoreError::App(AppError::storage(error)))?;
                report.restored += 1;
            }
        }
        Ok(report)
    }
}

#[derive(Debug)]
pub enum RestoreError {
    Format(BackupError),
    App(AppError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::demo;
    use crate::i18n::Lang;
    use caudal_sync::memory::MemoryStore;
    use chrono::NaiveDate;
    use pollster::block_on;

    const USER: Uuid = Uuid::from_u128(0xA11CE);

    async fn seeded(user: Uuid) -> AppCore<MemoryStore> {
        let core = AppCore::open(MemoryStore::default(), user, || 5_000, || 3)
            .await
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let demo = demo::sample(user, today, Lang::Es);
        core.save(Table::Profiles, &demo.profile).await.unwrap();
        for account in &demo.accounts {
            core.save(Table::Accounts, account).await.unwrap();
        }
        for category in &demo.categories {
            core.save(Table::Categories, category).await.unwrap();
        }
        for tx in demo.transactions.iter().chain(&demo.history) {
            core.save(Table::Transactions, tx).await.unwrap();
        }
        core.save_debt(&demo.debt).await.unwrap();
        core.save(Table::Budgets, &demo.budget).await.unwrap();
        core.save(Table::Goals, &demo.goal).await.unwrap();
        for item in &demo.contributions {
            core.save(Table::GoalContributions, item).await.unwrap();
        }
        for rule in &demo.rules {
            core.save(Table::RecurringRules, rule).await.unwrap();
        }
        for asset in &demo.assets {
            core.save(Table::Assets, asset).await.unwrap();
        }
        for draft in &demo.trades {
            core.save_trade(draft).await.unwrap();
        }
        for alert in &demo.alerts {
            core.save(Table::PriceAlerts, alert).await.unwrap();
        }
        // Algo en la papelera también debe sobrevivir.
        let tx = &demo.transactions[0];
        core.delete(Table::Transactions, tx.meta.id).await.unwrap();
        core
    }

    fn tables(text: &str) -> Value {
        serde_json::from_str::<Value>(text).unwrap()["tables"].clone()
    }

    #[test]
    fn exportar_borrar_y_restaurar_deja_la_cuenta_identica() {
        block_on(async {
            let core = seeded(USER).await;
            let before = core.export_backup("2026-11-12").await.unwrap();
            let data_before = core.load().await.unwrap();

            let deleted = core.delete_all_data().await.unwrap();
            assert!(deleted > 50);
            let empty = core.load().await.unwrap();
            assert!(empty.transactions.is_empty() && empty.accounts.is_empty());
            assert!(empty.profile.is_some(), "el perfil se conserva");

            let report = core.restore_backup(&before).await.unwrap();
            assert_eq!(report.skipped, 0);
            let after = core.export_backup("2026-11-12").await.unwrap();
            assert_eq!(tables(&before), tables(&after));

            let data_after = core.load().await.unwrap();
            assert_eq!(
                data_before.transactions,
                data_after
                    .transactions
                    .iter()
                    .map(|tx| {
                        let mut tx = tx.clone();
                        let original = data_before.transaction(tx.meta.id).unwrap();
                        tx.meta.updated_at = original.meta.updated_at.clone();
                        tx
                    })
                    .collect::<Vec<_>>()
            );
            assert_eq!(data_before.trash.len(), data_after.trash.len());
            let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
            assert_eq!(
                crate::views::dashboard(&data_before, today, Lang::Es),
                crate::views::dashboard(&data_after, today, Lang::Es)
            );
        });
    }

    #[test]
    fn restaurar_en_otra_cuenta_reescribe_el_dueno() {
        block_on(async {
            let backup = seeded(USER)
                .await
                .export_backup("2026-11-12")
                .await
                .unwrap();
            let other = Uuid::from_u128(0xB0B);
            let core = AppCore::open(MemoryStore::default(), other, || 9_000, || 4)
                .await
                .unwrap();
            let report = core.restore_backup(&backup).await.unwrap();
            assert!(report.restored > 50);
            let data = core.load().await.unwrap();
            assert_eq!(data.profile.as_ref().unwrap().meta.id, other);
            assert!(data.accounts.iter().all(|a| a.meta.user_id == other));
            assert_eq!(data.accounts.len(), 3);
        });
    }

    #[test]
    fn rechaza_archivos_ajenos_y_filas_invalidas() {
        block_on(async {
            let core = AppCore::open(MemoryStore::default(), USER, || 1, || 1)
                .await
                .unwrap();
            assert!(matches!(
                core.restore_backup("{\"hola\": 1}").await,
                Err(RestoreError::Format(BackupError::NotABackup))
            ));
            assert!(matches!(
                core.restore_backup("no es json").await,
                Err(RestoreError::Format(BackupError::NotABackup))
            ));
            let newer = json!({"format": BACKUP_FORMAT, "version": 99, "tables": {}}).to_string();
            assert!(matches!(
                core.restore_backup(&newer).await,
                Err(RestoreError::Format(BackupError::NewerVersion))
            ));
            let odd = json!({
                "format": BACKUP_FORMAT,
                "version": 1,
                "tables": {
                    "tags": [
                        {"id": Uuid::from_u128(1).to_string(), "name": "ok", "extra": "se descarta"},
                        {"id": "no-es-uuid", "name": "mala"},
                        {"name": "sin id"},
                        "texto"
                    ],
                    "attachments": [{
                        "id": Uuid::from_u128(2).to_string(),
                        "transaction_id": Uuid::from_u128(3).to_string(),
                        "file_name": "a.jpg", "mime_type": "image/jpeg", "size_bytes": 1,
                        "storage_path": "otra-cuenta/x"
                    }],
                    "tabla_desconocida": [{"id": "x"}]
                }
            })
            .to_string();
            let report = core.restore_backup(&odd).await.unwrap();
            assert_eq!(
                report,
                RestoreReport {
                    restored: 1,
                    skipped: 4
                }
            );
            let tag = core
                .repo()
                .get(Table::Tags, Uuid::from_u128(1))
                .await
                .unwrap()
                .unwrap();
            assert!(!tag.row.contains_key("extra"));
        });
    }
}
