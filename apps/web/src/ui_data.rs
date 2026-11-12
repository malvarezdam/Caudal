//! Datos del usuario en la web: copia completa, CSV, reglas, plantillas,
//! comprobantes, actividad, avisos y privacidad.

use crate::state::{Dialog, Item, Page, today};
use crate::ui::{close, dialog_buttons, parse_uuid, state, uuid_options};
use caudal_app_state::csv_io::{
    CsvTable, DateOrder, ImportMapping, ImportPreview, RowStatus, preview,
};
use caudal_app_state::i18n::FORM_KINDS;
use caudal_app_state::model::TransactionKind;
use caudal_app_state::notices::{Action, NoticeTarget, activity, notices};
use caudal_app_state::rules::{RuleForm, TemplateForm, rules_view, templates_view};
use caudal_app_state::views::Tone;
use caudal_app_state::{Data, Lang, Table, privacy, texts};
use leptos::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportState {
    pub table: CsvTable,
    pub mapping: ImportMapping,
    pub preview: ImportPreview,
}

impl ImportState {
    pub fn new(data: &Data, table: CsvTable, lang: Lang) -> Self {
        let mapping = ImportMapping::guess(&table, data);
        let preview = preview(data, &table, &mapping, lang);
        Self {
            table,
            mapping,
            preview,
        }
    }
}

fn pick_csv() {
    let state = state();
    crate::files::pick(".csv,.txt,text/csv", move |_, bytes| {
        let lang = state.lang.get_untracked();
        match CsvTable::parse(&bytes) {
            Ok(table) => state.open_dialog(Dialog::Import(Box::new(ImportState::new(
                &state.data.get_untracked(),
                table,
                lang,
            )))),
            Err(_) => state.error.set(Some(texts(lang).csv_unreadable.to_owned())),
        }
    });
}

fn pick_backup() {
    let state = state();
    crate::files::pick(
        ".json,application/json",
        move |_, bytes| match String::from_utf8(bytes) {
            Ok(text) => state.restore_backup(text),
            Err(_) => state.error.set(Some(
                texts(state.lang.get_untracked()).backup_invalid.to_owned(),
            )),
        },
    );
}

/// Abre el selector para adjuntar un comprobante al movimiento.
pub fn pick_attachment(transaction_id: Uuid) {
    let state = state();
    crate::files::pick("image/*,application/pdf", move |name, bytes| {
        let data = state.data.get_untracked();
        match caudal_app_state::attachments::new_attachment(&data, transaction_id, &name, &bytes) {
            Ok(attachment) => state.add_attachment(attachment, bytes),
            Err(problem) => state
                .error
                .set(Some(problem.message(state.lang.get_untracked()).to_owned())),
        }
    });
}

#[component]
pub fn DataCards() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <section class="card">
            <h3>{t.data_section}</h3>
            <div class="row wrap">
                <button on:click=move |_| state.export_backup()>{t.export_backup}</button>
                <button on:click=move |_| pick_backup()>{t.restore_backup}</button>
            </div>
            <div class="row wrap">
                <button on:click=move |_| {
                    let text = caudal_app_state::csv_io::export_transactions(&state.data.get_untracked(), lang);
                    crate::files::download(&format!("caudal-movimientos-{}.csv", today()), "text/csv", text.as_bytes());
                }>{t.export_csv}</button>
                <button on:click=move |_| pick_csv()>{t.import_csv}</button>
            </div>
            <div class="row wrap">
                <button on:click=move |_| state.go(Page::Rules)>{t.rules_title}</button>
                <button on:click=move |_| state.go(Page::Activity)>{t.activity}</button>
            </div>
            <button class="danger" on:click=move |_| state.open_dialog(Dialog::ConfirmDeleteAll)>{t.delete_all}</button>
        </section>
        <section class="card">
            <h3>{t.privacy}</h3>
            <label class="check"><input type="checkbox" prop:checked=move || state.privacy.get().private
                on:change=move |ev| state.set_private(event_target_checked(&ev))/>{t.private_mode}</label>
            <p class="muted small">{t.pin_hint}</p>
            {move || if state.privacy.get().pin_hash.is_some() {
                view! {
                    <div class="row wrap">
                        <button on:click=move |_| state.locked.set(true)>{t.lock_now}</button>
                        <button on:click=move |_| state.set_pin(None)>{t.remove_pin}</button>
                    </div>
                }.into_any()
            } else {
                view! { <button on:click=move |_| state.open_dialog(Dialog::SetPin(String::new()))>{t.set_pin}</button> }.into_any()
            }}
        </section>
    }
}

