//! Dominio y cálculos puros. Sin IO; compila a nativo y WASM.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Importe exacto con su moneda (código ISO 4217 o de cripto).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("monedas distintas: {0} y {1}")]
    CurrencyMismatch(String, String),
    #[error("desbordamiento al operar importes")]
    Overflow,
}

impl Money {
    pub fn new(amount: Decimal, currency: impl Into<String>) -> Self {
        Self {
            amount,
            currency: currency.into(),
        }
    }

    pub fn checked_add(&self, other: &Money) -> Result<Money, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch(
                self.currency.clone(),
                other.currency.clone(),
            ));
        }
        let amount = self
            .amount
            .checked_add(other.amount)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(amount, self.currency.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn usd(value: &str) -> Money {
        Money::new(Decimal::from_str(value).unwrap(), "USD")
    }

    #[test]
    fn suma_decimal_exacta() {
        assert_eq!(usd("0.1").checked_add(&usd("0.2")).unwrap(), usd("0.3"));
    }

    #[test]
    fn rechaza_monedas_distintas() {
        let eur = Money::new(Decimal::ONE, "EUR");
        assert!(matches!(
            usd("1").checked_add(&eur),
            Err(MoneyError::CurrencyMismatch(_, _))
        ));
    }

    #[test]
    fn serializa_decimal_como_texto() {
        let json = serde_json_like(&usd("1234.5600"));
        assert!(json.contains("\"1234.5600\""), "{json}");
    }

    fn serde_json_like(money: &Money) -> String {
        serde_json::to_string(money).unwrap()
    }
}
