//! PostgREST como servidor del motor de sincronización.

use crate::{SupabaseClient, SupabaseError, read_json};
use caudal_sync::{Record, Remote, Row, Table};
use serde_json::Value;
use std::collections::BTreeSet;

/// Columnas enteras: el resto de números son decimales y se pasan a texto.
const INTEGER_COLUMNS: [&str; 3] = ["server_seq", "interval_count", "size_bytes"];

/// PostgREST devuelve `numeric` como número JSON. Con `arbitrary_precision`
/// el texto se conserva exacto y aquí se convierte en cadena, que es lo que
/// espera `rust_decimal`. Nunca pasa por `f64`.
pub fn normalize_numbers(row: &mut Row) {
    for (column, value) in row.iter_mut() {
        if let Value::Number(number) = value
            && !INTEGER_COLUMNS.contains(&column.as_str())
        {
            *value = Value::String(number.to_string());
        }
    }
}

impl Remote for SupabaseClient {
    type Error = SupabaseError;

    async fn push(&self, table: Table, rows: Vec<Row>) -> Result<(), SupabaseError> {
        if rows.is_empty() {
            return Ok(());
        }
        let columns: BTreeSet<&str> = rows
            .iter()
            .flat_map(|row| row.keys().map(String::as_str))
            .collect();
        let columns = columns.into_iter().collect::<Vec<_>>().join(",");
        let token = self.access_token().await?;
        let response = self
            .http
            .post(self.endpoint(&format!("/rest/v1/{}", table.as_str())))
            .query(&[("on_conflict", "id"), ("columns", columns.as_str())])
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .header(
                "Prefer",
                "resolution=merge-duplicates,missing=default,return=minimal",
            )
            .json(&rows)
            .send()
            .await?;
        read_json(response).await.map(|_| ())
    }

    async fn pull(
        &self,
        table: Table,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Record>, SupabaseError> {
        let token = self.access_token().await?;
        let response = self
            .http
            .get(self.endpoint(&format!("/rest/v1/{}", table.as_str())))
            .query(&[
                ("select", "*".to_owned()),
                ("server_seq", format!("gt.{after}")),
                ("order", "server_seq.asc".to_owned()),
                ("limit", limit.to_string()),
            ])
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(token)
            .send()
            .await?;
        let Value::Array(rows) = read_json(response).await? else {
            return Err(SupabaseError::Api {
                status: 200,
                code: None,
                message: "se esperaba una lista de filas".into(),
            });
        };
        rows.into_iter()
            .map(|row| {
                let Value::Object(mut row) = row else {
                    return Err(caudal_sync::RecordError::NotAnObject.into());
                };
                normalize_numbers(&mut row);
                Ok(Record::from_row(table, row)?)
            })
            .collect()
    }
}
