//! Estado de la interfaz y distribución de la pantalla.

use crate::platform::{Platform, Prefs, today};
use crate::ui::{auth::AuthUi, dialogs::Dialog, pages, widgets};
use crate::worker::{self, Command, Event, WorkerHandle};
use caudal_app_state::forms::{ProfileForm, TransactionForm};
use caudal_app_state::views::TransactionFilter;
use caudal_app_state::{Data, Lang, SyncState, texts};
use eframe::egui::{self, Align, Layout, RichText, ThemePreference, Ui};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Transactions,
    Accounts,
    Investments,
    Reports,
    Plans,
    Debts,
    Recurring,
    Categories,
    Rules,
    Activity,
    Trash,
    Settings,
}

impl Page {
    pub const ALL: [Page; 13] = [
        Page::Home,
        Page::Transactions,
        Page::Accounts,
        Page::Investments,
        Page::Reports,
        Page::Plans,
        Page::Debts,
        Page::Recurring,
        Page::Categories,
        Page::Rules,
        Page::Activity,
        Page::Trash,
        Page::Settings,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let texts = texts(lang);
        match self {
            Page::Home => texts.home,
            Page::Transactions => texts.transactions,
            Page::Accounts => texts.accounts,
            Page::Investments => texts.investments,
            Page::Reports => texts.reports,
            Page::Plans => texts.plans,
            Page::Debts => texts.debts,
            Page::Recurring => texts.recurring,
            Page::Categories => texts.categories,
            Page::Rules => texts.rules_title,
            Page::Activity => texts.activity,
            Page::Trash => texts.trash,
            Page::Settings => texts.settings,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("light") => ThemeChoice::Light,
            Some("dark") => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    fn preference(self) -> ThemePreference {
        match self {
            ThemeChoice::System => ThemePreference::System,
            ThemeChoice::Light => ThemePreference::Light,
            ThemeChoice::Dark => ThemePreference::Dark,
        }
    }
}

pub enum Session {
    Loading,
    SignedOut,
    SignedIn { email: Option<String>, demo: bool },
}

pub struct Onboarding {
    pub form: ProfileForm,
    pub suggested: bool,
    pub error: Option<String>,
}

pub struct CaudalApp {
    pub worker: WorkerHandle,
    pub platform: Platform,
    pub prefs: Prefs,
    pub lang: Lang,
    pub theme: ThemeChoice,
    pub session: Session,
    pub auth: AuthUi,
    pub onboarding: Option<Onboarding>,
    pub data: Arc<Data>,
    pub sync: SyncState,
    pub page: Page,
    pub filter: TransactionFilter,
    pub limit: usize,
    pub show_archived: bool,
    pub dialog: Option<Dialog>,
    pub toast: Option<(String, f64)>,
    pub error: Option<String>,
    pub busy: bool,
    pub first_sync_done: bool,
    pub cache: crate::cache::ViewCache,
    pub report_period: caudal_app_state::analysis::Period,
    pub report_table: bool,
    pub shots: Option<crate::shots::ShotScript>,
    /// Con PIN definido, la app abre bloqueada.
    pub locked: bool,
    pub pin_entry: String,
    pub pin_error: bool,
    pub pending_attachments: std::collections::BTreeSet<uuid::Uuid>,
    #[cfg(target_os = "android")]
    pub ime: crate::android::Bridge,
}

pub const PAGE_SIZE: usize = 50;

impl CaudalApp {
    pub fn new(cc: &eframe::CreationContext<'_>, platform: Platform) -> Self {
        crate::ui::theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::files::init(&cc.egui_ctx);
        let prefs = Prefs::load(&platform);
        caudal_app_state::privacy::set_private(prefs.private);
        let locked = prefs.pin_hash.is_some();
        let lang = prefs.lang.as_deref().map_or_else(system_lang, Lang::parse);
        let theme = ThemeChoice::parse(prefs.theme.as_deref());
        cc.egui_ctx.set_theme(theme.preference());
        // Solo en desarrollo: variable CAUDAL_DEMO o archivo "demo" en la carpeta de datos (Android).
        let demo = cfg!(debug_assertions)
            && (std::env::var_os("CAUDAL_DEMO").is_some()
                || platform.data_dir.join("demo").exists());
        let worker = worker::spawn(platform.clone(), cc.egui_ctx.clone(), lang, demo);
        Self {
            worker,
            platform,
            prefs,
            lang,
            theme,
            session: Session::Loading,
            auth: AuthUi::default(),
            onboarding: None,
            data: Arc::new(Data::default()),
            sync: SyncState::Synced,
            page: Page::Home,
            filter: TransactionFilter::default(),
            limit: PAGE_SIZE,
            show_archived: false,
            dialog: None,
            toast: None,
            error: None,
            busy: false,
            first_sync_done: false,
            cache: Default::default(),
            report_period: caudal_app_state::analysis::period_containing(today(), false),
            report_table: false,
            shots: crate::shots::ShotScript::from_env(),
            locked,
            pin_entry: String::new(),
            pin_error: false,
            pending_attachments: Default::default(),
            #[cfg(target_os = "android")]
            ime: Default::default(),
        }
    }

    /// Clave del caché: cambia con los datos, el idioma o el día.
    pub fn key(&self) -> crate::cache::Key {
        crate::cache::Key {
            data: Arc::as_ptr(&self.data) as usize,
            lang: self.lang,
            today: today(),
            private: self.prefs.private,
        }
    }

    pub fn send(&self, command: Command) {
        self.worker.send(command);
    }

    pub fn set_private(&mut self, private: bool) {
        self.prefs.private = private;
        self.prefs.save(&self.platform);
        caudal_app_state::privacy::set_private(private);
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.prefs.lang = Some(lang.code().to_owned());
        self.prefs.save(&self.platform);
        self.send(Command::SetLang(lang));
    }

    pub fn set_theme(&mut self, ctx: &egui::Context, theme: ThemeChoice) {
        self.theme = theme;
        ctx.set_theme(theme.preference());
        self.prefs.theme = Some(theme.code().to_owned());
        self.prefs.save(&self.platform);
    }

    pub fn toast(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        let until = ctx.input(|input| input.time) + 2.5;
        self.toast = Some((message.into(), until));
    }

    pub fn open_new_transaction(&mut self) {
        self.dialog = Some(Dialog::Transaction {
            form: Box::new(TransactionForm::new(&self.data, today())),
            error: None,
        });
    }

    /// Archivos que eligió o guardó el usuario.
    fn handle_files(&mut self, ctx: &egui::Context) {
        use crate::files::{FileEvent, Purpose};
        let t = texts(self.lang);
        for event in crate::files::poll() {
            match event {
                FileEvent::Saved(true) => self.toast(ctx, t.file_saved),
                FileEvent::Saved(false) => self.error = Some(t.csv_unreadable.to_owned()),
                FileEvent::Opened {
                    purpose: Purpose::ImportCsv,
                    bytes,
                    ..
                } => match caudal_app_state::csv_io::CsvTable::parse(&bytes) {
                    Ok(table) => {
                        self.dialog = Some(crate::ui::dialogs::Dialog::Import(Box::new(
                            crate::ui::data::ImportState::new(&self.data, table, self.lang),
                        )));
                    }
                    Err(_) => self.error = Some(t.csv_unreadable.to_owned()),
                },
                FileEvent::Opened {
                    purpose: Purpose::RestoreBackup,
                    bytes,
                    ..
                } => match String::from_utf8(bytes) {
                    Ok(text) => self.send(Command::RestoreBackup(text)),
                    Err(_) => self.error = Some(t.backup_invalid.to_owned()),
                },
                FileEvent::Opened {
                    purpose: Purpose::Attach(transaction_id),
                    name,
                    bytes,
                } => match caudal_app_state::attachments::new_attachment(
                    &self.data,
                    transaction_id,
                    &name,
                    &bytes,
                ) {
                    Ok(attachment) => self.send(Command::AddAttachment {
                        attachment: Box::new(attachment),
                        bytes,
                    }),
                    Err(problem) => self.error = Some(problem.message(self.lang).to_owned()),
                },
            }
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        self.handle_files(ctx);
        for event in self.worker.poll() {
            match event {
                Event::SignedIn { email, demo } => {
                    self.session = Session::SignedIn { email, demo };
                    self.auth = AuthUi::default();
                    self.error = None;
                }
                Event::SignedOut => {
                    self.session = Session::SignedOut;
                    self.first_sync_done = false;
                    self.data = Arc::new(Data::default());
                    self.onboarding = None;
                    self.dialog = None;
                    self.page = Page::Home;
                    self.sync = SyncState::Synced;
                }
                Event::RecoveryVerified => self.auth.recovery_verified(),
                Event::Data(data) => {
                    if let Some(profile) = &data.profile {
                        self.onboarding = None;
                        if let Some(locale) = &profile.locale {
                            let lang = Lang::parse(locale);
                            if lang != self.lang {
                                self.set_lang(lang);
                            }
                        }
                    } else if self.onboarding.is_none() {
                        self.onboarding = Some(Onboarding {
                            form: ProfileForm::new(self.lang),
                            suggested: true,
                            error: None,
                        });
                    }
                    self.data = data;
                }
                Event::Sync(state) => {
                    if state != SyncState::Syncing {
                        self.first_sync_done = true;
                    }
                    self.sync = state;
                }
                Event::Busy(busy) => self.busy = busy,
                Event::Info(message) => {
                    self.auth.info = Some(message);
                    self.auth.error = None;
                }
                Event::Error(message) => {
                    if matches!(self.session, Session::SignedIn { .. }) {
                        self.error = Some(message);
                    } else {
                        self.auth.error = Some(message);
                        self.auth.info = None;
                    }
                }
                Event::Saved => self.toast(ctx, texts(self.lang).saved),
                Event::FileReady { name, mime, bytes } => crate::files::save(&name, mime, &bytes),
                Event::Attachment { id, bytes } => {
                    if let Some(crate::ui::dialogs::Dialog::Attachment {
                        attachment,
                        bytes: slot,
                        loaded,
                    }) = self.dialog.as_mut()
                        && attachment.meta.id == id
                    {
                        *slot = bytes;
                        *loaded = true;
                    }
                }
                Event::PendingAttachments(pending) => self.pending_attachments = pending,
            }
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, wide: bool) {
        let texts = texts(self.lang);
        ui.horizontal(|ui| {
            if !wide {
                ui.label(
                    RichText::new(texts.app_name)
                        .strong()
                        .size(18.0)
                        .color(widgets::ACCENT),
                );
            } else {
                ui.label(
                    RichText::new(self.page.label(self.lang))
                        .strong()
                        .size(18.0),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let syncing = self.sync == SyncState::Syncing;
                if !matches!(self.sync, SyncState::LocalOnly)
                    && ui
                        .add_enabled(
                            !syncing,
                            egui::Button::new(RichText::new(texts.sync_now).small()),
                        )
                        .on_hover_text(texts.sync_now)
                        .clicked()
                {
                    self.send(Command::SyncNow);
                }
                if syncing {
                    ui.spinner();
                }
                let label = self.sync.label(self.lang);
                let color = if self.sync.is_problem() {
                    widgets::warning_color(ui)
                } else {
                    ui.visuals().weak_text_color()
                };
                ui.add(egui::Label::new(RichText::new(label).small().color(color)).truncate());
            });
        });
        if matches!(self.sync, SyncState::SignInRequired { .. }) {
            ui.horizontal(|ui| {
                if ui.button(texts.sign_in).clicked() {
                    self.send(Command::SignOut);
                }
            });
        }
        if let Some(error) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(error).color(widgets::tone_color(
                    ui,
                    caudal_app_state::views::Tone::Negative,
                )));
                if ui.small_button("x").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn side_nav(&mut self, ui: &mut Ui) {
        let texts = texts(self.lang);
        ui.add_space(8.0);
        ui.label(
            RichText::new(texts.app_name)
                .strong()
                .size(22.0)
                .color(widgets::ACCENT),
        );
        ui.add_space(12.0);
        if ui
            .add_sized(
                [ui.available_width(), 34.0],
                egui::Button::new(RichText::new(format!("+  {}", texts.new_transaction)).strong()),
            )
            .clicked()
        {
            self.open_new_transaction();
        }
        ui.add_space(10.0);
        for page in Page::ALL {
            let selected = self.page == page;
            let mut label = page.label(self.lang).to_owned();
            if page == Page::Trash && !self.data.trash.is_empty() {
                label = format!("{label} ({})", self.data.trash.len());
            }
            if ui
                .add_sized(
                    [ui.available_width(), 30.0],
                    egui::Button::selectable(selected, label),
                )
                .clicked()
            {
                self.go(page);
            }
        }
    }

    fn bottom_nav(&mut self, ui: &mut Ui) {
        let texts = texts(self.lang);
        ui.spacing_mut().button_padding = egui::vec2(2.0, 6.0);
        let pages = [
            Page::Home,
            Page::Transactions,
            Page::Accounts,
            Page::Settings,
        ];
        ui.columns(5, |columns| {
            for (index, column) in columns.iter_mut().enumerate() {
                column.vertical_centered_justified(|ui| {
                    if index == 2 {
                        let button = egui::Button::new(
                            RichText::new("+")
                                .size(22.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(widgets::ACCENT)
                        .min_size(egui::vec2(48.0, 40.0));
                        if ui
                            .add(button)
                            .on_hover_text(texts.new_transaction)
                            .clicked()
                        {
                            self.open_new_transaction();
                        }
                        return;
                    }
                    let page = pages[if index < 2 { index } else { index - 1 }];
                    let selected = self.page == page
                        || (page == Page::Settings
                            && matches!(
                                self.page,
                                Page::Categories
                                    | Page::Trash
                                    | Page::Debts
                                    | Page::Recurring
                                    | Page::Reports
                                    | Page::Plans
                                    | Page::Investments
                                    | Page::Rules
                                    | Page::Activity
                            ));
                    let label = RichText::new(page.label(self.lang)).size(10.5);
                    if ui
                        .add(
                            egui::Button::selectable(selected, label)
                                .min_size(egui::vec2(0.0, 40.0)),
                        )
                        .clicked()
                    {
                        self.go(page);
                    }
                });
            }
        });
    }

    pub fn go(&mut self, page: Page) {
        self.page = page;
        self.limit = PAGE_SIZE;
    }

    fn main_ui(&mut self, ui: &mut Ui) {
        let wide = ui.available_width() >= 720.0;
        if wide {
            egui::Panel::left("nav")
                .exact_size(210.0)
                .resizable(false)
                .show(ui, |ui| self.side_nav(ui));
        } else {
            egui::Panel::bottom("bottom-nav")
                .resizable(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    self.bottom_nav(ui);
                    ui.add_space(4.0);
                });
        }
        egui::Panel::top("status").resizable(false).show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui, wide);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(900.0));
                    match self.page {
                        Page::Home => pages::home(self, ui),
                        Page::Transactions => pages::transactions(self, ui),
                        Page::Accounts => pages::accounts(self, ui),
                        Page::Investments => crate::ui::invest::investments(self, ui),
                        Page::Reports => crate::ui::reports::reports(self, ui),
                        Page::Plans => crate::ui::reports::plans(self, ui),
                        Page::Debts => crate::ui::debts::debts(self, ui),
                        Page::Recurring => crate::ui::debts::recurring(self, ui),
                        Page::Categories => pages::categories(self, ui),
                        Page::Rules => crate::ui::data::rules_page(self, ui),
                        Page::Activity => crate::ui::data::activity_page(self, ui),
                        Page::Trash => pages::trash(self, ui),
                        Page::Settings => pages::settings(self, ui),
                    }
                    ui.add_space(24.0);
                });
        });
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((message, until)) = self.toast.clone() else {
            return;
        };
        let now = ctx.input(|input| input.time);
        if now > until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -80.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(message);
                });
            });
    }
}

