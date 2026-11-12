//! Catálogo de activos conocidos para rellenar el formulario al buscar. No es
//! una lista cerrada: cualquier símbolo de EODHD o id de CoinGecko se puede
//! escribir a mano, y un activo sin fuente se valora con precio manual.

use caudal_core::model::{AssetKind, PriceProviderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: &'static str,
    /// Símbolo en la fuente: `AAPL.US` en EODHD, `bitcoin` en CoinGecko.
    pub symbol: &'static str,
    /// Texto corto para mostrar: AAPL, BTC.
    pub ticker: &'static str,
    pub kind: AssetKind,
    pub provider: PriceProviderId,
    pub currency: &'static str,
}

const fn stock(
    name: &'static str,
    symbol: &'static str,
    ticker: &'static str,
    currency: &'static str,
) -> CatalogEntry {
    CatalogEntry {
        name,
        symbol,
        ticker,
        kind: AssetKind::Stock,
        provider: PriceProviderId::Eodhd,
        currency,
    }
}

const fn etf(
    name: &'static str,
    symbol: &'static str,
    ticker: &'static str,
    currency: &'static str,
) -> CatalogEntry {
    CatalogEntry {
        name,
        symbol,
        ticker,
        kind: AssetKind::Etf,
        provider: PriceProviderId::Eodhd,
        currency,
    }
}

const fn crypto(name: &'static str, symbol: &'static str, ticker: &'static str) -> CatalogEntry {
    CatalogEntry {
        name,
        symbol,
        ticker,
        kind: AssetKind::Crypto,
        provider: PriceProviderId::Coingecko,
        currency: "USD",
    }
}

pub const CATALOG: &[CatalogEntry] = &[
    // ETF
    etf("SPDR S&P 500 ETF", "SPY.US", "SPY", "USD"),
    etf("Vanguard S&P 500 ETF", "VOO.US", "VOO", "USD"),
    etf("iShares Core S&P 500 ETF", "IVV.US", "IVV", "USD"),
    etf("Vanguard Total Stock Market ETF", "VTI.US", "VTI", "USD"),
    etf("Vanguard Total World Stock ETF", "VT.US", "VT", "USD"),
    etf("Invesco QQQ (Nasdaq 100)", "QQQ.US", "QQQ", "USD"),
    etf("iShares MSCI Emerging Markets ETF", "EEM.US", "EEM", "USD"),
    etf(
        "Vanguard FTSE Developed Markets ETF",
        "VEA.US",
        "VEA",
        "USD",
    ),
    etf(
        "iShares Core MSCI World UCITS ETF",
        "EUNL.XETRA",
        "EUNL",
        "EUR",
    ),
    etf(
        "Vanguard FTSE All-World UCITS ETF",
        "VWCE.XETRA",
        "VWCE",
        "EUR",
    ),
    etf(
        "iShares Core S&P 500 UCITS ETF",
        "SXR8.XETRA",
        "SXR8",
        "EUR",
    ),
    etf("SPDR Gold Shares", "GLD.US", "GLD", "USD"),
    etf("iShares 20+ Year Treasury Bond ETF", "TLT.US", "TLT", "USD"),
    etf("Vanguard Total Bond Market ETF", "BND.US", "BND", "USD"),
    etf("iShares MSCI Chile ETF", "ECH.US", "ECH", "USD"),
    // Acciones de EE. UU.
    stock("Apple", "AAPL.US", "AAPL", "USD"),
    stock("Microsoft", "MSFT.US", "MSFT", "USD"),
    stock("NVIDIA", "NVDA.US", "NVDA", "USD"),
    stock("Alphabet (Google) clase A", "GOOGL.US", "GOOGL", "USD"),
    stock("Amazon", "AMZN.US", "AMZN", "USD"),
    stock("Meta Platforms", "META.US", "META", "USD"),
    stock("Tesla", "TSLA.US", "TSLA", "USD"),
    stock("Berkshire Hathaway clase B", "BRK-B.US", "BRK.B", "USD"),
    stock("JPMorgan Chase", "JPM.US", "JPM", "USD"),
    stock("Visa", "V.US", "V", "USD"),
    stock("Johnson & Johnson", "JNJ.US", "JNJ", "USD"),
    stock("Coca-Cola", "KO.US", "KO", "USD"),
    stock("Walmart", "WMT.US", "WMT", "USD"),
    stock("Netflix", "NFLX.US", "NFLX", "USD"),
    stock("MercadoLibre", "MELI.US", "MELI", "USD"),
    stock("Taiwan Semiconductor (ADR)", "TSM.US", "TSM", "USD"),
    // Europa
    stock("ASML", "ASML.AS", "ASML", "EUR"),
    stock("SAP", "SAP.XETRA", "SAP", "EUR"),
    stock("Siemens", "SIE.XETRA", "SIE", "EUR"),
    stock("LVMH", "MC.PA", "MC", "EUR"),
    stock("TotalEnergies", "TTE.PA", "TTE", "EUR"),
    stock("Banco Santander", "SAN.MC", "SAN", "EUR"),
    stock("Inditex", "ITX.MC", "ITX", "EUR"),
    stock("Nestlé", "NESN.SW", "NESN", "CHF"),
    stock("Novo Nordisk", "NOVO-B.CO", "NOVO B", "DKK"),
    // Latinoamérica
    stock("SQM serie B", "SQM-B.SN", "SQM-B", "CLP"),
    stock("Falabella", "FALABELLA.SN", "FALABELLA", "CLP"),
    stock("Banco de Chile", "CHILE.SN", "CHILE", "CLP"),
    stock("Empresas Copec", "COPEC.SN", "COPEC", "CLP"),
    stock("Cencosud", "CENCOSUD.SN", "CENCOSUD", "CLP"),
    stock("Petrobras PN", "PETR4.SA", "PETR4", "BRL"),
    stock("Vale", "VALE3.SA", "VALE3", "BRL"),
    stock("América Móvil", "AMXB.MX", "AMXB", "MXN"),
    stock("Grupo Financiero Banorte", "GFNORTEO.MX", "GFNORTEO", "MXN"),
    // Criptomonedas (CoinGecko, en dólares)
    crypto("Bitcoin", "bitcoin", "BTC"),
    crypto("Ethereum", "ethereum", "ETH"),
    crypto("Solana", "solana", "SOL"),
    crypto("BNB", "binancecoin", "BNB"),
    crypto("XRP", "ripple", "XRP"),
    crypto("Cardano", "cardano", "ADA"),
    crypto("Dogecoin", "dogecoin", "DOGE"),
    crypto("Tether", "tether", "USDT"),
    crypto("USD Coin", "usd-coin", "USDC"),
    crypto("Polkadot", "polkadot", "DOT"),
    crypto("Chainlink", "chainlink", "LINK"),
    crypto("Litecoin", "litecoin", "LTC"),
    crypto("Avalanche", "avalanche-2", "AVAX"),
];

