//! Posiciones de inversión por precio promedio: cantidad, costo, ganancia
//! realizada y no realizada, dividendos y valor de mercado.

use crate::fx::FxTable;
use crate::model::{Asset, AssetPrice, PriceProviderId, Trade, TradeKind};
use crate::money::{AMOUNT_SCALE, Money, MoneyError};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    /// Precio diario del proveedor.
    Market,
    /// Precio que puso el usuario.
    Manual,
    /// Sin otro dato: el de la última operación con precio (compra, venta o traspaso).
    LastTrade,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub asset_id: Uuid,
    pub quantity: Decimal,
    /// Costo de lo que queda, en la moneda del activo.
    pub cost_basis: Decimal,
    pub average_cost: Option<Decimal>,
    /// Ganancia de las ventas menos comisiones sueltas.
    pub realized: Decimal,
    pub dividends: Decimal,
    pub price: Option<(Decimal, NaiveDate, PriceSource)>,
    pub market_value: Option<Decimal>,
    pub unrealized: Option<Decimal>,
    /// Se vendió más de lo que había: los números son orientativos.
    pub oversold: bool,
    /// Operaciones en otra moneda sin tipo de cambio: no se cuentan.
    pub unconverted: Vec<Uuid>,
}

/// Monto total de una operación: cantidad × precio, o el precio si no hay cantidad.
fn gross(trade: &Trade) -> Decimal {
    match (trade.quantity, trade.price) {
        (Some(quantity), Some(price)) => quantity * price,
        (None, Some(price)) => price,
        _ => Decimal::ZERO,
    }
}

/// Precio vigente en `as_of`: el más reciente entre mercado y manual (a igual
/// fecha gana el manual); si no hay ninguno, el de la última operación.
/// Un precio de mercado en otra moneda (una cripto cotizada en dólares y
/// registrada en pesos) se convierte con el tipo de cambio de su fecha.
pub fn price_at(
    asset: &Asset,
    trades: &[&Trade],
    prices: &[AssetPrice],
    fx: &FxTable,
    as_of: NaiveDate,
) -> Option<(Decimal, NaiveDate, PriceSource)> {
    let market = match (asset.provider, &asset.provider_symbol) {
        (PriceProviderId::Manual, _) | (_, None) => None,
        (provider, Some(symbol)) => prices
            .iter()
            .filter(|price| {
                price.provider == provider
                    && price.symbol.eq_ignore_ascii_case(symbol)
                    && price.price_on <= as_of
            })
            .filter_map(|price| {
                let value = if price.currency == asset.currency {
                    price.price
                } else {
                    let rate = fx.rate(&price.currency, &asset.currency, price.price_on)?;
                    (price.price * rate).round_dp(AMOUNT_SCALE)
                };
                Some((value, price.price_on, PriceSource::Market))
            })
            .max_by_key(|(_, date, _)| *date),
    };
    let manual = asset.manual_price.and_then(|price| {
        let date = asset.manual_price_on.unwrap_or(NaiveDate::MIN);
        (date <= as_of).then_some((price, date, PriceSource::Manual))
    });
    let best = match (market, manual) {
        (Some(market), Some(manual)) => Some(if manual.1 >= market.1 { manual } else { market }),
        (market, manual) => market.or(manual),
    };
    best.or_else(|| {
        trades
            .iter()
            .filter(|trade| {
                matches!(
                    trade.kind,
                    TradeKind::Buy
                        | TradeKind::Sell
                        | TradeKind::TransferIn
                        | TradeKind::TransferOut
                ) && trade.price.is_some()
                    && trade.traded_on <= as_of
            })
            .max_by_key(|trade| trade.traded_on)
            .and_then(|trade| {
                trade
                    .price
                    .map(|price| (price, trade.traded_on, PriceSource::LastTrade))
            })
    })
}

