//! Componentes de la interfaz web.

use crate::state::{AppState, Dialog, DialogKind, Item, Page, Session, today};
use caudal_app_state::auth::MIN_PASSWORD_LEN;
use caudal_app_state::format::{amount_for_input, format_date};
use caudal_app_state::forms::{
    AccountForm, CategoryForm, ProfileForm, TagForm, TransactionForm, balance_adjustment,
};
use caudal_app_state::i18n::{ACCOUNT_KINDS, CATEGORY_KINDS, FORM_KINDS};
use caudal_app_state::model::{CategoryKind, TransactionKind};
use caudal_app_state::views::{self, Tone, TransactionFilter, TransactionView};
use caudal_app_state::{COMMON_CURRENCIES, Data, Lang, SyncState, Table, texts};
use leptos::prelude::*;
use uuid::Uuid;

pub(crate) fn state() -> AppState {
    expect_context::<AppState>()
}

pub(crate) fn tone_class(tone: Tone) -> &'static str {
    match tone {
        Tone::Positive => "pos",
        Tone::Negative => "neg",
        Tone::Neutral => "",
    }
}

pub(crate) fn parse_uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value).ok()
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);
    let needs_onboarding = Memo::new(move |_| state.data.with(|data| data.profile.is_none()));
    view! {
        {move || {
            let _ = state.lang.get();
            match state.session.get() {
                Session::Loading => view! { <div class="center muted">"…"</div> }.into_any(),
                Session::SignedOut => view! { <Auth/> }.into_any(),
                Session::SignedIn { .. } => {
                    if needs_onboarding.get() && !state.first_sync_done.get() {
                        view! { <div class="center muted">{texts(state.lang.get_untracked()).syncing}</div> }.into_any()
                    } else if needs_onboarding.get() {
                        view! { <Onboarding/> }.into_any()
                    } else if state.locked.get() {
                        view! { <crate::ui_data::LockScreen/> }.into_any()
                    } else {
                        view! { <Shell/> }.into_any()
                    }
                }
            }
        }}
        <Dialogs/>
        {move || state.toast.get().map(|message| view! { <div class="toast">{message}</div> })}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    SignIn,
    SignUp,
    RecoverEmail,
    RecoverCode,
}

#[component]
fn Auth() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let mode = RwSignal::new(AuthMode::SignIn);
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let repeat = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let switch = move |next: AuthMode| {
        mode.set(next);
        password.set(String::new());
        repeat.set(String::new());
        state.auth_error.set(None);
        state.auth_info.set(None);
    };
    let check_new = move || {
        if password.get_untracked().chars().count() < MIN_PASSWORD_LEN {
            state.auth_error.set(Some(t.password_too_short.into()));
            false
        } else if password.get_untracked() != repeat.get_untracked() {
            state.auth_error.set(Some(t.passwords_differ.into()));
            false
        } else {
            true
        }
    };
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        state.auth_error.set(None);
        if state.recovery_verified.get_untracked() {
            if check_new() {
                state.update_password(password.get_untracked());
            }
            return;
        }
        if email.get_untracked().trim().is_empty() {
            state.auth_error.set(Some(t.email_required.into()));
            return;
        }
        match mode.get_untracked() {
            AuthMode::SignIn => state.sign_in(email.get_untracked(), password.get_untracked()),
            AuthMode::SignUp => {
                if check_new() {
                    state.sign_up(email.get_untracked(), password.get_untracked());
                }
            }
            AuthMode::RecoverEmail => {
                mode.set(AuthMode::RecoverCode);
                state.send_recovery(email.get_untracked());
            }
            AuthMode::RecoverCode => {
                state.verify_recovery(email.get_untracked(), code.get_untracked())
            }
        }
    };
    let new_password = move || state.recovery_verified.get();
    view! {
        <div class="auth">
            <h1 class="brand big">{t.app_name}</h1>
            <form class="card" on:submit=submit>
                <h2>{move || {
                    if new_password() { t.new_password } else {
                        match mode.get() {
                            AuthMode::SignIn => t.sign_in,
                            AuthMode::SignUp => t.sign_up,
                            _ => t.forgot_password,
                        }
                    }
                }}</h2>
                <Show when=move || !new_password()>
                    <label>{t.email}
                        <input type="email" autocomplete="email" placeholder="nombre@correo.com"
                            prop:value=move || email.get() on:input=move |ev| email.set(event_target_value(&ev))/>
                    </label>
                </Show>
                <Show when=move || new_password() || matches!(mode.get(), AuthMode::SignIn | AuthMode::SignUp)>
                    <label>{move || if new_password() { t.new_password } else { t.password }}
                        <input type="password" autocomplete=move || if mode.get() == AuthMode::SignIn && !new_password() { "current-password" } else { "new-password" }
                            prop:value=move || password.get() on:input=move |ev| password.set(event_target_value(&ev))/>
                    </label>
                </Show>
                <Show when=move || new_password() || mode.get() == AuthMode::SignUp>
                    <label>{t.password_repeat}
                        <input type="password" autocomplete="new-password"
                            prop:value=move || repeat.get() on:input=move |ev| repeat.set(event_target_value(&ev))/>
                    </label>
                </Show>
                <Show when=move || !new_password() && mode.get() == AuthMode::RecoverCode>
                    <label>{t.code}
                        <input inputmode="numeric" autocomplete="one-time-code" placeholder="123456"
                            prop:value=move || code.get() on:input=move |ev| code.set(event_target_value(&ev))/>
                    </label>
                </Show>
                {move || state.auth_error.get().map(|error| view! { <p class="neg">{error}</p> })}
                {move || state.auth_info.get().map(|info| view! { <p class="accent">{info}</p> })}
                <div class="row">
                    <button class="primary" type="submit" disabled=move || state.busy.get()>
                        {move || {
                            if new_password() { t.save_password } else {
                                match mode.get() {
                                    AuthMode::SignIn => t.sign_in,
                                    AuthMode::SignUp => t.sign_up,
                                    AuthMode::RecoverEmail => t.send_code,
                                    AuthMode::RecoverCode => t.confirm,
                                }
                            }
                        }}
                    </button>
                    <Show when=move || state.busy.get()><span class="muted">"…"</span></Show>
                </div>
                <div class="links">
                    {move || match mode.get() {
                        AuthMode::SignIn => view! {
                            <a href="#" on:click=move |ev| { ev.prevent_default(); switch(AuthMode::SignUp) }>{t.no_account_yet}</a>
                            <a href="#" on:click=move |ev| { ev.prevent_default(); switch(AuthMode::RecoverEmail) }>{t.forgot_password}</a>
                        }.into_any(),
                        AuthMode::SignUp => view! {
                            <a href="#" on:click=move |ev| { ev.prevent_default(); switch(AuthMode::SignIn) }>{t.have_account}</a>
                        }.into_any(),
                        _ => view! {
                            <a href="#" on:click=move |ev| { ev.prevent_default(); state.recovery_verified.set(false); switch(AuthMode::SignIn) }>{t.back}</a>
                        }.into_any(),
                    }}
                </div>
            </form>
            <LangPicker/>
        </div>
    }
}

