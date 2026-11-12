//! Los precios llegan de Supabase, se guardan en el dispositivo y se piden
//! solo desde el último dato conocido.

use caudal_app_state::auth::{Session, SessionUser, SupabaseClient, SupabaseConfig};
use caudal_app_state::investments::{AssetForm, portfolio_view};
use caudal_app_state::{AppCore, Lang, Table};
use caudal_sync::memory::MemoryStore;
use chrono::NaiveDate;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: Uuid = Uuid::from_u128(7);

fn day(text: &str) -> NaiveDate {
    text.parse().unwrap()
}

#[tokio::test]
async fn baja_guarda_y_luego_pide_solo_lo_nuevo() {
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
    let core = AppCore::open(MemoryStore::default(), USER, || 1_000, || 1)
        .await
        .unwrap();
    let mut profile = caudal_app_state::forms::ProfileForm::new(Lang::Es);
    profile.base_currency = "CLP".into();
    core.save(Table::Profiles, &profile.validate(USER).unwrap())
        .await
        .unwrap();
    let data = core.load().await.unwrap();
    let mut form = AssetForm::new(&data, day("2026-11-10"));
    form.fill_from(&caudal_app_state::catalog::search("btc")[0]);
    form.currency = "CLP".into();
    let asset = form.validate(&data, Lang::Es).unwrap();
    core.save(Table::Assets, &asset).await.unwrap();
    let buy = caudal_app_state::investments::TradeForm {
        quantity: "0,5".into(),
        price: "60000000".into(),
        ..caudal_app_state::investments::TradeForm::new(Some(asset.meta.id), day("2026-01-10"))
    }
    .validate(&core.load().await.unwrap(), Lang::Es)
    .unwrap();
    core.save_trade(&buy).await.unwrap();

    // Primera vez: 400 días de historial.
    Mock::given(method("GET"))
        .and(path("/rest/v1/asset_prices"))
        .and(query_param("price_on", "gte.2025-10-06"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"provider":"coingecko","symbol":"bitcoin","price_on":"2026-11-09","price":72000,"currency":"USD"}]"#,
            "application/json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/fx_rates"))
        .and(query_param("rate_on", "gte.2025-10-06"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"base":"USD","quote":"CLP","rate_on":"2026-11-09","rate":"960"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let data = core.load().await.unwrap();
    assert!(
        core.refresh_market(&client, &data, day("2026-11-10"))
            .await
            .unwrap()
    );

    let data = core.load().await.unwrap();
    assert_eq!(data.prices.len(), 1);
    let view = portfolio_view(&data, day("2026-11-10"), Lang::Es);
    // 0,5 × 72.000 USD × 960.
    assert_eq!(view.total, "34.560.000 CLP");
    assert!(view.positions[0].price.contains("mercado"));

    // Segunda vez: desde el último dato menos 3 días; un cierre corregido reemplaza al anterior.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/asset_prices"))
        .and(query_param("price_on", "gte.2026-11-06"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"provider":"coingecko","symbol":"bitcoin","price_on":"2026-11-09","price":71000,"currency":"USD"},
                {"provider":"coingecko","symbol":"bitcoin","price_on":"2026-11-10","price":73000,"currency":"USD"}]"#,
            "application/json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/v1/fx_rates"))
        .and(query_param("rate_on", "gte.2026-11-06"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        core.refresh_market(&client, &data, day("2026-11-10"))
            .await
            .unwrap()
    );
    let data = core.load().await.unwrap();
    assert_eq!(data.prices.len(), 2);
    assert_eq!(data.prices[0].price.to_string(), "71000");

    // Sin red: se conserva lo guardado.
    drop(server);
    assert!(
        core.refresh_market(&client, &data, day("2026-11-11"))
            .await
            .is_err()
    );
    assert_eq!(core.load().await.unwrap().prices.len(), 2);
}
