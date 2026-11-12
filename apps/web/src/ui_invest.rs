//! Inversiones en la web: cartera, posiciones, operaciones y formularios.

use crate::state::{Dialog, Item, today};
use crate::ui::{close, dialog_buttons, parse_uuid, state, tone_class, uuid_options};
use crate::ui_reports::LineChart;
use caudal_app_state::catalog;
use caudal_app_state::i18n::{ASSET_KINDS, PROVIDERS, TRADE_KINDS};
use caudal_app_state::investments::{
    AlertForm, AlertView, AssetForm, PositionView, TradeForm, TradeKindFields, portfolio_view,
};
use caudal_app_state::model::{AlertDirection, PriceProviderId};
use caudal_app_state::{Lang, Table, texts};
use leptos::prelude::*;

fn position_card(position: PositionView, lang: Lang) -> impl IntoView {
    let state = state();
    let t = texts(lang);
    let id = position.id;
    let edit_label = if position.manual {
        t.manual_price
    } else {
        t.edit
    };
    let gain = position
        .unrealized
        .clone()
        .zip(position.return_pct.clone())
        .map(|(gain, pct)| format!("{gain} ({pct})"));
    let mut cost_line = format!("{} {}", t.cost, position.cost);
    if let Some(average) = &position.average_cost {
        cost_line = format!("{cost_line} · {} {average}", t.average_cost);
    }
    let trade_count = position.trades.len();
    view! {
        <section class="card">
            <div class="row between">
                <strong>{position.name.clone()}</strong>
                <strong>{position.value.clone().unwrap_or_else(|| t.no_price.to_owned())}</strong>
            </div>
            <div class="row between">
                <span class="muted small">{position.subtitle.clone()}</span>
                {gain.map(|gain| view! { <span class=format!("small {}", tone_class(position.unrealized_tone))>{gain}</span> })}
            </div>
            <p>{format!("{} {} · {}", t.quantity, position.quantity, position.price)}</p>
            <p class="muted small">{cost_line}</p>
            {position.realized.clone().map(|realized| view! { <p class="muted small">{format!("{} {realized}", t.realized)}</p> })}
            {position.price_note.clone().map(|note| view! { <p class="warn small">{note}</p> })}
            {position.oversold.then(|| view! { <p class="warn small">{t.oversold}</p> })}
            {position.alerts.iter().map(|alert| {
                let alert_id = alert.id;
                view! {
                    <div class="row between">
                        <span class=if alert.triggered { "small warn" } else { "small muted" }>{format!("{}: {}", t.alerts, alert.condition)}</span>
                        <button class="small" on:click=move |_| {
                            if let Some(model) = state.data.get_untracked().alerts.iter().find(|a| a.meta.id == alert_id) {
                                state.open_dialog(Dialog::Alert(AlertForm::edit(model, lang)));
                            }
                        }>{t.edit}</button>
                    </div>
                }
            }).collect_view()}
            <div class="row wrap">
                {(!position.manual).then(|| view! {
                    <button class="small" on:click=move |_| state.open_dialog(Dialog::Alert(AlertForm::new(id)))>"+ " {t.new_alert}</button>
                })}
                <button class="small" on:click=move |_| state.open_dialog(Dialog::Trade(TradeForm::new(Some(id), today())))>{t.record_trade}</button>
                <button class="small" on:click=move |_| {
                    if let Some(asset) = state.data.get_untracked().asset(id) {
                        state.open_dialog(Dialog::Asset(AssetForm::edit(asset, today(), lang)));
                    }
                }>{edit_label}</button>
            </div>
            {(trade_count > 0).then(|| view! {
                <details class="trades">
                    <summary class="muted small">{format!("{} ({trade_count})", t.trades)}</summary>
                    {position.trades.iter().map(|row| {
                        let trade_id = row.id;
                        view! {
                            <div class="row between trade">
                                <span class="small">
                                    <span class="muted">{row.date.clone()}</span>" "
                                    <strong>{row.kind}</strong>" "
                                    {row.detail.clone()}
                                    {row.account.clone().map(|account| view! { " " <span class="muted">{account}</span> })}
                                </span>
                                <button class="small" on:click=move |_| {
                                    if let Some(trade) = state.data.get_untracked().trades.iter().find(|trade| trade.meta.id == trade_id) {
                                        state.open_dialog(Dialog::Trade(TradeForm::edit(trade, lang)));
                                    }
                                }>{t.edit}</button>
                            </div>
                        }
                    }).collect_view()}
                </details>
            })}
        </section>
    }
}