#[component]
fn LangPicker() -> impl IntoView {
    let state = state();
    view! {
        <div class="chips">
            {Lang::ALL.into_iter().map(|option| view! {
                <button class="chip" class:active=move || state.lang.get() == option
                    on:click=move |_| state.set_lang(option)>{option.name()}</button>
            }).collect_view()}
        </div>
    }
}

#[component]
fn Onboarding() -> impl IntoView {
    let state = state();
    let form = RwSignal::new(ProfileForm::new(state.lang.get_untracked()));
    let suggested = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let t = texts(state.lang.get_untracked());
    let start = move |_| {
        let current = form.get_untracked();
        match current.validate(state.data.get_untracked().user_id) {
            Ok(profile) => state.onboard(profile, suggested.get_untracked(), current.lang),
            Err(problem) => error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <div class="auth">
            <div class="card">
                <h2>{t.welcome}</h2>
                <p class="muted">{t.welcome_hint}</p>
                <label>{t.base_currency}</label>
                <div class="chips">
                    {COMMON_CURRENCIES.into_iter().map(|code| view! {
                        <button class="chip" class:active=move || form.with(|f| f.base_currency == code)
                            on:click=move |_| form.update(|f| f.base_currency = code.into())>{code}</button>
                    }).collect_view()}
                </div>
                <input prop:value=move || form.with(|f| f.base_currency.clone())
                    on:input=move |ev| form.update(|f| f.base_currency = event_target_value(&ev))/>
                <label>{t.language}</label>
                <div class="chips">
                    {Lang::ALL.into_iter().map(|option| view! {
                        <button class="chip" class:active=move || form.with(|f| f.lang == option)
                            on:click=move |_| { form.update(|f| f.lang = option); state.set_lang(option); }>{option.name()}</button>
                    }).collect_view()}
                </div>
                <label>{t.display_name}
                    <input prop:value=move || form.with(|f| f.display_name.clone())
                        on:input=move |ev| form.update(|f| f.display_name = event_target_value(&ev))/>
                </label>
                <label class="check">
                    <input type="checkbox" prop:checked=move || suggested.get()
                        on:change=move |ev| suggested.set(event_target_checked(&ev))/>
                    {t.suggested_categories}
                </label>
                {move || error.get().map(|error| view! { <p class="neg">{error}</p> })}
                <button class="primary" on:click=start>{t.start}</button>
            </div>
        </div>
    }
}

#[component]
fn Shell() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let nav_button = move |page: Page| {
        view! {
            <button class="nav" class:active=move || state.page.get() == page on:click=move |_| state.go(page)>
                {move || {
                    let trash = state.data.with(|data| data.trash.len());
                    if page == Page::Trash && trash > 0 { format!("{} ({trash})", page.label(lang)) } else { page.label(lang).to_owned() }
                }}
            </button>
        }
    };
    view! {
        <div class="shell">
            <aside class="sidebar">
                <h1 class="brand">{t.app_name}</h1>
                <button class="primary wide" on:click=move |_| state.open_new_transaction()>"+ " {t.new_transaction}</button>
                {Page::ALL.into_iter().map(nav_button).collect_view()}
            </aside>
            <div class="content">
                <header class="topbar">
                    <span class="brand mobile-only">{t.app_name}</span>
                    <span class="title desktop-only">{move || state.page.get().label(lang)}</span>
                    <SyncStatus/>
                </header>
                {move || state.error.get().map(|error| view! {
                    <div class="banner neg">{error}<button class="link" on:click=move |_| state.error.set(None)>"x"</button></div>
                })}
                <main>
                    {move || match state.page.get() {
                        Page::Home => view! { <Home/> }.into_any(),
                        Page::Transactions => view! { <Transactions/> }.into_any(),
                        Page::Accounts => view! { <Accounts/> }.into_any(),
                        Page::Investments => view! { <crate::ui_invest::Investments/> }.into_any(),
                        Page::Reports => view! { <crate::ui_reports::Reports/> }.into_any(),
                        Page::Plans => view! { <crate::ui_reports::Plans/> }.into_any(),
                        Page::Debts => view! { <crate::ui_debts::Debts/> }.into_any(),
                        Page::Recurring => view! { <crate::ui_debts::Recurring/> }.into_any(),
                        Page::Categories => view! { <Categories/> }.into_any(),
                        Page::Rules => view! { <crate::ui_data::RulesPage/> }.into_any(),
                        Page::Activity => view! { <crate::ui_data::ActivityPage/> }.into_any(),
                        Page::Trash => view! { <Trash/> }.into_any(),
                        Page::Settings => view! { <Settings/> }.into_any(),
                    }}
                </main>
            </div>
            <nav class="bottombar">
                {nav_button(Page::Home)}
                {nav_button(Page::Transactions)}
                <button class="primary plus" aria-label=t.new_transaction on:click=move |_| state.open_new_transaction()>"+"</button>
                {nav_button(Page::Accounts)}
                <button class="nav" class:active=move || matches!(state.page.get(), Page::Settings | Page::Categories | Page::Trash | Page::Debts | Page::Recurring | Page::Reports | Page::Plans | Page::Investments | Page::Rules | Page::Activity)
                    on:click=move |_| state.go(Page::Settings)>{Page::Settings.label(lang)}</button>
            </nav>
        </div>
    }
}

