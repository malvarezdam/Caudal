//! Caudal para la web, con Leptos.

#[cfg(target_arch = "wasm32")]
mod files;
#[cfg(target_arch = "wasm32")]
mod state;
#[cfg(target_arch = "wasm32")]
mod ui;
#[cfg(target_arch = "wasm32")]
mod ui_data;
#[cfg(target_arch = "wasm32")]
mod ui_debts;
#[cfg(target_arch = "wasm32")]
mod ui_invest;
#[cfg(target_arch = "wasm32")]
mod ui_reports;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(ui::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