#[component]
pub fn LockScreen() -> impl IntoView {
    let state = state();
    let t = texts(state.lang.get_untracked());
    let pin = RwSignal::new(String::new());
    let wrong = RwSignal::new(false);
    let unlock = move || {
        let privacy = state.privacy.get_untracked();
        if let (Some(hash), Some(salt)) = (privacy.pin_hash, privacy.pin_salt) {
            if privacy::verify_pin(&pin.get_untracked(), &salt, &hash) {
                state.locked.set(false);
                wrong.set(false);
            } else {
                wrong.set(true);
            }
        }
        pin.set(String::new());
    };
    view! {
        <div class="lock">
            <h1 class="brand big">{t.app_name}</h1>
            <label>{t.enter_pin}
                <input type="password" inputmode="numeric" autofocus prop:value=move || pin.get()
                    on:input=move |ev| pin.set(event_target_value(&ev))
                    on:keydown=move |ev| if ev.key() == "Enter" { unlock() }/>
            </label>
            <button class="primary" on:click=move |_| unlock()>{t.unlock}</button>
            {move || wrong.get().then(|| view! { <p class="neg">{t.wrong_pin}</p> })}
            <p class="muted small">{t.forgot_pin}</p>
            <button class="link" on:click=move |_| {
                state.set_pin(None);
                state.locked.set(false);
                state.sign_out();
            }>{t.sign_out}</button>
        </div>
    }
}

#[component]
pub fn Notices() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    move || {
        let list = notices(&state.data.get(), today(), lang);
        (!list.is_empty()).then(|| view! {
            <section class="card">
                <p class="warn"><strong>{t.notices}</strong></p>
                {list.into_iter().map(|notice| {
                    let page = match notice.target {
                        NoticeTarget::Debts => Page::Debts,
                        NoticeTarget::Plans => Page::Plans,
                        NoticeTarget::Investments => Page::Investments,
                    };
                    view! {
                        <button class=if notice.tone == Tone::Negative { "line neg" } else { "line" }
                            on:click=move |_| state.go(page)>{notice.text}</button>
                    }
                }).collect_view()}
            </section>
        })
    }
}

#[component]
pub fn RulesPage() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2>{t.rules_title}</h2>
        <p class="muted small">{t.rule_hint}</p>
        <button on:click=move |_| state.open_dialog(Dialog::Rule(RuleForm::new()))>"+ " {t.new_rule}</button>
        {move || rules_view(&state.data.get()).into_iter().map(|rule| {
            let id = rule.id;
            view! {
                <section class="card row between">
                    <span class:muted=!rule.active><strong>{format!("\"{}\"", rule.pattern)}</strong>{format!(" » {}", rule.category)}</span>
                    <button class="small" on:click=move |_| {
                        if let Some(model) = state.data.get_untracked().category_rules.iter().find(|r| r.meta.id == id) {
                            state.open_dialog(Dialog::Rule(RuleForm::edit(model)));
                        }
                    }>{t.edit}</button>
                </section>
            }
        }).collect_view()}
        <h2>{t.templates_title}</h2>
        <button on:click=move |_| {
            let base = caudal_app_state::forms::TransactionForm::new(&state.data.get_untracked(), today());
            state.open_dialog(Dialog::Template(TemplateForm::from_transaction(&base)));
        }>"+ " {t.new_template}</button>
        {move || templates_view(&state.data.get(), lang).into_iter().map(|template| {
            let id = template.id;
            view! {
                <section class="card row between">
                    <span><strong>{template.name.clone()}</strong>" "<span class="muted small">{template.detail.clone()}</span></span>
                    <button class="small" on:click=move |_| {
                        if let Some(model) = state.data.get_untracked().templates.iter().find(|m| m.meta.id == id) {
                            state.open_dialog(Dialog::Template(TemplateForm::edit(model, lang)));
                        }
                    }>{t.edit}</button>
                </section>
            }
        }).collect_view()}
    }
}

