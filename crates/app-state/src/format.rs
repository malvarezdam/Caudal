//! Lectura y presentación de importes y fechas según el idioma.

use crate::i18n::Lang;
use caudal_core::money::{AMOUNT_SCALE, Currency, Money};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountError {
    Empty,
    Invalid,
    TooManyDecimals,
}

/// Monedas que no usan decimales.
pub(crate) const ZERO_DECIMALS: [&str; 10] = [
    "CLP", "JPY", "KRW", "VND", "ISK", "PYG", "UGX", "XAF", "XOF", "IDR",
];

/// Acepta `1.234,56`, `1,234.56`, `12,5`, `12.5`, espacios y signo. Con un solo
/// separador seguido de exactamente tres cifras, decide el idioma: en español
/// `1.234` es mil doscientos treinta y cuatro.
pub fn parse_amount(input: &str, lang: Lang) -> Result<Decimal, AmountError> {
    let mut text: String = input
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '\'')
        .collect();
    if text.is_empty() {
        return Err(AmountError::Empty);
    }
    let negative = text.starts_with('-');
    if negative || text.starts_with('+') {
        text.remove(0);
    }
    let dots = text.matches('.').count();
    let commas = text.matches(',').count();
    let decimal = match (dots, commas) {
        (0, 0) => None,
        (_, 0) | (0, _) => {
            let separator = if dots > 0 { '.' } else { ',' };
            let count = dots.max(commas);
            let digits_after = text.len() - text.rfind(separator).map_or(0, |at| at + 1);
            if count > 1 || (digits_after == 3 && separator == lang.thousands_separator()) {
                None
            } else {
                Some(separator)
            }
        }
        _ => {
            let last_dot = text.rfind('.');
            let last_comma = text.rfind(',');
            if last_dot > last_comma {
                Some('.')
            } else {
                Some(',')
            }
        }
    };
    let mut normalized = String::with_capacity(text.len() + 1);
    if negative {
        normalized.push('-');
    }
    for character in text.chars() {
        match character {
            '0'..='9' => normalized.push(character),
            separator if Some(separator) == decimal => normalized.push('.'),
            '.' | ',' => {}
            _ => return Err(AmountError::Invalid),
        }
    }
    if decimal.is_some() && text.matches(decimal.unwrap_or('.')).count() > 1 {
        return Err(AmountError::Invalid);
    }
    let digits_before_decimal = normalized
        .trim_start_matches('-')
        .split('.')
        .next()
        .unwrap_or_default()
        .len();
    if digits_before_decimal > 16 {
        return Err(AmountError::Invalid);
    }
    let value = Decimal::from_str(&normalized).map_err(|_| AmountError::Invalid)?;
    if value.scale() > AMOUNT_SCALE {
        return Err(AmountError::TooManyDecimals);
    }
    Ok(value)
}

/// Decimales a mostrar: los de la moneda, o más si el importe los tiene (máx. 8).
pub(crate) fn shown_decimals(amount: Decimal, currency: &Currency) -> u32 {
    let base = if ZERO_DECIMALS.contains(&currency.as_str()) {
        0
    } else {
        2
    };
    base.max(amount.normalize().scale().min(8))
}

/// Importe con separadores del idioma, sin moneda.
pub fn format_amount(amount: Decimal, currency: &Currency, lang: Lang) -> String {
    let decimals = shown_decimals(amount, currency);
    let text = format!("{:.*}", decimals as usize, amount.round_dp(decimals).abs());
    let (integer, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(lang.thousands_separator());
        }
        grouped.push(digit);
    }
    let sign = if amount.is_sign_negative() && !amount.round_dp(decimals).is_zero() {
        "-"
    } else {
        ""
    };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}{}{fraction}", lang.decimal_separator())
    }
}

pub fn format_money(money: &Money, lang: Lang) -> String {
    if crate::privacy::is_private() {
        return format!("{} {}", crate::privacy::MASK, money.currency);
    }
    format!(
        "{} {}",
        format_amount(money.amount, &money.currency, lang),
        money.currency
    )
}

/// Importe para editar en un campo de texto: sin separador de miles.
pub fn amount_for_input(amount: Decimal, lang: Lang) -> String {
    let text = amount.normalize().to_string();
    match lang {
        Lang::Es => text.replace('.', ","),
        Lang::En => text,
    }
}

pub fn format_date(date: NaiveDate, lang: Lang) -> String {
    match lang {
        Lang::Es => date.format("%d/%m/%Y").to_string(),
        Lang::En => date.format("%Y-%m-%d").to_string(),
    }
}

/// Lee una fecha escrita a mano en el formato del idioma o en ISO.
pub fn parse_date(input: &str, lang: Lang) -> Option<NaiveDate> {
    let input = input.trim();
    NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .ok()
        .or_else(|| match lang {
            Lang::Es => NaiveDate::parse_from_str(input, "%d/%m/%Y").ok(),
            Lang::En => None,
        })
}

