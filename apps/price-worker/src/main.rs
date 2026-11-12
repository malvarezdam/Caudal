//! Worker de precios: corre 1 o 2 veces al día en GitHub Actions.
//!
//! Variables (secretos del repositorio, nunca en el código):
//! - `SUPABASE_SECRET_KEY`: clave secreta o `service_role` del proyecto.
//! - `EODHD_API_KEY`: acciones y ETF. Sin ella se actualizan solo criptos y divisas.
//! - `COINGECKO_API_KEY` (opcional): clave demo de CoinGecko.
//! - `SUPABASE_URL` (opcional): por defecto, el proyecto de la app.
//!
//! Prueba sin Supabase, solo lectura de las fuentes:
//! `cargo run -p caudal-price-worker -- --dry-run eodhd:AAPL.US coingecko:bitcoin fx:CLP,EUR`

mod run;
mod sources;
mod store;

use caudal_core::model::PriceProviderId;
use sources::{CoinGecko, CurrencyApi, Eodhd, Frankfurter, FxSource, Held, PriceProvider, client};
use std::collections::BTreeSet;
use std::process::ExitCode;

const DEFAULT_URL: &str = "https://vmrlfftmkgyfgmqqjexm.supabase.co";

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let http = client();
    let today = chrono::Utc::now().date_naive();
    if args.first().map(String::as_str) == Some("--dry-run") {
        return dry_run(http, &args[1..]).await;
    }

    let Some(key) = env("SUPABASE_SECRET_KEY") else {
        println!("::warning::SUPABASE_SECRET_KEY sin definir: no se actualizan precios.");
        return ExitCode::SUCCESS;
    };
    let url = env("SUPABASE_URL").unwrap_or_else(|| DEFAULT_URL.to_owned());
    let store = store::Supabase::new(&url, key);
    let eodhd = env("EODHD_API_KEY").map(|token| Eodhd::new(http.clone(), token));
    let coingecko = CoinGecko::new(http.clone(), env("COINGECKO_API_KEY"));
    let ecb = Frankfurter::new(http.clone());
    let ecb_supported = ecb.supported().await.unwrap_or_else(|error| {
        println!("::warning::Frankfurter no responde ({error}); se usa solo el respaldo.");
        BTreeSet::new()
    });
    let fallback = CurrencyApi::new(http);
    let sources = run::Sources {
        eodhd: eodhd.as_ref(),
        coingecko: &coingecko,
        ecb: &ecb,
        ecb_supported,
        fallback: &fallback,
    };
    let log = run::run(&store, &sources, today, run::Limits::default()).await;
    println!(
        "{} símbolos, {} precios y {} tipos de cambio guardados; {} avisos.",
        log.symbols,
        log.prices,
        log.fx_rates,
        log.errors.len()
    );
    if log.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Consulta las fuentes reales e imprime lo que se guardaría.
async fn dry_run(http: reqwest::Client, args: &[String]) -> ExitCode {
    let mut stocks = Vec::new();
    let mut cryptos = Vec::new();
    let mut quotes = BTreeSet::new();
    for arg in args {
        match arg.split_once(':') {
            Some(("eodhd", symbol)) => stocks.push(Held {
                provider: PriceProviderId::Eodhd,
                symbol: symbol.into(),
                currency: "USD".into(),
            }),
            Some(("coingecko", symbol)) => cryptos.push(Held {
                provider: PriceProviderId::Coingecko,
                symbol: symbol.into(),
                currency: "USD".into(),
            }),
            Some(("fx", list)) => quotes.extend(list.split(',').map(|q| q.trim().to_uppercase())),
            _ => {
                eprintln!("argumento no reconocido: {arg}");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut ok = true;
    let token = env("EODHD_API_KEY").unwrap_or_else(|| "demo".to_owned());
    let eodhd = Eodhd::new(http.clone(), token);
    let coingecko = CoinGecko::new(http.clone(), env("COINGECKO_API_KEY"));
    let since = chrono::Utc::now().date_naive() - chrono::Days::new(10);
    for (name, result) in [
        (
            "eodhd",
            if stocks.is_empty() {
                None
            } else {
                Some(eodhd.latest(&stocks).await)
            },
        ),
        (
            "coingecko",
            if cryptos.is_empty() {
                None
            } else {
                Some(coingecko.latest(&cryptos).await)
            },
        ),
    ] {
        match result {
            Some(Ok(fetched)) => {
                for price in fetched.prices {
                    println!(
                        "{name} {} {} {} {}",
                        price.symbol, price.price_on, price.price, price.currency
                    );
                }
                for symbol in fetched.missing {
                    println!("{name} {symbol}: sin dato");
                }
            }
            Some(Err(error)) => {
                ok = false;
                println!("{name}: {error}");
            }
            None => {}
        }
    }
    for held in stocks.iter().take(1) {
        match eodhd.history(held, since).await {
            Ok(rows) => println!("eodhd historial {}: {} días", held.symbol, rows.len()),
            Err(error) => println!("eodhd historial {}: {error}", held.symbol),
        }
    }
    for held in cryptos.iter().take(1) {
        match coingecko.history(held, since).await {
            Ok(rows) => println!("coingecko historial {}: {} días", held.symbol, rows.len()),
            Err(error) => println!("coingecko historial {}: {error}", held.symbol),
        }
    }
    let ecb = Frankfurter::new(http.clone());
    let supported = ecb.supported().await.unwrap_or_default();
    let (ecb_quotes, rest): (BTreeSet<String>, BTreeSet<String>) = quotes
        .into_iter()
        .partition(|quote| supported.contains(quote));
    let fallback = CurrencyApi::new(http);
    for (name, result) in [
        ("frankfurter", ecb.latest(&ecb_quotes).await),
        ("respaldo", fallback.latest(&rest).await),
        (
            "respaldo historial",
            fallback
                .history(&rest, since, since + chrono::Days::new(7))
                .await,
        ),
        (
            "frankfurter historial",
            ecb.history(&ecb_quotes, since, since + chrono::Days::new(3))
                .await,
        ),
    ] {
        match result {
            Ok(rates) => {
                for rate in rates {
                    println!("{name} USD→{} {} {}", rate.quote, rate.rate_on, rate.rate);
                }
            }
            Err(error) => {
                ok = false;
                println!("{name}: {error}");
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