#[component]
pub fn ActivityPage() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let offset = -(js_sys::Date::new_0().get_timezone_offset() as i32);
    view! {
        <h2>{t.activity}</h2>
        <section class="card">
            {move || activity(&state.data.get(), 100, offset, lang).into_iter().map(|item| view! {
                <p class="small activity">
                    <span class="muted">{item.when}</span>" "
                    <strong class:neg=item.action == Action::Deleted>{item.action_label}</strong>" "
                    <span class="muted">{item.kind_label}</span>" "
                    {item.label}
                </p>
            }).collect_view()}
        </section>
    }
}

fn read<T: Clone + 'static>(pick: fn(&Dialog) -> Option<&T>) -> impl Fn() -> Option<T> + Copy {
    let state = state();
    move || {
        state
            .dialog
            .with(|dialog| dialog.as_ref().and_then(pick).cloned())
    }
}

#[component]
pub fn ConfirmDeleteAllDialog() -> impl IntoView {
    let state = state();
    let t = texts(state.lang.get_untracked());
    view! {
        <h2>{t.question(t.delete_all)}</h2>
        <p class="muted">{t.delete_all_confirm}</p>
        <div class="row">
            <button class="danger" on:click=move |_| { close(state); state.delete_all(); }>{t.delete_all}</button>
            <button on:click=move |_| close(state)>{t.cancel}</button>
        </div>
    }
}

#[component]
pub fn SetPinDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let pin = read(|d| match d {
        Dialog::SetPin(pin) => Some(pin),
        _ => None,
    });
    let save = move || {
        let value = pin().unwrap_or_default();
        if !privacy::valid_pin(&value) {
            state.dialog_error.set(Some(t.pin_invalid.into()));
            return;
        }
        let salt = privacy::new_salt();
        state.set_pin(Some((privacy::hash_pin(&value, &salt), salt)));
        close(state);
    };
    view! {
        <h2>{t.set_pin}</h2>
        <p class="muted">{t.pin_hint}</p>
        <input type="password" inputmode="numeric" autofocus prop:value=move || pin().unwrap_or_default()
            on:input=move |ev| {
                let value: String = event_target_value(&ev).chars().filter(char::is_ascii_digit).collect();
                state.dialog.set(Some(Dialog::SetPin(value)));
            }/>
        {dialog_buttons(state, lang, save, None)}
    }
}

fn category_options(data: &Data) -> Vec<(Uuid, String)> {
    let mut list = data.categories_for(TransactionKind::Expense);
    for item in data.categories_for(TransactionKind::Income) {
        if !list.iter().any(|(id, _)| *id == item.0) {
            list.push(item);
        }
    }
    list
}

#[component]
pub fn RuleDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Rule(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut RuleForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Rule(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let options = category_options(&data);
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data) {
            Ok(rule) => {
                close(state);
                state.save(Item::CategoryRule(rule));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|rule| {
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::CategoryRules,
                id: rule.meta.id,
                label: rule.pattern.clone(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit_rule } else { t.new_rule }}</h2>
        <label>{t.rule_pattern}<input placeholder="uber" prop:value=move || form().map(|f| f.pattern).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.pattern = v.clone()) }/></label>
        <label>{t.category}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.category_id.is_none())>"—"</option>
                {uuid_options(options, move || form().and_then(|f| f.category_id))}
            </select>
        </label>
        <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.active)
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.active = v) }/>{t.alert_active}</label>
        <p class="muted small">{t.rule_hint}</p>
        {dialog_buttons(state, lang, save, delete)}
    }
}

#[component]
pub fn TemplateDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Template(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut TemplateForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Template(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let accounts: Vec<(Uuid, String)> = data
        .selectable_accounts(None)
        .into_iter()
        .map(|a| (a.meta.id, a.name.clone()))
        .collect();
    let category_data = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(template) => {
                close(state);
                state.save(Item::Template(template));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|template| {
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::TransactionTemplates,
                id: template.meta.id,
                label: template.name.clone(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit_template } else { t.new_template }}</h2>
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <div class="chips">
            {FORM_KINDS.into_iter().map(|option| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.kind == option) on:click=move |_| edit(&|f| f.kind = option)>{t.transaction_kind(option)}</button>
            }).collect_view()}
        </div>
        <label>{t.amount}</label>
        <div class="row amount-row">
            <input class="amount" inputmode="decimal" prop:value=move || form().map(|f| f.amount).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.amount = v.clone()) }/>
            <input class="currency" prop:value=move || form().map(|f| f.currency).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.account}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.account_id.is_none())>{t.no_account}</option>
                {uuid_options(accounts, move || form().and_then(|f| f.account_id))}
            </select>
        </label>
        {move || {
            let kind = form().map_or(TransactionKind::Expense, |f| f.kind);
            (kind != TransactionKind::Transfer).then(|| {
                let options = category_data.categories_for(kind);
                view! {
                    <label>{t.category}
                        <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.category_id = value) }>
                            <option value="" prop:selected=move || form().is_some_and(|f| f.category_id.is_none())>{t.no_category}</option>
                            {uuid_options(options, move || form().and_then(|f| f.category_id))}
                        </select>
                    </label>
                }
            })
        }}
        <label>{t.description}<input prop:value=move || form().map(|f| f.description).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.description = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, delete)}
    }
}

