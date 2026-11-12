//! Fuentes de precios y tipos de cambio. Cada una implementa un trait, para
//! poder cambiarla o añadir respaldos sin tocar el resto del worker.

use caudal_core::Currency;
use caudal_core::fx::FxRate;
use caudal_core::model::{AssetPrice, PriceProviderId};
use chrono::{DateTime, Days, NaiveDate};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("red: {0}")]
    Http(#[from] reqwest::Error),
    #[error("respuesta inesperada: {0}")]
    Format(String),
    #[error("{0}")]
    Status(String),
}

/// Símbolo en poder de algún usuario.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Held {
    pub provider: PriceProviderId,
    pub symbol: String,
    /// Moneda con la que lo registró el primer usuario; solo se usa si la
    /// bolsa no la define.
    pub currency: String,
}

/// Lo que devuelve una consulta: precios encontrados y símbolos sin dato.
#[derive(Debug, Default)]
pub struct Fetched {
    pub prices: Vec<AssetPrice>,
    pub missing: Vec<String>,
}

#[allow(async_fn_in_trait)]
pub trait PriceProvider {
    fn id(&self) -> PriceProviderId;
    /// Último precio de cada símbolo, con una sola consulta por símbolo como máximo.
    async fn latest(&self, held: &[Held]) -> Result<Fetched, SourceError>;
    /// Cierres diarios desde `from`, para un símbolo nuevo o tras un hueco.
    async fn history(&self, held: &Held, from: NaiveDate) -> Result<Vec<AssetPrice>, SourceError>;
}

#[allow(async_fn_in_trait)]
pub trait FxSource {
    /// Tipos USD → cada moneda pedida que la fuente conoce.
    async fn latest(&self, quotes: &BTreeSet<String>) -> Result<Vec<FxRate>, SourceError>;
    async fn history(
        &self,
        quotes: &BTreeSet<String>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<FxRate>, SourceError>;
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("caudal-price-worker")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("cliente HTTP")
}

/// Número JSON o texto → decimal exacto. "NA", null o negativos no valen.
pub fn decimal(value: &Value) -> Option<Decimal> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => return None,
    };
    Decimal::from_str(&text)
        .or_else(|_| Decimal::from_scientific(&text))
        .ok()
        .filter(|value| *value >= Decimal::ZERO)
        .map(|value| value.round_dp(caudal_core::money::AMOUNT_SCALE).normalize())
}

fn currency(code: &str) -> Option<Currency> {
    Currency::new(&code.trim().to_uppercase()).ok()
}

async fn get_json(request: reqwest::RequestBuilder) -> Result<Value, SourceError> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let body: String = body.chars().take(200).collect();
        return Err(SourceError::Status(format!("HTTP {status}: {body}")));
    }
    Ok(response.json().await?)
}

/// Moneda en que cotiza cada bolsa de EODHD, y un factor para las que usan
/// subunidades (Londres cotiza en peniques).
pub fn exchange_currency(symbol: &str) -> Option<(&'static str, Decimal)> {
    let exchange = symbol.rsplit_once('.')?.1.to_ascii_uppercase();
    let code = match exchange.as_str() {
        "US" => "USD",
        "XETRA" | "F" | "BE" | "DU" | "HM" | "MU" | "STU" | "PA" | "AS" | "BR" | "MC" | "MI"
        | "LS" | "VI" | "HE" | "IR" => "EUR",
        "LSE" => return Some(("GBP", Decimal::new(1, 2))),
        "SW" => "CHF",
        "CO" => "DKK",
        "ST" => "SEK",
        "OL" => "NOK",
        "WAR" => "PLN",
        "TO" | "V" | "NEO" => "CAD",
        "SN" => "CLP",
        "SA" => "BRL",
        "MX" => "MXN",
        "BA" => "ARS",
        "LIM" => "PEN",
        "AU" => "AUD",
        "HK" => "HKD",
        "KO" | "KQ" => "KRW",
        "TSE" => "JPY",
        "SHG" | "SHE" => "CNY",
        "NSE" | "BSE" => "INR",
        "JSE" => "ZAR",
        _ => return None,
    };
    Some((code, Decimal::ONE))
}

pub struct Eodhd {
    pub http: reqwest::Client,
    pub base: String,
    pub token: String,
}

impl Eodhd {
    pub fn new(http: reqwest::Client, token: String) -> Self {
        Self {
            http,
            base: "https://eodhd.com/api".into(),
            token,
        }
    }