/// Recorre las operaciones del activo hasta `as_of`, en orden de fecha.
pub fn position(
    asset: &Asset,
    trades: &[Trade],
    prices: &[AssetPrice],
    fx: &FxTable,
    as_of: NaiveDate,
) -> Result<Position, MoneyError> {
    let mut own: Vec<&Trade> = trades
        .iter()
        .filter(|trade| {
            trade.asset_id == asset.meta.id && !trade.meta.is_deleted() && trade.traded_on <= as_of
        })
        .collect();
    own.sort_by_key(|trade| (trade.traded_on, trade.meta.id));

    let mut quantity = Decimal::ZERO;
    let mut cost = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut dividends = Decimal::ZERO;
    let mut oversold = false;
    let mut unconverted = Vec::new();
    for trade in &own {
        // Montos en la moneda del activo.
        let rate = if trade.currency == asset.currency {
            Some(Decimal::ONE)
        } else {
            fx.rate(&trade.currency, &asset.currency, trade.traded_on)
        };
        let Some(rate) = rate else {
            unconverted.push(trade.meta.id);
            continue;
        };
        let convert = |value: Decimal| (value * rate).round_dp(AMOUNT_SCALE);
        let fee = convert(trade.fee.unwrap_or_default());
        let units = trade.quantity.unwrap_or(Decimal::ONE);
        match trade.kind {
            TradeKind::Buy | TradeKind::TransferIn => {
                quantity += units;
                cost += convert(gross(trade)) + fee;
            }
            TradeKind::Sell | TradeKind::TransferOut => {
                if units > quantity {
                    oversold = true;
                }
                let average = if quantity > Decimal::ZERO {
                    cost / quantity
                } else {
                    Decimal::ZERO
                };
                let removed =
                    (average * units.min(quantity.max(Decimal::ZERO))).round_dp(AMOUNT_SCALE);
                if trade.kind == TradeKind::Sell {
                    realized += convert(gross(trade)) - fee - removed;
                } else {
                    realized -= fee;
                }
                cost -= removed;
                quantity -= units;
            }
            TradeKind::Dividend => dividends += convert(gross(trade)) - fee,
            TradeKind::Fee => realized -= convert(gross(trade)) + fee,
            TradeKind::Split => {
                if let Some(ratio) = trade.split_ratio.filter(|ratio| *ratio > Decimal::ZERO) {
                    quantity *= ratio;
                }
            }
        }
    }
    if quantity <= Decimal::ZERO {
        cost = Decimal::ZERO;
    }

    let price = price_at(asset, &own, prices, fx, as_of);
    let market_value =
        price.map(|(price, _, _)| (price * quantity.max(Decimal::ZERO)).round_dp(AMOUNT_SCALE));
    Ok(Position {
        asset_id: asset.meta.id,
        quantity,
        cost_basis: cost,
        average_cost: (quantity > Decimal::ZERO).then(|| cost / quantity),
        realized,
        dividends,
        price,
        market_value,
        unrealized: market_value.map(|value| value - cost),
        oversold,
        unconverted,
    })
}

