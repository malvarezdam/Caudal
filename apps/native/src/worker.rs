//! Hilo de fondo: base de datos local, red y sincronización. La interfaz nunca espera.

use crate::platform::{Platform, now_millis, random_node, today};
use caudal_app_state::auth::{self, SignUpOutcome, SupabaseClient};
use caudal_app_state::debts::{DebtDraft, SharedDraft};
use caudal_app_state::model::{
    Account, Asset, Budget, Category, Contact, DebtPayment, Goal, GoalContribution, Profile,
    RecurringRule, Tag, Transaction,
};
use caudal_app_state::{AppCore, Data, Lang, SyncState, Table, texts};
use caudal_store_sqlite::SqliteStore;
use eframe::egui;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as async_mpsc;
use uuid::Uuid;

const DEMO_USER: Uuid = Uuid::from_u128(0xDE30);
const SYNC_EVERY: Duration = Duration::from_secs(60);
const SYNC_AFTER_WRITE: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone)]
pub enum Item {
    Transaction(Transaction),
    Account(Account),
    Category(Category),
    Tag(Tag),
    Profile(Profile),
    Contact(Contact),
    Rule(RecurringRule),
    Budget(Budget),
    Goal(Goal),
    Contribution(GoalContribution),
    CategoryRule(caudal_app_state::model::CategoryRule),
    Template(caudal_app_state::model::TransactionTemplate),
    Asset(Asset),
    Trade(caudal_app_state::investments::TradeDraft),
    Alert(caudal_app_state::model::PriceAlert),
    Debt(DebtDraft),
    Payment(DebtPayment, Option<Transaction>),
    Shared(SharedDraft),
    /// Confirma un movimiento pendiente.
    Confirm(Uuid),
}

#[derive(Debug, Clone)]
pub enum Command {
    SignIn {
        email: String,
        password: String,
    },
    SignUp {
        email: String,
        password: String,
    },
    SendRecovery {
        email: String,
    },
    VerifyRecovery {
        email: String,
        code: String,
    },
    UpdatePassword {
        password: String,
    },
    SignOut,
    Onboard {
        profile: Box<Profile>,
        suggested: bool,
        lang: Lang,
    },
    Save(Box<Item>),
    Delete {
        table: Table,
        id: Uuid,
    },
    Restore {
        table: Table,
        id: Uuid,
    },
    SyncNow,
    SetLang(Lang),
    /// Prepara la copia completa; llega como `Event::FileReady`.
    ExportBackup,
    RestoreBackup(String),
    DeleteAllData,
    DeleteAccount,
    Import(Vec<Transaction>),
    AddAttachment {
        attachment: Box<caudal_app_state::model::Attachment>,
        bytes: Vec<u8>,
    },
    LoadAttachment(Box<caudal_app_state::model::Attachment>),
}

#[derive(Debug, Clone)]
pub enum Event {
    SignedIn {
        email: Option<String>,
        demo: bool,
    },
    SignedOut,
    RecoveryVerified,
    Data(Arc<Data>),
    Sync(SyncState),
    Busy(bool),
    Info(String),
    Error(String),
    Saved,
    FileReady {
        name: String,
        mime: &'static str,
        bytes: Vec<u8>,
    },
    /// El archivo de un comprobante; `None` si todavía no está disponible.
    Attachment {
        id: Uuid,
        bytes: Option<Vec<u8>>,
    },
    PendingAttachments(std::collections::BTreeSet<Uuid>),
}

pub struct WorkerHandle {
    commands: async_mpsc::UnboundedSender<Command>,
    events: mpsc::Receiver<Event>,
}

impl WorkerHandle {
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

pub fn spawn(platform: Platform, ctx: egui::Context, lang: Lang, demo: bool) -> WorkerHandle {
    let (command_tx, command_rx) = async_mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("caudal-worker".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime de tokio");
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async move {
                let worker = Rc::new(Worker {
                    platform,
                    ctx,
                    events: event_tx,
                    client: Rc::new(auth::client()),
                    core: RefCell::new(None),
                    demo: Cell::new(false),
                    lang: Cell::new(lang),
                    syncing: Cell::new(false),
                    sync_again: Cell::new(false),
                    failures: Cell::new(0),
                });
                worker.start(demo).await;
                run(worker, command_rx).await;
            });
        })
        .expect("hilo de fondo");
    WorkerHandle {
        commands: command_tx,
        events: event_rx,
    }
}

