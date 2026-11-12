//! Solo en desarrollo: recorre las pantallas y guarda capturas PNG en la
//! carpeta de `CAUDAL_SHOTS`, para revisar la interfaz sin tocar el teclado.

use crate::app::{CaudalApp, Page, Session};
use eframe::egui::{self, ViewportCommand};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
enum Step {
    Size(f32, f32),
    Go(Page),
    NewTransaction,
    NewTrade,
    NewAsset,
    ImportSample,
    EditFirstTransaction,
    Lock(bool),
    CloseDialog,
    Shot(&'static str),
}

pub struct ShotScript {
    dir: PathBuf,
    steps: Vec<Step>,
    index: usize,
    next_at: f64,
    saving: Option<Arc<AtomicBool>>,
}

impl ShotScript {
    pub fn from_env() -> Option<Self> {
        if !cfg!(debug_assertions) {
            return None;
        }
        let dir = PathBuf::from(std::env::var_os("CAUDAL_SHOTS")?);
        let _ = std::fs::create_dir_all(&dir);
        use Step::*;
        let steps = vec![
            Size(1100.0, 720.0),
            Shot("01-inicio"),
            Go(Page::Transactions),
            Shot("02-movimientos"),
            Go(Page::Accounts),
            Shot("03-cuentas"),
            Go(Page::Categories),
            Shot("04-categorias"),
            Go(Page::Reports),
            Shot("12-informes"),
            Go(Page::Plans),
            Shot("13-presupuestos-metas"),
            Go(Page::Investments),
            Shot("14-inversiones"),
            NewTrade,
            Shot("15-nueva-operacion"),
            CloseDialog,
            NewAsset,
            Shot("16-nuevo-activo"),
            CloseDialog,
            Go(Page::Debts),
            Shot("10-deudas"),
            Go(Page::Recurring),
            Shot("11-periodicos"),
            Go(Page::Rules),
            Shot("19-reglas-plantillas"),
            Go(Page::Activity),
            Shot("20-actividad"),
            ImportSample,
            Shot("21-importar-csv"),
            CloseDialog,
            EditFirstTransaction,
            Shot("22-editar-movimiento"),
            CloseDialog,
            Lock(true),
            Shot("23-bloqueo"),
            Lock(false),
            Go(Page::Settings),
            Shot("05-ajustes"),
            NewTransaction,
            Shot("06-nuevo-movimiento"),
            CloseDialog,
            Go(Page::Investments),
            Size(1100.0, 1500.0),
            Shot("18-inversiones-posiciones"),
            Size(390.0, 780.0),
            Go(Page::Home),
            Shot("07-movil-inicio"),
            Go(Page::Transactions),
            Shot("08-movil-movimientos"),
            NewTransaction,
            Shot("09-movil-nuevo"),
            CloseDialog,
            Go(Page::Investments),
            Shot("17-movil-inversiones"),
        ];
        Some(Self {
            dir,
            steps,
            index: 0,
            next_at: 0.0,
            saving: None,
        })
    }

    pub fn advance(&mut self, app: &mut CaudalApp, ctx: &egui::Context) {
        ctx.request_repaint();
        let signed_in = matches!(app.session, Session::SignedIn { .. }) && app.onboarding.is_none();
        let signed_out = matches!(app.session, Session::SignedOut);
        if !signed_in && !signed_out {
            return;
        }
        if let Some(flag) = &self.saving {
            if !flag.load(Ordering::SeqCst) {
                return;
            }
            self.saving = None;
        }
        // Tiempo para que la interfaz y sus animaciones se asienten tras cada paso.
        let now = ctx.input(|input| input.time);
        if now < self.next_at {
            return;
        }
        self.next_at = now + 0.6;
        let Some(step) = self.steps.get(self.index).copied() else {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        };
        self.index += 1;
        if signed_out && !matches!(step, Step::Shot(_) | Step::Size(..)) {
            return;
        }
        match step {
            Step::Size(width, height) => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(width, height)));
            }
            Step::Go(page) => app.go(page),
            Step::NewTransaction => app.open_new_transaction(),
            Step::NewTrade => {
                let asset = app.data.assets.first().map(|asset| asset.meta.id);
                let mut form =
                    caudal_app_state::investments::TradeForm::new(asset, crate::platform::today());
                form.quantity = "2".into();
                form.price = "224".into();
                app.dialog = Some(crate::ui::dialogs::Dialog::Trade {
                    form: Box::new(form),
                    error: None,
                });
            }
            Step::ImportSample => {
                let sample = "Fecha;Descripción;Cargos;Abonos\n\
                    10/11/2026;UBER *TRIP;5.990;\n\
                    10/11/2026;Supermercado Lider;23.450;\n\
                    11/11/2026;Remuneración;;750.000\n\
                    sin fecha;fila rota;1;\n";
                if let Ok(table) = caudal_app_state::csv_io::CsvTable::parse(sample.as_bytes()) {
                    app.dialog = Some(crate::ui::dialogs::Dialog::Import(Box::new(
                        crate::ui::data::ImportState::new(&app.data, table, app.lang),
                    )));
                }
            }
            Step::EditFirstTransaction => {
                if let Some(tx) = app
                    .data
                    .transactions
                    .iter()
                    .find(|tx| !tx.meta.is_deleted())
                {
                    app.dialog = Some(crate::ui::dialogs::Dialog::Transaction {
                        form: Box::new(caudal_app_state::forms::TransactionForm::edit(
                            tx, app.lang,
                        )),
                        error: None,
                    });
                }
            }
            Step::Lock(on) => {
                app.locked = on;
                if on {
                    app.prefs.pin_hash = Some("x".into());
                    app.prefs.pin_salt = Some("x".into());
                } else {
                    app.prefs.pin_hash = None;
                    app.prefs.pin_salt = None;
                }
            }
            Step::NewAsset => {
                app.dialog = Some(crate::ui::dialogs::Dialog::Asset {
                    form: Box::new(caudal_app_state::investments::AssetForm::new(
                        &app.data,
                        crate::platform::today(),
                    )),
                    query: "eth".into(),
                    error: None,
                });
            }
            Step::CloseDialog => app.dialog = None,
            Step::Shot(name) => {
                let name = if signed_out {
                    format!("00-acceso-{name}")
                } else {
                    name.to_owned()
                };
                let path = self.dir.join(format!("{name}.png"));
                let done = Arc::new(AtomicBool::new(false));
                self.saving = Some(Arc::clone(&done));
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    save_png(&path, &image);
                    done.store(true, Ordering::SeqCst);
                    repaint.request_repaint();
                });
            }
        }
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) {
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(image.as_raw());
    }
}
