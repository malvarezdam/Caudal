//! Cliente contra un servidor simulado en localhost.

use caudal_supabase::{
    Session, SessionUser, SignUpOutcome, SupabaseClient, SupabaseConfig, SupabaseError,
};
use caudal_sync::{Remote, Table};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, header, headers, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sb_publishable_prueba";
const USER: Uuid = Uuid::from_u128(5);

fn client(server: &MockServer) -> SupabaseClient {
    SupabaseClient::new(SupabaseConfig {
        url: server.uri(),
        publishable_key: KEY.into(),
    })
}

fn token_body(access: &str) -> Value {
    json!({
        "access_token": access,
        "token_type": "bearer",
        "expires_in": 3600,
        "expires_at": 4_000_000_000i64,
        "refresh_token": format!("refresh-{access}"),
        "user": {"id": USER.to_string(), "email": "a@example.invalid", "aud": "authenticated"}
    })
}

fn signed_in(client: &SupabaseClient, expires_at: i64) {
    client.set_session(Some(Session {
        access_token: "viejo".into(),
        refresh_token: "refresh-viejo".into(),
        expires_at,
        user: SessionUser {
            id: USER,
            email: None,
        },
    }));
}

#[tokio::test]
async fn inicia_sesion_y_guarda_la_sesion() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "password"))
        .and(header("apikey", KEY))
        .and(body_partial_json(json!({"email": "a@example.invalid"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("nuevo")))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    let session = client
        .sign_in("a@example.invalid", "secreta")
        .await
        .unwrap();
    assert_eq!(session.user.id, USER);
    assert_eq!(client.session(), Some(session));
}

#[tokio::test]
async fn muestra_el_mensaje_del_servidor() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 400, "error_code": "invalid_credentials", "msg": "Invalid login credentials"
        })))
        .mount(&server)
        .await;
    let error = client(&server)
        .sign_in("a@example.invalid", "mala")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Invalid login credentials");
    assert!(
        matches!(error, SupabaseError::Api { status: 400, code: Some(ref code), .. } if code == "invalid_credentials")
    );
}

#[tokio::test]
async fn registro_con_confirmacion_por_correo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/signup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": USER.to_string(), "email": "a@example.invalid", "confirmation_sent_at": "2026-11-09T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let client = client(&server);
    let outcome = client
        .sign_up("a@example.invalid", "secreta")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        SignUpOutcome::ConfirmationRequired {
            email: "a@example.invalid".into()
        }
    );
    assert!(client.session().is_none());
}

#[tokio::test]
async fn recupera_la_contrasena_con_codigo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/verify"))
        .and(body_partial_json(
            json!({"type": "recovery", "token": "123456"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("recuperado")))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/auth/v1/user"))
        .and(header("authorization", "Bearer recuperado"))
        .and(body_partial_json(json!({"password": "nueva"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": USER.to_string()})))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    client
        .verify_recovery_code("a@example.invalid", " 123456 ")
        .await
        .unwrap();
    client.update_password("nueva").await.unwrap();
}

#[tokio::test]
async fn sube_filas_sin_cursor_y_renueva_el_token_caducado() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .and(query_param("grant_type", "refresh_token"))
        .and(body_partial_json(json!({"refresh_token": "refresh-viejo"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("renovado")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/rest/v1/tags"))
        .and(query_param("on_conflict", "id"))
        .and(query_param("columns", "id,name,updated_at,user_id"))
        .and(header("authorization", "Bearer renovado"))
        .and(headers(
            "prefer",
            vec![
                "resolution=merge-duplicates",
                "missing=default",
                "return=minimal",
            ],
        ))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server);
    signed_in(&client, 0);
    let row = json!({"id": Uuid::nil().to_string(), "user_id": USER.to_string(), "updated_at": "1", "name": "ñandú"});
    client
        .push(Table::Tags, vec![row.as_object().unwrap().clone()])
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(body[0]["name"], "ñandú");
    assert!(body[0].get("server_seq").is_none());
}

#[tokio::test]
async fn baja_importes_exactos_sin_pasar_por_f64() {
    let server = MockServer::start().await;
    let raw = format!(
        r#"[{{"id":"{}","user_id":"{}","updated_at":"2","deleted_at":null,"server_seq":7,
             "interval_count":3,"amount":1234567890123456.123456789012,"tag_ids":[]}}]"#,
        Uuid::nil(),
        USER
    );
    Mock::given(method("GET"))
        .and(path("/rest/v1/transactions"))
        .and(query_param("server_seq", "gt.5"))
        .and(query_param("order", "server_seq.asc"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(raw, "application/json"))
        .mount(&server)
        .await;
    let client = client(&server);
    signed_in(&client, 4_000_000_000);
    let records = client.pull(Table::Transactions, 5, 100).await.unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.server_seq(), Some(7));
    assert_eq!(record.row["amount"], "1234567890123456.123456789012");
    assert_eq!(record.row["interval_count"], 3);
}

#[tokio::test]
async fn sesion_rechazada_obliga_a_entrar_de_nuevo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/v1/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "code": 400, "error_code": "refresh_token_not_found", "msg": "Invalid Refresh Token"
        })))
        .mount(&server)
        .await;
    let client = client(&server);
    signed_in(&client, 0);
    let error = client.pull(Table::Tags, 0, 10).await.unwrap_err();
    assert!(error.requires_sign_in());
    assert!(client.session().is_none());
}

