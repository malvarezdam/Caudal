//! Páginas principales.

use super::dialogs::Dialog;
use super::widgets::{self, card, heading, muted, tone_color, transaction_row, uuid_combo};
use crate::app::{CaudalApp, PAGE_SIZE, Page, Session, ThemeChoice};
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::forms::{AccountForm, CategoryForm, ProfileForm, TagForm, TransactionForm};
use caudal_app_state::i18n::FORM_KINDS;
use caudal_app_state::model::TransactionKind;
use caudal_app_state::views::{self, TransactionFilter};
use caudal_app_state::{COMMON_CURRENCIES, Lang, Table, texts};
use eframe::egui::{self, Align, Layout, RichText, Ui};
use uuid::Uuid;

fn open_transaction(app: &mut CaudalApp, id: Uuid) {
    if let Some(tx) = app.data.transaction(id) {
        app.dialog = Some(Dialog::Transaction {
            form: Box::new(TransactionForm::edit(tx, app.lang)),
            error: None,
        });
    }
}

pub fn home(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    let key = app.key();
    let view = app
        .cache
        .dashboard
        .get(key, || views::dashboard(&app.data, today(), lang));
    let pending = app.cache.pending.get(key, || {
        caudal_app_state::recurring::recurring_view(&app.data, today(), lang).pending
    });
    crate::ui::data::notices_card(app, ui);
    crate::ui::debts::pending_card(app, ui, &pending);
    let alerts = app.cache.alerts.get(key, || {
        caudal_app_state::investments::triggered_alerts(&app.data, today(), lang)
    });
    crate::ui::invest::triggered_card(app, ui, &alerts);

    card(ui, |ui| {
        ui.label(muted(ui, texts.net_worth));
        if view.net_worth.is_empty() {
            ui.label(RichText::new("0").size(28.0).strong());
        }
        for total in &view.net_worth {
            ui.label(RichText::new(total).size(28.0).strong());
        }
        if let Some(investments) = &view.investments
            && ui
                .add(egui::Label::new(muted(ui, investments).small()).sense(egui::Sense::click()))
                .clicked()
        {
            app.go(Page::Investments);
        }
        if view.net_worth_incomplete {
            ui.label(
                RichText::new(texts.unconverted_note)
                    .small()
                    .color(widgets::warning_color(ui)),
            );
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.label(muted(ui, texts.this_month));
        ui.columns(3, |columns| {
            let items = [
                (texts.income, &view.month.income, views::Tone::Positive),
                (texts.expense, &view.month.expense, views::Tone::Negative),
                (texts.savings, &view.month.savings, view.month.savings_tone),
            ];
            for (column, (label, value, tone)) in columns.iter_mut().zip(items) {
                column.label(muted(column, label).small());
                column.label(
                    RichText::new(value)
                        .strong()
                        .color(tone_color(column, tone)),
                );
            }
        });
        if view.month.has_unconverted {
            ui.label(
                RichText::new(texts.unconverted_note)
                    .small()
                    .color(widgets::warning_color(ui)),
            );
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(texts.accounts).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button(format!("+ {}", texts.new_account))
                    .clicked()
                {
                    app.dialog = Some(Dialog::Account {
                        form: Box::new(AccountForm::new(&app.data)),
                        error: None,
                    });
                }
            });
        });
        if view.accounts.is_empty() {
            ui.label(muted(ui, texts.no_accounts));
        }
        for account in &view.accounts {
            let response = ui
                .horizontal(|ui| {
                    ui.label(&account.name);
                    ui.label(muted(ui, account.kind_label).small());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(&account.balance_text)
                                .color(tone_color(ui, account.tone)),
                        );
                    });
                })
                .response
                .interact(egui::Sense::click());
            if response.clicked() {
                app.filter = TransactionFilter {
                    account_id: Some(account.id),
                    ..TransactionFilter::default()
                };
                app.go(Page::Transactions);
            }
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(texts.recent).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button(texts.view_all).clicked() {
                    app.filter = TransactionFilter::default();
                    app.go(Page::Transactions);
                }
            });
        });
        if view.recent.is_empty() {
            ui.label(muted(ui, texts.no_transactions));
        }
        let mut open = None;
        for tx in &view.recent {
            if transaction_row(ui, tx, lang).clicked() {
                open = Some(tx.id);
            }
        }
        if let Some(id) = open {
            open_transaction(app, id);
        }
    });
    let key = app.key();
    let budgets = app.cache.budgets.get(key, || {
        caudal_app_state::planning::budgets_view(&app.data, today(), lang)
    });
    if !budgets.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new(texts.budgets).strong());
        crate::ui::reports::budget_cards(app, ui, &budgets, 3);
    }
    ui.add_space(8.0);
    crate::ui::debts::upcoming_card(app, ui, 30, 5);
}

