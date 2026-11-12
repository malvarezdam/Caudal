#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Caudal")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([360.0, 520.0]),
        ..Default::default()
    };
    caudal_native::run(options, caudal_native::Platform::desktop())
}

#[cfg(target_os = "android")]
fn main() {}
