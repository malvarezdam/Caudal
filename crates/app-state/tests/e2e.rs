//! Pruebas de extremo a extremo contra el proyecto Supabase de desarrollo.
//!
//! Solo se ejecutan cuando se piden expresamente:
//! ```powershell
//! $env:CAUDAL_E2E="1"
//! $env:CAUDAL_E2E_DOMAIN="tudominio.com"
//! cargo test -p caudal-app-state --test e2e -- --ignored --nocapture
//! ```
//!
//! Crean cuentas temporales `caudal-prueba-<aleatorio>@<dominio>` con contraseñas
//! generadas al vuelo en memoria y las eliminan al terminar cada prueba con
//! `AppCore::delete_account`. Nunca usan la clave secreta ni credenciales reales.

use caudal_app_state::attachments::new_attachment;
use caudal_app_state::auth::{SignUpOutcome, SupabaseClient, SupabaseConfig};
use caudal_app_state::forms::{AccountForm, ProfileForm, TransactionForm};
use caudal_app_state::model::AccountKind;
use caudal_app_state::{AppCore, Lang, SyncState, Table};
use caudal_sync::memory::MemoryStore;
use chrono::NaiveDate;
use std::cell::Cell;
use std::rc::Rc;
use uuid::Uuid;

fn e2e_domain() -> Option<String> {
    if std::env::var("CAUDAL_E2E").ok().as_deref() != Some("1") {
        eprintln!("Omitida: define CAUDAL_E2E=1 para correr las pruebas contra Supabase.");
        return None;
    }
    let domain = std::env::var("CAUDAL_E2E_DOMAIN")
        .ok()
        .map(|value| value.trim().trim_start_matches('@').to_owned())
        .filter(|value| !value.is_empty())
        .expect("CAUDAL_E2E=1 exige definir CAUDAL_E2E_DOMAIN con el dominio de prueba");
    Some(domain)
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
}

struct TestAccount {
    email: String,
    password: String,
    user_id: Uuid,
    client: SupabaseClient,
}

impl TestAccount {
    async fn sign_up(domain: &str) -> Self {
        let random_id = Uuid::now_v7().simple().to_string();
        let email = format!("caudal-prueba-{random_id}@{domain}");
        let password = format!(
            "Caudal!{}#{}",
            Uuid::now_v7().simple(),
            Uuid::now_v7().simple()
        );
        let client = SupabaseClient::new(SupabaseConfig::development());
        let session = match client
            .sign_up(&email, &password)
            .await
            .expect("error al registrar cuenta de prueba en Supabase")
        {
            SignUpOutcome::SignedIn(session) => session,
            SignUpOutcome::ConfirmationRequired { .. } => panic!(
                "Supabase pidió confirmar el correo ({email}). Desactiva 'Confirm email' en \
                 Authentication -> Sign In / Providers -> Email mientras corren las pruebas E2E."
            ),
        };
        Self {
            email,
            password,
            user_id: session.user.id,
            client,
        }
    }

    async fn open_device(&self, clock: Rc<Cell<u64>>, node: u64) -> AppCore<MemoryStore> {
        AppCore::open(
            MemoryStore::default(),
            self.user_id,
            move || {
                let next = clock.get() + 1;
                clock.set(next);
                next
            },
            move || node,
        )
        .await
        .expect("abrir AppCore de prueba")
    }
}

