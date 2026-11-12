//! Datos de gráficos listos para dibujar. Las dos interfaces pintan lo mismo:
//! aquí se decide la escala, las marcas del eje y los textos.
//! Los valores se pasan a `f64` solo para dibujar; los importes viajan como texto.

use crate::i18n::Lang;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// Colores de serie validados para daltonismo y contraste (claro, oscuro).
pub const SERIES_COLORS: [(&str, &str); 2] = [("#2a78d6", "#3987e5"), ("#eb6834", "#d95926")];

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    /// Posición en `SERIES_COLORS`: el color sigue a la serie, nunca a su orden.
    pub slot: usize,
    pub values: Vec<Option<f64>>,
    /// Texto exacto de cada valor, para la ayuda emergente y la tabla.
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub labels: Vec<String>,
    pub series: Vec<ChartSeries>,
    pub min: f64,
    pub max: f64,
    pub ticks: Vec<Tick>,
}

pub fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// "950", "12.500", "45 mil", "1,2 M" (o "45K", "1.2M" en inglés).
pub fn compact(value: f64, lang: Lang) -> String {
    if crate::privacy::is_private() {
        return "•••".to_owned();
    }
    let abs = value.abs();
    let sign = if value < 0.0 { "-" } else { "" };
    let (scaled, suffix) = if abs >= 1_000_000.0 {
        (abs / 1_000_000.0, if lang == Lang::Es { " M" } else { "M" })
    } else if abs >= 10_000.0 {
        (abs / 1_000.0, if lang == Lang::Es { " mil" } else { "K" })
    } else {
        let whole = Decimal::from_f64_retain(abs.round()).unwrap_or_default();
        let currency = caudal_core::Currency::new("CLP").expect("CLP es válido");
        return format!(
            "{sign}{}",
            crate::format::format_amount(whole, &currency, lang)
        );
    };
    let mut text = if scaled >= 100.0 || (scaled.fract()).abs() < 0.05 {
        format!("{:.0}", scaled)
    } else {
        format!("{:.1}", scaled)
    };
    if lang == Lang::Es {
        text = text.replace('.', ",");
    }
    format!("{sign}{text}{suffix}")
}

/// Marcas redondas (1, 2 o 5 × 10ⁿ) que cubren el rango y siempre incluyen 0.
pub fn nice_ticks(min: f64, max: f64, lang: Lang) -> (f64, f64, Vec<Tick>) {
    let low = min.min(0.0);
    let high = max.max(0.0);
    let span = (high - low).max(1.0);
    let raw = span / 4.0;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|factor| factor * magnitude)
        .find(|step| *step >= raw)
        .unwrap_or(10.0 * magnitude);
    let start = (low / step).floor() * step;
    let end = (high / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = start;
    while value <= end + step * 0.001 {
        ticks.push(Tick {
            value,
            label: compact(value, lang),
        });
        value += step;
    }
    (start, end.max(start + step), ticks)
}

impl Chart {
    pub fn new(labels: Vec<String>, series: Vec<ChartSeries>, lang: Lang) -> Self {
        let values = series
            .iter()
            .flat_map(|s| s.values.iter().flatten().copied());
        let (min, max) = values.fold((0.0f64, 0.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let (min, max, ticks) = nice_ticks(min, max, lang);
        Self {
            labels,
            series,
            min,
            max,
            ticks,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.series.iter().all(|series| {
            series
                .values
                .iter()
                .all(|value| value.is_none_or(|v| v == 0.0))
        })
    }

    /// Posición vertical relativa (0 abajo, 1 arriba) de un valor.
    pub fn fraction(&self, value: f64) -> f64 {
        if self.max <= self.min {
            0.0
        } else {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquetas_compactas() {
        assert_eq!(compact(950.0, Lang::Es), "950");
        assert_eq!(compact(9_500.0, Lang::Es), "9.500");
        assert_eq!(compact(45_000.0, Lang::Es), "45 mil");
        assert_eq!(compact(1_250_000.0, Lang::Es), "1,2 M");
        assert_eq!(compact(1_250_000.0, Lang::En), "1.2M");
        assert_eq!(compact(-20_000.0, Lang::En), "-20K");
    }

    #[test]
    fn marcas_redondas_que_incluyen_cero() {
        let (min, max, ticks) = nice_ticks(0.0, 1_430_000.0, Lang::Es);
        assert_eq!(min, 0.0);
        assert!(max >= 1_430_000.0);
        let labels: Vec<_> = ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["0", "500 mil", "1 M", "1,5 M"]);
        let (min, _, ticks) = nice_ticks(-300.0, 900.0, Lang::En);
        assert!(min <= -300.0);
        assert!(ticks.iter().any(|t| t.value == 0.0));
    }
}
