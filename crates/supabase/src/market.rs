//! Precios y tipos de cambio que escribe el worker. Los clientes solo leen
//! estas tablas: nunca llaman a los proveedores de mercado.

use crate::remote::normalize_numbers;
use crate::{SupabaseClient, SupabaseError, read_json};
use caudal_core::fx::FxRate;
use caudal_core::model::AssetPrice;
use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeSet;

const PAGE: usize = 1000;

/// Lista para `in.(...)` de PostgREST, con comillas para admitir puntos y guiones.
fn in_list(values: &BTreeSet<String>) -> String {
    let quoted: Vec<String> = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('"', "")))
        .collect();
    format!("in.({})", quoted.join(","))
}

impl SupabaseClient {
    async fn read_all<T: DeserializeOwned>(
        &self,
        table: &str,
        filters: Vec<(&str, String)>,
        order: &str,
    ) -> Result<Vec<T>, SupabaseError> {
        let token = self.access_token().await?;
        let mut out = Vec::new();
        loop {
            let mut query = filters.clone();
            query.push(("order", order.to_owned()));
            query.push(("limit", PAGE.to_string()));
            query.push(("offset", out.len().to_string()));
            let response = self
                .http
                .get(self.endpoint(&format!("/rest/v1/{table}")))
                .query(&query)
                .header("apikey", &self.config.publishable_key)
                .bearer_auth(&token)
                .send()
                .await?;
            let Value::Array(rows) = read_json(response).await? else {
                return Err(SupabaseError::Api {
                    status: 200,
                    code: None,
                    message: format!("{table}: se esperaba una lista"),
                });
            };
            let count = rows.len();
            for row in rows {
                let Value::Object(mut row) = row else {
                    continue;
                };
                normalize_numbers(&mut row);
                // Una fila que no se entiende (moneda rara) no impide leer las demás.
                if let Ok(item) = serde_json::from_value(Value::Object(row)) {
                    out.push(item);
                }
            }
            if count < PAGE {
                return Ok(out);
            }
        }
    }

    /// Precios de estos símbolos desde `since`. Se piden también en mayúsculas
    /// y minúsculas, porque cada usuario pudo escribir el símbolo a su manera.
    pub async fn fetch_prices(
        &self,
        symbols: &BTreeSet<String>,
        since: NaiveDate,
    ) -> Result<Vec<AssetPrice>, SupabaseError> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        let variants: BTreeSet<String> = symbols
            .iter()
            .flat_map(|s| [s.clone(), s.to_uppercase(), s.to_lowercase()])
            .collect();
        self.read_all(
            "asset_prices",
            vec![
                (
                    "select",
                    "provider,symbol,price_on,price,currency".to_owned(),
                ),
                ("symbol", in_list(&variants)),
                ("price_on", format!("gte.{since}")),
            ],
            "price_on.asc,symbol.asc",
        )
        .await
    }

    /// Tipos USD → cada moneda pedida desde `since`.
    pub async fn fetch_rates(
        &self,
        quotes: &BTreeSet<String>,
        since: NaiveDate,
    ) -> Result<Vec<FxRate>, SupabaseError> {
        if quotes.is_empty() {
            return Ok(Vec::new());
        }
        self.read_all(
            "fx_rates",
            vec![
                ("select", "base,quote,rate_on,rate".to_owned()),
                ("base", "eq.USD".to_owned()),
                ("quote", in_list(quotes)),
                ("rate_on", format!("gte.{since}")),
            ],
            "rate_on.asc,quote.asc",
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_con_comillas() {
        let values: BTreeSet<String> = ["AAPL.US".to_owned(), "SQM-B.SN".to_owned()].into();
        assert_eq!(in_list(&values), "in.(\"AAPL.US\",\"SQM-B.SN\")");
    }
}
