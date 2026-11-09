//! `LocalStore` nativo. Se completa en la Fase 2.

use rusqlite::Connection;

/// Guarda y relee un valor. Valida SQLite empaquetado en cada target nativo.
pub fn roundtrip(conn: &Connection, value: &str) -> rusqlite::Result<String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS probe (value TEXT NOT NULL); DELETE FROM probe;",
    )?;
    conn.execute("INSERT INTO probe (value) VALUES (?1)", [value])?;
    conn.query_row("SELECT value FROM probe", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conserva_acentos() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(
            roundtrip(&conn, "Economía ñandú").unwrap(),
            "Economía ñandú"
        );
    }
}