pub fn transactions(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    let data = app.data.clone();
    let wide = ui.available_width() >= 640.0;

    let filters = |app: &mut CaudalApp, ui: &mut Ui| {
        let changed_before = app.filter.clone();
        widgets::text_field(ui, &mut app.filter.text, texts.search);
        ui.horizontal_wrapped(|ui| {
            let accounts: Vec<(Uuid, String)> = data
                .accounts
                .iter()
                .map(|account| (account.meta.id, account.name.clone()))
                .collect();
            uuid_combo(
                ui,
                "filter-account",
                &mut app.filter.account_id,
                Some(texts.all_accounts),
                &accounts,
            );

            let mut category = app.filter.category_id;
            let selected = match category {
                None => texts.all_categories.to_owned(),
                Some(None) => texts.no_category.to_owned(),
                Some(Some(id)) => data.category_path(id).unwrap_or_default(),
            };
            egui::ComboBox::from_id_salt("filter-category")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut category, None, texts.all_categories);
                    ui.selectable_value(&mut category, Some(None), texts.no_category);
                    for (id, path) in data.categories_for(TransactionKind::Transfer) {
                        ui.selectable_value(&mut category, Some(Some(id)), path);
                    }
                });
            app.filter.category_id = category;

            let mut kind = app.filter.kind;
            egui::ComboBox::from_id_salt("filter-kind")
                .selected_text(kind.map_or(texts.all_kinds, |kind| texts.transaction_kind(kind)))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut kind, None, texts.all_kinds);
                    for option in FORM_KINDS.into_iter().chain([TransactionKind::Adjustment]) {
                        ui.selectable_value(
                            &mut kind,
                            Some(option),
                            texts.transaction_kind(option),
                        );
                    }
                });
            app.filter.kind = kind;
        });
        ui.horizontal_wrapped(|ui| {
            for (label, value, salt) in [
                (texts.from, &mut app.filter.from, "filter-from"),
                (texts.to, &mut app.filter.to, "filter-to"),
            ] {
                ui.label(muted(ui, label).small());
                match value {
                    Some(date) => {
                        widgets::date_field(ui, salt, date, lang, today());
                        if ui.small_button("x").clicked() {
                            *value = None;
                        }
                    }
                    None => {
                        if ui.small_button("…").clicked() {
                            *value = Some(today());
                        }
                    }
                }
            }
            if !app.filter.is_empty() && ui.button(texts.clear_filters).clicked() {
                app.filter = TransactionFilter::default();
            }
        });
        if app.filter != changed_before {
            app.limit = PAGE_SIZE;
        }
    };

    if wide {
        heading(ui, texts.transactions);
        filters(app, ui);
    } else {
        egui::CollapsingHeader::new(RichText::new(texts.search).strong())
            .default_open(!app.filter.is_empty())
            .show(ui, |ui| filters(app, ui));
    }
    ui.add_space(6.0);

    let key = (app.key(), app.filter.clone(), app.limit);
    let (list, total) = app.cache.transactions.get(key, || {
        views::transactions(&data, &app.filter, app.limit, lang)
    });
    if list.is_empty() {
        ui.label(muted(ui, texts.no_transactions));
    }
    let mut open = None;
    card(ui, |ui| {
        for (index, tx) in list.iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            if transaction_row(ui, tx, lang).clicked() {
                open = Some(tx.id);
            }
        }
    });
    if let Some(id) = open {
        open_transaction(app, id);
    }
    if total > list.len() {
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            if ui
                .button(format!("{} ({}/{total})", texts.load_more, list.len()))
                .clicked()
            {
                app.limit += PAGE_SIZE;
            }
        });
    }
}

