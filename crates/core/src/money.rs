//! Importes exactos con moneda.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Decimales que admite la base de datos (`numeric(28,12)`).
pub const AMOUNT_SCALE: u32 = 12;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("código de moneda no válido: {0}")]
    InvalidCurrency(String),
    #[error("monedas distintas: {0} y {1}")]
    CurrencyMismatch(Currency, Currency),
    #[error("desbordamiento al operar importes")]
    Overflow,
}

/// Código ISO 4217 o de cripto: de 2 a 10 mayúsculas o dígitos.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);

impl Currency {
    pub fn new(code: &str) -> Result<Self, MoneyError> {
        let valid = (2..=10).contains(&code.len())
            && code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit());
        if valid {
            Ok(Self(code.to_owned()))
        } else {
            Err(MoneyError::InvalidCurrency(code.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Currency {
    type Error = MoneyError;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Self::new(&code)
    }
}

impl From<Currency> for String {
    fn from(currency: Currency) -> Self {
        currency.0
    }
}

/// Importe exacto con su moneda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Self::new(Decimal::ZERO, currency)
    }

    pub fn checked_add(&self, other: &Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        let amount = self
            .amount
            .checked_add(other.amount)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(amount, self.currency.clone()))
    }

    pub fn checked_sub(&self, other: &Money) -> Result<Money, MoneyError> {
        self.ensure_same_currency(other)?;
        let amount = self
            .amount
            .checked_sub(other.amount)
            .ok_or(MoneyError::Overflow)?;
        Ok(Money::new(amount, self.currency.clone()))
    }

    fn ensure_same_currency(&self, other: &Money) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch(
                self.currency.clone(),
                other.currency.clone(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::str::FromStr;

    fn usd(value: &str) -> Money {
        Money::new(
            Decimal::from_str(value).unwrap(),
            Currency::new("USD").unwrap(),
        )
    }

    #[test]
    fn suma_decimal_exacta() {
        assert_eq!(usd("0.1").checked_add(&usd("0.2")).unwrap(), usd("0.3"));
    }

    #[test]
    fn rechaza_monedas_distintas() {
        let eur = Money::zero(Currency::new("EUR").unwrap());
        assert!(matches!(
            usd("1").checked_add(&eur),
            Err(MoneyError::CurrencyMismatch(_, _))
        ));
    }

    #[test]
    fn detecta_desbordamiento() {
        let max = Money::new(Decimal::MAX, Currency::new("USD").unwrap());
        assert_eq!(max.checked_add(&usd("1")), Err(MoneyError::Overflow));
    }

    #[test]
    fn valida_codigos_de_moneda() {
        for valid in ["USD", "CLP", "BTC", "USDT", "1INCH"] {
            assert!(Currency::new(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "U", "usd", "US D", "ÑAN", "ABCDEFGHIJK"] {
            assert!(Currency::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn serializa_decimal_como_texto() {
        let json = serde_json::to_string(&usd("1234.5600")).unwrap();
        assert!(json.contains("\"1234.5600\""), "{json}");
        assert_eq!(
            serde_json::from_str::<Money>(&json).unwrap(),
            usd("1234.56")
        );
    }

    #[test]
    fn json_rechaza_moneda_invalida() {
        let json = r#"{"amount":"1","currency":"usd"}"#;
        assert!(serde_json::from_str::<Money>(json).is_err());
    }

    fn cents() -> impl Strategy<Value = Decimal> {
        (-1_000_000_000_000i64..1_000_000_000_000).prop_map(|value| Decimal::new(value, 2))
    }

    proptest! {
        #[test]
        fn sumar_y_restar_vuelve_al_origen(a in cents(), b in cents()) {
            let currency = Currency::new("USD").unwrap();
            let a = Money::new(a, currency.clone());
            let b = Money::new(b, currency);
            prop_assert_eq!(a.checked_add(&b).unwrap().checked_sub(&b).unwrap(), a);
        }
    }
}
