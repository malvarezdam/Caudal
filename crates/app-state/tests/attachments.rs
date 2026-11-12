//! Un comprobante agregado sin red espera en el dispositivo y se sube después.

use caudal_app_state::AppCore;
use caudal_app_state::attachments::new_attachment;
use caudal_app_state::auth::{Session, SessionUser, SupabaseClient, SupabaseConfig};
use caudal_sync::memory::MemoryStore;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: Uuid = Uuid::from_u128(11);

#[tokio::test]
async fn espera_sin_red_y_se_sube_despues() {
    let core = AppCore::open(MemoryStore::default(), USER, || 1_000, || 1)
        .await
        .unwrap();
    let data = core.load().await.unwrap();
    let png = [0x89u8, 0x50, 0x4E, 0x47, 1, 2, 3];
    let attachment = new_attachment(&data, Uuid::from_u128(5), "boleta.png", &png).unwrap();
    core.add_attachment(&attachment, &png).await.unwrap();
    assert_eq!(core.pending_attachments().await.unwrap().len(), 1);

    // Sin red: se puede ver igual.
    let local = core.attachment_bytes(None, &attachment).await.unwrap();
    assert_eq!(local.as_deref(), Some(&png[..]));

    let server = MockServer::start().await;
    let client = SupabaseClient::new(SupabaseConfig {
        url: server.uri(),
        publishable_key: "sb_publishable_prueba".into(),
    });
    client.set_session(Some(Session {
        access_token: "t".into(),
        refresh_token: "r".into(),
        expires_at: 4_000_000_000,
        user: SessionUser {
            id: USER,
            email: None,
        },
    }));
    let object = format!("/storage/v1/object/receipts/{}", attachment.storage_path);

    // El servidor falla: el archivo sigue pendiente.
    Mock::given(method("POST"))
        .and(path(object.clone()))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    assert!(core.upload_pending_attachments(&client).await.is_err());
    assert_eq!(core.pending_attachments().await.unwrap().len(), 1);

    Mock::given(method("POST"))
        .and(path(object.clone()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(core.upload_pending_attachments(&client).await.unwrap(), 1);
    assert!(core.pending_attachments().await.unwrap().is_empty());
    let sent = server.received_requests().await.unwrap();
    assert_eq!(sent.last().unwrap().body, png.to_vec());

    // Ya subido: se baja desde Storage.
    Mock::given(method("GET"))
        .and(path(object))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.to_vec()))
        .mount(&server)
        .await;
    let remote = core
        .attachment_bytes(Some(&client), &attachment)
        .await
        .unwrap();
    assert_eq!(remote.as_deref(), Some(&png[..]));
}

#[tokio::test]
async fn eliminar_cuenta_limpia_storage_remoto_y_almacen_local() {
    let core = AppCore::open(MemoryStore::default(), USER, || 1_000, || 1)
        .await
        .unwrap();
    let data = core.load().await.unwrap();
    let png = [0x89u8, 0x50, 0x4E, 0x47, 1, 2, 3];
    let attachment = new_attachment(&data, Uuid::from_u128(5), "boleta.png", &png).unwrap();
    core.add_attachment(&attachment, &png).await.unwrap();

    let server = MockServer::start().await;
    let client = SupabaseClient::new(SupabaseConfig {
        url: server.uri(),
        publishable_key: "sb_publishable_prueba".into(),
    });
    client.set_session(Some(Session {
        access_token: "t".into(),
        refresh_token: "r".into(),
        expires_at: 4_000_000_000,
        user: SessionUser {
            id: USER,
            email: None,
        },
    }));

    Mock::given(method("POST"))
        .and(path("/storage/v1/object/list/receipts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let object = format!("/storage/v1/object/receipts/{}", attachment.storage_path);
    Mock::given(method("DELETE"))
        .and(path(object))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/rest/v1/rpc/delete_account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(null)))
        .expect(1)
        .mount(&server)
        .await;

    core.delete_account(&client).await.unwrap();

    assert_eq!(client.session(), None);

    let loaded = core.load().await.unwrap();
    assert!(loaded.attachments.is_empty());
    assert!(core.pending_attachments().await.unwrap().is_empty());
}
