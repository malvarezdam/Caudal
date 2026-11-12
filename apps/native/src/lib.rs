//! Caudal para Windows y Android, con egui.

mod android;
mod app;
mod cache;
mod files;
mod platform;
mod shots;
mod ui;
mod worker;

pub use platform::Platform;

pub fn run(options: eframe::NativeOptions, platform: Platform) -> eframe::Result {
    eframe::run_native(
        "Caudal",
        options,
        Box::new(move |cc| Ok(Box::new(app::CaudalApp::new(cc, platform)))),
    )
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let _ = android::APP.set(app.clone());
    let platform = Platform::android(&app);
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(error) = run(options, platform) {
        eprintln!("error al iniciar la app: {error}");
    }
}
