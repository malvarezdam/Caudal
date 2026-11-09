//! UI Leptos. Por ahora contiene la prueba WASM de la Fase 0.

#[cfg(target_arch = "wasm32")]
fn main() {
    use leptos::prelude::*;

    console_error_panic_hook::set_once();

    leptos::mount::mount_to_body(|| {
        let (idb, set_idb) = signal("pendiente".to_string());
        let (http, set_http) = signal("pendiente".to_string());

        wasm_bindgen_futures::spawn_local(async move {
            let result = match caudal_store_idb::roundtrip("Economía ñandú").await {
                Ok(value) => format!("OK: {value:?}"),
                Err(error) => format!("ERROR: {error}"),
            };
            set_idb.set(result);

            let origin = window().location().origin().unwrap_or_default();
            let result = match caudal_supabase::fetch_text(&format!("{origin}/index.html")).await {
                Ok(body) => format!("OK: {} bytes", body.len()),
                Err(error) => format!("ERROR: {error}"),
            };
            set_http.set(result);
        });

        view! {
            <main>
                <h1>"Prueba WASM"</h1>
                <p id="idb">"IndexedDB: " {idb}</p>
                <p id="http">"reqwest: " {http}</p>
            </main>
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