    fn price(&self, held: &Held, date: NaiveDate, close: &Value) -> Option<AssetPrice> {
        let (code, factor) = exchange_currency(&held.symbol)
            .map(|(code, factor)| (code.to_owned(), factor))
            .unwrap_or_else(|| (held.currency.clone(), Decimal::ONE));
        Some(AssetPrice {
            provider: PriceProviderId::Eodhd,
            symbol: held.symbol.clone(),
            price_on: date,
            price: (decimal(close)? * factor).normalize(),
            currency: currency(&code)?,
        })
    }
}

/// EODHD admite varios símbolos por consulta con `s=`; cada uno cuenta en la cuota.
const EODHD_BATCH: usize = 15;

impl PriceProvider for Eodhd {
    fn id(&self) -> PriceProviderId {
        PriceProviderId::Eodhd
    }

    async fn latest(&self, held: &[Held]) -> Result<Fetched, SourceError> {
        let mut fetched = Fetched::default();
        for chunk in held.chunks(EODHD_BATCH) {
            let first = &chunk[0].symbol;
            let rest: Vec<&str> = chunk[1..].iter().map(|h| h.symbol.as_str()).collect();
            let mut request = self
                .http
                .get(format!("{}/real-time/{first}", self.base))
                .query(&[("api_token", self.token.as_str()), ("fmt", "json")]);
            if !rest.is_empty() {
                request = request.query(&[("s", rest.join(","))]);
            }
            let body = get_json(request).await?;
            let items = match body {
                Value::Array(items) => items,
                item @ Value::Object(_) => vec![item],
                other => return Err(SourceError::Format(other.to_string())),
            };
            for held in chunk {
                let item = items.iter().find(|item| {
                    item.get("code")
                        .and_then(Value::as_str)
                        .is_some_and(|code| code.eq_ignore_ascii_case(&held.symbol))
                });
                let price = item.and_then(|item| {
                    let timestamp = item.get("timestamp")?.as_i64()?;
                    let date = DateTime::from_timestamp(timestamp, 0)?.date_naive();
                    self.price(held, date, item.get("close")?)
                });
                match price {
                    Some(price) => fetched.prices.push(price),
                    None => fetched.missing.push(held.symbol.clone()),
                }
            }
        }
        Ok(fetched)
    }

    async fn history(&self, held: &Held, from: NaiveDate) -> Result<Vec<AssetPrice>, SourceError> {
        let request = self
            .http
            .get(format!("{}/eod/{}", self.base, held.symbol))
            .query(&[
                ("api_token", self.token.as_str()),
                ("fmt", "json"),
                ("period", "d"),
                ("from", &from.to_string()),
            ]);
        let Value::Array(rows) = get_json(request).await? else {
            return Err(SourceError::Format(format!("historial de {}", held.symbol)));
        };
        Ok(rows
            .iter()
            .filter_map(|row| {
                let date = row.get("date")?.as_str()?.parse().ok()?;
                // Cierre sin ajustar: los splits ya están en las operaciones del usuario.
                self.price(held, date, row.get("close")?)
            })
            .collect())
    }
}

pub struct CoinGecko {
    pub http: reqwest::Client,
    pub base: String,
    pub key: Option<String>,
    /// Pausa entre consultas de historial, por el límite por minuto del plan gratuito.
    pub pause: Duration,
}

impl CoinGecko {
    pub fn new(http: reqwest::Client, key: Option<String>) -> Self {
        Self {
            http,
            base: "https://api.coingecko.com/api/v3".into(),
            key,
            pause: Duration::from_millis(2500),
        }
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        let request = self.http.get(format!("{}{path}", self.base));
        match &self.key {
            Some(key) => request.header("x-cg-demo-api-key", key),
            None => request,
        }
    }
}

/// Las criptos se guardan siempre en dólares; la app convierte a la moneda de cada usuario.
fn usd() -> Currency {
    Currency::new("USD").expect("USD es válido")
}

impl PriceProvider for CoinGecko {
    fn id(&self) -> PriceProviderId {
        PriceProviderId::Coingecko
    }

