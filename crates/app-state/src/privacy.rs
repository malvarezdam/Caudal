//! PIN local y modo privado. El PIN es una cortina para quien tome el
//! teléfono prestado, no cifrado: los datos siguen en la base local y la
//! sesión se puede cerrar para entrar con la contraseña.

use sha2::{Digest, Sha256};
use std::cell::Cell;

thread_local! {
    // Por hilo: las vistas se preparan en el hilo de la interfaz, y así las
    // pruebas en paralelo no se pisan.
    static PRIVATE: Cell<bool> = const { Cell::new(false) };
}

/// Con el modo privado, los montos se muestran como "•••••".
pub fn set_private(enabled: bool) {
    PRIVATE.with(|flag| flag.set(enabled));
}

pub fn is_private() -> bool {
    PRIVATE.with(Cell::get)
}

pub const MASK: &str = "•••••";

/// Rondas de SHA-256: hacen lento probar los 10⁸ PIN posibles con el archivo
/// de preferencias en la mano.
const ROUNDS: u32 = 50_000;

pub fn valid_pin(pin: &str) -> bool {
    (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit())
}

pub fn new_salt() -> String {
    uuid::Uuid::now_v7().simple().to_string()
}

pub fn hash_pin(pin: &str, salt: &str) -> String {
    let mut digest = Sha256::digest(format!("{salt}:{pin}").as_bytes());
    for _ in 1..ROUNDS {
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(digest);
        digest = hasher.finalize();
    }
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Comparación en tiempo constante.
pub fn verify_pin(pin: &str, salt: &str, hash: &str) -> bool {
    let candidate = hash_pin(pin, salt);
    candidate.len() == hash.len()
        && candidate
            .bytes()
            .zip(hash.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_con_sal() {
        assert!(valid_pin("1234") && valid_pin("12345678"));
        assert!(!valid_pin("123") && !valid_pin("12a4") && !valid_pin("123456789"));
        let salt = new_salt();
        let hash = hash_pin("2468", &salt);
        assert!(verify_pin("2468", &salt, &hash));
        assert!(!verify_pin("2469", &salt, &hash));
        assert_ne!(
            hash,
            hash_pin("2468", &new_salt()),
            "la sal cambia el resultado"
        );
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    use crate::format::format_money;
    use crate::i18n::Lang;
    use caudal_core::{Currency, Money};
    use rust_decimal::Decimal;

    #[test]
    fn el_modo_privado_oculta_montos() {
        let money = Money::new(Decimal::from(1500), Currency::new("CLP").unwrap());
        set_private(true);
        let hidden = format_money(&money, Lang::Es);
        set_private(false);
        assert_eq!(hidden, "••••• CLP");
        assert_eq!(format_money(&money, Lang::Es), "1.500 CLP");
    }
}