fn system_lang() -> Lang {
    std::env::var("LANG")
        .ok()
        .map(|value| Lang::parse(&value))
        .unwrap_or(Lang::Es)
}

impl eframe::App for CaudalApp {
    #[cfg(target_os = "android")]
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.ime.pump(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        widgets::reset_numeric_fields(&ctx);
        self.handle_events(&ctx);
        let insets = crate::android::safe_area(&ctx);
        let full = ui.max_rect();
        let inner = egui::Rect::from_min_max(
            full.min + egui::vec2(insets.left, insets.top),
            full.max - egui::vec2(insets.right, insets.bottom),
        );
        ui.painter().rect_filled(full, 0.0, ui.visuals().panel_fill);
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        let ui = &mut ui;
        match self.session {
            Session::Loading => {
                ui.centered_and_justified(|ui| ui.spinner());
            }
            Session::SignedOut => crate::ui::auth::show(self, ui),
            Session::SignedIn { .. } => {
                if self.onboarding.is_some() && !self.first_sync_done {
                    // Un dispositivo nuevo baja primero el perfil que ya pueda existir.
                    ui.centered_and_justified(|ui| {
                        ui.label(texts(self.lang).syncing);
                    });
                } else if self.locked {
                    crate::ui::data::lock_screen(self, ui);
                } else if self.onboarding.is_some() {
                    crate::ui::onboarding::show(self, ui);
                } else {
                    self.main_ui(ui);
                }
            }
        }
        crate::ui::dialogs::show(self, &ctx);
        self.toast_ui(&ctx);
        if let Some(mut shots) = self.shots.take() {
            shots.advance(self, &ctx);
            self.shots = Some(shots);
        }
    }
}