    async fn latest(&self, held: &[Held]) -> Result<Fetched, SourceError> {
        let mut fetched = Fetched::default();
        for chunk in held.chunks(200) {
            let ids: Vec<String> = chunk.iter().map(|h| h.symbol.to_lowercase()).collect();
            let body = get_json(self.request("/simple/price").query(&[
                ("ids", ids.join(",")),
                ("vs_currencies", "usd".to_owned()),
                ("include_last_updated_at", "true".to_owned()),
            ]))
            .await?;
            for held in chunk {
                let item = body.get(held.symbol.to_lowercase());
                let price = item.and_then(|item| {
                    let date = item
                        .get("last_updated_at")
                        .and_then(Value::as_i64)
                        .and_then(|t| DateTime::from_timestamp(t, 0))?
                        .date_naive();
                    Some(AssetPrice {
                        provider: PriceProviderId::Coingecko,
                        symbol: held.symbol.clone(),
                        price_on: date,
                        price: decimal(item.get("usd")?)?,
                        currency: usd(),
                    })
                });
                match price {
                    Some(price) => fetched.prices.push(price),
                    None => fetched.missing.push(held.symbol.clone()),
                }
            }
        }
        Ok(fetched)
    }

    async fn history(&self, held: &Held, from: NaiveDate) -> Result<Vec<AssetPrice>, SourceError> {
        tokio::time::sleep(self.pause).await;
        let today = chrono::Utc::now().date_naive();
        // El plan gratuito da hasta 365 días.
        let days = (today - from).num_days().clamp(1, 365);
        let body = get_json(
            self.request(&format!(
                "/coins/{}/market_chart",
                held.symbol.to_lowercase()
            ))
            .query(&[
                ("vs_currency", "usd".to_owned()),
                ("days", days.to_string()),
                ("interval", "daily".to_owned()),
            ]),
        )
        .await?;
        let Some(Value::Array(points)) = body.get("prices") else {
            return Err(SourceError::Format(format!("historial de {}", held.symbol)));
        };
        // Un punto por día: el último de cada fecha.
        let mut by_day: BTreeMap<NaiveDate, Decimal> = BTreeMap::new();
        for point in points {
            let Some(pair) = point.as_array() else {
                continue;
            };
            let (Some(time), Some(price)) = (pair.first(), pair.get(1).and_then(decimal)) else {
                continue;
            };
            // Milisegundos: entero, o decimal en algunos puntos.
            let Some(millis) = time
                .as_i64()
                .or_else(|| time.as_f64().map(|value| value as i64))
            else {
                continue;
            };
            if let Some(moment) = DateTime::from_timestamp_millis(millis) {
                by_day.insert(moment.date_naive(), price);
            }
        }
        Ok(by_day
            .into_iter()
            .filter(|(date, _)| *date >= from)
            .map(|(date, price)| AssetPrice {
                provider: PriceProviderId::Coingecko,
                symbol: held.symbol.clone(),
                price_on: date,
                price,
                currency: usd(),
            })
            .collect())
    }
}

fn rates(
    date: NaiveDate,
    rates: &Value,
    quotes: &BTreeSet<String>,
    lowercase: bool,
) -> Vec<FxRate> {
    quotes
        .iter()
        .filter(|quote| quote.as_str() != "USD")
        .filter_map(|quote| {
            let key = if lowercase {
                quote.to_lowercase()
            } else {
                quote.clone()
            };
            let rate = decimal(rates.get(&key)?).filter(|rate| !rate.is_zero())?;
            Some(FxRate {
                base: usd(),
                quote: currency(quote)?,
                rate_on: date,
                rate,
            })
        })
        .collect()
}

/// Frankfurter: tipos del Banco Central Europeo, unas 30 monedas, con historial.
pub struct Frankfurter {
    pub http: reqwest::Client,
    pub base: String,
}

impl Frankfurter {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            base: "https://api.frankfurter.dev/v1".into(),
        }
    }

    pub async fn supported(&self) -> Result<BTreeSet<String>, SourceError> {
        let body = get_json(self.http.get(format!("{}/currencies", self.base))).await?;
        let Value::Object(map) = body else {
            return Err(SourceError::Format("lista de monedas".into()));
        };
        Ok(map.keys().cloned().collect())
    }
}

