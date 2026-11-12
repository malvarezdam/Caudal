//! Archivos en el navegador: descargar lo que genera la app y leer lo que
//! elige el usuario. Nada sale del dispositivo salvo lo que se sincroniza.

use leptos::prelude::document;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

fn blob(mime: &str, bytes: &[u8]) -> Option<web_sys::Blob> {
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&array);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).ok()
}

/// Dirección temporal para mostrar un comprobante en otra pestaña.
pub fn object_url(mime: &str, bytes: &[u8]) -> Option<String> {
    web_sys::Url::create_object_url_with_blob(&blob(mime, bytes)?).ok()
}

/// Descarga `bytes` con el nombre dado.
pub fn download(name: &str, mime: &str, bytes: &[u8]) {
    let Some(url) = object_url(mime, bytes) else {
        return;
    };
    if let Ok(anchor) = document().create_element("a")
        && let Ok(anchor) = anchor.dyn_into::<web_sys::HtmlAnchorElement>()
    {
        anchor.set_href(&url);
        anchor.set_download(name);
        anchor.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// Abre el selector de archivos; `done` recibe nombre y contenido.
pub fn pick(accept: &str, done: impl FnOnce(String, Vec<u8>) + 'static) {
    let Ok(input) = document().create_element("input") else {
        return;
    };
    let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() else {
        return;
    };
    input.set_type("file");
    input.set_accept(accept);
    let reader = input.clone();
    let mut done = Some(done);
    let on_change = Closure::<dyn FnMut()>::new(move || {
        let Some(file) = reader.files().and_then(|files| files.get(0)) else {
            return;
        };
        let Some(done) = done.take() else { return };
        let name = file.name();
        leptos::task::spawn_local(async move {
            if let Ok(buffer) = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                done(name, bytes);
            }
        });
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    // El cierre vive mientras exista el elemento.
    on_change.forget();
    input.click();
}
