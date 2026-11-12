//! Lectura y escritura en Supabase con la clave secreta del worker.
//! La clave solo existe en los secretos de GitHub Actions.

use crate::sources::{Held, SourceError, client};
use caudal_core::fx::FxRate;
use caudal_core::model::{AssetPrice, PriceProviderId};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Primera y última fecha con dato de un símbolo o par de monedas.
pub type Coverage<K> = BTreeMap<K, (NaiveDate, NaiveDate)>;

/// Resumen de una ejecución, que queda en `price_runs`.
#[derive(Debug, Clone, Serialize)]
pub struct RunLog {
    pub started_at: DateTime<Utc>,
    pub ok: bool,
    pub symbols: usize,
    pub prices: usize,
    pub fx_rates: usize,
    pub errors: Vec<String>,
}

#[allow(async_fn_in_trait)]
pub trait MarketStore {
    async fn held_symbols(&self) -> Result<Vec<Held>, SourceError>;
    async fn currencies_in_use(&self) -> Result<Vec<String>, SourceError>;
    async fn price_coverage(&self) -> Result<Coverage<(PriceProviderId, String)>, SourceError>;
    async fn fx_coverage(&self) -> Result<Coverage<String>, SourceError>;
    /// Un precio por activo y día: si ya hay uno, se sobrescribe.
    async fn save_prices(&self, prices: &[AssetPrice]) -> Result<(), SourceError>;
    async fn save_rates(&self, rates: &[FxRate]) -> Result<(), SourceError>;
    async fn log_run(&self, log: &RunLog) -> Result<(), SourceError>;
}

pub struct Supabase {
    http: reqwest::Client,
    url: String,
    key: String,
}

fn provider(name: &str) -> Option<PriceProviderId> {
    match name {
        "eodhd" => Some(PriceProviderId::Eodhd),
        "coingecko" => Some(PriceProviderId::Coingecko),
        _ => None,
    }
}

pub fn provider_name(provider: PriceProviderId) -> &'static str {
    match provider {
        PriceProviderId::Eodhd => "eodhd",
        PriceProviderId::Coingecko => "coingecko",
        PriceProviderId::Manual => "manual",
    }
}

fn date(row: &Value, key: &str) -> Option<NaiveDate> {
    row.get(key)?.as_str()?.parse().ok()
}

fn text(row: &Value, key: &str) -> Option<String> {
    row.get(key)?.as_str().map(str::to_owned)
}

impl Supabase {
    pub fn new(url: &str, key: String) -> Self {
        Self {
            http: client(),
            url: url.trim_end_matches('/').to_owned(),
            key,
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self
            .http
            .request(method, format!("{}/rest/v1/{path}", self.url))
            .header("apikey", &self.key);
        // Las claves `service_role` antiguas son JWT; las nuevas `sb_secret_` no.
        if self.key.starts_with("eyJ") {
            request.bearer_auth(&self.key)
        } else {
            request
        }
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, SourceError> {
        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let body: String = body.chars().take(300).collect();
            return Err(SourceError::Status(format!(
                "Supabase HTTP {status}: {body}"
            )));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|error| SourceError::Format(error.to_string()))
    }

    async fn rpc(&self, name: &str) -> Result<Vec<Value>, SourceError> {
        match self
            .send(
                self.request(reqwest::Method::POST, &format!("rpc/{name}"))
                    .json(&json!({})),
            )
            .await?
        {
            Value::Array(rows) => Ok(rows),
            other => Err(SourceError::Format(format!("{name}: {other}"))),
        }
    }

    async fn upsert(
        &self,
        table: &str,
        conflict: &str,
        rows: Vec<Value>,
    ) -> Result<(), SourceError> {
        for chunk in rows.chunks(500) {
            self.send(
                self.request(reqwest::Method::POST, table)
                    .query(&[("on_conflict", conflict)])
                    .header("Prefer", "resolution=merge-duplicates,return=minimal")
                    .json(chunk),
            )
            .await?;
        }
        Ok(())
    }
}

impl MarketStore for Supabase {
    async fn held_symbols(&self) -> Result<Vec<Held>, SourceError> {
        Ok(self
            .rpc("held_symbols")
            .await?
            .iter()
            .filter_map(|row| {
                Some(Held {
                    provider: provider(row.get("provider")?.as_str()?)?,
                    symbol: text(row, "symbol")?.trim().to_owned(),
                    currency: text(row, "currency")?,
                })
            })
            .filter(|held| !held.symbol.is_empty())
            .collect())
    }

    async fn currencies_in_use(&self) -> Result<Vec<String>, SourceError> {
        Ok(self
            .rpc("currencies_in_use")
            .await?
            .iter()
            .filter_map(|row| text(row, "currency"))
            .collect())
    }