/// Fecha local a partir de milisegundos UTC y la diferencia horaria en minutos.
pub fn local_date(now_millis: u64, utc_offset_minutes: i32) -> NaiveDate {
    let local_millis = now_millis as i64 + i64::from(utc_offset_minutes) * 60_000;
    chrono::DateTime::from_timestamp_millis(local_millis)
        .map(|moment| moment.date_naive())
        .unwrap_or_default()
}

/// Decimales habituales de la moneda: 0 para CLP, JPY…; 2 para el resto.
pub fn currency_decimals(currency: &Currency) -> u32 {
    if ZERO_DECIMALS.contains(&currency.as_str()) {
        0
    } else {
        2
    }
}

/// Monto calculado (promedio, estimación, interés simulado) redondeado a los
/// decimales de su moneda: un cálculo no inventa centavos en CLP.
pub fn format_estimate(money: &Money, lang: Lang) -> String {
    let rounded = money.amount.round_dp(currency_decimals(&money.currency));
    format_money(&Money::new(rounded, money.currency.clone()), lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    fn cur(code: &str) -> Currency {
        Currency::new(code).unwrap()
    }

    #[test]
    fn lee_importes_en_ambos_formatos() {
        let cases = [
            ("1.234,56", Lang::Es, "1234.56"),
            ("1,234.56", Lang::Es, "1234.56"),
            ("1,234.56", Lang::En, "1234.56"),
            ("12,5", Lang::Es, "12.5"),
            ("12.5", Lang::Es, "12.5"),
            ("1.234", Lang::Es, "1234"),
            ("1.234", Lang::En, "1.234"),
            ("1,234", Lang::En, "1234"),
            ("1,234", Lang::Es, "1.234"),
            ("1.234.567", Lang::Es, "1234567"),
            (" 2 500 ", Lang::Es, "2500"),
            ("-30", Lang::Es, "-30"),
            ("0,000000000001", Lang::Es, "0.000000000001"),
        ];
        for (input, lang, expected) in cases {
            assert_eq!(parse_amount(input, lang), Ok(dec(expected)), "{input}");
        }
    }

    #[test]
    fn rechaza_lo_que_no_es_un_importe() {
        assert_eq!(parse_amount("", Lang::Es), Err(AmountError::Empty));
        assert_eq!(parse_amount("12a", Lang::Es), Err(AmountError::Invalid));
        assert_eq!(
            parse_amount("1,2,3.4.5", Lang::Es),
            Err(AmountError::Invalid)
        );
        assert_eq!(
            parse_amount("0,0000000000001", Lang::Es),
            Err(AmountError::TooManyDecimals)
        );
        assert_eq!(
            parse_amount("12345678901234567", Lang::Es),
            Err(AmountError::Invalid)
        );
    }

    #[test]
    fn muestra_segun_moneda_e_idioma() {
        assert_eq!(
            format_amount(dec("1234567.5"), &cur("USD"), Lang::Es),
            "1.234.567,50"
        );
        assert_eq!(
            format_amount(dec("1234567.5"), &cur("USD"), Lang::En),
            "1,234,567.50"
        );
        assert_eq!(format_amount(dec("15000"), &cur("CLP"), Lang::Es), "15.000");
        assert_eq!(
            format_amount(dec("0.00012345"), &cur("BTC"), Lang::En),
            "0.00012345"
        );
        assert_eq!(format_amount(dec("-20"), &cur("EUR"), Lang::Es), "-20,00");
        assert_eq!(format_amount(dec("999"), &cur("USD"), Lang::Es), "999,00");
        assert_eq!(
            format_money(&Money::new(dec("10"), cur("CLP")), Lang::Es),
            "10 CLP"
        );
    }

    #[test]
    fn nunca_oculta_decimales_reales() {
        assert_eq!(
            format_amount(dec("10.125"), &cur("USD"), Lang::En),
            "10.125"
        );
        assert_eq!(format_amount(dec("10.5"), &cur("CLP"), Lang::Es), "10,5");
    }

    #[test]
    fn importe_para_editar_vuelve_igual() {
        for lang in Lang::ALL {
            let text = amount_for_input(dec("1234.50"), lang);
            assert_eq!(parse_amount(&text, lang), Ok(dec("1234.5")));
        }
    }

    #[test]
    fn fechas() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        assert_eq!(format_date(date, Lang::Es), "09/11/2026");
        assert_eq!(parse_date("09/11/2026", Lang::Es), Some(date));
        assert_eq!(parse_date("2026-11-09", Lang::En), Some(date));
        // 2026-11-10 02:00 UTC es todavía el 9 en Chile (UTC-3).
        let millis = 1_794_276_000_000;
        assert_eq!(local_date(millis, -180), date);
    }
}