#[component]
fn SyncStatus() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <span class="sync" class:warn=move || state.sync.with(SyncState::is_problem)>
            {move || state.sync.with(|sync| sync.label(lang))}
        </span>
        <Show when=move || !matches!(state.sync.get(), SyncState::LocalOnly)>
            <button class="small" disabled=move || state.sync.get() == SyncState::Syncing
                on:click=move |_| state.request_sync()>{t.sync_now}</button>
        </Show>
        <Show when=move || matches!(state.sync.get(), SyncState::SignInRequired { .. })>
            <button class="small" on:click=move |_| state.sign_out()>{t.sign_in}</button>
        </Show>
    }
}

fn open_transaction(state: AppState, id: Uuid) {
    let data = state.data.get_untracked();
    if let Some(tx) = data.transaction(id) {
        state.open_dialog(Dialog::Transaction(TransactionForm::edit(
            tx,
            state.lang.get_untracked(),
        )));
    }
}

pub(crate) fn transaction_rows(rows: Vec<TransactionView>, lang: Lang) -> impl IntoView {
    let state = state();
    let pending = texts(lang).pending;
    rows.into_iter()
        .map(|row| {
            let id = row.id;
            let subtitle = if row.pending {
                format!("{pending} · {}", row.subtitle)
            } else {
                row.subtitle.clone()
            };
            view! {
                <button class="tx" class:pending=row.pending on:click=move |_| open_transaction(state, id)>
                    <span class="tx-main">
                        <span class="tx-title">{row.title}</span>
                        <span class="tx-sub muted">{subtitle}</span>
                    </span>
                    <span class="tx-side">
                        <span class=format!("tx-amount {}", tone_class(row.tone))>{row.amount_text}</span>
                        <span class="tx-sub muted">{row.date_text}</span>
                    </span>
                </button>
            }
        })
        .collect_view()
}

#[component]
fn Home() -> impl IntoView {
    view! {
        <crate::ui_data::Notices/>
        <crate::ui_debts::PendingCard/>
        <HomeMain/>
        <crate::ui_reports::HomeBudgets/>
        <crate::ui_debts::UpcomingCard days=30 max_items=5/>
    }
}

#[component]
fn HomeMain() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    move || {
        let data = state.data.get();
        let view = views::dashboard(&data, today(), lang);
        let unconverted = view.month.has_unconverted;
        view! {
            {crate::ui_invest::triggered_card(caudal_app_state::investments::triggered_alerts(&data, today(), lang), lang)}
            <section class="card">
                <p class="muted">{t.net_worth}</p>
                {if view.net_worth.is_empty() { vec!["0".to_owned()] } else { view.net_worth.clone() }
                    .into_iter().map(|total| view! { <p class="hero">{total}</p> }).collect_view()}
                {view.investments.clone().map(|text| view! { <button class="link muted small" on:click=move |_| state.go(Page::Investments)>{text}</button> })}
                {view.net_worth_incomplete.then(|| view! { <p class="warn small">{t.unconverted_note}</p> })}
            </section>
            <section class="card">
                <p class="muted">{t.this_month}</p>
                <div class="grid3">
                    <div><span class="muted small">{t.income}</span><strong class="pos">{view.month.income.clone()}</strong></div>
                    <div><span class="muted small">{t.expense}</span><strong class="neg">{view.month.expense.clone()}</strong></div>
                    <div><span class="muted small">{t.savings}</span><strong class=tone_class(view.month.savings_tone)>{view.month.savings.clone()}</strong></div>
                </div>
                {unconverted.then(|| view! { <p class="warn small">{t.unconverted_note}</p> })}
            </section>
            <section class="card">
                <div class="row between"><strong>{t.accounts}</strong>
                    <button class="small" on:click=move |_| state.new_account()>"+ " {t.new_account}</button></div>
                {view.accounts.is_empty().then(|| view! { <p class="muted">{t.no_accounts}</p> })}
                {view.accounts.into_iter().map(|account| {
                    let id = account.id;
                    view! {
                        <button class="line" on:click=move |_| {
                            state.filter.set(TransactionFilter { account_id: Some(id), ..TransactionFilter::default() });
                            state.go(Page::Transactions);
                        }>
                            <span>{account.name}" "<span class="muted small">{account.kind_label}</span></span>
                            <span class=tone_class(account.tone)>{account.balance_text}</span>
                        </button>
                    }
                }).collect_view()}
            </section>
            <section class="card">
                <div class="row between"><strong>{t.recent}</strong>
                    <button class="small" on:click=move |_| { state.filter.set(TransactionFilter::default()); state.go(Page::Transactions); }>{t.view_all}</button></div>
                {view.recent.is_empty().then(|| view! { <p class="muted">{t.no_transactions}</p> })}
                {transaction_rows(view.recent, lang)}
            </section>
        }
    }
}

pub(crate) fn uuid_options(
    options: Vec<(Uuid, String)>,
    selected: impl Fn() -> Option<Uuid> + Copy + Send + Sync + 'static,
) -> impl IntoView {
    options
        .into_iter()
        .map(move |(id, label)| view! { <option value=id.to_string() prop:selected=move || selected() == Some(id)>{label}</option> })
        .collect_view()
}