async fn run(worker: Rc<Worker>, mut commands: async_mpsc::UnboundedReceiver<Command>) {
    let mut ticker = tokio::time::interval(SYNC_EVERY);
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => worker.handle(command).await,
                None => break,
            },
            _ = ticker.tick() => worker.request_sync(),
        }
    }
}

struct Worker {
    platform: Platform,
    ctx: egui::Context,
    events: mpsc::Sender<Event>,
    client: Rc<SupabaseClient>,
    core: RefCell<Option<Rc<AppCore<SqliteStore>>>>,
    demo: Cell<bool>,
    lang: Cell<Lang>,
    syncing: Cell<bool>,
    sync_again: Cell<bool>,
    failures: Cell<u32>,
}

impl Worker {
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    fn error(&self, message: impl Into<String>) {
        self.emit(Event::Error(message.into()));
    }

    fn core(&self) -> Option<Rc<AppCore<SqliteStore>>> {
        self.core.borrow().clone()
    }

    async fn start(self: &Rc<Self>, demo: bool) {
        if demo {
            if self.open(DEMO_USER, "caudal-demo").await {
                self.demo.set(true);
                self.seed_demo().await;
                self.emit(Event::SignedIn {
                    email: None,
                    demo: true,
                });
                self.emit(Event::Sync(SyncState::LocalOnly));
            }
            return;
        }
        match self.platform.session().load() {
            Some(session) => {
                let user = session.user.clone();
                self.client.set_session(Some(session));
                // Se abre sin esperar a la red: los datos locales están disponibles al instante.
                if self.open(user.id, &format!("caudal-{}", user.id)).await {
                    self.emit(Event::SignedIn {
                        email: user.email,
                        demo: false,
                    });
                    self.request_sync();
                }
            }
            None => self.emit(Event::SignedOut),
        }
    }

    /// La primera vez, el modo demostración se llena con datos de ejemplo.
    async fn seed_demo(&self) {
        let Some(core) = self.core() else { return };
        let demo = caudal_app_state::defaults::demo::sample(DEMO_USER, today(), self.lang.get());
        core.set_market(demo.market.clone());
        let existing = core.load().await.ok();
        let seeded = existing.as_ref().is_some_and(|data| data.profile.is_some());
        let has_assets = existing
            .as_ref()
            .is_some_and(|data| !data.assets.is_empty());
        let mut result = Ok(());
        // Una demostración creada antes de las inversiones recibe solo la cartera.
        if !has_assets {
            for asset in &demo.assets {
                result = result.and(core.save(Table::Assets, asset).await);
            }
            for draft in &demo.trades {
                result = result.and(core.save_trade(draft).await);
            }
            for alert in &demo.alerts {
                result = result.and(core.save(Table::PriceAlerts, alert).await);
            }
        }
        // Igual con las reglas y plantillas, que llegaron en la Fase 8.
        if existing
            .as_ref()
            .is_none_or(|data| data.templates.is_empty())
        {
            for rule in &demo.category_rules {
                result = result.and(core.save(Table::CategoryRules, rule).await);
            }
            for template in &demo.templates {
                result = result.and(core.save(Table::TransactionTemplates, template).await);
            }
        }
        if seeded {
            if let Err(error) = result {
                self.error(error.message(self.lang.get()));
            }
            // Con los precios de ejemplo ya cargados.
            self.reload().await;
            return;
        }
        result = result.and(core.save(Table::Profiles, &demo.profile).await);
        for account in &demo.accounts {
            result = result.and(core.save(Table::Accounts, account).await);
        }
        for category in &demo.categories {
            result = result.and(core.save(Table::Categories, category).await);
        }
        for tx in &demo.transactions {
            result = result.and(core.save(Table::Transactions, tx).await);
        }
        result = result.and(core.save_debt(&demo.debt).await);
        for tx in &demo.history {
            result = result.and(core.save(Table::Transactions, tx).await);
        }
        result = result.and(core.save(Table::Budgets, &demo.budget).await);
        result = result.and(core.save(Table::Goals, &demo.goal).await);
        for item in &demo.contributions {
            result = result.and(core.save(Table::GoalContributions, item).await);
        }
        for rule in &demo.rules {
            result = result.and(core.save(Table::RecurringRules, rule).await);
        }
        if let Err(error) = result {
            self.error(error.message(self.lang.get()));
        }
        self.reload().await;
    }