#[component]
pub fn Investments() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    view! {
        <h2>{t.investments}</h2>
        <div class="row wrap">
            <button on:click=move |_| state.open_dialog(Dialog::Asset(AssetForm::new(&state.data.get_untracked(), today())))>"+ " {t.new_asset}</button>
            {move || state.data.get().assets.iter().any(|asset| !asset.meta.is_deleted()).then(|| view! {
                <button on:click=move |_| state.open_dialog(Dialog::Trade(TradeForm::new(None, today())))>"+ " {t.record_trade}</button>
            })}
        </div>
        {move || {
            let view = portfolio_view(&state.data.get(), today(), lang);
            if view.is_empty {
                return view! { <p class="muted">{t.no_assets}</p> }.into_any();
            }
            let closed = view.closed.len();
            view! {
                {triggered_card(view.triggered.clone(), lang)}
                <section class="card">
                    <span class="muted small">{t.market_value}</span>
                    <p class="hero">{view.total.clone()}</p>
                    <dl class="totals">
                        <dt class="muted">{t.cost}</dt><dd>{view.cost.clone()}</dd>
                        <dt class="muted">{t.unrealized}</dt><dd class=tone_class(view.unrealized_tone)>{view.unrealized.clone()}</dd>
                        <dt class="muted">{t.realized}</dt><dd>{view.realized.clone()}</dd>
                    </dl>
                    {view.note.clone().map(|note| view! { <p class="warn small">{note}</p> })}
                </section>
                {(!view.chart.is_empty()).then(|| view! {
                    <section class="card">
                        <h3>{t.portfolio_chart}</h3>
                        <LineChart chart=view.chart.clone()/>
                    </section>
                })}
                {(view.allocation.len() > 1).then(|| view! {
                    <section class="card">
                        <h3>{t.allocation}</h3>
                        {view.allocation.iter().map(|row| view! {
                            <div class="row between"><span>{row.label}</span>
                                <span>{row.value.clone()}" "<span class="muted small">{row.percent.clone()}</span></span></div>
                            <div class="meter"><span class="s1" style=format!("width: {:.2}%", row.share * 100.0)></span></div>
                        }).collect_view()}
                    </section>
                })}
                {view.positions.into_iter().map(|position| position_card(position, lang)).collect_view()}
                {(closed > 0).then(|| view! {
                    <details>
                        <summary>{format!("{} ({closed})", t.closed_positions)}</summary>
                        {view.closed.into_iter().map(|position| position_card(position, lang)).collect_view()}
                    </details>
                })}
            }
            .into_any()
        }}
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
pub fn AssetDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Asset(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut AssetForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Asset(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let query = RwSignal::new(String::new());
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Asset(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|asset| {
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::Assets,
                id: asset.meta.id,
                label: asset.name.clone(),
            })
        }) as Box<dyn Fn()>
    });
    let currency = move || form().map(|f| f.currency).unwrap_or_default();
    view! {
        <h2>{if original.is_some() { t.edit_asset } else { t.new_asset }}</h2>
        {original.is_none().then(|| view! {
            <label>{t.search_asset}<input placeholder="AAPL, VOO, bitcoin…" prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))/></label>
            <div class="results">
                {move || catalog::search(&query.get()).into_iter().map(|entry| view! {
                    <button class="line small" on:click=move |_| { edit(&|f| f.fill_from(&entry)); query.set(String::new()); }>
                        {format!("{} · {} · {}", entry.ticker, entry.name, entry.currency)}
                    </button>
                }).collect_view()}
            </div>
            <p class="muted small">{t.manual_asset}</p>
        })}
        <label>{t.name}<input prop:value=move || form().map(|f| f.name).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.name = v.clone()) }/></label>
        <div class="row amount-row">
            <select on:change=move |ev| { if let Some(kind) = picked(&ev, &ASSET_KINDS) { edit(&|f| f.kind = kind) } }>
                {ASSET_KINDS.iter().enumerate().map(|(index, kind)| {
                    let kind = *kind;
                    view! { <option value=index.to_string() prop:selected=move || form().is_some_and(|f| f.kind == kind)>{t.asset_kind(kind)}</option> }
                }).collect_view()}
            </select>
            <input class="currency" prop:value=currency on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.currency = v.clone()) }/>
        </div>
        <label>{t.provider}
            <select on:change=move |ev| { if let Some(provider) = picked(&ev, &PROVIDERS) { edit(&|f| f.provider = provider) } }>
                {PROVIDERS.iter().enumerate().map(|(index, provider)| {
                    let provider = *provider;
                    view! { <option value=index.to_string() prop:selected=move || form().is_some_and(|f| f.provider == provider)>{t.provider(provider)}</option> }
                }).collect_view()}
            </select>
        </label>
        {move || form().is_some_and(|f| f.provider != PriceProviderId::Manual).then(|| view! {
            <label>{t.symbol}<input placeholder="AAPL.US" prop:value=move || form().map(|f| f.symbol).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.symbol = v.clone()) }/></label>
        })}
        <label>{move || format!("{} ({})", t.manual_price, currency())}
            <input class="amount" inputmode="decimal" prop:value=move || form().map(|f| f.manual_price).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.manual_price = v.clone()) }/></label>
        {move || form().is_some_and(|f| !f.manual_price.trim().is_empty()).then(|| view! {
            <label>{t.date}<input type="date" prop:value=move || form().map(|f| f.manual_price_on.to_string()).unwrap_or_default()
                on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.manual_price_on = date) }/></label>
        })}
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, delete)}
    }
}