fn column_select(
    label: &'static str,
    none: &'static str,
    headers: Vec<String>,
    get: impl Fn() -> Option<usize> + Copy + Send + Sync + 'static,
    set: impl Fn(Option<usize>) + Copy + 'static,
) -> impl IntoView {
    view! {
        <label>{label}
            <select on:change=move |ev| set(event_target_value(&ev).parse().ok())>
                <option value="" prop:selected=move || get().is_none()>{none}</option>
                {headers.into_iter().enumerate().map(|(index, header)| view! {
                    <option value=index.to_string() prop:selected=move || get() == Some(index)>
                        {if header.is_empty() { format!("#{}", index + 1) } else { header }}
                    </option>
                }).collect_view()}
            </select>
        </label>
    }
}

#[component]
pub fn ImportDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let current = read(|d| match d {
        Dialog::Import(s) => Some(s.as_ref()),
        _ => None,
    });
    let Some(initial) = current() else {
        return ().into_any();
    };
    let preview_data = StoredValue::new(data.clone());
    // Cada cambio de configuración recalcula la vista previa.
    let edit = move |change: &dyn Fn(&mut ImportMapping)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Import(s)) = d {
                change(&mut s.mapping);
                s.preview = preview_data.with_value(|d| preview(d, &s.table, &s.mapping, lang));
            }
        })
    };
    let mapping = move || current().map(|s| s.mapping).unwrap_or_default();
    let headers = initial.table.headers.clone();
    let accounts: Vec<(Uuid, String)> = data
        .selectable_accounts(None)
        .into_iter()
        .map(|a| (a.meta.id, format!("{} ({})", a.name, a.currency)))
        .collect();
    let account_data = data.clone();
    view! {
        <h2>{t.import_csv}</h2>
        <div class="grid2">
            {column_select(t.column_date, t.none, headers.clone(), move || mapping().date, move |v| edit(&|m| m.date = v))}
            {column_select(t.column_amount, t.none, headers.clone(), move || mapping().amount, move |v| edit(&|m| m.amount = v))}
            {column_select(t.column_debit, t.none, headers.clone(), move || mapping().debit, move |v| edit(&|m| m.debit = v))}
            {column_select(t.column_credit, t.none, headers.clone(), move || mapping().credit, move |v| edit(&|m| m.credit = v))}
            {column_select(t.column_description, t.none, headers.clone(), move || mapping().description, move |v| edit(&|m| m.description = v))}
            {column_select(t.column_category, t.none, headers, move || mapping().category, move |v| edit(&|m| m.category = v))}
        </div>
        <div class="chips">
            <button class="chip" class:active=move || mapping().date_order == DateOrder::DayFirst on:click=move |_| edit(&|m| m.date_order = DateOrder::DayFirst)>{t.day_first}</button>
            <button class="chip" class:active=move || mapping().date_order == DateOrder::MonthFirst on:click=move |_| edit(&|m| m.date_order = DateOrder::MonthFirst)>{t.month_first}</button>
        </div>
        <label class="check"><input type="checkbox" prop:checked=move || mapping().invert
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|m| m.invert = v) }/>{t.invert_sign}</label>
        <label>{t.target_account}
            <select on:change=move |ev| {
                let value = parse_uuid(&event_target_value(&ev));
                let currency = value.and_then(|id| account_data.account(id)).map(|a| a.currency.to_string());
                edit(&|m| { m.account_id = value; if let Some(c) = &currency { m.currency = c.clone(); } });
            }>
                <option value="" prop:selected=move || mapping().account_id.is_none()>{t.no_account}</option>
                {uuid_options(accounts, move || mapping().account_id)}
            </select>
        </label>
        <label>{t.currency}<input class="currency" prop:value=move || mapping().currency
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|m| m.currency = v.clone()) }/></label>
        <label class="check"><input type="checkbox" prop:checked=move || mapping().include_duplicates
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|m| m.include_duplicates = v) }/>{t.include_duplicates}</label>
        {move || current().map(|s| {
            let p = s.preview;
            let count = p.transactions.len();
            let rows = p.rows.len();
            view! {
                <p>{format!("{} {} · {} {} · {} {}", p.new, t.import_new, p.duplicates, t.import_duplicates, p.invalid, t.import_invalid)}</p>
                <div class="preview">
                    {p.rows.into_iter().take(40).map(|row| {
                        let class = match row.status {
                            RowStatus::New => "",
                            RowStatus::Duplicate => "muted",
                            RowStatus::Invalid => "neg",
                        };
                        let text = match &row.category {
                            Some(category) => format!("{} · {category}", row.description),
                            None => row.description.clone(),
                        };
                        view! { <p class=format!("small {class}")>{format!("{}  {}  {text}", row.date, row.amount)}</p> }
                    }).collect_view()}
                    {(rows > 40).then(|| view! { <p class="muted small">{format!("… {}", rows - 40)}</p> })}
                </div>
                <div class="row">
                    <button class="primary" disabled=count == 0 on:click=move |_| {
                        let transactions = current().map(|s| s.preview.transactions).unwrap_or_default();
                        close(state);
                        state.import(transactions);
                    }>{format!("{} {count}", t.import_action)}</button>
                    <button on:click=move |_| close(state)>{t.cancel}</button>
                </div>
            }
        })}
    }
    .into_any()
}