pub fn accounts(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    heading(ui, texts.accounts);
    ui.horizontal(|ui| {
        if ui.button(format!("+ {}", texts.new_account)).clicked() {
            app.dialog = Some(Dialog::Account {
                form: Box::new(AccountForm::new(&app.data)),
                error: None,
            });
        }
        ui.checkbox(&mut app.show_archived, texts.show_archived);
    });
    ui.add_space(6.0);
    let key = (app.key(), app.show_archived);
    let list = app
        .cache
        .accounts
        .get(key, || views::accounts(&app.data, lang, app.show_archived));
    if list.is_empty() {
        ui.label(muted(ui, texts.no_accounts));
    }
    for account in list {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let mut name = RichText::new(&account.name).strong();
                    if account.archived {
                        name = name.italics();
                    }
                    ui.label(name);
                    let mut detail = format!("{} · {}", account.kind_label, account.currency);
                    if account.archived {
                        detail = format!("{detail} · {}", texts.archived);
                    }
                    ui.label(muted(ui, detail).small());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&account.balance_text)
                            .size(17.0)
                            .strong()
                            .color(tone_color(ui, account.tone)),
                    );
                });
            });
            if account.unconverted > 0 {
                ui.label(
                    RichText::new(texts.unconverted_note)
                        .small()
                        .color(widgets::warning_color(ui)),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(texts.transactions).clicked() {
                    app.filter = TransactionFilter {
                        account_id: Some(account.id),
                        ..TransactionFilter::default()
                    };
                    app.go(Page::Transactions);
                }
                if ui.small_button(texts.adjust_balance).clicked() {
                    app.dialog = Some(Dialog::Adjust {
                        account_id: account.id,
                        value: caudal_app_state::format::amount_for_input(account.balance, lang),
                        error: None,
                    });
                }
                if ui.small_button(texts.edit).clicked()
                    && let Some(model) = app.data.account(account.id)
                {
                    app.dialog = Some(Dialog::Account {
                        form: Box::new(AccountForm::edit(model, lang)),
                        error: None,
                    });
                }
                if ui.small_button(texts.delete).clicked() {
                    app.dialog = Some(Dialog::ConfirmDelete {
                        table: Table::Accounts,
                        id: account.id,
                        label: account.name.clone(),
                    });
                }
            });
        });
        ui.add_space(6.0);
    }
}

pub fn categories(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    let data = app.data.clone();
    heading(ui, texts.categories);
    if ui.button(format!("+ {}", texts.new_category)).clicked() {
        app.dialog = Some(Dialog::Category {
            form: Box::new(CategoryForm::new()),
            error: None,
        });
    }
    ui.add_space(6.0);
    card(ui, |ui| {
        if data.categories.is_empty() {
            ui.label(muted(ui, texts.no_categories));
        }
        let roots = data.categories.iter().filter(|category| {
            category
                .parent_id
                .is_none_or(|parent| data.category(parent).is_none())
        });
        for root in roots {
            category_row(app, ui, root.meta.id, &root.name, root.kind, 0.0);
            for child in data
                .categories
                .iter()
                .filter(|c| c.parent_id == Some(root.meta.id))
            {
                category_row(app, ui, child.meta.id, &child.name, child.kind, 18.0);
            }
        }
    });

    ui.add_space(12.0);
    heading(ui, texts.tags);
    if ui.button(format!("+ {}", texts.new_tag)).clicked() {
        app.dialog = Some(Dialog::Tag {
            form: TagForm::default(),
            error: None,
        });
    }
    ui.add_space(6.0);
    card(ui, |ui| {
        if data.tags.is_empty() {
            ui.label(muted(ui, texts.no_tags));
        }
        ui.horizontal_wrapped(|ui| {
            for tag in &data.tags {
                if ui.button(format!("#{}", tag.name)).clicked() {
                    app.dialog = Some(Dialog::Tag {
                        form: TagForm::edit(tag),
                        error: None,
                    });
                }
            }
        });
    });
}

fn category_row(
    app: &mut CaudalApp,
    ui: &mut Ui,
    id: Uuid,
    name: &str,
    kind: caudal_app_state::model::CategoryKind,
    indent: f32,
) {
    let texts = texts(app.lang);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.label(name);
        ui.label(muted(ui, texts.category_kind(kind)).small());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button(texts.delete).clicked() {
                app.dialog = Some(Dialog::ConfirmDelete {
                    table: Table::Categories,
                    id,
                    label: name.to_owned(),
                });
            }
            if ui.small_button(texts.edit).clicked()
                && let Some(category) = app.data.category(id)
            {
                app.dialog = Some(Dialog::Category {
                    form: Box::new(CategoryForm::edit(category)),
                    error: None,
                });
            }
        });
    });
}

