//! Una ejecución: reúne los símbolos en poder de algún usuario, consulta cada
//! uno una sola vez, completa el historial que falte y guarda todo.
//! Si una fuente falla, lo ya guardado no se toca: la app sigue mostrando el
//! último precio con su fecha.

use crate::sources::{FxSource, Held, PriceProvider};
use crate::store::{MarketStore, RunLog, provider_name};
use caudal_core::fx::FxRate;
use caudal_core::model::{AssetPrice, PriceProviderId};
use chrono::{Days, NaiveDate, Utc};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Días de historial para un símbolo o moneda nuevos.
    pub history_days: u64,
    /// Historiales por ejecución, para no agotar la cuota de una vez.
    pub backfills: usize,
    /// Un hueco más largo que esto se rellena con historial.
    pub gap_days: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            history_days: 400,
            backfills: 25,
            gap_days: 5,
        }
    }
}

pub struct Sources<'a, E, C, B, R> {
    /// Sin clave de EODHD, las acciones y ETF se saltan con un aviso.
    pub eodhd: Option<&'a E>,
    pub coingecko: &'a C,
    pub ecb: &'a B,
    /// Monedas que publica el BCE; el resto va al respaldo.
    pub ecb_supported: BTreeSet<String>,
    pub fallback: &'a R,
}

struct Report {
    errors: Vec<String>,
    failed: bool,
}

impl Report {
    fn warn(&mut self, message: String) {
        println!("::warning::{message}");
        self.errors.push(message);
    }

    fn fail(&mut self, message: String) {
        println!("::error::{message}");
        self.errors.push(message);
        self.failed = true;
    }
}

/// Desde cuándo hace falta historial, o `None` si ya está al día.
fn backfill_from(
    coverage: Option<(NaiveDate, NaiveDate)>,
    today: NaiveDate,
    limits: Limits,
) -> Option<NaiveDate> {
    match coverage {
        None => today.checked_sub_days(Days::new(limits.history_days)),
        Some((_, last)) if last + Days::new(limits.gap_days) < today => Some(last),
        Some(_) => None,
    }
}

async fn prices_of<P: PriceProvider>(
    provider: &P,
    held: &[Held],
    coverage: &crate::store::Coverage<(PriceProviderId, String)>,
    today: NaiveDate,
    limits: Limits,
    budget: &mut usize,
    report: &mut Report,
) -> Vec<AssetPrice> {
    if held.is_empty() {
        return Vec::new();
    }
    let name = provider_name(provider.id());
    let mut prices = match provider.latest(held).await {
        Ok(fetched) => {
            for symbol in fetched.missing {
                report.warn(format!("{name}: sin precio para {symbol}"));
            }
            fetched.prices
        }
        Err(error) => {
            report.fail(format!("{name}: {error}"));
            // Si la fuente no responde, tampoco se intenta el historial.
            return Vec::new();
        }
    };
    for item in held {
        let covered = coverage
            .iter()
            .find(|((provider, symbol), _)| {
                *provider == item.provider && symbol.eq_ignore_ascii_case(&item.symbol)
            })
            .map(|(_, range)| *range);
        let Some(from) = backfill_from(covered, today, limits) else {
            continue;
        };
        if *budget == 0 {
            report.warn(format!(
                "{name}: historial de {} para la próxima ejecución",
                item.symbol
            ));
            continue;
        }
        *budget -= 1;
        match provider.history(item, from).await {
            Ok(mut rows) => prices.append(&mut rows),
            Err(error) => report.warn(format!("{name}: historial de {}: {error}", item.symbol)),
        }
    }
    prices
}

/// Último dato de cada clave: si una fuente repite un día, gana el más reciente.
fn dedup_prices(prices: Vec<AssetPrice>) -> Vec<AssetPrice> {
    let mut map = std::collections::BTreeMap::new();
    for price in prices {
        let key = (
            provider_name(price.provider),
            price.symbol.clone(),
            price.price_on,
        );
        map.insert(key, price);
    }
    map.into_values().collect()
}