/// Comprobantes de un movimiento ya guardado, dentro de su formulario.
pub fn attachments_section(transaction_id: Uuid, lang: Lang) -> impl IntoView {
    let state = state();
    let t = texts(lang);
    view! {
        <label>{t.attachments_title}</label>
        {move || {
            let pending = state.pending_attachments.get();
            state.data.get().attachments_for(transaction_id).into_iter().cloned().map(|attachment| {
                let mut name = format!("{} ({} KB)", attachment.file_name, (attachment.size_bytes + 1023) / 1024);
                if pending.contains(&attachment.meta.id) {
                    name = format!("{name} · {}", t.attachment_pending);
                }
                view! {
                    <button class="link" on:click=move |_| {
                        state.open_dialog(Dialog::Attachment { attachment: attachment.clone(), url: None, loaded: false });
                        state.load_attachment(attachment.clone());
                    }>{name}</button>
                }
            }).collect_view()
        }}
        <button class="small" on:click=move |_| pick_attachment(transaction_id)>"+ " {t.attach}</button>
    }
}

#[component]
pub fn AttachmentDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let current = move || {
        state.dialog.with(|d| match d {
            Some(Dialog::Attachment {
                attachment,
                url,
                loaded,
            }) => Some((attachment.clone(), url.clone(), *loaded)),
            _ => None,
        })
    };
    view! {
        {move || current().map(|(attachment, url, loaded)| {
            let id = attachment.meta.id;
            let name = attachment.file_name.clone();
            let is_image = attachment.mime_type.starts_with("image/") && attachment.mime_type != "image/heic";
            view! {
                <h2>{attachment.file_name.clone()}</h2>
                {match (url, loaded) {
                    (_, false) => view! { <p class="muted">"…"</p> }.into_any(),
                    (None, true) => view! { <p class="muted">{t.attachment_unavailable}</p> }.into_any(),
                    (Some(url), true) => view! {
                        {is_image.then(|| view! { <img class="receipt" src=url.clone() alt=name.clone()/> })}
                        <div class="row">
                            <a class="button" href=url.clone() target="_blank" rel="noopener">{t.open_file}</a>
                            <a class="button" href=url download=name.clone()>{t.save_copy}</a>
                        </div>
                    }.into_any(),
                }}
                <div class="row between">
                    <button on:click=move |_| close(state)>{t.back}</button>
                    <button class="danger" on:click=move |_| state.open_dialog(Dialog::ConfirmDelete {
                        table: Table::Attachments,
                        id,
                        label: attachment.file_name.clone(),
                    })>{t.delete}</button>
                </div>
            }
        })}
    }
}
