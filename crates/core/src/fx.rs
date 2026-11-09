//! Tipos de cambio históricos.

use crate::money::{AMOUNT_SCALE, Currency, Money};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// `1 base = rate quote` en la fecha indicada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxRate {
    pub base: Currency,
    pub quote: Currency,
    pub rate_on: NaiveDate,
    pub rate: Decimal,
}

/// Consulta de tipos de cambio: usa el último conocido en la fecha pedida o antes.
#[derive(Debug, Clone, Default)]
pub struct FxTable {
    rates: BTreeMap<(Currency, Currency), BTreeMap<NaiveDate, Decimal>>,
    currencies: BTreeSet<Currency>,
}

impl FxTable {
    /// Ignora tasas nulas o negativas, que no se pueden invertir ni tienen sentido.
    pub fn new(rates: impl IntoIterator<Item = FxRate>) -> Self {
        let mut table = Self::default();
        for rate in rates {
            if rate.rate <= Decimal::ZERO {
                continue;
            }
            table.currencies.insert(rate.base.clone());
            table.currencies.insert(rate.quote.clone());
            table
                .rates
                .entry((rate.base, rate.quote))
                .or_default()
                .insert(rate.rate_on, rate.rate);
        }
        table
    }

    /// Tasa directa, inversa o cruzada a través de una moneda intermedia.
    pub fn rate(&self, from: &Currency, to: &Currency, date: NaiveDate) -> Option<Decimal> {
        if from == to {
            return Some(Decimal::ONE);
        }
        self.pair_rate(from, to, date).or_else(|| {
            self.currencies.iter().find_map(|pivot| {
                let first = self.pair_rate(from, pivot, date)?;
                let second = self.pair_rate(pivot, to, date)?;
                first.checked_mul(second)
            })
        })
    }

    /// `None` si no hay tasa disponible o el resultado no cabe.
    pub fn convert(&self, money: &Money, to: &Currency, date: NaiveDate) -> Option<Money> {
        let rate = self.rate(&money.currency, to, date)?;
        let amount = money.amount.checked_mul(rate)?.round_dp(AMOUNT_SCALE);
        Some(Money::new(amount, to.clone()))
    }

    fn pair_rate(&self, from: &Currency, to: &Currency, date: NaiveDate) -> Option<Decimal> {
        self.latest(from, to, date).or_else(|| {
            self.latest(to, from, date)
                .and_then(|rate| Decimal::ONE.checked_div(rate))
        })
    }

    fn latest(&self, from: &Currency, to: &Currency, date: NaiveDate) -> Option<Decimal> {
        self.rates
            .get(&(from.clone(), to.clone()))?
            .range(..=date)
            .next_back()
            .map(|(_, rate)| *rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn cur(code: &str) -> Currency {
        Currency::new(code).unwrap()
    }

    fn dec(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::from_str(value).unwrap()
    }

    fn rate(base: &str, quote: &str, date: &str, value: &str) -> FxRate {
        FxRate {
            base: cur(base),
            quote: cur(quote),
            rate_on: day(date),
            rate: dec(value),
        }
    }

    fn table() -> FxTable {
        FxTable::new([
            rate("USD", "CLP", "2026-01-01", "900"),
            rate("USD", "CLP", "2026-02-01", "950"),
            rate("USD", "EUR", "2026-01-01", "0.5"),
        ])
    }

    #[test]
    fn misma_moneda_no_necesita_tasa() {
        assert_eq!(
            FxTable::default().rate(&cur("CLP"), &cur("CLP"), day("2026-01-01")),
            Some(Decimal::ONE)
        );
    }

    #[test]
    fn usa_la_ultima_tasa_conocida() {
        let fx = table();
        let usd = cur("USD");
        let clp = cur("CLP");
        assert_eq!(fx.rate(&usd, &clp, day("2026-01-15")), Some(dec("900")));
        assert_eq!(fx.rate(&usd, &clp, day("2026-02-01")), Some(dec("950")));
        assert_eq!(fx.rate(&usd, &clp, day("2025-12-31")), None);
    }

    #[test]
    fn invierte_y_cruza_tasas() {
        let fx = table();
        let date = day("2026-01-15");
        let clp = Money::new(dec("900"), cur("CLP"));
        assert_eq!(
            fx.convert(&clp, &cur("USD"), date).unwrap().amount,
            dec("1")
        );
        assert_eq!(
            fx.convert(&clp, &cur("EUR"), date).unwrap().amount,
            dec("0.5")
        );
    }

    #[test]
    fn sin_ruta_no_convierte() {
        let money = Money::new(dec("1"), cur("USD"));
        assert_eq!(
            table().convert(&money, &cur("JPY"), day("2026-01-15")),
            None
        );
    }

    #[test]
    fn ignora_tasas_no_positivas() {
        let fx = FxTable::new([rate("USD", "ARS", "2026-01-01", "0")]);
        assert_eq!(fx.rate(&cur("ARS"), &cur("USD"), day("2026-01-02")), None);
    }
}