    /// Abre la base local del usuario y envía sus datos.
    async fn open(self: &Rc<Self>, user_id: Uuid, name: &str) -> bool {
        let store = match SqliteStore::open(self.platform.database_path(name)) {
            Ok(store) => store,
            Err(error) => {
                self.error(error.to_string());
                return false;
            }
        };
        match AppCore::open(store, user_id, now_millis, random_node).await {
            Ok(core) => {
                *self.core.borrow_mut() = Some(Rc::new(core));
                self.reload().await;
                true
            }
            Err(error) => {
                self.error(error.message(self.lang.get()));
                false
            }
        }
    }

    async fn reload(&self) {
        let Some(core) = self.core() else { return };
        // Las ocurrencias periódicas que ya vencieron aparecen sin esperar a la red.
        if let Err(error) = core.generate_recurring(today()).await {
            self.error(error.message(self.lang.get()));
        }
        match core.load().await {
            Ok(data) => {
                crate::android::write_notices(
                    &self.platform.data_dir,
                    &data,
                    today(),
                    self.lang.get(),
                );
                self.emit(Event::Data(Arc::new(data)));
            }
            Err(error) => self.error(error.message(self.lang.get())),
        }
        if let Ok(pending) = core.pending_attachments().await {
            self.emit(Event::PendingAttachments(pending));
        }
        if self.demo.get() {
            return;
        }
        if let Ok(pending) = core.pending_changes().await
            && !self.syncing.get()
            && pending > 0
        {
            self.emit(Event::Sync(SyncState::Pending(pending)));
        }
    }

    async fn signed_in(self: &Rc<Self>) {
        let Some(session) = self.client.session() else {
            return;
        };
        self.platform.session().save(&session);
        if self
            .open(session.user.id, &format!("caudal-{}", session.user.id))
            .await
        {
            self.emit(Event::SignedIn {
                email: session.user.email,
                demo: false,
            });
            self.request_sync();
        }
    }

