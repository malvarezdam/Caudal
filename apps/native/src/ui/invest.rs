//! Inversiones: cartera, posiciones, operaciones y sus formularios.

use super::charts::{line_chart, ratio_bar, series_color};
use super::dialogs::{Dialog, Outcome, buttons, title};
use super::widgets::{
    amount_field, card, date_field, error_label, field_label, heading, muted, text_field,
    tone_color, uuid_combo, warning_color,
};
use crate::app::CaudalApp;
use crate::platform::today;
use crate::worker::{Command, Item};
use caudal_app_state::catalog;
use caudal_app_state::i18n::{ASSET_KINDS, PROVIDERS, TRADE_KINDS};
use caudal_app_state::investments::{
    AlertForm, AlertView, AssetForm, PositionView, TradeForm, TradeKindFields, portfolio_view,
};
use caudal_app_state::model::{AlertDirection, PriceProviderId};
use caudal_app_state::{Data, Lang, Table, texts};
use eframe::egui::{self, Align, Layout, RichText, Ui};

pub fn investments(app: &mut CaudalApp, ui: &mut Ui) {
    let lang = app.lang;
    let t = texts(lang);
    let data = app.data.clone();
    heading(ui, t.investments);
    ui.horizontal_wrapped(|ui| {
        if ui.button(format!("+ {}", t.new_asset)).clicked() {
            app.dialog = Some(Dialog::Asset {
                form: Box::new(AssetForm::new(&data, today())),
                query: String::new(),
                error: None,
            });
        }
        let has_assets = data.assets.iter().any(|asset| !asset.meta.is_deleted());
        if has_assets && ui.button(format!("+ {}", t.record_trade)).clicked() {
            app.dialog = Some(Dialog::Trade {
                form: Box::new(TradeForm::new(None, today())),
                error: None,
            });
        }
    });
    ui.add_space(6.0);
    let key = app.key();
    let view = app
        .cache
        .portfolio
        .get(key, || portfolio_view(&data, today(), lang));
    if view.is_empty {
        ui.label(muted(ui, t.no_assets));
        return;
    }
    triggered_card(app, ui, &view.triggered);

    card(ui, |ui| {
        ui.label(muted(ui, t.market_value).small());
        ui.label(RichText::new(&view.total).strong().size(24.0));
        egui::Grid::new("portfolio-totals")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.label(muted(ui, t.cost));
                ui.label(&view.cost);
                ui.end_row();
                ui.label(muted(ui, t.unrealized));
                ui.label(
                    RichText::new(&view.unrealized).color(tone_color(ui, view.unrealized_tone)),
                );
                ui.end_row();
                ui.label(muted(ui, t.realized));
                ui.label(&view.realized);
                ui.end_row();
            });
        if let Some(note) = &view.note {
            ui.label(RichText::new(note).small().color(warning_color(ui)));
        }
    });
    ui.add_space(6.0);

    if !view.chart.is_empty() {
        card(ui, |ui| {
            ui.label(RichText::new(t.portfolio_chart).strong());
            line_chart(ui, &view.chart, 170.0);
        });
        ui.add_space(6.0);
    }

    if view.allocation.len() > 1 {
        card(ui, |ui| {
            ui.label(RichText::new(t.allocation).strong());
            for row in &view.allocation {
                ui.horizontal(|ui| {
                    ui.label(row.label);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted(ui, &row.percent).small());
                        ui.label(&row.value);
                    });
                });
                ratio_bar(ui, row.share, series_color(ui, 0));
            }
        });
        ui.add_space(6.0);
    }

    for position in &view.positions {
        position_card(app, ui, &data, position);
        ui.add_space(6.0);
    }
    if !view.closed.is_empty() {
        egui::CollapsingHeader::new(format!("{} ({})", t.closed_positions, view.closed.len()))
            .id_salt("closed-positions")
            .show(ui, |ui| {
                for position in &view.closed {
                    position_card(app, ui, &data, position);
                    ui.add_space(6.0);
                }
            });
    }
}