#[component]
fn Transactions() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let accounts: Vec<(Uuid, String)> = data
        .accounts
        .iter()
        .map(|a| (a.meta.id, a.name.clone()))
        .collect();
    let categories = data.categories_for(TransactionKind::Transfer);
    let set_filter = move |change: &dyn Fn(&mut TransactionFilter)| {
        state.filter.update(|filter| change(filter));
        state.limit.set(crate::state::PAGE_SIZE);
    };
    view! {
        <h2 class="desktop-only">{t.transactions}</h2>
        <div class="filters">
            <input type="search" placeholder=t.search prop:value=move || state.filter.with(|f| f.text.clone())
                on:input=move |ev| { let value = event_target_value(&ev); set_filter(&|f| f.text = value.clone()) }/>
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); set_filter(&|f| f.account_id = value) }>
                <option value="" prop:selected=move || state.filter.with(|f| f.account_id.is_none())>{t.all_accounts}</option>
                {uuid_options(accounts, move || state.filter.with(|f| f.account_id))}
            </select>
            <select on:change=move |ev| {
                let value = event_target_value(&ev);
                let category = match value.as_str() { "" => None, "none" => Some(None), id => Some(parse_uuid(id)) };
                set_filter(&|f| f.category_id = category)
            }>
                <option value="" prop:selected=move || state.filter.with(|f| f.category_id.is_none())>{t.all_categories}</option>
                <option value="none" prop:selected=move || state.filter.with(|f| f.category_id == Some(None))>{t.no_category}</option>
                {categories.into_iter().map(|(id, label)| view! {
                    <option value=id.to_string() prop:selected=move || state.filter.with(|f| f.category_id == Some(Some(id)))>{label}</option>
                }).collect_view()}
            </select>
            <select on:change=move |ev| {
                let value = event_target_value(&ev);
                let kind = FORM_KINDS.into_iter().chain([TransactionKind::Adjustment]).find(|k| format!("{k:?}") == value);
                set_filter(&|f| f.kind = kind)
            }>
                <option value="" prop:selected=move || state.filter.with(|f| f.kind.is_none())>{t.all_kinds}</option>
                {FORM_KINDS.into_iter().chain([TransactionKind::Adjustment]).map(|kind| view! {
                    <option value=format!("{kind:?}") prop:selected=move || state.filter.with(|f| f.kind == Some(kind))>{t.transaction_kind(kind)}</option>
                }).collect_view()}
            </select>
            <label class="inline">{t.from}
                <input type="date" prop:value=move || state.filter.with(|f| f.from.map(|d| d.to_string()).unwrap_or_default())
                    on:change=move |ev| { let value = event_target_value(&ev).parse().ok(); set_filter(&|f| f.from = value) }/>
            </label>
            <label class="inline">{t.to}
                <input type="date" prop:value=move || state.filter.with(|f| f.to.map(|d| d.to_string()).unwrap_or_default())
                    on:change=move |ev| { let value = event_target_value(&ev).parse().ok(); set_filter(&|f| f.to = value) }/>
            </label>
            <Show when=move || !state.filter.with(TransactionFilter::is_empty)>
                <button class="small" on:click=move |_| state.filter.set(TransactionFilter::default())>{t.clear_filters}</button>
            </Show>
        </div>
        {move || {
            let data = state.data.get();
            let (rows, total) = views::transactions(&data, &state.filter.get(), state.limit.get(), lang);
            let shown = rows.len();
            view! {
                {(shown == 0).then(|| view! { <p class="muted">{t.no_transactions}</p> })}
                <section class="card list">{transaction_rows(rows, lang)}</section>
                {(total > shown).then(|| view! {
                    <button class="center-button" on:click=move |_| state.limit.update(|l| *l += crate::state::PAGE_SIZE)>
                        {format!("{} ({shown}/{total})", t.load_more)}
                    </button>
                })}
            }
        }}
    }
}

#[component]
fn Accounts() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2 class="desktop-only">{t.accounts}</h2>
        <div class="row">
            <button on:click=move |_| state.new_account()>"+ " {t.new_account}</button>
            <label class="check"><input type="checkbox" prop:checked=move || state.show_archived.get()
                on:change=move |ev| state.show_archived.set(event_target_checked(&ev))/>{t.show_archived}</label>
        </div>
        {move || {
            let data = state.data.get();
            let list = views::accounts(&data, lang, state.show_archived.get());
            view! {
                {list.is_empty().then(|| view! { <p class="muted">{t.no_accounts}</p> })}
                {list.into_iter().map(|account| {
                    let id = account.id;
                    let balance = account.balance;
                    let name = account.name.clone();
                    let mut detail = format!("{} · {}", account.kind_label, account.currency);
                    if account.archived { detail = format!("{detail} · {}", t.archived); }
                    view! {
                        <section class="card">
                            <div class="row between">
                                <div><strong>{account.name.clone()}</strong><br/><span class="muted small">{detail}</span></div>
                                <span class=format!("big-amount {}", tone_class(account.tone))>{account.balance_text}</span>
                            </div>
                            {(account.unconverted > 0).then(|| view! { <p class="warn small">{t.unconverted_note}</p> })}
                            <div class="row wrap">
                                <button class="small" on:click=move |_| {
                                    state.filter.set(TransactionFilter { account_id: Some(id), ..TransactionFilter::default() });
                                    state.go(Page::Transactions);
                                }>{t.transactions}</button>
                                <button class="small" on:click=move |_| state.open_dialog(Dialog::Adjust { account_id: id, value: amount_for_input(balance, lang) })>{t.adjust_balance}</button>
                                <button class="small" on:click=move |_| {
                                    if let Some(model) = state.data.get_untracked().account(id) {
                                        state.open_dialog(Dialog::Account(AccountForm::edit(model, lang)));
                                    }
                                }>{t.edit}</button>
                                <button class="small" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete { table: Table::Accounts, id, label: name.clone() })>{t.delete}</button>
                            </div>
                        </section>
                    }
                }).collect_view()}
            }
        }}
    }
}