/// Valor de la cartera en moneda base en `date`, y los activos que no se
/// pudieron valorar o convertir.
pub fn portfolio_value(
    assets: &[Asset],
    trades: &[Trade],
    prices: &[AssetPrice],
    fx: &FxTable,
    base: &crate::money::Currency,
    date: NaiveDate,
) -> Result<(Decimal, Vec<Uuid>), MoneyError> {
    let mut total = Decimal::ZERO;
    let mut missing = Vec::new();
    for asset in assets.iter().filter(|asset| !asset.meta.is_deleted()) {
        let position = position(asset, trades, prices, fx, date)?;
        if position.quantity.is_zero() {
            continue;
        }
        match position
            .market_value
            .and_then(|value| fx.convert(&Money::new(value, asset.currency.clone()), base, date))
        {
            Some(value) => {
                total = total
                    .checked_add(value.amount)
                    .ok_or(MoneyError::Overflow)?
            }
            None => missing.push(asset.meta.id),
        }
    }
    Ok((total, missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::FxRate;
    use crate::ledger::test_support::*;
    use crate::model::AssetKind;

    fn day(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn asset(value: u128, currency: &str, provider: PriceProviderId, symbol: &str) -> Asset {
        Asset {
            meta: meta(value),
            name: symbol.into(),
            kind: AssetKind::Stock,
            currency: cur(currency),
            provider,
            provider_symbol: Some(symbol.into()),
            manual_price: None,
            manual_price_on: None,
            note: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn trade(
        value: u128,
        asset: u128,
        kind: TradeKind,
        date: &str,
        quantity: Option<&str>,
        price: Option<&str>,
        fee: Option<&str>,
        currency: &str,
    ) -> Trade {
        Trade {
            meta: meta(value),
            asset_id: id(asset),
            kind,
            traded_on: day(date),
            quantity: quantity.map(dec),
            price: price.map(dec),
            fee: fee.map(dec),
            currency: cur(currency),
            split_ratio: None,
            account_id: None,
            transaction_id: None,
            note: None,
        }
    }

    fn market(
        symbol: &str,
        provider: PriceProviderId,
        date: &str,
        price: &str,
        currency: &str,
    ) -> AssetPrice {
        AssetPrice {
            provider,
            symbol: symbol.into(),
            price_on: day(date),
            price: dec(price),
            currency: cur(currency),
        }
    }

    /// Cartera de referencia calculada a mano: compras, split 2:1, venta
    /// parcial con comisión, dividendo y un ETF en euros.
    fn reference() -> (Vec<Asset>, Vec<Trade>, Vec<AssetPrice>, FxTable) {
        use PriceProviderId::{Coingecko, Eodhd};
        let assets = vec![
            asset(1, "USD", Eodhd, "ACME.US"),
            asset(2, "EUR", Eodhd, "EUETF.XETRA"),
            asset(3, "USD", Coingecko, "bitcoin"),
        ];
        let mut split = trade(
            13,
            1,
            TradeKind::Split,
            "2026-03-01",
            None,
            None,
            None,
            "USD",
        );
        split.split_ratio = Some(dec("2"));
        let trades = vec![
            trade(
                11,
                1,
                TradeKind::Buy,
                "2026-01-10",
                Some("10"),
                Some("100"),
                Some("1"),
                "USD",
            ),
            trade(
                12,
                1,
                TradeKind::Buy,
                "2026-02-10",
                Some("10"),
                Some("120"),
                Some("1"),
                "USD",
            ),
            split,
            trade(
                14,
                1,
                TradeKind::Sell,
                "2026-04-10",
                Some("15"),
                Some("70"),
                Some("2"),
                "USD",
            ),
            trade(
                15,
                1,
                TradeKind::Dividend,
                "2026-05-10",
                None,
                Some("10"),
                None,
                "USD",
            ),
            trade(
                21,
                2,
                TradeKind::Buy,
                "2026-01-15",
                Some("5"),
                Some("200"),
                None,
                "EUR",
            ),
            trade(
                31,
                3,
                TradeKind::Buy,
                "2026-02-01",
                Some("0.5"),
                Some("40000"),
                None,
                "USD",
            ),
        ];
        let prices = vec![
            market("ACME.US", Eodhd, "2026-11-08", "60", "USD"),
            market("ACME.US", Eodhd, "2026-11-01", "58", "USD"),
            market("EUETF.XETRA", Eodhd, "2026-11-08", "210", "EUR"),
            market("bitcoin", Coingecko, "2026-11-09", "60000", "USD"),
        ];
        let fx = FxTable::new([FxRate {
            base: cur("EUR"),
            quote: cur("USD"),
            rate_on: day("2026-01-01"),
            rate: dec("1.1"),
        }]);
        (assets, trades, prices, fx)
    }

    #[test]
    fn cartera_de_referencia_con_split_y_venta_parcial() {
        let (assets, trades, prices, fx) = reference();
        let today = day("2026-11-09");
        let acme = position(&assets[0], &trades, &prices, &fx, today).unwrap();
        assert_eq!(acme.quantity, dec("25"));
        assert_eq!(acme.cost_basis, dec("1376.25"));
        assert_eq!(acme.average_cost, Some(dec("55.05")));
        assert_eq!(acme.realized, dec("222.25"));
        assert_eq!(acme.dividends, dec("10"));
        assert_eq!(
            acme.price,
            Some((dec("60"), day("2026-11-08"), PriceSource::Market))
        );
        assert_eq!(acme.market_value, Some(dec("1500")));
        assert_eq!(acme.unrealized, Some(dec("123.75")));
        assert!(!acme.oversold);

        let etf = position(&assets[1], &trades, &prices, &fx, today).unwrap();
        assert_eq!(
            (etf.market_value, etf.unrealized),
            (Some(dec("1050")), Some(dec("50")))
        );

        let (total, missing) =
            portfolio_value(&assets, &trades, &prices, &fx, &cur("USD"), today).unwrap();
        // 1.500 + 1.050 × 1,1 + 0,5 × 60.000
        assert_eq!(total, dec("32655"));
        assert!(missing.is_empty());
    }

    #[test]
    fn valor_en_una_fecha_pasada() {
        let (assets, trades, prices, fx) = reference();
        // Antes del split y de la venta: 20 acciones a 120 (última compra).
        let acme = position(&assets[0], &trades, &prices, &fx, day("2026-02-20")).unwrap();
        assert_eq!(acme.quantity, dec("20"));
        assert_eq!(
            acme.price,
            Some((dec("120"), day("2026-02-10"), PriceSource::LastTrade))
        );
    }

    #[test]
    fn precio_manual_mas_reciente_gana_al_de_mercado() {
        let (mut assets, trades, prices, fx) = reference();
        assets[0].manual_price = Some(dec("65"));
        assets[0].manual_price_on = Some(day("2026-11-09"));
        let acme = position(&assets[0], &trades, &prices, &fx, day("2026-11-09")).unwrap();
        assert_eq!(
            acme.price.map(|p| (p.0, p.2)),
            Some((dec("65"), PriceSource::Manual))
        );
        assets[0].manual_price_on = Some(day("2026-10-01"));
        let acme = position(&assets[0], &trades, &prices, &fx, day("2026-11-09")).unwrap();
        assert_eq!(acme.price.map(|p| p.2), Some(PriceSource::Market));
    }

    #[test]
    fn precio_en_otra_moneda_se_convierte_con_el_cambio_de_su_fecha() {
        let btc = asset(9, "CLP", PriceProviderId::Coingecko, "bitcoin");
        let trades = [trade(
            90,
            9,
            TradeKind::Buy,
            "2026-01-10",
            Some("0.5"),
            Some("50000000"),
            None,
            "CLP",
        )];
        let prices = [
            market(
                "bitcoin",
                PriceProviderId::Coingecko,
                "2026-11-08",
                "70000",
                "USD",
            ),
            market(
                "bitcoin",
                PriceProviderId::Coingecko,
                "2026-11-09",
                "72000",
                "USD",
            ),
        ];
        let rate = |date: &str, value: &str| FxRate {
            base: cur("USD"),
            quote: cur("CLP"),
            rate_on: day(date),
            rate: dec(value),
        };
        let fx = FxTable::new([rate("2026-11-01", "950"), rate("2026-11-09", "960")]);
        let position = position(&btc, &trades, &prices, &fx, day("2026-11-10")).unwrap();
        assert_eq!(
            position.price,
            Some((dec("69120000"), day("2026-11-09"), PriceSource::Market))
        );
        assert_eq!(position.market_value, Some(dec("34560000")));
        // Sin tipo de cambio no se inventa: queda el de la última operación.
        let none = position_without_fx(&btc, &trades, &prices);
        assert_eq!(none.map(|p| p.2), Some(PriceSource::LastTrade));
    }

    fn position_without_fx(
        asset: &Asset,
        trades: &[Trade],
        prices: &[AssetPrice],
    ) -> Option<(Decimal, NaiveDate, PriceSource)> {
        position(
            asset,
            trades,
            prices,
            &FxTable::default(),
            day("2026-11-10"),
        )
        .unwrap()
        .price
    }

    #[test]
    fn activo_manual_y_venta_de_mas() {
        let mut house = asset(4, "CLP", PriceProviderId::Manual, "casa");
        house.provider_symbol = None;
        house.manual_price = Some(dec("150000000"));
        let trades = vec![
            trade(
                41,
                4,
                TradeKind::Buy,
                "2020-01-01",
                None,
                Some("100000000"),
                None,
                "CLP",
            ),
            trade(
                42,
                4,
                TradeKind::Sell,
                "2026-01-01",
                Some("2"),
                Some("1"),
                None,
                "CLP",
            ),
        ];
        let state = position(
            &house,
            &trades[..1],
            &[],
            &FxTable::default(),
            day("2026-11-09"),
        )
        .unwrap();
        assert_eq!(state.quantity, Decimal::ONE);
        assert_eq!(state.unrealized, Some(dec("50000000")));
        let oversold =
            position(&house, &trades, &[], &FxTable::default(), day("2026-11-09")).unwrap();
        assert!(oversold.oversold);
    }

    #[test]
    fn operacion_en_otra_moneda_sin_tasa_se_aparta() {
        let (assets, mut trades, prices, _) = reference();
        trades.push(trade(
            16,
            1,
            TradeKind::Buy,
            "2026-06-01",
            Some("1"),
            Some("50"),
            None,
            "JPY",
        ));
        let acme = position(
            &assets[0],
            &trades,
            &prices,
            &FxTable::default(),
            day("2026-11-09"),
        )
        .unwrap();
        assert_eq!(acme.unconverted, vec![id(16)]);
        assert_eq!(acme.quantity, dec("25"));
    }
}
