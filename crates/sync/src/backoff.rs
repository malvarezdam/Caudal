//! Espera entre reintentos de sincronización cuando no hay conexión.

use std::time::Duration;

const BASE: Duration = Duration::from_secs(2);
const MAX: Duration = Duration::from_secs(300);

/// Exponencial con tope y variación aleatoria, para no reintentar todos a la vez.
#[derive(Debug, Default, Clone)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    /// `jitter` en [0, 1]. Devuelve cuánto esperar antes del siguiente intento.
    pub fn next_delay(&mut self, jitter: f64) -> Duration {
        let ceiling = BASE.saturating_mul(1u32 << self.failures.min(16)).min(MAX);
        self.failures = self.failures.saturating_add(1);
        let jitter = jitter.clamp(0.0, 1.0);
        ceiling.mul_f64(0.5 + 0.5 * jitter)
    }

    /// Tras una sincronización correcta.
    pub fn reset(&mut self) {
        self.failures = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crece_hasta_el_tope_y_se_reinicia() {
        let mut backoff = Backoff::default();
        assert_eq!(backoff.next_delay(1.0), Duration::from_secs(2));
        assert_eq!(backoff.next_delay(1.0), Duration::from_secs(4));
        for _ in 0..40 {
            backoff.next_delay(1.0);
        }
        assert_eq!(backoff.next_delay(1.0), MAX);
        assert_eq!(backoff.next_delay(0.0), MAX / 2);
        backoff.reset();
        assert_eq!(backoff.next_delay(1.0), Duration::from_secs(2));
    }
}