#[component]
fn Categories() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let row = move |id: Uuid, name: String, kind: CategoryKind, child: bool| {
        let label = name.clone();
        view! {
            <div class="line" class:child=child>
                <span>{name}" "<span class="muted small">{t.category_kind(kind)}</span></span>
                <span class="row">
                    <button class="small" on:click=move |_| {
                        if let Some(category) = state.data.get_untracked().category(id) {
                            state.open_dialog(Dialog::Category(CategoryForm::edit(category)));
                        }
                    }>{t.edit}</button>
                    <button class="small" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete { table: Table::Categories, id, label: label.clone() })>{t.delete}</button>
                </span>
            </div>
        }
    };
    view! {
        <h2>{t.categories}</h2>
        <button on:click=move |_| state.new_category()>"+ " {t.new_category}</button>
        {move || {
            let data = state.data.get();
            let roots: Vec<_> = data.categories.iter()
                .filter(|c| c.parent_id.is_none_or(|parent| data.category(parent).is_none()))
                .cloned().collect();
            view! {
                <section class="card">
                    {data.categories.is_empty().then(|| view! { <p class="muted">{t.no_categories}</p> })}
                    {roots.into_iter().map(|root| {
                        let children: Vec<_> = data.categories.iter().filter(|c| c.parent_id == Some(root.meta.id)).cloned().collect();
                        view! {
                            {row(root.meta.id, root.name.clone(), root.kind, false)}
                            {children.into_iter().map(|child| row(child.meta.id, child.name.clone(), child.kind, true)).collect_view()}
                        }
                    }).collect_view()}
                </section>
            }
        }}
        <h2>{t.tags}</h2>
        <button on:click=move |_| state.new_tag()>"+ " {t.new_tag}</button>
        {move || {
            let data = state.data.get();
            view! {
                <section class="card chips">
                    {data.tags.is_empty().then(|| view! { <p class="muted">{t.no_tags}</p> })}
                    {data.tags.iter().cloned().map(|tag| {
                        let label = format!("#{}", tag.name);
                        view! { <button class="chip" on:click=move |_| state.open_dialog(Dialog::Tag(TagForm::edit(&tag)))>{label}</button> }
                    }).collect_view()}
                </section>
            }
        }}
    }
}

#[component]
fn Trash() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2>{t.trash}</h2>
        <p class="muted">{t.trash_hint}</p>
        {move || {
            let data = state.data.get();
            if data.trash.is_empty() {
                return view! { <p class="muted">{t.trash_empty}</p> }.into_any();
            }
            view! {
                <section class="card">
                    {views::trash(&data, lang).into_iter().map(|item| {
                        let (table, id) = (item.table, item.id);
                        view! {
                            <div class="line">
                                <span>{item.label}<br/><span class="muted small">{item.kind_label}</span></span>
                                <button class="small" on:click=move |_| state.restore(table, id)>{t.restore}</button>
                            </div>
                        }
                    }).collect_view()}
                </section>
            }.into_any()
        }}
    }
}

#[component]
fn Settings() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let profile_form = move || {
        state
            .data
            .with(|data| data.profile.as_ref().map(ProfileForm::edit))
    };
    let save_profile = move |change: &dyn Fn(&mut ProfileForm)| {
        if let Some(mut form) = profile_form() {
            change(&mut form);
            if let Ok(profile) = form.validate(state.data.get_untracked().user_id) {
                state.save(Item::Profile(profile));
            }
        }
    };
    view! {
        <h2 class="desktop-only">{t.settings}</h2>
        <section class="card mobile-only">
            <button class="line" on:click=move |_| state.go(Page::Investments)>{t.investments}</button>
            <button class="line" on:click=move |_| state.go(Page::Rules)>{t.rules_title}</button>
            <button class="line" on:click=move |_| state.go(Page::Activity)>{t.activity}</button>
            <button class="line" on:click=move |_| state.go(Page::Reports)>{t.reports}</button>
            <button class="line" on:click=move |_| state.go(Page::Plans)>{t.plans}</button>
            <button class="line" on:click=move |_| state.go(Page::Debts)>{t.debts}</button>
            <button class="line" on:click=move |_| state.go(Page::Recurring)>{t.recurring}</button>
            <button class="line" on:click=move |_| state.go(Page::Categories)>{t.categories}</button>
            <button class="line" on:click=move |_| state.go(Page::Trash)>{t.trash}</button>
        </section>
        <section class="card">
            {move || match state.session.get() {
                Session::SignedIn { demo: true, .. } => view! { <p class="warn">{t.demo_mode}</p> }.into_any(),
                Session::SignedIn { email: Some(email), .. } => view! {
                    <p class="muted small">{t.signed_in_as}</p><p><strong>{email}</strong></p>
                }.into_any(),
                _ => ().into_any(),
            }}
            <label>{t.display_name}
                <input prop:value=move || profile_form().map(|f| f.display_name).unwrap_or_default()
                    on:change=move |ev| { let value = event_target_value(&ev); save_profile(&|f| f.display_name = value.clone()) }/>
            </label>
            <label>{t.base_currency}
                <select on:change=move |ev| { let value = event_target_value(&ev); save_profile(&|f| f.base_currency = value.clone()) }>
                    {COMMON_CURRENCIES.into_iter().map(|code| view! {
                        <option value=code prop:selected=move || profile_form().is_some_and(|f| f.base_currency == code)>{code}</option>
                    }).collect_view()}
                </select>
            </label>
        </section>
        <section class="card">
            <label>{t.language}</label>
            <div class="chips">
                {Lang::ALL.into_iter().map(|option| view! {
                    <button class="chip" class:active=move || state.lang.get() == option on:click=move |_| {
                        state.set_lang(option);
                        save_profile(&|f| f.lang = option);
                    }>{option.name()}</button>
                }).collect_view()}
            </div>
            <label>{t.theme}</label>
            <div class="chips">
                {[("system", t.theme_system), ("light", t.theme_light), ("dark", t.theme_dark)].into_iter().map(|(code, label)| view! {
                    <button class="chip" class:active=move || state.theme.get() == code on:click=move |_| state.set_theme(code)>{label}</button>
                }).collect_view()}
            </div>
        </section>
        <crate::ui_data::DataCards/>
        <section class="card">
            <button on:click=move |_| state.open_dialog(Dialog::ConfirmSignOut)>{t.sign_out}</button>
        </section>
    }
}

