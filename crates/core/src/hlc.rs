//! Reloj lógico híbrido: marcas que crecen siempre, aunque el reloj del
//! dispositivo retroceda, y que se ordenan correctamente como texto.

use crate::model::Hlc;

/// Mayor valor que caben en 12 cifras hexadecimales (año 10889).
const MAX_MILLIS: u64 = 0xFFFF_FFFF_FFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HlcParts {
    pub millis: u64,
    pub counter: u16,
    pub node: u64,
}

impl HlcParts {
    /// Formato fijo `mmmmmmmmmmmmcccc-nnnnnnnnnnnnnnnn`: el orden del texto es
    /// el orden de (milisegundos, contador, nodo).
    pub fn encode(self) -> Hlc {
        Hlc(format!(
            "{:012x}{:04x}-{:016x}",
            self.millis.min(MAX_MILLIS),
            self.counter,
            self.node
        ))
    }

    pub fn decode(stamp: &Hlc) -> Option<Self> {
        let text = stamp.0.as_str();
        if text.len() != 33 || text.as_bytes()[16] != b'-' {
            return None;
        }
        Some(Self {
            millis: u64::from_str_radix(&text[..12], 16).ok()?,
            counter: u16::from_str_radix(&text[12..16], 16).ok()?,
            node: u64::from_str_radix(&text[17..], 16).ok()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HlcClock {
    last: HlcParts,
}

impl HlcClock {
    /// `last` es la última marca emitida por este dispositivo, si existe.
    pub fn new(node: u64, last: Option<&Hlc>) -> Self {
        let mut clock = Self {
            last: HlcParts {
                millis: 0,
                counter: 0,
                node,
            },
        };
        if let Some(last) = last {
            clock.observe(last);
        }
        clock
    }

    /// Nueva marca, siempre mayor que cualquier otra emitida u observada.
    pub fn tick(&mut self, now_millis: u64) -> Hlc {
        let now = now_millis.min(MAX_MILLIS);
        if now > self.last.millis {
            self.last.millis = now;
            self.last.counter = 0;
        } else if self.last.counter == u16::MAX {
            self.last.millis += 1;
            self.last.counter = 0;
        } else {
            self.last.counter += 1;
        }
        self.last.encode()
    }

    /// Avanza el reloj hasta una marca recibida. Ignora marcas mal formadas.
    pub fn observe(&mut self, stamp: &Hlc) {
        if let Some(seen) = HlcParts::decode(stamp)
            && (seen.millis, seen.counter) > (self.last.millis, self.last.counter)
        {
            self.last.millis = seen.millis;
            self.last.counter = seen.counter;
        }
    }

    pub fn last(&self) -> Hlc {
        self.last.encode()
    }

    pub fn node(&self) -> u64 {
        self.last.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn crece_aunque_el_reloj_retroceda() {
        let mut clock = HlcClock::new(1, None);
        let first = clock.tick(1_000);
        let second = clock.tick(500);
        let third = clock.tick(500);
        assert!(first < second && second < third);
    }

    #[test]
    fn supera_lo_observado() {
        let mut other = HlcClock::new(2, None);
        let remote = other.tick(9_000);
        let mut clock = HlcClock::new(1, None);
        clock.observe(&remote);
        assert!(clock.tick(1_000) > remote);
    }

    #[test]
    fn retoma_desde_la_ultima_marca_guardada() {
        let mut clock = HlcClock::new(1, None);
        let saved = clock.tick(5_000);
        let mut reopened = HlcClock::new(1, Some(&saved));
        assert!(reopened.tick(10) > saved);
    }

    #[test]
    fn ignora_marcas_mal_formadas() {
        let mut clock = HlcClock::new(1, None);
        clock.observe(&Hlc("0001".into()));
        clock.observe(&Hlc("zzzzzzzzzzzzzzzz-0000000000000001".into()));
        assert_eq!(HlcParts::decode(&clock.last()).unwrap().millis, 0);
    }

    #[test]
    fn el_contador_desborda_hacia_el_milisegundo() {
        let mut clock = HlcClock::new(1, None);
        clock.observe(
            &HlcParts {
                millis: 7,
                counter: u16::MAX,
                node: 3,
            }
            .encode(),
        );
        let next = HlcParts::decode(&clock.tick(0)).unwrap();
        assert_eq!((next.millis, next.counter), (8, 0));
    }

    proptest! {
        #[test]
        fn el_texto_ordena_igual_que_las_partes(
            a in (0u64..=MAX_MILLIS, any::<u16>(), any::<u64>()),
            b in (0u64..=MAX_MILLIS, any::<u16>(), any::<u64>()),
        ) {
            let a = HlcParts { millis: a.0, counter: a.1, node: a.2 };
            let b = HlcParts { millis: b.0, counter: b.1, node: b.2 };
            prop_assert_eq!(a.cmp(&b), a.encode().cmp(&b.encode()));
            prop_assert_eq!(HlcParts::decode(&a.encode()), Some(a));
        }

        #[test]
        fn las_marcas_siempre_crecen(times in proptest::collection::vec(0u64..10_000, 1..200)) {
            let mut clock = HlcClock::new(42, None);
            let mut previous = clock.last();
            for now in times {
                let next = clock.tick(now);
                prop_assert!(next > previous);
                previous = next;
            }
        }
    }
}