fn position_card(app: &mut CaudalApp, ui: &mut Ui, data: &Data, position: &PositionView) {
    let lang = app.lang;
    let t = texts(lang);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&position.name).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(position.value.as_deref().unwrap_or(t.no_price)).strong());
            });
        });
        ui.horizontal(|ui| {
            ui.label(muted(ui, &position.subtitle).small());
            if let (Some(gain), Some(pct)) = (&position.unrealized, &position.return_pct) {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{gain} ({pct})"))
                            .small()
                            .color(tone_color(ui, position.unrealized_tone)),
                    );
                });
            }
        });
        ui.label(format!(
            "{} {} · {}",
            t.quantity, position.quantity, position.price
        ));
        let mut cost_line = format!("{} {}", t.cost, position.cost);
        if let Some(average) = &position.average_cost {
            cost_line = format!("{cost_line} · {} {average}", t.average_cost);
        }
        ui.label(muted(ui, cost_line).small());
        if let Some(realized) = &position.realized {
            ui.label(muted(ui, format!("{} {realized}", t.realized)).small());
        }
        if let Some(note) = &position.price_note {
            ui.label(RichText::new(note).small().color(warning_color(ui)));
        }
        if position.oversold {
            ui.label(RichText::new(t.oversold).small().color(warning_color(ui)));
        }
        for alert in &position.alerts {
            ui.horizontal(|ui| {
                let color = if alert.triggered {
                    warning_color(ui)
                } else {
                    ui.visuals().weak_text_color()
                };
                ui.label(
                    RichText::new(format!("{}: {}", t.alerts, alert.condition))
                        .small()
                        .color(color),
                );
                if ui.small_button(t.edit).clicked()
                    && let Some(model) = data.alerts.iter().find(|a| a.meta.id == alert.id)
                {
                    app.dialog = Some(Dialog::Alert {
                        form: Box::new(AlertForm::edit(model, lang)),
                        error: None,
                    });
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            if !position.manual && ui.small_button(format!("+ {}", t.new_alert)).clicked() {
                app.dialog = Some(Dialog::Alert {
                    form: Box::new(AlertForm::new(position.id)),
                    error: None,
                });
            }
            if ui.small_button(t.record_trade).clicked() {
                app.dialog = Some(Dialog::Trade {
                    form: Box::new(TradeForm::new(Some(position.id), today())),
                    error: None,
                });
            }
            let label = if position.manual {
                t.manual_price
            } else {
                t.edit
            };
            if ui.small_button(label).clicked()
                && let Some(asset) = data.asset(position.id)
            {
                app.dialog = Some(Dialog::Asset {
                    form: Box::new(AssetForm::edit(asset, today(), lang)),
                    query: String::new(),
                    error: None,
                });
            }
        });
        if !position.trades.is_empty() {
            egui::CollapsingHeader::new(
                muted(ui, format!("{} ({})", t.trades, position.trades.len())).small(),
            )
            .id_salt(("trades", position.id))
            .show(ui, |ui| {
                for row in &position.trades {
                    ui.horizontal(|ui| {
                        ui.label(muted(ui, &row.date).small());
                        ui.label(RichText::new(row.kind).small().strong());
                        ui.label(RichText::new(&row.detail).small());
                        if let Some(account) = &row.account {
                            ui.label(muted(ui, account).small());
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button(t.edit).clicked()
                                && let Some(trade) =
                                    data.trades.iter().find(|trade| trade.meta.id == row.id)
                            {
                                app.dialog = Some(Dialog::Trade {
                                    form: Box::new(TradeForm::edit(trade, lang)),
                                    error: None,
                                });
                            }
                        });
                    });
                }
            });
        }
    });
}