/// Ventana modal. Se vuelve a construir solo al cambiar de tipo, así los campos no pierden el foco.
#[component]
fn Dialogs() -> impl IntoView {
    let state = state();
    let kind = Memo::new(move |_| {
        state
            .dialog
            .with(|dialog| dialog.as_ref().map(Dialog::kind))
    });
    move || {
        let kind = kind.get()?;
        let _ = state.lang.get();
        Some(view! {
            <div class="backdrop" on:click=move |_| state.dialog.set(None)>
                <div class="dialog card" on:click=|ev| ev.stop_propagation()>
                    {match kind {
                        DialogKind::Transaction => view! { <TransactionDialog/> }.into_any(),
                        DialogKind::Account => view! { <AccountDialog/> }.into_any(),
                        DialogKind::Category => view! { <CategoryDialog/> }.into_any(),
                        DialogKind::Tag => view! { <TagDialog/> }.into_any(),
                        DialogKind::Adjust => view! { <AdjustDialog/> }.into_any(),
                        DialogKind::ConfirmDelete => view! { <ConfirmDelete/> }.into_any(),
                        DialogKind::ConfirmSignOut => view! { <ConfirmSignOut/> }.into_any(),
                        DialogKind::Contact => view! { <crate::ui_debts::ContactDialog/> }.into_any(),
                        DialogKind::Debt => view! { <crate::ui_debts::DebtDialog/> }.into_any(),
                        DialogKind::Payment => view! { <crate::ui_debts::PaymentDialog/> }.into_any(),
                        DialogKind::Shared => view! { <crate::ui_debts::SharedDialog/> }.into_any(),
                        DialogKind::Recurring => view! { <crate::ui_debts::RecurringDialog/> }.into_any(),
                        DialogKind::Budget => view! { <crate::ui_reports::BudgetDialog/> }.into_any(),
                        DialogKind::Goal => view! { <crate::ui_reports::GoalDialog/> }.into_any(),
                        DialogKind::Contribution => view! { <crate::ui_reports::ContributionDialog/> }.into_any(),
                        DialogKind::Simulate => view! { <crate::ui_reports::SimulateDialog/> }.into_any(),
                        DialogKind::Asset => view! { <crate::ui_invest::AssetDialog/> }.into_any(),
                        DialogKind::Trade => view! { <crate::ui_invest::TradeDialog/> }.into_any(),
                        DialogKind::Alert => view! { <crate::ui_invest::AlertDialog/> }.into_any(),
                        DialogKind::ConfirmDeleteAll => view! { <crate::ui_data::ConfirmDeleteAllDialog/> }.into_any(),
                        DialogKind::SetPin => view! { <crate::ui_data::SetPinDialog/> }.into_any(),
                        DialogKind::Rule => view! { <crate::ui_data::RuleDialog/> }.into_any(),
                        DialogKind::Template => view! { <crate::ui_data::TemplateDialog/> }.into_any(),
                        DialogKind::Import => view! { <crate::ui_data::ImportDialog/> }.into_any(),
                        DialogKind::Attachment => view! { <crate::ui_data::AttachmentDialog/> }.into_any(),
                    }}
                    {move || state.dialog_error.get().map(|error| view! { <p class="neg">{error}</p> })}
                </div>
            </div>
        })
    }
}

pub(crate) fn close(state: AppState) {
    state.dialog.set(None);
}

pub(crate) fn dialog_buttons(
    state: AppState,
    lang: Lang,
    on_save: impl Fn() + 'static,
    on_delete: Option<Box<dyn Fn()>>,
) -> impl IntoView {
    let t = texts(lang);
    view! {
        <div class="row actions">
            <button class="primary" on:click=move |_| on_save()>{t.save}</button>
            <button on:click=move |_| close(state)>{t.cancel}</button>
            {on_delete.map(|delete| view! { <button class="danger push" on:click=move |_| delete()>{t.delete}</button> })}
        </div>
    }
}