#[tokio::test]
#[ignore = "requiere CAUDAL_E2E=1 y CAUDAL_E2E_DOMAIN contra el Supabase de desarrollo"]
async fn registro_inicio_sincronizacion_dos_dispositivos_y_borrado() {
    let Some(domain) = e2e_domain() else { return };
    let account = TestAccount::sign_up(&domain).await;

    // Cierra sesión y vuelve a entrar con la contraseña generada.
    account.client.sign_out().await.expect("cerrar sesión");
    assert!(account.client.session().is_none());
    let signed = account
        .client
        .sign_in(&account.email, &account.password)
        .await
        .expect("iniciar sesión");
    assert_eq!(signed.user.id, account.user_id);

    let clock_a = Rc::new(Cell::new(1_700_000_000_000u64));
    let clock_b = Rc::new(Cell::new(1_700_000_000_000u64));
    let device_a = account.open_device(clock_a.clone(), 0xA1).await;
    let device_b = account.open_device(clock_b.clone(), 0xB2).await;

    // Dispositivo A crea perfil, cuenta y un movimiento, y sincroniza.
    let profile = ProfileForm {
        base_currency: "CLP".into(),
        lang: Lang::Es,
        display_name: "Prueba E2E".into(),
    }
    .validate(account.user_id)
    .unwrap();
    device_a
        .complete_onboarding(&profile, false, Lang::Es)
        .await
        .unwrap();

    let data_a = device_a.load().await.unwrap();
    let bank = AccountForm {
        name: "Cuenta principal".into(),
        kind: AccountKind::Bank,
        currency: "CLP".into(),
        opening_balance: "100000".into(),
        ..AccountForm::new(&data_a)
    }
    .validate(account.user_id, Lang::Es)
    .unwrap();
    device_a.save(Table::Accounts, &bank).await.unwrap();

    let data_a = device_a.load().await.unwrap();
    let mut tx_form = TransactionForm::new(&data_a, today());
    tx_form.amount = "12500".into();
    tx_form.description = "Café inicial".into();
    let tx = tx_form
        .validate(&data_a, account.user_id, Lang::Es)
        .unwrap();
    let tx_id = tx.meta.id;
    device_a.save(Table::Transactions, &tx).await.unwrap();

    assert_eq!(
        device_a.sync(&account.client, today()).await.unwrap(),
        SyncState::Synced
    );

    // Dispositivo B baja los datos y ve exactamente lo mismo.
    assert_eq!(
        device_b.sync(&account.client, today()).await.unwrap(),
        SyncState::Synced
    );
    let data_b = device_b.load().await.unwrap();
    assert_eq!(data_b.accounts.len(), 1);
    assert_eq!(data_b.transactions.len(), 1);
    assert_eq!(
        data_b.transactions[0].description.as_deref(),
        Some("Café inicial")
    );

    // Ambos dispositivos editan sin red; B tiene marca posterior y además crea otro movimiento.
    clock_a.set(1_700_000_010_000);
    let mut edit_a =
        TransactionForm::edit(&device_a.load().await.unwrap().transactions[0], Lang::Es);
    edit_a.description = "Editado en A".into();
    let tx_a = edit_a
        .validate(&device_a.load().await.unwrap(), account.user_id, Lang::Es)
        .unwrap();
    device_a.save(Table::Transactions, &tx_a).await.unwrap();

    clock_b.set(1_700_000_020_000);
    let mut edit_b = TransactionForm::edit(&data_b.transactions[0], Lang::Es);
    edit_b.description = "Editado en B (más reciente)".into();
    edit_b.amount = "15900".into();
    let tx_b = edit_b.validate(&data_b, account.user_id, Lang::Es).unwrap();
    device_b.save(Table::Transactions, &tx_b).await.unwrap();

    let mut second_form = TransactionForm::new(&data_b, today());
    second_form.amount = "4200".into();
    second_form.description = "Panadería".into();
    let second_tx = second_form
        .validate(&data_b, account.user_id, Lang::Es)
        .unwrap();
    device_b
        .save(Table::Transactions, &second_tx)
        .await
        .unwrap();

    // Reconexión: A sube primero y luego B, o viceversa; el resultado converge sin duplicados.
    assert_eq!(
        device_a.sync(&account.client, today()).await.unwrap(),
        SyncState::Synced
    );
    assert_eq!(
        device_b.sync(&account.client, today()).await.unwrap(),
        SyncState::Synced
    );
    assert_eq!(
        device_a.sync(&account.client, today()).await.unwrap(),
        SyncState::Synced
    );

    let final_a = device_a.load().await.unwrap();
    let final_b = device_b.load().await.unwrap();
    assert_eq!(final_a.transactions.len(), 2, "no debe duplicarse nada");
    assert_eq!(final_b.transactions.len(), 2, "no debe duplicarse nada");
    let winner_a = final_a.transaction(tx_id).unwrap();
    let winner_b = final_b.transaction(tx_id).unwrap();
    assert_eq!(
        winner_a.description.as_deref(),
        Some("Editado en B (más reciente)")
    );
    assert_eq!(winner_a.amount, winner_b.amount);

    // Limpieza final: elimina la cuenta creada y comprueba que ya no puede iniciar sesión.
    device_a
        .delete_account(&account.client)
        .await
        .expect("eliminar cuenta de prueba");
    assert!(
        account
            .client
            .sign_in(&account.email, &account.password)
            .await
            .is_err(),
        "la cuenta eliminada no debe poder iniciar sesión"
    );
}