fn dedup_rates(rates: Vec<FxRate>) -> Vec<FxRate> {
    let mut map = std::collections::BTreeMap::new();
    for rate in rates {
        map.insert((rate.quote.to_string(), rate.rate_on), rate);
    }
    map.into_values().collect()
}

pub async fn run<S, E, C, B, R>(
    store: &S,
    sources: &Sources<'_, E, C, B, R>,
    today: NaiveDate,
    limits: Limits,
) -> RunLog
where
    S: MarketStore,
    E: PriceProvider,
    C: PriceProvider,
    B: FxSource,
    R: FxSource,
{
    let started_at = Utc::now();
    let mut report = Report {
        errors: Vec::new(),
        failed: false,
    };

    let held = match store.held_symbols().await {
        Ok(held) => held,
        Err(error) => {
            report.fail(format!("símbolos: {error}"));
            Vec::new()
        }
    };
    let coverage = match store.price_coverage().await {
        Ok(coverage) => Some(coverage),
        Err(error) => {
            report.warn(format!(
                "cobertura de precios: {error}; sin historial esta vez"
            ));
            None
        }
    };
    // Sin cobertura conocida no se pide historial: podría repetir todo.
    let mut budget = if coverage.is_some() {
        limits.backfills
    } else {
        0
    };
    let coverage = coverage.unwrap_or_default();

    let by = |provider: PriceProviderId| -> Vec<Held> {
        held.iter()
            .filter(|h| h.provider == provider)
            .cloned()
            .collect()
    };
    let stocks = by(PriceProviderId::Eodhd);
    let cryptos = by(PriceProviderId::Coingecko);

    let mut prices = Vec::new();
    match sources.eodhd {
        Some(eodhd) => prices.extend(
            prices_of(
                eodhd,
                &stocks,
                &coverage,
                today,
                limits,
                &mut budget,
                &mut report,
            )
            .await,
        ),
        None if !stocks.is_empty() => report.warn(format!(
            "EODHD_API_KEY sin definir: {} acciones y ETF sin actualizar",
            stocks.len()
        )),
        None => {}
    }
    prices.extend(
        prices_of(
            sources.coingecko,
            &cryptos,
            &coverage,
            today,
            limits,
            &mut budget,
            &mut report,
        )
        .await,
    );
    let prices = dedup_prices(prices);
    if !prices.is_empty()
        && let Err(error) = store.save_prices(&prices).await
    {
        report.fail(format!("guardar precios: {error}"));
    }

    // Tipos de cambio: USD → cada moneda en uso o de algún precio.
    let mut quotes: BTreeSet<String> = match store.currencies_in_use().await {
        Ok(list) => list.into_iter().collect(),
        Err(error) => {
            report.fail(format!("monedas en uso: {error}"));
            BTreeSet::new()
        }
    };
    quotes.extend(prices.iter().map(|price| price.currency.to_string()));
    quotes.extend(held.iter().map(|held| held.currency.to_uppercase()));
    quotes.remove("USD");
    let (ecb_quotes, other_quotes): (BTreeSet<String>, BTreeSet<String>) = quotes
        .iter()
        .cloned()
        .partition(|quote| sources.ecb_supported.contains(quote));

    let mut rates = Vec::new();
    let mut fallback_quotes = other_quotes.clone();
    match sources.ecb.latest(&ecb_quotes).await {
        Ok(mut found) => rates.append(&mut found),
        Err(error) => {
            report.warn(format!("Frankfurter: {error}; se usa el respaldo"));
            fallback_quotes.extend(ecb_quotes.iter().cloned());
        }
    }
    match sources.fallback.latest(&fallback_quotes).await {
        Ok(mut found) => rates.append(&mut found),
        Err(error) => report.fail(format!("tipos de cambio de respaldo: {error}")),
    }

    if let Ok(fx_coverage) = store.fx_coverage().await {
        let start = |set: &BTreeSet<String>| -> (BTreeSet<String>, Option<NaiveDate>) {
            let mut needed = BTreeSet::new();
            let mut from: Option<NaiveDate> = None;
            for quote in set {
                if let Some(date) = backfill_from(fx_coverage.get(quote).copied(), today, limits) {
                    needed.insert(quote.clone());
                    from = Some(from.map_or(date, |current| current.min(date)));
                }
            }
            (needed, from)
        };
        let ecb_set: BTreeSet<String> = ecb_quotes.difference(&fallback_quotes).cloned().collect();
        let (needed, from) = start(&ecb_set);
        if let Some(from) = from {
            match sources.ecb.history(&needed, from, today).await {
                Ok(mut found) => rates.append(&mut found),
                Err(error) => report.warn(format!("historial Frankfurter: {error}")),
            }
        }
        let (needed, from) = start(&fallback_quotes);
        if let Some(from) = from {
            match sources.fallback.history(&needed, from, today).await {
                Ok(mut found) => rates.append(&mut found),
                Err(error) => report.warn(format!("historial de respaldo: {error}")),
            }
        }
    }
    let found: BTreeSet<String> = rates.iter().map(|rate| rate.quote.to_string()).collect();
    for quote in quotes.difference(&found) {
        report.warn(format!("sin tipo de cambio USD→{quote}"));
    }
    let rates = dedup_rates(rates);
    if !rates.is_empty()
        && let Err(error) = store.save_rates(&rates).await
    {
        report.fail(format!("guardar tipos de cambio: {error}"));
    }

    let log = RunLog {
        started_at,
        ok: !report.failed,
        symbols: held.len(),
        prices: prices.len(),
        fx_rates: rates.len(),
        errors: report.errors,
    };
    if let Err(error) = store.log_run(&log).await {
        println!("::warning::registro de la ejecución: {error}");
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{Fetched, SourceError};
    use crate::store::Coverage;
    use caudal_core::Currency;
    use rust_decimal::Decimal;
    use std::cell::RefCell;

    fn day(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    #[derive(Default)]
    struct Memory {
        held: Vec<Held>,
        currencies: Vec<String>,
        coverage: Coverage<(PriceProviderId, String)>,
        fx_coverage: Coverage<String>,
        prices: RefCell<Vec<AssetPrice>>,
        rates: RefCell<Vec<FxRate>>,
        runs: RefCell<Vec<RunLog>>,
    }

    impl MarketStore for Memory {
        async fn held_symbols(&self) -> Result<Vec<Held>, SourceError> {
            Ok(self.held.clone())
        }
        async fn currencies_in_use(&self) -> Result<Vec<String>, SourceError> {
            Ok(self.currencies.clone())
        }
        async fn price_coverage(&self) -> Result<Coverage<(PriceProviderId, String)>, SourceError> {
            Ok(self.coverage.clone())
        }
        async fn fx_coverage(&self) -> Result<Coverage<String>, SourceError> {
            Ok(self.fx_coverage.clone())
        }
        async fn save_prices(&self, prices: &[AssetPrice]) -> Result<(), SourceError> {
            self.prices.borrow_mut().extend_from_slice(prices);
            Ok(())
        }
        async fn save_rates(&self, rates: &[FxRate]) -> Result<(), SourceError> {
            self.rates.borrow_mut().extend_from_slice(rates);
            Ok(())
        }
        async fn log_run(&self, log: &RunLog) -> Result<(), SourceError> {
            self.runs.borrow_mut().push(log.clone());
            Ok(())
        }
    }

    struct Fake {
        id: PriceProviderId,
        fail: bool,
        latest_calls: RefCell<Vec<Vec<String>>>,
        history_calls: RefCell<Vec<(String, NaiveDate)>>,
    }

    impl Fake {
        fn new(id: PriceProviderId, fail: bool) -> Self {
            Self {
                id,
                fail,
                latest_calls: RefCell::default(),
                history_calls: RefCell::default(),
            }
        }
    }

    fn price(symbol: &str, provider: PriceProviderId, date: NaiveDate, value: i64) -> AssetPrice {
        AssetPrice {
            provider,
            symbol: symbol.into(),
            price_on: date,
            price: Decimal::from(value),
            currency: Currency::new("USD").unwrap(),
        }
    }

    impl PriceProvider for Fake {
        fn id(&self) -> PriceProviderId {
            self.id
        }
        async fn latest(&self, held: &[Held]) -> Result<Fetched, SourceError> {
            self.latest_calls
                .borrow_mut()
                .push(held.iter().map(|h| h.symbol.clone()).collect());
            if self.fail {
                return Err(SourceError::Status("HTTP 503".into()));
            }
            Ok(Fetched {
                prices: held
                    .iter()
                    .filter(|h| h.symbol != "DESLISTADA.US")
                    .map(|h| price(&h.symbol, self.id, day("2026-11-10"), 100))
                    .collect(),
                missing: held
                    .iter()
                    .filter(|h| h.symbol == "DESLISTADA.US")
                    .map(|h| h.symbol.clone())
                    .collect(),
            })
        }
        async fn history(
            &self,
            held: &Held,
            from: NaiveDate,
        ) -> Result<Vec<AssetPrice>, SourceError> {
            self.history_calls
                .borrow_mut()
                .push((held.symbol.clone(), from));
            Ok(vec![price(&held.symbol, self.id, from, 90)])
        }
    }

    struct FakeFx {
        known: &'static [&'static str],
        history_calls: RefCell<Vec<(BTreeSet<String>, NaiveDate)>>,
    }

    impl FxSource for FakeFx {
        async fn latest(&self, quotes: &BTreeSet<String>) -> Result<Vec<FxRate>, SourceError> {
            Ok(quotes
                .iter()
                .filter(|q| self.known.contains(&q.as_str()))
                .map(|q| FxRate {
                    base: Currency::new("USD").unwrap(),
                    quote: Currency::new(q).unwrap(),
                    rate_on: day("2026-11-10"),
                    rate: Decimal::from(2),
                })
                .collect())
        }
        async fn history(
            &self,
            quotes: &BTreeSet<String>,
            from: NaiveDate,
            _to: NaiveDate,
        ) -> Result<Vec<FxRate>, SourceError> {
            self.history_calls.borrow_mut().push((quotes.clone(), from));
            Ok(Vec::new())
        }
    }

    fn held(provider: PriceProviderId, symbol: &str, currency: &str) -> Held {
        Held {
            provider,
            symbol: symbol.into(),
            currency: currency.into(),
        }
    }

    fn block<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn una_consulta_por_simbolo_historial_solo_si_falta_y_avisos() {
        let today = day("2026-11-10");
        let mut store = Memory {
            held: vec![
                held(PriceProviderId::Eodhd, "AAPL.US", "USD"),
                held(PriceProviderId::Eodhd, "DESLISTADA.US", "USD"),
                held(PriceProviderId::Coingecko, "bitcoin", "CLP"),
            ],
            currencies: vec!["CLP".into(), "EUR".into(), "USD".into()],
            ..Memory::default()
        };
        // AAPL ya tiene historial al día; bitcoin quedó con un hueco.
        store.coverage.insert(
            (PriceProviderId::Eodhd, "aapl.us".into()),
            (day("2025-10-01"), day("2026-11-09")),
        );
        store.coverage.insert(
            (PriceProviderId::Coingecko, "bitcoin".into()),
            (day("2025-10-01"), day("2026-10-20")),
        );
        store
            .fx_coverage
            .insert("EUR".into(), (day("2025-10-01"), day("2026-11-09")));

        let eodhd = Fake::new(PriceProviderId::Eodhd, false);
        let coingecko = Fake::new(PriceProviderId::Coingecko, false);
        let ecb = FakeFx {
            known: &["EUR"],
            history_calls: RefCell::default(),
        };
        let fallback = FakeFx {
            known: &["CLP"],
            history_calls: RefCell::default(),
        };
        let sources = Sources {
            eodhd: Some(&eodhd),
            coingecko: &coingecko,
            ecb: &ecb,
            ecb_supported: ["EUR".to_owned()].into(),
            fallback: &fallback,
        };
        let log = block(run(&store, &sources, today, Limits::default()));

        assert_eq!(
            *eodhd.latest_calls.borrow(),
            vec![vec!["AAPL.US".to_owned(), "DESLISTADA.US".to_owned()]]
        );
        // Historial: la nueva sin datos (400 días) y el hueco de bitcoin desde su último día.
        assert_eq!(
            *eodhd.history_calls.borrow(),
            vec![("DESLISTADA.US".to_owned(), day("2025-10-06"))]
        );
        assert_eq!(
            *coingecko.history_calls.borrow(),
            vec![("bitcoin".to_owned(), day("2026-10-20"))]
        );
        // EUR está al día; CLP es nueva y va al respaldo.
        assert!(ecb.history_calls.borrow().is_empty());
        assert_eq!(
            fallback.history_calls.borrow()[0].0,
            ["CLP".to_owned()].into()
        );

        assert!(log.ok, "{:?}", log.errors);
        assert_eq!(log.symbols, 3);
        assert!(log.errors.iter().any(|e| e.contains("DESLISTADA.US")));
        assert_eq!(store.rates.borrow().len(), 2);
        assert_eq!(store.runs.borrow().len(), 1);
    }

    #[test]
    fn si_una_fuente_cae_se_guarda_lo_demas_y_la_ejecucion_falla() {
        let store = Memory {
            held: vec![
                held(PriceProviderId::Eodhd, "AAPL.US", "USD"),
                held(PriceProviderId::Coingecko, "bitcoin", "USD"),
            ],
            ..Memory::default()
        };
        let eodhd = Fake::new(PriceProviderId::Eodhd, true);
        let coingecko = Fake::new(PriceProviderId::Coingecko, false);
        let fx = FakeFx {
            known: &[],
            history_calls: RefCell::default(),
        };
        let sources = Sources {
            eodhd: Some(&eodhd),
            coingecko: &coingecko,
            ecb: &fx,
            ecb_supported: BTreeSet::new(),
            fallback: &fx,
        };
        let log = block(run(&store, &sources, day("2026-11-10"), Limits::default()));
        assert!(!log.ok);
        assert!(eodhd.history_calls.borrow().is_empty());
        let saved = store.prices.borrow();
        assert!(
            saved
                .iter()
                .all(|p| p.provider == PriceProviderId::Coingecko)
        );
        assert!(!saved.is_empty());
    }

    #[test]
    fn sin_clave_de_eodhd_se_avisa_sin_fallar() {
        let store = Memory {
            held: vec![held(PriceProviderId::Eodhd, "AAPL.US", "USD")],
            ..Memory::default()
        };
        let coingecko = Fake::new(PriceProviderId::Coingecko, false);
        let fx = FakeFx {
            known: &[],
            history_calls: RefCell::default(),
        };
        let sources: Sources<'_, Fake, Fake, FakeFx, FakeFx> = Sources {
            eodhd: None,
            coingecko: &coingecko,
            ecb: &fx,
            ecb_supported: BTreeSet::new(),
            fallback: &fx,
        };
        let log = block(run(&store, &sources, day("2026-11-10"), Limits::default()));
        assert!(log.ok);
        assert!(log.errors[0].contains("EODHD_API_KEY"));
    }

    #[test]
    fn el_limite_de_historiales_deja_el_resto_para_despues() {
        let store = Memory {
            held: (0..4)
                .map(|n| held(PriceProviderId::Coingecko, &format!("moneda{n}"), "USD"))
                .collect(),
            ..Memory::default()
        };
        let coingecko = Fake::new(PriceProviderId::Coingecko, false);
        let fx = FakeFx {
            known: &[],
            history_calls: RefCell::default(),
        };
        let sources: Sources<'_, Fake, Fake, FakeFx, FakeFx> = Sources {
            eodhd: None,
            coingecko: &coingecko,
            ecb: &fx,
            ecb_supported: BTreeSet::new(),
            fallback: &fx,
        };
        let limits = Limits {
            backfills: 2,
            ..Limits::default()
        };
        let log = block(run(&store, &sources, day("2026-11-10"), limits));
        assert_eq!(coingecko.history_calls.borrow().len(), 2);
        assert_eq!(
            log.errors
                .iter()
                .filter(|e| e.contains("próxima ejecución"))
                .count(),
            2
        );
    }
}