#[component]
fn TransactionDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let get = move |read: fn(&TransactionForm) -> String| {
        state.dialog.with(|dialog| match dialog {
            Some(Dialog::Transaction(form)) => read(form),
            _ => String::new(),
        })
    };
    let edit = move |change: &dyn Fn(&mut TransactionForm)| {
        state.dialog.update(|dialog| {
            if let Some(Dialog::Transaction(form)) = dialog {
                change(form);
            }
        });
    };
    let form = move || {
        state.dialog.with(|dialog| match dialog {
            Some(Dialog::Transaction(form)) => Some(form.clone()),
            _ => None,
        })
    };
    let Some(initial) = form() else {
        return ().into_any();
    };
    let editing = initial.original.clone();
    let kind = Memo::new(move |_| form().map_or(TransactionKind::Expense, |f| f.kind));
    let accounts: Vec<(Uuid, String)> = data
        .selectable_accounts(initial.account_id.or(initial.to_account_id))
        .into_iter()
        .map(|a| (a.meta.id, format!("{} ({})", a.name, a.currency)))
        .collect();
    let to_accounts = accounts.clone();
    let tags = data.tags.clone();
    let data_for_save = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data_for_save, data_for_save.user_id, lang) {
            Ok(tx) => {
                close(state);
                state.save(Item::Transaction(tx));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = editing.map(|original| {
        let label = views::transaction_view(&data, &original, lang).title;
        let id = original.meta.id;
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::Transactions,
                id,
                label: label.clone(),
            })
        }) as Box<dyn Fn()>
    });
    let data_for_select = data.clone();
    let data_for_received = data.clone();
    let category_data = data.clone();
    let rules_data = data.clone();
    let template_data = StoredValue::new(data.clone());
    let templates = if initial.original.is_none() {
        caudal_app_state::rules::templates_view(&data, lang)
    } else {
        Vec::new()
    };
    let attachments = initial
        .original
        .as_ref()
        .map(|original| crate::ui_data::attachments_section(original.meta.id, lang));
    let as_template = move || {
        let Some(current) = form() else { return };
        let result = template_data.with_value(|data| {
            caudal_app_state::rules::TemplateForm::from_transaction(&current).validate(data, lang)
        });
        match result {
            Ok(template) => state.save(Item::Template(template)),
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let apply_data = data.clone();
    view! {
        <h2>{if initial.original.is_some() { t.edit_transaction } else { t.new_transaction }}</h2>
        {(!templates.is_empty()).then(|| view! {
            <div class="chips">
                {templates.into_iter().map(|template| {
                    let id = template.id;
                    let data = apply_data.clone();
                    view! {
                        <button class="chip" title=template.detail.clone() on:click=move |_| {
                            if let Some(model) = data.templates.iter().find(|m| m.meta.id == id) {
                                let next = TransactionForm::from_template(model, &data, crate::state::today(), lang);
                                state.dialog.set(Some(Dialog::Transaction(next)));
                            }
                        }>{template.name.clone()}</button>
                    }
                }).collect_view()}
            </div>
        })}
        {move || if kind.get() == TransactionKind::Adjustment {
            view! { <p class="muted">{t.transaction_kind(TransactionKind::Adjustment)}</p> }.into_any()
        } else {
            view! {
                <div class="chips">
                    {FORM_KINDS.into_iter().map(|option| view! {
                        <button class="chip" class:active=move || kind.get() == option on:click=move |_| edit(&|f| f.kind = option)>{t.transaction_kind(option)}</button>
                    }).collect_view()}
                </div>
            }.into_any()
        }}
        <label>{t.amount}</label>
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" placeholder="0" autofocus
                prop:value=move || get(|f| f.amount.clone()) on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/>
            <input class="currency" prop:value=move || get(|f| f.currency.clone())
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.date}
            <input type="date" prop:value=move || get(|f| f.date.to_string())
                on:change=move |ev| { if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.date = date) } }/>
        </label>
        <label>{t.account}
            <select on:change=move |ev| {
                let value = parse_uuid(&event_target_value(&ev));
                let data = data_for_select.clone();
                edit(&|f| f.select_account(&data, value));
            }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.account_id.is_none())>{t.no_account}</option>
                {uuid_options(accounts, move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        {move || match kind.get() {
            TransactionKind::Transfer => {
                let data = data_for_received.clone();
                let options = to_accounts.clone();
                view! {
                    <label>{t.to_account}
                        <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.to_account_id = value) }>
                            <option value="" prop:selected=move || form().is_some_and(|f| f.to_account_id.is_none())>{t.no_account}</option>
                            {uuid_options(options, move || form().and_then(|f| f.to_account_id))}
                        </select>
                    </label>
                    <Show when=move || form().is_some_and(|f| f.needs_received_amount(&data))>
                        <label>{t.received_amount}
                            <input inputmode="decimal" prop:value=move || get(|f| f.received_amount.clone())
                                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.received_amount = v.clone()) }/>
                        </label>
                    </Show>
                }.into_any()
            }
            TransactionKind::Adjustment => ().into_any(),
            other => {
                let options = category_data.categories_for(other);
                view! {
                    <label>{t.category}
                        <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                            <option value="" prop:selected=move || form().is_some_and(|f| f.category_id.is_none())>{t.no_category}</option>
                            {uuid_options(options, move || form().and_then(|f| f.category_id))}
                        </select>
                    </label>
                }.into_any()
            }
        }}
        <details open=!initial.description.is_empty() || !initial.note.is_empty() || !initial.tag_ids.is_empty()>
            <summary>{t.optional_details}</summary>
            <label>{t.description}
                <input prop:value=move || get(|f| f.description.clone())
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        let data = rules_data.clone();
                        edit(&|f| { f.description = v.clone(); f.apply_rules(&data); });
                    }/>
            </label>
            {(!tags.is_empty()).then(|| view! {
                <label>{t.tags}</label>
                <div class="chips">
                    {tags.into_iter().map(|tag| {
                        let id = tag.meta.id;
                        view! {
                            <button class="chip" class:active=move || form().is_some_and(|f| f.tag_ids.contains(&id))
                                on:click=move |_| edit(&|f| if f.tag_ids.contains(&id) { f.tag_ids.retain(|t| *t != id) } else { f.tag_ids.push(id) })>
                                {format!("#{}", tag.name)}
                            </button>
                        }
                    }).collect_view()}
                </div>
            })}
            <label>{t.note}
                <textarea rows="2" prop:value=move || get(|f| f.note.clone())
                    on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }></textarea>
            </label>
        </details>
        {attachments}
        {move || form().is_some_and(|f| !f.description.trim().is_empty()).then(|| view! {
            <button class="small" on:click=move |_| as_template()>{t.save_as_template}</button>
        })}
        {dialog_buttons(state, lang, save, delete)}
    }
    .into_any()
}