#[tokio::test]
#[ignore = "requiere CAUDAL_E2E=1 y CAUDAL_E2E_DOMAIN contra el Supabase de desarrollo"]
async fn comprobantes_aislamiento_entre_cuentas_y_eliminacion_total() {
    let Some(domain) = e2e_domain() else { return };
    let user_a = TestAccount::sign_up(&domain).await;
    let user_b = TestAccount::sign_up(&domain).await;

    let clock_a = Rc::new(Cell::new(1_700_000_100_000u64));
    let clock_b = Rc::new(Cell::new(1_700_000_100_000u64));
    let core_a1 = user_a.open_device(clock_a.clone(), 0x11).await;
    let core_a2 = user_a.open_device(clock_a.clone(), 0x12).await;
    let core_b = user_b.open_device(clock_b.clone(), 0x22).await;

    // Usuario A guarda un movimiento con un comprobante PNG y sincroniza.
    let data_a = core_a1.load().await.unwrap();
    let mut tx_form = TransactionForm::new(&data_a, today());
    tx_form.amount = "9990".into();
    tx_form.description = "Boleta de A".into();
    let tx_a = tx_form.validate(&data_a, user_a.user_id, Lang::Es).unwrap();
    core_a1.save(Table::Transactions, &tx_a).await.unwrap();

    let png = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
    let data_a = core_a1.load().await.unwrap();
    let attachment = new_attachment(&data_a, tx_a.meta.id, "boleta.png", &png).unwrap();
    core_a1.add_attachment(&attachment, &png).await.unwrap();
    assert_eq!(core_a1.pending_attachments().await.unwrap().len(), 1);

    assert_eq!(
        core_a1.sync(&user_a.client, today()).await.unwrap(),
        SyncState::Synced
    );
    assert!(core_a1.pending_attachments().await.unwrap().is_empty());

    // Otro dispositivo de A descarga los datos y el archivo desde Storage.
    assert_eq!(
        core_a2.sync(&user_a.client, today()).await.unwrap(),
        SyncState::Synced
    );
    let data_a2 = core_a2.load().await.unwrap();
    assert_eq!(data_a2.attachments.len(), 1);
    let downloaded = core_a2
        .attachment_bytes(Some(&user_a.client), &data_a2.attachments[0])
        .await
        .unwrap();
    assert_eq!(downloaded.as_deref(), Some(&png[..]));

    // Usuario B guarda su propia cuenta y sincroniza: no ve ni las filas ni el archivo de A.
    let data_b = core_b.load().await.unwrap();
    let bank_b = AccountForm {
        name: "Cuenta exclusiva de B".into(),
        kind: AccountKind::Cash,
        currency: "USD".into(),
        opening_balance: "50".into(),
        ..AccountForm::new(&data_b)
    }
    .validate(user_b.user_id, Lang::Es)
    .unwrap();
    core_b.save(Table::Accounts, &bank_b).await.unwrap();
    assert_eq!(
        core_b.sync(&user_b.client, today()).await.unwrap(),
        SyncState::Synced
    );

    let data_b = core_b.load().await.unwrap();
    assert_eq!(data_b.accounts.len(), 1);
    assert_eq!(data_b.accounts[0].name, "Cuenta exclusiva de B");
    assert!(
        data_b.transactions.is_empty(),
        "B no debe ver movimientos de A"
    );
    assert!(
        data_b.attachments.is_empty(),
        "B no debe ver comprobantes de A"
    );
    let b_reads_a = core_b
        .attachment_bytes(Some(&user_b.client), &attachment)
        .await
        .unwrap();
    assert!(
        b_reads_a.is_none(),
        "B no debe poder descargar el archivo de A en Storage"
    );
    assert!(
        user_b
            .client
            .upload_receipt(&attachment.storage_path, png.to_vec(), "image/png")
            .await
            .is_err(),
        "B no debe poder escribir en la carpeta de A en Storage"
    );

    // Eliminar la cuenta A borra sus archivos, sus filas y su usuario, sin tocar a B.
    core_a1
        .delete_account(&user_a.client)
        .await
        .expect("eliminar cuenta A");
    let wiped_a = core_a1.load().await.unwrap();
    assert!(wiped_a.transactions.is_empty() && wiped_a.attachments.is_empty());
    assert!(
        user_a
            .client
            .sign_in(&user_a.email, &user_a.password)
            .await
            .is_err(),
        "A ya no existe en Auth"
    );

    assert_eq!(
        core_b.sync(&user_b.client, today()).await.unwrap(),
        SyncState::Synced
    );
    let data_b_after = core_b.load().await.unwrap();
    assert_eq!(
        data_b_after.accounts.len(),
        1,
        "borrar a A no toca nada de B"
    );

    // Limpieza final de B.
    core_b
        .delete_account(&user_b.client)
        .await
        .expect("eliminar cuenta B");
    assert!(
        user_b
            .client
            .sign_in(&user_b.email, &user_b.password)
            .await
            .is_err(),
        "B ya no existe en Auth"
    );
}