#[component]
pub fn TradeDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Trade(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut TradeForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Trade(f)) = d {
                change(f)
            }
        })
    };
    let original = form().and_then(|f| f.original);
    let assets: Vec<(uuid::Uuid, String)> = data
        .assets
        .iter()
        .filter(|asset| !asset.meta.is_deleted())
        .map(|asset| (asset.meta.id, asset.name.clone()))
        .collect();
    let lookup = data.clone();
    let currency = move || {
        form()
            .and_then(|f| f.asset_id)
            .and_then(|id| lookup.asset(id))
            .map(|asset| asset.currency.to_string())
            .unwrap_or_default()
    };
    let kind = move || {
        form()
            .map(|f| f.kind)
            .unwrap_or(caudal_app_state::model::TradeKind::Buy)
    };
    let account_data = data.clone();
    let delete_data = data.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(draft) => {
                close(state);
                state.save(Item::Trade(draft));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|trade| {
        let name = delete_data
            .asset(trade.asset_id)
            .map(|asset| asset.name.clone())
            .unwrap_or_default();
        let label = format!("{}: {name}", t.trade_kind(trade.kind));
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::Trades,
                id: trade.meta.id,
                label: label.clone(),
            })
        }) as Box<dyn Fn()>
    });
    let amount_input = move |value: fn(&TradeForm) -> String, set: fn(&mut TradeForm, String)| {
        view! {
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || form().map(|f| value(&f)).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| set(f, v.clone())) }/>
        }
    };
    view! {
        <h2>{t.record_trade}</h2>
        <label>{t.asset}
            <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| { f.asset_id = value; f.account_id = None; }) }>
                <option value="" prop:selected=move || form().is_some_and(|f| f.asset_id.is_none())>"—"</option>
                {uuid_options(assets, move || form().and_then(|f| f.asset_id))}
            </select>
        </label>
        <div class="chips">
            {TRADE_KINDS.into_iter().map(|option| view! {
                <button class="chip" class:active=move || kind() == option on:click=move |_| edit(&|f| f.kind = option)>{t.trade_kind(option)}</button>
            }).collect_view()}
        </div>
        <label>{t.date}<input type="date" prop:value=move || form().map(|f| f.date.to_string()).unwrap_or_default()
            on:change=move |ev| if let Ok(date) = event_target_value(&ev).parse() { edit(&|f| f.date = date) }/></label>
        {move || kind().uses_quantity().then(|| view! {
            <label>{t.quantity}{amount_input(|f| f.quantity.clone(), |f, v| f.quantity = v)}</label>
        })}
        {move || if kind().uses_price() {
            let label = if kind().price_is_total() { t.total_amount } else { t.unit_price };
            view! {
                <label>{format!("{label} ({})", currency())}{amount_input(|f| f.price.clone(), |f, v| f.price = v)}</label>
                <label>{format!("{} ({})", t.fee, currency())}{amount_input(|f| f.fee.clone(), |f, v| f.fee = v)}</label>
            }.into_any()
        } else {
            view! { <label>{t.split_ratio}{amount_input(|f| f.split_ratio.clone(), |f, v| f.split_ratio = v)}</label> }.into_any()
        }}
        {move || kind().moves_money().then(|| {
            let accounts = form().map(|f| f.accounts(&account_data)).unwrap_or_default();
            view! {
                <label>{t.pay_from}
                    <select on:change=move |ev| { let value = parse_uuid(&event_target_value(&ev)); edit(&|f| f.account_id = value) }>
                        <option value="" prop:selected=move || form().is_some_and(|f| f.account_id.is_none())>{t.none}</option>
                        {uuid_options(accounts, move || form().and_then(|f| f.account_id))}
                    </select>
                </label>
            }
        })}
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, delete)}
    }
}