    async fn handle(self: &Rc<Self>, command: Command) {
        let lang = self.lang.get();
        let auth_error = |error: &auth::SupabaseError| auth::error_message(error, lang);
        match command {
            Command::SetLang(lang) => {
                self.lang.set(lang);
                self.reload().await;
            }
            Command::SignIn { email, password } => {
                self.emit(Event::Busy(true));
                match self.client.sign_in(email.trim(), &password).await {
                    Ok(_) => self.signed_in().await,
                    Err(error) => self.error(auth_error(&error)),
                }
                self.emit(Event::Busy(false));
            }
            Command::SignUp { email, password } => {
                self.emit(Event::Busy(true));
                match self.client.sign_up(email.trim(), &password).await {
                    Ok(SignUpOutcome::SignedIn(_)) => self.signed_in().await,
                    Ok(SignUpOutcome::ConfirmationRequired { .. }) => {
                        self.emit(Event::Info(texts(lang).check_email.into()));
                    }
                    Err(error) => self.error(auth_error(&error)),
                }
                self.emit(Event::Busy(false));
            }
            Command::SendRecovery { email } => {
                self.emit(Event::Busy(true));
                match self.client.send_password_recovery(email.trim()).await {
                    Ok(()) => self.emit(Event::Info(texts(lang).code_sent.into())),
                    Err(error) => self.error(auth_error(&error)),
                }
                self.emit(Event::Busy(false));
            }
            Command::VerifyRecovery { email, code } => {
                self.emit(Event::Busy(true));
                match self.client.verify_recovery_code(email.trim(), &code).await {
                    Ok(_) => self.emit(Event::RecoveryVerified),
                    Err(error) => self.error(auth_error(&error)),
                }
                self.emit(Event::Busy(false));
            }
            Command::UpdatePassword { password } => {
                self.emit(Event::Busy(true));
                match self.client.update_password(&password).await {
                    Ok(()) => self.signed_in().await,
                    Err(error) => self.error(auth_error(&error)),
                }
                self.emit(Event::Busy(false));
            }
            Command::SignOut => {
                if !self.demo.get() {
                    let _ = self.client.sign_out().await;
                    self.platform.session().clear();
                }
                self.client.set_session(None);
                *self.core.borrow_mut() = None;
                self.demo.set(false);
                crate::android::write_notices(
                    &self.platform.data_dir,
                    &Data::default(),
                    today(),
                    self.lang.get(),
                );
                self.emit(Event::SignedOut);
            }
            Command::Onboard {
                profile,
                suggested,
                lang,
            } => {
                let Some(core) = self.core() else { return };
                match core.complete_onboarding(&profile, suggested, lang).await {
                    Ok(()) => self.after_write().await,
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::Save(item) => {
                let Some(core) = self.core() else { return };
                let result = match &*item {
                    Item::Transaction(model) => core.save(Table::Transactions, model).await,
                    Item::Account(model) => core.save(Table::Accounts, model).await,
                    Item::Category(model) => core.save(Table::Categories, model).await,
                    Item::Tag(model) => core.save(Table::Tags, model).await,
                    Item::Profile(model) => core.save(Table::Profiles, model).await,
                    Item::Contact(model) => core.save(Table::Contacts, model).await,
                    Item::Rule(model) => core.save(Table::RecurringRules, model).await,
                    Item::Budget(model) => core.save(Table::Budgets, model).await,
                    Item::Goal(model) => core.save(Table::Goals, model).await,
                    Item::Contribution(model) => core.save(Table::GoalContributions, model).await,
                    Item::CategoryRule(model) => core.save(Table::CategoryRules, model).await,
                    Item::Template(model) => core.save(Table::TransactionTemplates, model).await,
                    Item::Asset(model) => core.save(Table::Assets, model).await,
                    Item::Trade(draft) => core.save_trade(draft).await,
                    Item::Alert(model) => core.save(Table::PriceAlerts, model).await,
                    Item::Debt(draft) => core.save_debt(draft).await,
                    Item::Payment(payment, movement) => {
                        core.save_payment(payment, movement.as_ref()).await
                    }
                    Item::Shared(draft) => core.save_shared(draft).await,
                    Item::Confirm(id) => core.confirm(*id).await,
                };
                match result {
                    Ok(()) => {
                        self.emit(Event::Saved);
                        self.after_write().await;
                    }
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::Delete { table, id } => {
                let Some(core) = self.core() else { return };
                match core.delete(table, id).await {
                    Ok(()) => self.after_write().await,
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::Restore { table, id } => {
                let Some(core) = self.core() else { return };
                match core.restore(table, id).await {
                    Ok(()) => self.after_write().await,
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::SyncNow => {
                self.failures.set(0);
                self.request_sync();
            }
            Command::ExportBackup => {
                let Some(core) = self.core() else { return };
                let day = today().to_string();
                match core.export_backup(&day).await {
                    Ok(text) => self.emit(Event::FileReady {
                        name: format!("caudal-copia-{day}.json"),
                        mime: "application/json",
                        bytes: text.into_bytes(),
                    }),
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::RestoreBackup(text) => {
                let Some(core) = self.core() else { return };
                let t = texts(lang);
                match core.restore_backup(&text).await {
                    Ok(report) => {
                        self.emit(Event::Info(format!(
                            "{}: {} · {}: {}",
                            t.restored, report.restored, t.skipped, report.skipped
                        )));
                        self.after_write().await;
                    }
                    Err(caudal_app_state::backup::RestoreError::Format(problem)) => {
                        self.error(match problem {
                            caudal_app_state::backup::BackupError::NotABackup => {
                                t.backup_invalid.to_owned()
                            }
                            caudal_app_state::backup::BackupError::NewerVersion => {
                                t.backup_newer.to_owned()
                            }
                        })
                    }
                    Err(caudal_app_state::backup::RestoreError::App(error)) => {
                        self.error(error.message(lang))
                    }
                }
            }
            Command::DeleteAllData => {
                let Some(core) = self.core() else { return };
                match core.delete_all_data().await {
                    Ok(_) => self.after_write().await,
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::DeleteAccount => {
                self.emit(Event::Busy(true));
                if self.demo.get() {
                    if let Some(core) = self.core() {
                        let _ = core.clear_local().await;
                    }
                } else {
                    let user_id = self.client.session().map(|s| s.user.id);
                    if let Some(core) = self.core()
                        && let Err(error) = core.delete_account(&self.client).await
                    {
                        self.error(error.message(lang));
                        self.emit(Event::Busy(false));
                        return;
                    }
                    self.platform.session().clear();
                    // Primero se cierra la base: con la conexión abierta, Windows no deja
                    // borrar el archivo y los datos quedarían en el dispositivo.
                    *self.core.borrow_mut() = None;
                    if let Some(id) = user_id {
                        let path = self.platform.database_path(&format!("caudal-{id}"));
                        for suffix in ["", "-wal", "-shm", "-journal"] {
                            let mut file = path.clone().into_os_string();
                            file.push(suffix);
                            let _ = std::fs::remove_file(file);
                        }
                    }
                }
                self.client.set_session(None);
                *self.core.borrow_mut() = None;
                self.demo.set(false);
                crate::android::write_notices(
                    &self.platform.data_dir,
                    &Data::default(),
                    today(),
                    self.lang.get(),
                );
                self.emit(Event::Busy(false));
                self.emit(Event::SignedOut);
            }
            Command::Import(transactions) => {
                let Some(core) = self.core() else { return };
                let mut result = Ok(());
                for tx in &transactions {
                    result = result.and(core.save(Table::Transactions, tx).await);
                }
                match result {
                    Ok(()) => {
                        self.emit(Event::Info(format!(
                            "{}: {}",
                            texts(lang).imported,
                            transactions.len()
                        )));
                        self.after_write().await;
                    }
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::AddAttachment { attachment, bytes } => {
                let Some(core) = self.core() else { return };
                match core.add_attachment(&attachment, &bytes).await {
                    Ok(()) => {
                        self.emit(Event::Saved);
                        self.after_write().await;
                    }
                    Err(error) => self.error(error.message(lang)),
                }
            }
            Command::LoadAttachment(attachment) => {
                let Some(core) = self.core() else { return };
                let client = (!self.demo.get() && self.client.session().is_some())
                    .then(|| self.client.as_ref());
                match core.attachment_bytes(client, &attachment).await {
                    Ok(bytes) => self.emit(Event::Attachment {
                        id: attachment.meta.id,
                        bytes,
                    }),
                    Err(error) => {
                        self.emit(Event::Attachment {
                            id: attachment.meta.id,
                            bytes: None,
                        });
                        self.error(error.message(lang));
                    }
                }
            }
        }
    }

    async fn after_write(self: &Rc<Self>) {
        self.reload().await;
        let worker = Rc::clone(self);
        tokio::task::spawn_local(async move {
            tokio::time::sleep(SYNC_AFTER_WRITE).await;
            worker.request_sync();
        });
    }

    fn request_sync(self: &Rc<Self>) {
        if self.demo.get() || self.client.session().is_none() || self.core().is_none() {
            return;
        }
        if self.syncing.get() {
            self.sync_again.set(true);
            return;
        }
        self.syncing.set(true);
        let worker = Rc::clone(self);
        tokio::task::spawn_local(async move {
            loop {
                worker.sync_once().await;
                if !worker.sync_again.replace(false) {
                    break;
                }
            }
            worker.syncing.set(false);
        });
    }

    async fn sync_once(self: &Rc<Self>) {
        let Some(core) = self.core() else { return };
        self.emit(Event::Sync(SyncState::Syncing));
        let state = match core.sync(&self.client, today()).await {
            Ok(state) => state,
            Err(error) => {
                self.error(error.message(self.lang.get()));
                SyncState::Offline {
                    pending: core.pending_changes().await.unwrap_or(0),
                }
            }
        };
        if let Some(session) = self.client.session()
            && self.platform.session().is_stale(&session)
        {
            self.platform.session().save(&session);
        }
        self.reload().await;
        if matches!(state, SyncState::Offline { .. }) {
            // Reintento con espera creciente: 5 s, 10 s, 20 s… hasta el ciclo normal.
            let failures = self.failures.get().saturating_add(1);
            self.failures.set(failures);
            let delay = Duration::from_secs(5 << failures.min(4)).min(SYNC_EVERY);
            let worker = Rc::clone(self);
            tokio::task::spawn_local(async move {
                tokio::time::sleep(delay).await;
                worker.request_sync();
            });
        } else {
            self.failures.set(0);
        }
        self.emit(Event::Sync(state));
    }
}