#[tokio::test]
async fn sin_sesion_no_hay_peticiones() {
    let server = MockServer::start().await;
    let error = client(&server).pull(Table::Tags, 0, 10).await.unwrap_err();
    assert!(matches!(error, SupabaseError::NotSignedIn));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn baja_precios_y_tipos_de_cambio_sin_perder_decimales() {
    let server = MockServer::start().await;
    // El número viaja como número JSON; con arbitrary_precision no pasa por f64.
    let body = r#"[
        {"provider": "eodhd", "symbol": "AAPL.US", "price_on": "2026-11-10", "price": 347.920000000001, "currency": "USD"},
        {"provider": "coingecko", "symbol": "bitcoin", "price_on": "2026-11-10", "price": 72926, "currency": "USD"},
        {"provider": "eodhd", "symbol": "RARA", "price_on": "2026-11-10", "price": 1, "currency": "no-es-moneda"}
    ]"#;
    Mock::given(method("GET"))
        .and(path("/rest/v1/asset_prices"))
        .and(query_param("price_on", "gte.2026-11-01"))
        .and(header("authorization", "Bearer viejo"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/fx_rates"))
        .and(query_param("base", "eq.USD"))
        .and(query_param("quote", "in.(\"CLP\")"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"base": "USD", "quote": "CLP", "rate_on": "2026-11-09", "rate": 976.73061504}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client(&server);
    signed_in(&client, 4_000_000_000);
    let since = "2026-11-01".parse().unwrap();
    let symbols = ["aapl.us".to_owned(), "bitcoin".to_owned()].into();
    let prices = client.fetch_prices(&symbols, since).await.unwrap();
    assert_eq!(prices.len(), 2, "la fila ilegible se omite");
    assert_eq!(prices[0].price.to_string(), "347.920000000001");
    let requested = &server.received_requests().await.unwrap()[0];
    let query = requested
        .url
        .query()
        .unwrap_or_default()
        .replace("%22", "\"")
        .replace("%2C", ",");
    assert!(
        query.contains("AAPL.US") && query.contains("aapl.us"),
        "{query}"
    );

    let rates = client
        .fetch_rates(&["CLP".to_owned()].into(), since)
        .await
        .unwrap();
    assert_eq!(rates[0].rate.to_string(), "976.73061504");
}

#[tokio::test]
async fn sube_y_baja_comprobantes_en_la_carpeta_del_usuario() {
    let server = MockServer::start().await;
    let path = format!("{USER}/archivo");
    Mock::given(method("POST"))
        .and(wiremock::matchers::path(format!(
            "/storage/v1/object/receipts/{path}"
        )))
        .and(header("content-type", "image/jpeg"))
        .and(header("x-upsert", "true"))
        .and(header("authorization", "Bearer viejo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Key": "receipts/x"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path(format!(
            "/storage/v1/object/receipts/{path}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3]))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path(format!(
            "/storage/v1/object/receipts/{USER}/falta"
        )))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"error": "not_found", "message": "Object not found"})),
        )
        .mount(&server)
        .await;
    let client = client(&server);
    signed_in(&client, 4_000_000_000);
    client
        .upload_receipt(&path, vec![9, 9], "image/jpeg")
        .await
        .unwrap();
    assert_eq!(client.download_receipt(&path).await.unwrap(), vec![1, 2, 3]);
    let error = client
        .download_receipt(&format!("{USER}/falta"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not found"), "{error}");
}