/// Opción elegida en un `<select>` cuyos valores son posiciones de `options`.
fn picked<T: Copy>(ev: &leptos::ev::Event, options: &[T]) -> Option<T> {
    let index: usize = event_target_value(ev).parse().ok()?;
    options.get(index).copied()
}

/// Alertas cumplidas, en Inversiones y en el inicio. "Entendido" la pausa.
pub fn triggered_card(alerts: Vec<AlertView>, lang: Lang) -> impl IntoView {
    let state = state();
    let t = texts(lang);
    (!alerts.is_empty()).then(|| {
        let count = alerts.len();
        view! {
            <section class="card">
                <p class="warn"><strong>{format!("{} ({count})", t.alert_hit)}</strong></p>
                {alerts.into_iter().map(|alert| {
                    let id = alert.id;
                    view! {
                        <div class="row between wrap">
                            <span>{alert.text.clone()}</span>
                            <button class="small" on:click=move |_| {
                                if let Some(model) = state.data.get_untracked().alerts.iter().find(|a| a.meta.id == id) {
                                    state.save(Item::Alert(AlertForm::dismissed(model)));
                                }
                            }>{t.dismiss}</button>
                        </div>
                    }
                }).collect_view()}
            </section>
        }
    })
}

#[component]
pub fn AlertDialog() -> impl IntoView {
    let state = state();
    let lang = state.lang.get_untracked();
    let t = texts(lang);
    let data = state.data.get_untracked();
    let form = read(|d| match d {
        Dialog::Alert(f) => Some(f),
        _ => None,
    });
    let edit = move |change: &dyn Fn(&mut AlertForm)| {
        state.dialog.update(|d| {
            if let Some(Dialog::Alert(f)) = d {
                change(f)
            }
        })
    };
    let Some(initial) = form() else {
        return ().into_any();
    };
    let asset = data.asset(initial.asset_id).cloned();
    let name = asset.as_ref().map(|a| a.name.clone()).unwrap_or_default();
    let currency = asset
        .as_ref()
        .map(|a| a.currency.to_string())
        .unwrap_or_default();
    let original = initial.original.clone();
    let save = move || {
        let Some(current) = form() else { return };
        match current.validate(&data, lang) {
            Ok(model) => {
                close(state);
                state.save(Item::Alert(model));
            }
            Err(problem) => state.dialog_error.set(Some(problem.message(t).into())),
        }
    };
    let delete: Option<Box<dyn Fn()>> = original.clone().map(|alert| {
        Box::new(move || {
            state.open_dialog(Dialog::ConfirmDelete {
                table: Table::PriceAlerts,
                id: alert.meta.id,
                label: t.alerts.to_owned(),
            })
        }) as Box<dyn Fn()>
    });
    view! {
        <h2>{if original.is_some() { t.edit_alert } else { t.new_alert }}</h2>
        <p>{name}</p>
        <div class="chips">
            {[(AlertDirection::Above, t.alert_above), (AlertDirection::Below, t.alert_below)].into_iter().map(|(direction, label)| view! {
                <button class="chip" class:active=move || form().is_some_and(|f| f.direction == direction) on:click=move |_| edit(&|f| f.direction = direction)>{label}</button>
            }).collect_view()}
        </div>
        <label>{format!("{} ({currency})", t.target_price)}
            <input class="amount" inputmode="decimal" placeholder="0" prop:value=move || form().map(|f| f.target_price).unwrap_or_default()
                on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.target_price = v.clone()) }/></label>
        <label class="check"><input type="checkbox" prop:checked=move || form().is_some_and(|f| f.active)
            on:change=move |ev| { let v = event_target_checked(&ev); edit(&|f| f.active = v) }/>{t.alert_active}</label>
        <label>{t.note}<input prop:value=move || form().map(|f| f.note).unwrap_or_default()
            on:input=move |ev| { let v = event_target_value(&ev); edit(&|f| f.note = v.clone()) }/></label>
        {dialog_buttons(state, lang, save, delete)}
    }
    .into_any()
}