pub fn trash(app: &mut CaudalApp, ui: &mut Ui) {
    let texts = texts(app.lang);
    heading(ui, texts.trash);
    ui.label(muted(ui, texts.trash_hint));
    ui.add_space(6.0);
    if app.data.trash.is_empty() {
        ui.label(muted(ui, texts.trash_empty));
        return;
    }
    let mut restore = None;
    card(ui, |ui| {
        for item in views::trash(&app.data, app.lang) {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(&item.label);
                    ui.label(muted(ui, item.kind_label).small());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(texts.restore).clicked() {
                        restore = Some((item.table, item.id));
                    }
                });
            });
            ui.separator();
        }
    });
    if let Some((table, id)) = restore {
        app.send(Command::Restore { table, id });
    }
}

pub fn settings(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let texts = texts(lang);
    let ctx = ui.ctx().clone();
    heading(ui, texts.settings);

    let narrow = ui.available_width() < 720.0;
    if narrow {
        card(ui, |ui| {
            for page in [
                Page::Investments,
                Page::Reports,
                Page::Plans,
                Page::Debts,
                Page::Recurring,
                Page::Categories,
                Page::Rules,
                Page::Activity,
                Page::Trash,
            ] {
                if ui.button(page.label(lang)).clicked() {
                    app.go(page);
                }
            }
        });
        ui.add_space(8.0);
    }

    let mut profile_form = app.data.profile.as_ref().map(ProfileForm::edit);
    card(ui, |ui| {
        if let Session::SignedIn { email, demo } = &app.session {
            if *demo {
                ui.label(RichText::new(texts.demo_mode).color(widgets::warning_color(ui)));
            } else if let Some(email) = email {
                ui.label(muted(ui, texts.signed_in_as).small());
                ui.label(RichText::new(email).strong());
            }
        }
        if let Some(form) = profile_form.as_mut() {
            let before = form.clone();
            widgets::field_label(ui, texts.display_name);
            let name_response = widgets::text_field(ui, &mut form.display_name, "");
            widgets::field_label(ui, texts.base_currency);
            egui::ComboBox::from_id_salt("base-currency")
                .selected_text(form.base_currency.clone())
                .show_ui(ui, |ui| {
                    for code in COMMON_CURRENCIES {
                        ui.selectable_value(&mut form.base_currency, code.to_owned(), code);
                    }
                });
            let changed = form.base_currency != before.base_currency
                || (name_response.lost_focus() && form.display_name != before.display_name);
            if changed && let Ok(profile) = form.validate(app.data.user_id) {
                app.send(Command::Save(Box::new(Item::Profile(profile))));
            }
        }
    });
    ui.add_space(8.0);

    card(ui, |ui| {
        widgets::field_label(ui, texts.language);
        ui.horizontal(|ui| {
            for option in Lang::ALL {
                if ui.selectable_label(lang == option, option.name()).clicked() && option != lang {
                    app.set_lang(option);
                    if let Some(form) = app.data.profile.as_ref().map(ProfileForm::edit) {
                        let form = ProfileForm {
                            lang: option,
                            ..form
                        };
                        if let Ok(profile) = form.validate(app.data.user_id) {
                            app.send(Command::Save(Box::new(Item::Profile(profile))));
                        }
                    }
                }
            }
        });
        widgets::field_label(ui, texts.theme);
        ui.horizontal(|ui| {
            for (choice, label) in [
                (ThemeChoice::System, texts.theme_system),
                (ThemeChoice::Light, texts.theme_light),
                (ThemeChoice::Dark, texts.theme_dark),
            ] {
                if ui.selectable_label(app.theme == choice, label).clicked() {
                    app.set_theme(&ctx, choice);
                }
            }
        });
    });
    ui.add_space(8.0);

    crate::ui::data::settings_cards(app, ui);

    card(ui, |ui| {
        if ui.button(texts.sign_out).clicked() {
            app.dialog = Some(Dialog::ConfirmSignOut);
        }
    });
}

/// Etiquetas de los tipos que se pueden elegir en el formulario.
pub fn kind_options(lang: Lang) -> Vec<(TransactionKind, &'static str)> {
    let texts = texts(lang);
    FORM_KINDS
        .into_iter()
        .map(|kind| (kind, texts.transaction_kind(kind)))
        .collect()
}
