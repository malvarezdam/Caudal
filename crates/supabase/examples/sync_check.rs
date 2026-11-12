//! Comprobación de sincronización contra el Supabase real, con tu cuenta.
//!
//! Simula dos dispositivos y deja tres etiquetas de prueba en la papelera.
//! Las credenciales se leen de variables de entorno y nunca se guardan.
//!
//! PowerShell:
//!   $env:CAUDAL_EMAIL = "tu@correo"; $env:CAUDAL_PASSWORD = "..."
//!   cargo run -p caudal-supabase --example sync_check

use caudal_store_sqlite::SqliteStore;
use caudal_supabase::{SupabaseClient, SupabaseConfig};
use caudal_sync::{Repository, Row, Table, sync};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type Device = Repository<SqliteStore>;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn tag(id: Uuid, name: &str) -> Row {
    json!({"id": id.to_string(), "name": name, "color": null})
        .as_object()
        .cloned()
        .unwrap_or_default()
}

async fn device(user: Uuid) -> Result<Device, Box<dyn std::error::Error>> {
    let node = Uuid::new_v4().as_u64_pair().0;
    Ok(Repository::open(
        SqliteStore::open_in_memory()?,
        user,
        now_millis,
        move || node,
    )
    .await?)
}

async fn name_in(device: &Device, id: Uuid) -> Result<String, Box<dyn std::error::Error>> {
    let record = device
        .get(Table::Tags, id)
        .await?
        .ok_or("no llegó el registro")?;
    let deleted = if record.is_deleted() {
        " (papelera)"
    } else {
        ""
    };
    Ok(format!(
        "{}{deleted}",
        record.row["name"].as_str().unwrap_or_default()
    ))
}

fn check(label: &str, ok: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("{} {label}", if ok { "OK   " } else { "FALLO" });
    if ok {
        Ok(())
    } else {
        Err(label.to_owned().into())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (Ok(email), Ok(password)) = (
        std::env::var("CAUDAL_EMAIL"),
        std::env::var("CAUDAL_PASSWORD"),
    ) else {
        eprintln!(
            "Define CAUDAL_EMAIL y CAUDAL_PASSWORD (ver el comentario al inicio del archivo)."
        );
        std::process::exit(2);
    };
    let client = SupabaseClient::new(SupabaseConfig::development());
    let session = client.sign_in(&email, &password).await?;
    println!(
        "Sesión iniciada para {}",
        session.user.email.as_deref().unwrap_or("?")
    );
    let user = session.user.id;

    let a = device(user).await?;
    let b = device(user).await?;
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let decimals = Uuid::now_v7();

    a.save(Table::Tags, tag(first, "sync-check: creada en A"))
        .await?;
    sync(&a, &client, 50).await?;
    sync(&b, &client, 50).await?;
    check(
        "B recibe lo creado en A",
        name_in(&b, first).await? == "sync-check: creada en A",
    )?;

    // Conflicto: A edita "sin conexión" y B edita después.
    a.save(Table::Tags, tag(first, "sync-check: editada en A"))
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    b.save(Table::Tags, tag(first, "sync-check: editada en B"))
        .await?;
    sync(&b, &client, 50).await?;
    sync(&a, &client, 50).await?;
    sync(&b, &client, 50).await?;
    check(
        "gana el último cambio en ambos",
        name_in(&a, first).await? == "sync-check: editada en B"
            && name_in(&b, first).await? == "sync-check: editada en B",
    )?;

    // Repetir la subida no duplica ni pisa nada.
    b.save(Table::Tags, tag(second, "sync-check: segunda"))
        .await?;
    sync(&b, &client, 1).await?;
    sync(&b, &client, 1).await?;
    sync(&a, &client, 1).await?;
    check(
        "sin duplicados con lotes de 1",
        name_in(&a, second).await? == "sync-check: segunda",
    )?;

    // Importes exactos ida y vuelta por PostgREST.
    let mut account = json!({
        "id": decimals.to_string(), "name": "sync-check: cuenta", "kind": "other",
        "currency": "USD", "opening_balance": "1234567890123456.123456789012",
        "archived": false, "color": null, "icon": null, "note": null
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    account.insert("deleted_at".into(), json!(null));
    a.save(Table::Accounts, account).await?;
    sync(&a, &client, 50).await?;
    sync(&b, &client, 50).await?;
    let balance = b
        .get(Table::Accounts, decimals)
        .await?
        .ok_or("no llegó la cuenta")?
        .row["opening_balance"]
        .clone();
    check(
        "importe exacto de 28 dígitos",
        balance
            .as_str()
            .is_some_and(|text| text.starts_with("1234567890123456.123456789012")),
    )?;

    for (table, id) in [
        (Table::Tags, first),
        (Table::Tags, second),
        (Table::Accounts, decimals),
    ] {
        a.delete(table, id).await?;
    }
    sync(&a, &client, 50).await?;
    sync(&b, &client, 50).await?;
    check(
        "el borrado llega como papelera",
        name_in(&b, first).await?.ends_with("(papelera)"),
    )?;

    client.sign_out().await?;
    println!("Todo correcto. Los registros de prueba quedaron en la papelera.");
    Ok(())
}