/// Minúsculas y sin tildes, para comparar lo que escribe el usuario.
pub(crate) fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'Á' | 'À' | 'Ä' => 'a',
            'é' | 'è' | 'ë' | 'É' | 'È' | 'Ë' => 'e',
            'í' | 'ì' | 'ï' | 'Í' | 'Ì' | 'Ï' => 'i',
            'ó' | 'ò' | 'ö' | 'Ó' | 'Ò' | 'Ö' => 'o',
            'ú' | 'ù' | 'ü' | 'Ú' | 'Ù' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

/// Hasta 8 coincidencias: primero el ticker exacto, luego los que empiezan
/// por lo escrito y al final los que lo contienen.
pub fn search(query: &str) -> Vec<CatalogEntry> {
    let query = fold(query.trim());
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, usize, CatalogEntry)> = CATALOG
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let ticker = fold(entry.ticker);
            let symbol = fold(entry.symbol);
            let name = fold(entry.name);
            let rank = if ticker == query || symbol == query {
                0
            } else if ticker.starts_with(&query) || symbol.starts_with(&query) {
                1
            } else if name.starts_with(&query) {
                2
            } else if name.contains(&query) || symbol.contains(&query) {
                3
            } else {
                return None;
            };
            Some((rank, index, *entry))
        })
        .collect();
    hits.sort_by_key(|(rank, index, _)| (*rank, *index));
    hits.into_iter()
        .take(8)
        .map(|(_, _, entry)| entry)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busca_por_ticker_nombre_y_sin_tildes() {
        assert_eq!(search("aapl")[0].symbol, "AAPL.US");
        assert_eq!(search("btc")[0].symbol, "bitcoin");
        assert_eq!(search("america movil")[0].symbol, "AMXB.MX");
        assert_eq!(search("s&p")[0].ticker, "SPY");
        assert!(search("   ").is_empty());
        assert!(search("a").len() <= 8);
    }

    #[test]
    fn catalogo_valido() {
        for entry in CATALOG {
            assert!(
                caudal_core::Currency::new(entry.currency).is_ok(),
                "{}",
                entry.name
            );
            let automatic = entry.provider != PriceProviderId::Manual;
            assert!(automatic && !entry.symbol.is_empty());
        }
        let mut symbols: Vec<&str> = CATALOG.iter().map(|e| e.symbol).collect();
        symbols.sort();
        symbols.dedup();
        assert_eq!(symbols.len(), CATALOG.len());
    }
}