    async fn price_coverage(&self) -> Result<Coverage<(PriceProviderId, String)>, SourceError> {
        Ok(self
            .rpc("price_coverage")
            .await?
            .iter()
            .filter_map(|row| {
                let key = (
                    provider(row.get("provider")?.as_str()?)?,
                    text(row, "symbol")?,
                );
                Some((key, (date(row, "first_on")?, date(row, "last_on")?)))
            })
            .collect())
    }

    async fn fx_coverage(&self) -> Result<Coverage<String>, SourceError> {
        Ok(self
            .rpc("fx_coverage")
            .await?
            .iter()
            .filter(|row| row.get("base").and_then(Value::as_str) == Some("USD"))
            .filter_map(|row| {
                Some((
                    text(row, "quote")?,
                    (date(row, "first_on")?, date(row, "last_on")?),
                ))
            })
            .collect())
    }

    async fn save_prices(&self, prices: &[AssetPrice]) -> Result<(), SourceError> {
        let now = Utc::now().to_rfc3339();
        let rows = prices
            .iter()
            .map(|price| {
                json!({
                    "provider": provider_name(price.provider),
                    "symbol": price.symbol,
                    "price_on": price.price_on.to_string(),
                    // Texto: PostgREST lo guarda en `numeric` sin pasar por f64.
                    "price": price.price.to_string(),
                    "currency": price.currency.as_str(),
                    "fetched_at": now,
                })
            })
            .collect();
        self.upsert("asset_prices", "provider,symbol,price_on", rows)
            .await
    }

    async fn save_rates(&self, rates: &[FxRate]) -> Result<(), SourceError> {
        let rows = rates
            .iter()
            .map(|rate| {
                json!({
                    "base": rate.base.as_str(),
                    "quote": rate.quote.as_str(),
                    "rate_on": rate.rate_on.to_string(),
                    "rate": rate.rate.to_string(),
                })
            })
            .collect();
        self.upsert("fx_rates", "base,quote,rate_on", rows).await
    }

    async fn log_run(&self, log: &RunLog) -> Result<(), SourceError> {
        self.send(
            self.request(reqwest::Method::POST, "price_runs")
                .header("Prefer", "return=minimal")
                .json(log),
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudal_core::Currency;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use wiremock::matchers::{header, header_exists, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn lee_simbolos_y_guarda_precios_como_texto_con_upsert() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/rpc/held_symbols"))
            .and(header("apikey", "sb_secret_prueba"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"provider": "eodhd", "symbol": "AAPL.US", "currency": "USD"},
                {"provider": "coingecko", "symbol": " bitcoin ", "currency": "CLP"},
                {"provider": "otra", "symbol": "X", "currency": "USD"}
            ])))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/asset_prices"))
            .and(query_param("on_conflict", "provider,symbol,price_on"))
            .and(header_exists("prefer"))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;

        let store = Supabase::new(&server.uri(), "sb_secret_prueba".into());
        let held = store.held_symbols().await.unwrap();
        assert_eq!(held.len(), 2);
        assert_eq!(held[1].symbol, "bitcoin");

        store
            .save_prices(&[AssetPrice {
                provider: PriceProviderId::Eodhd,
                symbol: "AAPL.US".into(),
                price_on: "2026-11-10".parse().unwrap(),
                price: Decimal::from_str("347.92").unwrap(),
                currency: Currency::new("USD").unwrap(),
            }])
            .await
            .unwrap();
        let sent = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&sent.last().unwrap().body).unwrap();
        let prefer = sent
            .last()
            .unwrap()
            .headers
            .get("prefer")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(prefer, "resolution=merge-duplicates,return=minimal");
        assert_eq!(body[0]["price"], json!("347.92"));
        assert_eq!(body[0]["provider"], json!("eodhd"));
        assert_eq!(body[0]["price_on"], json!("2026-11-10"));
    }

    #[tokio::test]
    async fn una_clave_jwt_tambien_va_en_authorization() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/rpc/currencies_in_use"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"currency": "CLP"}])))
            .mount(&server)
            .await;
        let store = Supabase::new(&server.uri(), "eyJ.prueba".into());
        assert_eq!(
            store.currencies_in_use().await.unwrap(),
            vec!["CLP".to_owned()]
        );
    }

    #[tokio::test]
    async fn un_error_del_servidor_se_informa() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Invalid API key"))
            .mount(&server)
            .await;
        let store = Supabase::new(&server.uri(), "mala".into());
        let error = store.held_symbols().await.unwrap_err().to_string();
        assert!(
            error.contains("401") && error.contains("Invalid API key"),
            "{error}"
        );
    }
}