pub fn asset_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut AssetForm,
    query: &mut String,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_asset
        } else {
            t.new_asset
        },
    );
    if editing.is_none() {
        field_label(ui, t.search_asset);
        text_field(ui, query, "AAPL, VOO, bitcoin…");
        for entry in catalog::search(query) {
            let label = format!("{} · {} · {}", entry.ticker, entry.name, entry.currency);
            if ui
                .add(egui::Button::new(RichText::new(label).small()).frame(false))
                .clicked()
            {
                form.fill_from(&entry);
                query.clear();
            }
        }
        ui.label(muted(ui, t.manual_asset).small());
    }
    field_label(ui, t.name);
    text_field(ui, &mut form.name, "");
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("asset-kind")
            .selected_text(t.asset_kind(form.kind))
            .show_ui(ui, |ui| {
                for kind in ASSET_KINDS {
                    ui.selectable_value(&mut form.kind, kind, t.asset_kind(kind));
                }
            });
        ui.add_sized([80.0, 30.0], egui::TextEdit::singleline(&mut form.currency));
    });
    field_label(ui, t.provider);
    egui::ComboBox::from_id_salt("asset-provider")
        .selected_text(t.provider(form.provider))
        .width(ui.available_width().min(320.0))
        .show_ui(ui, |ui| {
            for provider in PROVIDERS {
                ui.selectable_value(&mut form.provider, provider, t.provider(provider));
            }
        });
    if form.provider != PriceProviderId::Manual {
        field_label(ui, t.symbol);
        text_field(ui, &mut form.symbol, "AAPL.US");
    }
    field_label(ui, &format!("{} ({})", t.manual_price, form.currency));
    amount_field(ui, &mut form.manual_price, "", false);
    if !form.manual_price.trim().is_empty() {
        date_field(
            ui,
            "asset-price-date",
            &mut form.manual_price_on,
            lang,
            today(),
        );
    }
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::Assets,
            id: original.meta.id,
            label: original.name,
        });
    }
    if save {
        return match form.validate(data, lang) {
            Ok(asset) => Outcome::Send(Command::Save(Box::new(Item::Asset(asset)))),
            Err(problem) => {
                *error = Some(problem.message(t).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

pub fn trade_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut TradeForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(ui, t.record_trade);
    let assets: Vec<(uuid::Uuid, String)> = data
        .assets
        .iter()
        .filter(|asset| !asset.meta.is_deleted())
        .map(|asset| (asset.meta.id, asset.name.clone()))
        .collect();
    field_label(ui, t.asset);
    if uuid_combo(ui, "trade-asset", &mut form.asset_id, None, &assets) {
        form.account_id = None;
    }
    let currency = form
        .asset_id
        .and_then(|id| data.asset(id))
        .map(|asset| asset.currency.to_string())
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("trade-kind")
            .selected_text(t.trade_kind(form.kind))
            .show_ui(ui, |ui| {
                for kind in TRADE_KINDS {
                    ui.selectable_value(&mut form.kind, kind, t.trade_kind(kind));
                }
            });
    });
    field_label(ui, t.date);
    date_field(ui, "trade-date", &mut form.date, lang, today());
    if form.kind.uses_quantity() {
        field_label(ui, t.quantity);
        amount_field(ui, &mut form.quantity, "0", true);
    }
    if form.kind.uses_price() {
        let label = if form.kind.price_is_total() {
            t.total_amount
        } else {
            t.unit_price
        };
        field_label(ui, &format!("{label} ({currency})"));
        amount_field(ui, &mut form.price, "0", false);
        field_label(ui, &format!("{} ({currency})", t.fee));
        amount_field(ui, &mut form.fee, "0", false);
    } else {
        field_label(ui, t.split_ratio);
        amount_field(ui, &mut form.split_ratio, "2", false);
    }
    if form.kind.moves_money() {
        field_label(ui, t.pay_from);
        let accounts = form.accounts(data);
        uuid_combo(
            ui,
            "trade-account",
            &mut form.account_id,
            Some(t.none),
            &accounts,
        );
    }
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        let name = data
            .asset(original.asset_id)
            .map(|asset| asset.name.clone())
            .unwrap_or_default();
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::Trades,
            id: original.meta.id,
            label: format!("{}: {name}", t.trade_kind(original.kind)),
        });
    }
    if save {
        return match form.validate(data, lang) {
            Ok(draft) => Outcome::Send(Command::Save(Box::new(Item::Trade(draft)))),
            Err(problem) => {
                *error = Some(problem.message(t).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}

/// Alertas cumplidas, en Inversiones y en el inicio. "Entendido" la pausa.
pub fn triggered_card(app: &mut CaudalApp, ui: &mut Ui, alerts: &[AlertView]) {
    if alerts.is_empty() {
        return;
    }
    let t = texts(app.lang);
    card(ui, |ui| {
        ui.label(
            RichText::new(format!("{} ({})", t.alert_hit, alerts.len()))
                .strong()
                .color(warning_color(ui)),
        );
        for alert in alerts {
            ui.horizontal_wrapped(|ui| {
                ui.label(&alert.text);
                if ui.small_button(t.dismiss).clicked()
                    && let Some(model) = app.data.alerts.iter().find(|a| a.meta.id == alert.id)
                {
                    app.send(Command::Save(Box::new(Item::Alert(AlertForm::dismissed(
                        model,
                    )))));
                }
            });
        }
    });
    ui.add_space(6.0);
}

pub fn alert_dialog(
    ui: &mut Ui,
    data: &Data,
    form: &mut AlertForm,
    error: &mut Option<String>,
    lang: Lang,
) -> Outcome {
    let t = texts(lang);
    let editing = form.original.clone();
    title(
        ui,
        if editing.is_some() {
            t.edit_alert
        } else {
            t.new_alert
        },
    );
    let asset = data.asset(form.asset_id);
    ui.label(asset.map(|asset| asset.name.as_str()).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut form.direction, AlertDirection::Above, t.alert_above);
        ui.selectable_value(&mut form.direction, AlertDirection::Below, t.alert_below);
    });
    let currency = asset
        .map(|asset| asset.currency.to_string())
        .unwrap_or_default();
    field_label(ui, &format!("{} ({currency})", t.target_price));
    amount_field(ui, &mut form.target_price, "0", true);
    ui.checkbox(&mut form.active, t.alert_active);
    field_label(ui, t.note);
    text_field(ui, &mut form.note, "");
    error_label(ui, error);
    let (save, cancel, delete) = buttons(ui, lang, editing.is_some());
    if cancel {
        return Outcome::Close;
    }
    if delete && let Some(original) = editing {
        return Outcome::Replace(Dialog::ConfirmDelete {
            table: Table::PriceAlerts,
            id: original.meta.id,
            label: t.alerts.to_owned(),
        });
    }
    if save {
        return match form.validate(data, lang) {
            Ok(alert) => Outcome::Send(Command::Save(Box::new(Item::Alert(alert)))),
            Err(problem) => {
                *error = Some(problem.message(t).into());
                Outcome::Keep
            }
        };
    }
    Outcome::Keep
}