#[component]
fn AccountDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let form = move || {
        state.dialog.with(|d| match d {
            Some(Dialog::Account(f)) => Some(f.clone()),
            _ => None,
        })
    };
    let edit = move |change: &dyn Fn(&mut AccountForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Account(f)) = d {
                change(f)
            }
        })
    };
    let get = move |read: fn(&AccountForm) -> String| form().map(|f| read(&f)).unwrap_or_default();
    let editing = form().is_some_and(|f| f.original.is_some());
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(state.data.get_untracked().user_id, lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Account(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <h2>{if editing { t.edit } else { t.new_account }}</h2>
        <label>{t.name}<input prop:value=move || get(|f| f.name.clone()) on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <label>{t.kind}</label>
        <div class="chips">
            {ACCOUNT_KINDS.into_iter().map(|kind| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.kind == kind) on:click=move |_| edit(&|f| f.kind = kind)>{t.account_kind(kind)}</button>
            }).collect_view()}
        </div>
        <label>{t.currency}<input prop:value=move || get(|f| f.currency.clone()) on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/></label>
        <label>{t.opening_balance}<input inputmode="decimal" prop:value=move || get(|f| f.opening_balance.clone()) on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.opening_balance = v.clone()) }/></label>
        <label>{t.note}<input prop:value=move || get(|f| f.note.clone()) on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {editing.then(|| view! {
            <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.archived)
                on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.archived = v) }/>{t.archived}</label>
        })}
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
fn CategoryDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let form = move || {
        state.dialog.with(|d| match d {
            Some(Dialog::Category(f)) => Some(f.clone()),
            _ => None,
        })
    };
    let edit = move |change: &dyn Fn(&mut CategoryForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Category(f)) = d {
                change(f)
            }
        })
    };
    let own = form().and_then(|f| f.original.map(|o| o.meta.id));
    let parents: Vec<(Uuid, String)> = state.data.with_untracked(|data: &std::sync::Arc<Data>| {
        data.categories
            .iter()
            .filter(|c| c.parent_id.is_none() && Some(c.meta.id) != own)
            .map(|c| (c.meta.id, c.name.clone()))
            .collect()
    });
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(state.data.get_untracked().user_id) {
            Ok(model) => {
                close(state);
                state.save(Item::Category(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <h2>{if own.is_some() { t.edit } else { t.new_category }}</h2>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <label>{t.kind}</label>
        <div class="chips">
            {CATEGORY_KINDS.into_iter().map(|kind| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.kind == kind) on:click=move |_| edit(&|f| f.kind = kind)>{t.category_kind(kind)}</button>
            }).collect_view()}
        </div>
        <label>{t.parent_category}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.parent_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.parent_id.is_none())>{t.none}</option>
                {uuid_options(parents, move || form().and_then(|f| f.parent_id))}
            </select>
        </label>
        {dialog_buttons(state, lang, save, None)}
    }
}

#[component]
fn TagDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let form = move || {
        state.dialog.with(|d| match d {
            Some(Dialog::Tag(f)) => Some(f.clone()),
            _ => None,
        })
    };
    let edit = move |change: &dyn Fn(&mut TagForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Tag(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(state.data.get_untracked().user_id) {
            Ok(model) => {
                close(state);
                state.save(Item::Tag(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|tag| {
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::Tags,
                id: tag.meta.id,
                label: tag.name.clone(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit } else { t.new_tag }}</h2>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, delete)}
    }
}

#[component]
fn AdjustDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let current = move || {
        state.dialog.with(|d| match d {
            Some(Dialog::Adjust { account_id, value }) => Some((*account_id, value.clone())),
            _ => None,
        })
    };
    let Some((account_id, _)) = current() else {
        return ().into_any();
    };
    let data = state.data.get_untracked();
    let Some(account) = data.account(account_id).cloned() else {
        return ().into_any();
    };
    let balance = views::accounts(&data, lang, true)
        .into_iter()
        .find(|v| v.id == account_id)
        .map(|v| v.balance)
        .unwrap_or_default();
    let label = format!("{} ({})", t.real_balance, account.currency);
    let name = account.name.clone();
    let save = move || {
        let Some((_, value)) = current() else { return };
        match balance_adjustment(&account, balance, &value, data.user_id, today(), lang) {
            Ok(Some(tx)) => {
                close(state);
                state.save(Item::Transaction(tx));
            }
            Ok(None) => close(state),
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    view! {
        <h2>{t.adjust_balance}</h2>
        <p>{name}</p>
        <label>{label}
            <input class="amount" inputmode="decimal" prop:value=move || current().map(|(_, v)| v).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); state.dialog.update(|d| if let Some(Dialog::Adjust { value, .. }) = d { *value = v.clone() }) }/>
        </label>
        {dialog_buttons(state, lang, save, None)}
    }
    .into_any()
}

#[component]
fn ConfirmDelete() -> impl IntoView {
    let state = state();
    let t = texts(state.lang.get_untracked());
    let Some((table, id, label)) = state.dialog.with_untracked(|d| match d {
        Some(Dialog::ConfirmDelete { table, id, label }) => Some((*table, *id, label.clone())),
        _ => None,
    }) else {
        return ().into_any();
    };
    view! {
        <h2>{t.question(t.delete)}</h2>
        <p><strong>{label}</strong></p>
        <p class="muted">{t.trash_hint}</p>
        <div class="row actions">
            <button class="danger" on:click=move |_| { close(state); state.delete(table, id); }>{t.delete}</button>
            <button on:click=move |_| close(state)>{t.cancel}</button>
        </div>
    }
    .into_any()
}

#[component]
fn ConfirmSignOut() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let note = if lang == Lang::Es {
        "Los cambios que aún no se subieron quedan guardados en este navegador y se subirán cuando vuelvas a entrar con esta cuenta."
    } else {
        "Changes not yet uploaded stay in this browser and will upload when you sign in again with this account."
    };
    view! {
        <h2>{t.question(t.sign_out)}</h2>
        <p class="muted">{note}</p>
        <div class="row actions">
            <button class="primary" on:click=move |_| { close(state); state.sign_out(); }>{t.sign_out}</button>
            <button on:click=move |_| close(state)>{t.cancel}</button>
        </div>
    }
}

#[allow(dead_code)]
fn date_text(date: chrono::NaiveDate, lang: Lang) -> String {
    format_date(date, lang)
}