impl FxSource for Frankfurter {
    async fn latest(&self, quotes: &BTreeSet<String>) -> Result<Vec<FxRate>, SourceError> {
        if quotes.is_empty() {
            return Ok(Vec::new());
        }
        let symbols: Vec<&str> = quotes.iter().map(String::as_str).collect();
        let body = get_json(
            self.http
                .get(format!("{}/latest", self.base))
                .query(&[("base", "USD"), ("symbols", &symbols.join(","))]),
        )
        .await?;
        let date = body
            .get("date")
            .and_then(Value::as_str)
            .and_then(|d| d.parse().ok())
            .ok_or_else(|| SourceError::Format("fecha".into()))?;
        Ok(rates(
            date,
            body.get("rates").unwrap_or(&Value::Null),
            quotes,
            false,
        ))
    }

    async fn history(
        &self,
        quotes: &BTreeSet<String>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<FxRate>, SourceError> {
        if quotes.is_empty() {
            return Ok(Vec::new());
        }
        let symbols: Vec<&str> = quotes.iter().map(String::as_str).collect();
        let body = get_json(
            self.http
                .get(format!("{}/{from}..{to}", self.base))
                .query(&[("base", "USD"), ("symbols", &symbols.join(","))]),
        )
        .await?;
        let Some(Value::Object(days)) = body.get("rates") else {
            return Err(SourceError::Format("historial de tipos".into()));
        };
        Ok(days
            .iter()
            .filter_map(|(date, values)| Some((date.parse().ok()?, values)))
            .flat_map(|(date, values)| rates(date, values, quotes, false))
            .collect())
    }
}

/// Respaldo para las monedas que el BCE no publica (CLP, COP, PEN, UYU…):
/// el conjunto diario de `@fawazahmed0/currency-api`, de dominio público.
pub struct CurrencyApi {
    pub http: reqwest::Client,
    pub base: String,
}

impl CurrencyApi {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            base: "https://cdn.jsdelivr.net/npm/@fawazahmed0".into(),
        }
    }

    async fn day(&self, tag: &str, quotes: &BTreeSet<String>) -> Result<Vec<FxRate>, SourceError> {
        let body = get_json(self.http.get(format!(
            "{}/currency-api@{tag}/v1/currencies/usd.min.json",
            self.base
        )))
        .await?;
        let date = body
            .get("date")
            .and_then(Value::as_str)
            .and_then(|d| d.parse().ok())
            .ok_or_else(|| SourceError::Format("fecha".into()))?;
        Ok(rates(
            date,
            body.get("usd").unwrap_or(&Value::Null),
            quotes,
            true,
        ))
    }
}

impl FxSource for CurrencyApi {
    async fn latest(&self, quotes: &BTreeSet<String>) -> Result<Vec<FxRate>, SourceError> {
        if quotes.is_empty() {
            return Ok(Vec::new());
        }
        self.day("latest", quotes).await
    }

    /// Un dato por semana: el historial solo sirve para valorar meses pasados.
    async fn history(
        &self,
        quotes: &BTreeSet<String>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<FxRate>, SourceError> {
        let mut out = Vec::new();
        let mut date = from;
        while date <= to && !quotes.is_empty() {
            match self.day(&date.to_string(), quotes).await {
                Ok(mut rows) => out.append(&mut rows),
                // Un día sin archivo no detiene el resto.
                Err(SourceError::Status(_)) => {}
                Err(error) => return Err(error),
            }
            date = date + Days::new(7);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decimales_exactos_y_valores_invalidos() {
        let parsed: Value =
            serde_json::from_str(r#"{"a": 347.77, "b": "NA", "c": 1.2e-5, "d": -1}"#).unwrap();
        assert_eq!(decimal(&parsed["a"]).unwrap().to_string(), "347.77");
        assert_eq!(decimal(&parsed["b"]), None);
        assert_eq!(decimal(&parsed["c"]).unwrap().to_string(), "0.000012");
        assert_eq!(decimal(&parsed["d"]), None);
        assert_eq!(decimal(&json!(null)), None);
    }

    #[test]
    fn moneda_por_bolsa() {
        assert_eq!(exchange_currency("AAPL.US").unwrap().0, "USD");
        assert_eq!(exchange_currency("SQM-B.SN").unwrap().0, "CLP");
        assert_eq!(exchange_currency("sap.xetra").unwrap().0, "EUR");
        let (code, factor) = exchange_currency("VOD.LSE").unwrap();
        assert_eq!((code, factor.to_string().as_str()), ("GBP", "0.01"));
        assert_eq!(exchange_currency("RARO.XYZ"), None);
        assert_eq!(exchange_currency("bitcoin"), None);
    }
}
