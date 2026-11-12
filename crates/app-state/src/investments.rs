//! Inversiones: activos, operaciones y la cartera lista para mostrar.
//! La app registra y muestra; no recomienda comprar ni vender.

use crate::catalog::CatalogEntry;
use crate::charts::{Chart, ChartSeries, to_f64};
use crate::data::Data;
use crate::debts::movement;
use crate::format::{
    amount_for_input, currency_decimals, format_amount, format_date, format_estimate, format_money,
};
use crate::forms::{
    FormError, amount, currency, meta, optional_amount, optional_text, required_name,
};
use crate::i18n::{Lang, texts};
use crate::views::{Tone, base_currency};
use caudal_core::fx::FxTable;
use caudal_core::model::{
    AlertDirection, Asset, AssetKind, PriceAlert, PriceProviderId, Trade, TradeKind, Transaction,
    TransactionKind,
};
use caudal_core::money::{Currency, Money};
use caudal_core::portfolio::{PriceSource, portfolio_value, position};
use caudal_core::series::month_starts;
use chrono::{Days, Months, NaiveDate};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Cantidad sin ceros sobrantes: "25", "0,0153".
pub fn format_quantity(quantity: Decimal, lang: Lang) -> String {
    let units = Currency::new("CLP").expect("CLP es válido");
    format_amount(quantity, &units, lang)
}

/// Precio unitario calculado (costo promedio): los decimales de la moneda, o
/// hasta 6 si vale menos de una unidad.
pub fn format_unit_price(price: Decimal, currency: &Currency, lang: Lang) -> String {
    let decimals = if price.abs() < Decimal::ONE {
        6
    } else {
        currency_decimals(currency)
    };
    format_money(
        &Money::new(price.round_dp(decimals).normalize(), currency.clone()),
        lang,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetForm {
    pub name: String,
    pub kind: AssetKind,
    pub currency: String,
    pub provider: PriceProviderId,
    pub symbol: String,
    pub manual_price: String,
    pub manual_price_on: NaiveDate,
    pub note: String,
    pub original: Option<Asset>,
}

impl AssetForm {
    /// Activo a mano: sin fuente automática, en la moneda principal.
    pub fn new(data: &Data, today: NaiveDate) -> Self {
        Self {
            name: String::new(),
            kind: AssetKind::Other,
            currency: base_currency(data).to_string(),
            provider: PriceProviderId::Manual,
            symbol: String::new(),
            manual_price: String::new(),
            manual_price_on: today,
            note: String::new(),
            original: None,
        }
    }

    pub fn fill_from(&mut self, entry: &CatalogEntry) {
        self.name = entry.name.to_owned();
        self.kind = entry.kind;
        self.currency = entry.currency.to_owned();
        self.provider = entry.provider;
        self.symbol = entry.symbol.to_owned();
    }

    pub fn edit(asset: &Asset, today: NaiveDate, lang: Lang) -> Self {
        Self {
            name: asset.name.clone(),
            kind: asset.kind,
            currency: asset.currency.to_string(),
            provider: asset.provider,
            symbol: asset.provider_symbol.clone().unwrap_or_default(),
            manual_price: asset
                .manual_price
                .map(|price| amount_for_input(price, lang))
                .unwrap_or_default(),
            manual_price_on: asset.manual_price_on.unwrap_or(today),
            note: asset.note.clone().unwrap_or_default(),
            original: Some(asset.clone()),
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<Asset, FormError> {
        let symbol = optional_text(&self.symbol);
        if self.provider != PriceProviderId::Manual && symbol.is_none() {
            return Err(FormError::SymbolRequired);
        }
        let manual_price = optional_amount(&self.manual_price, lang)?;
        Ok(Asset {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            name: required_name(&self.name)?,
            kind: self.kind,
            currency: currency(&self.currency)?,
            provider: self.provider,
            provider_symbol: symbol,
            manual_price,
            manual_price_on: manual_price.map(|_| self.manual_price_on),
            note: optional_text(&self.note),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeForm {
    pub asset_id: Option<Uuid>,
    pub kind: TradeKind,
    pub date: NaiveDate,
    pub quantity: String,
    /// Precio por unidad; en dividendos y comisiones, el monto total.
    pub price: String,
    pub fee: String,
    pub split_ratio: String,
    pub account_id: Option<Uuid>,
    pub note: String,
    pub original: Option<Trade>,
}

/// Lo que se guarda al registrar una operación.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeDraft {
    pub trade: Trade,
    /// Movimiento de la cuenta asociada, con el mismo id si ya existía.
    pub movement: Option<Transaction>,
    /// Movimiento anterior que ya no corresponde (se quitó la cuenta).
    pub remove_movement: Option<Uuid>,
}

/// Campos que pide cada tipo de operación.
pub trait TradeKindFields: Copy {
    fn uses_quantity(self) -> bool;
    fn uses_price(self) -> bool;
    fn price_is_total(self) -> bool;
    fn moves_money(self) -> bool;
}

impl TradeKindFields for TradeKind {
    fn uses_quantity(self) -> bool {
        !matches!(
            self,
            TradeKind::Dividend | TradeKind::Fee | TradeKind::Split
        )
    }

    fn uses_price(self) -> bool {
        self != TradeKind::Split
    }

    /// Dividendos y comisiones se registran como monto total.
    fn price_is_total(self) -> bool {
        matches!(self, TradeKind::Dividend | TradeKind::Fee)
    }

    fn moves_money(self) -> bool {
        matches!(
            self,
            TradeKind::Buy | TradeKind::Sell | TradeKind::Dividend | TradeKind::Fee
        )
    }
}

impl TradeForm {
    pub fn new(asset_id: Option<Uuid>, today: NaiveDate) -> Self {
        Self {
            asset_id,
            kind: TradeKind::Buy,
            date: today,
            quantity: String::new(),
            price: String::new(),
            fee: String::new(),
            split_ratio: String::new(),
            account_id: None,
            note: String::new(),
            original: None,
        }
    }

    pub fn edit(trade: &Trade, lang: Lang) -> Self {
        let text = |value: Option<Decimal>| {
            value
                .map(|value| amount_for_input(value, lang))
                .unwrap_or_default()
        };
        let price = match (trade.kind.price_is_total(), trade.quantity, trade.price) {
            // Un dividendo con cantidad × precio se edita como su total.
            (true, Some(quantity), Some(price)) => Some(quantity * price),
            (_, _, price) => price,
        };
        Self {
            asset_id: Some(trade.asset_id),
            kind: trade.kind,
            date: trade.traded_on,
            quantity: text(trade.quantity),
            price: text(price),
            fee: text(trade.fee),
            split_ratio: text(trade.split_ratio),
            account_id: trade.account_id,
            note: trade.note.clone().unwrap_or_default(),
            original: Some(trade.clone()),
        }
    }

    /// Cuentas en la moneda del activo, las únicas que pueden pagar o recibir.
    pub fn accounts(&self, data: &Data) -> Vec<(Uuid, String)> {
        self.asset_id
            .and_then(|id| data.asset(id))
            .map(|asset| data.accounts_in(asset.currency.as_str()))
            .unwrap_or_default()
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<TradeDraft, FormError> {
        let asset = self
            .asset_id
            .and_then(|id| data.asset(id))
            .ok_or(FormError::AssetRequired)?;
        let kind = self.kind;
        let quantity = if kind.uses_quantity() {
            let quantity =
                optional_amount(&self.quantity, lang)?.ok_or(FormError::QuantityRequired)?;
            if quantity.is_zero() {
                return Err(FormError::QuantityRequired);
            }
            Some(quantity)
        } else {
            None
        };
        let price = match kind {
            TradeKind::Split => None,
            TradeKind::TransferIn | TradeKind::TransferOut => optional_amount(&self.price, lang)?,
            _ => Some(amount(&self.price, lang, false)?),
        };
        let split_ratio = if kind == TradeKind::Split {
            let ratio = optional_amount(&self.split_ratio, lang)?
                .filter(|ratio| !ratio.is_zero())
                .ok_or(FormError::AmountRequired)?;
            Some(ratio)
        } else {
            None
        };
        let fee = if kind == TradeKind::Split {
            None
        } else {
            optional_amount(&self.fee, lang)?
        };
        let account = self
            .account_id
            .filter(|_| kind.moves_money())
            .map(|id| data.account(id).ok_or(FormError::AccountCurrencyMismatch))
            .transpose()?;
        if let Some(account) = account
            && account.currency != asset.currency
        {
            return Err(FormError::AccountCurrencyMismatch);
        }

        let previous_movement = self
            .original
            .as_ref()
            .and_then(|original| original.transaction_id);
        let gross = match (quantity, price) {
            (Some(quantity), Some(price)) => quantity * price,
            (None, Some(price)) => price,
            _ => Decimal::ZERO,
        };
        let fee_value = fee.unwrap_or_default();
        let movement = account.and_then(|account| {
            let t = texts(lang);
            let label = format!("{}: {}", t.trade_kind(kind), asset.name);
            let (signed, income) = match kind {
                TradeKind::Buy => (-(gross + fee_value), false),
                TradeKind::Sell => (gross - fee_value, false),
                TradeKind::Dividend => (gross - fee_value, true),
                TradeKind::Fee => (-(gross + fee_value), false),
                _ => return None,
            };
            if signed.is_zero() {
                return None;
            }
            let mut tx = movement(
                data.user_id,
                account.meta.id,
                signed,
                asset.currency.clone(),
                self.date,
                None,
                label,
            );
            if income && signed > Decimal::ZERO {
                tx.kind = TransactionKind::Income;
            }
            if let Some(id) = previous_movement {
                tx.meta.id = id;
                // Se conservan los datos del usuario en ese movimiento.
                if let Some(existing) = data.transaction(id) {
                    tx.category_id = existing.category_id;
                    tx.tag_ids = existing.tag_ids.clone();
                    tx.note = existing.note.clone();
                    tx.meta = existing.meta.clone();
                }
            }
            Some(tx)
        });
        let remove_movement = previous_movement.filter(|_| movement.is_none());

        let trade = Trade {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            asset_id: asset.meta.id,
            kind,
            traded_on: self.date,
            quantity,
            price,
            fee,
            currency: asset.currency.clone(),
            split_ratio,
            account_id: account.map(|account| account.meta.id),
            transaction_id: movement.as_ref().map(|tx| tx.meta.id),
            note: optional_text(&self.note),
        };
        Ok(TradeDraft {
            trade,
            movement,
            remove_movement,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeRow {
    pub id: Uuid,
    pub date: String,
    pub kind: &'static str,
    pub detail: String,
    pub account: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionView {
    pub id: Uuid,
    pub name: String,
    /// "Acción · AAPL.US".
    pub subtitle: String,
    pub quantity: String,
    /// "189,50 USD · mercado · 10/11/2026", o "Sin precio".
    pub price: String,
    pub value: Option<String>,
    pub cost: String,
    pub average_cost: Option<String>,
    pub unrealized: Option<String>,
    pub unrealized_tone: Tone,
    /// "+9,0 %" sobre el costo.
    pub return_pct: Option<String>,
    pub realized: Option<String>,
    pub oversold: bool,
    pub closed: bool,
    pub manual: bool,
    /// "Sin actualizar desde 03/11/2026" si el precio de mercado quedó atrás.
    pub price_note: Option<String>,
    pub alerts: Vec<AlertView>,
    pub trades: Vec<TradeRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AllocationRow {
    pub label: &'static str,
    pub value: String,
    pub share: f32,
    pub percent: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioView {
    pub total: String,
    pub cost: String,
    pub unrealized: String,
    pub unrealized_tone: Tone,
    pub realized: String,
    /// "Sin precio o tipo de cambio, no incluye: Casa".
    pub note: Option<String>,
    pub positions: Vec<PositionView>,
    pub closed: Vec<PositionView>,
    pub allocation: Vec<AllocationRow>,
    pub chart: Chart,
    /// Alertas activas que ya se cumplieron.
    pub triggered: Vec<AlertView>,
    pub is_empty: bool,
}

fn percent(value: Decimal, lang: Lang) -> String {
    let rounded = (value * Decimal::from(100)).round_dp(1).normalize();
    let sign = if rounded > Decimal::ZERO { "+" } else { "" };
    match lang {
        Lang::Es => format!("{sign}{} %", rounded.to_string().replace('.', ",")),
        Lang::En => format!("{sign}{rounded}%"),
    }
}

fn tone(value: Decimal) -> Tone {
    if value.is_zero() {
        Tone::Neutral
    } else if value.is_sign_negative() {
        Tone::Negative
    } else {
        Tone::Positive
    }
}

fn trade_detail(trade: &Trade, lang: Lang) -> String {
    let money = |value: Decimal| format_money(&Money::new(value, trade.currency.clone()), lang);
    let mut text = match (trade.kind, trade.quantity, trade.price) {
        (TradeKind::Split, _, _) => format!(
            "× {}",
            format_quantity(trade.split_ratio.unwrap_or(Decimal::ONE), lang)
        ),
        (_, Some(quantity), Some(price)) => {
            format!("{} × {}", format_quantity(quantity, lang), money(price))
        }
        (_, Some(quantity), None) => format_quantity(quantity, lang),
        (_, None, Some(price)) => money(price),
        (_, None, None) => String::new(),
    };
    if let Some(fee) = trade.fee.filter(|fee| !fee.is_zero()) {
        text = format!("{text} (+ {})", money(fee));
    }
    text
}

/// Cartera al día de hoy, en la moneda principal, y su valor al cierre de los
/// últimos 12 meses.
pub fn portfolio_view(data: &Data, today: NaiveDate, lang: Lang) -> PortfolioView {
    let t = texts(lang);
    let base = base_currency(data);
    let fx = FxTable::new(data.fx_rates.iter().cloned());
    let in_base = |value: Decimal, from: &Currency| {
        fx.convert(&Money::new(value, from.clone()), &base, today)
            .map(|money| money.amount)
    };
    let base_money = |value: Decimal| format_estimate(&Money::new(value, base.clone()), lang);

    let mut total = Decimal::ZERO;
    let mut cost_total = Decimal::ZERO;
    let mut realized_total = Decimal::ZERO;
    let mut missing: Vec<String> = Vec::new();
    let mut by_kind: Vec<(AssetKind, Decimal)> = Vec::new();
    let mut positions = Vec::new();
    let mut closed = Vec::new();

    for asset in data.assets.iter().filter(|asset| !asset.meta.is_deleted()) {
        let Ok(position) = position(asset, &data.trades, &data.prices, &fx, today) else {
            continue;
        };
        let own =
            |value: Decimal| format_estimate(&Money::new(value, asset.currency.clone()), lang);
        let is_closed = position.quantity <= Decimal::ZERO && !position.oversold;
        let realized = position.realized + position.dividends;

        if !is_closed {
            match position
                .market_value
                .and_then(|value| in_base(value, &asset.currency))
            {
                Some(value) => {
                    total += value;
                    if let Some(cost) = in_base(position.cost_basis, &asset.currency) {
                        cost_total += cost;
                    }
                    match by_kind.iter_mut().find(|(kind, _)| *kind == asset.kind) {
                        Some((_, sum)) => *sum += value,
                        None => by_kind.push((asset.kind, value)),
                    }
                }
                None => missing.push(asset.name.clone()),
            }
        }
        if let Some(value) = in_base(realized, &asset.currency) {
            realized_total += value;
        }

        let mut trades: Vec<&Trade> = data
            .trades
            .iter()
            .filter(|trade| trade.asset_id == asset.meta.id && !trade.meta.is_deleted())
            .collect();
        trades.sort_by_key(|trade| std::cmp::Reverse((trade.traded_on, trade.meta.id)));
        let source = |source: PriceSource| match source {
            PriceSource::Market => t.price_market,
            PriceSource::Manual => t.price_manual,
            PriceSource::LastTrade => t.price_last_trade,
        };
        let view = PositionView {
            id: asset.meta.id,
            name: asset.name.clone(),
            subtitle: match &asset.provider_symbol {
                Some(symbol) => format!("{} · {symbol}", t.asset_kind(asset.kind)),
                None => t.asset_kind(asset.kind).to_owned(),
            },
            quantity: format_quantity(position.quantity, lang),
            price: match position.price {
                Some((price, date, from)) => format!(
                    "{} · {} · {}",
                    format_money(&Money::new(price, asset.currency.clone()), lang),
                    source(from),
                    format_date(date, lang)
                ),
                None => t.no_price.to_owned(),
            },
            value: position.market_value.map(own),
            cost: own(position.cost_basis),
            average_cost: position
                .average_cost
                .map(|average| format_unit_price(average, &asset.currency, lang)),
            unrealized: position.unrealized.map(own),
            unrealized_tone: position.unrealized.map_or(Tone::Neutral, tone),
            return_pct: position
                .unrealized
                .filter(|_| position.cost_basis > Decimal::ZERO)
                .map(|gain| percent(gain / position.cost_basis, lang)),
            realized: (!realized.is_zero()).then(|| own(realized)),
            oversold: position.oversold,
            closed: is_closed,
            manual: asset.provider == PriceProviderId::Manual,
            price_note: price_note(asset, position.price, today, lang),
            alerts: alerts_for(data, asset, position.price.map(|(price, _, _)| price), lang),
            trades: trades
                .into_iter()
                .map(|trade| TradeRow {
                    id: trade.meta.id,
                    date: format_date(trade.traded_on, lang),
                    kind: t.trade_kind(trade.kind),
                    detail: trade_detail(trade, lang),
                    account: data.account_name(trade.account_id).map(str::to_owned),
                })
                .collect(),
        };
        if is_closed {
            closed.push(view);
        } else {
            positions.push((
                position
                    .market_value
                    .and_then(|v| in_base(v, &asset.currency)),
                view,
            ));
        }
    }
    positions.sort_by_key(|(value, _)| std::cmp::Reverse(value.unwrap_or_default()));

    by_kind.sort_by_key(|(_, value)| std::cmp::Reverse(*value));
    let allocation = by_kind
        .iter()
        .filter(|(_, value)| *value > Decimal::ZERO)
        .map(|(kind, value)| {
            let share = if total > Decimal::ZERO {
                *value / total
            } else {
                Decimal::ZERO
            };
            AllocationRow {
                label: t.asset_kind(*kind),
                value: base_money(*value),
                share: to_f64(share) as f32,
                percent: percent(share, lang).trim_start_matches('+').to_owned(),
            }
        })
        .collect();

    // Cierre de cada mes; el mes actual, hoy.
    let ends: Vec<NaiveDate> = month_starts(today, 12)
        .into_iter()
        .map(|start| {
            (start + Months::new(1))
                .checked_sub_days(Days::new(1))
                .unwrap_or(start)
                .min(today)
        })
        .collect();
    let values: Vec<Decimal> = ends
        .iter()
        .map(|date| {
            portfolio_value(&data.assets, &data.trades, &data.prices, &fx, &base, *date)
                .map(|(value, _)| value)
                .unwrap_or_default()
        })
        .collect();
    let chart = Chart::new(
        ends.iter()
            .map(|date| t.month_short(chrono::Datelike::month(date)).to_owned())
            .collect(),
        vec![ChartSeries {
            name: t.investments.to_owned(),
            slot: 0,
            values: values.iter().map(|value| Some(to_f64(*value))).collect(),
            texts: values.iter().map(|value| base_money(*value)).collect(),
        }],
        lang,
    );

    let unrealized = total - cost_total;
    PortfolioView {
        total: base_money(total),
        cost: base_money(cost_total),
        unrealized: base_money(unrealized),
        unrealized_tone: tone(unrealized),
        realized: base_money(realized_total),
        note: (!missing.is_empty()).then(|| format!("{} {}", t.not_valued, missing.join(", "))),
        triggered: triggered_alerts(data, today, lang),
        is_empty: positions.is_empty() && closed.is_empty(),
        positions: positions.into_iter().map(|(_, view)| view).collect(),
        closed,
        allocation,
        chart,
    }
}

/// Valor de la cartera en moneda principal, para el inicio.
pub fn portfolio_total(data: &Data, today: NaiveDate, lang: Lang) -> Option<String> {
    let live = data.assets.iter().any(|asset| !asset.meta.is_deleted());
    if !live {
        return None;
    }
    let base = base_currency(data);
    let fx = FxTable::new(data.fx_rates.iter().cloned());
    let (value, _) =
        portfolio_value(&data.assets, &data.trades, &data.prices, &fx, &base, today).ok()?;
    Some(format_estimate(&Money::new(value, base), lang))
}

/// Días sin precio nuevo a partir de los cuales se avisa: cubre fines de
/// semana y feriados de la bolsa.
const STALE_DAYS: i64 = 4;

fn price_note(
    asset: &Asset,
    price: Option<(Decimal, NaiveDate, PriceSource)>,
    today: NaiveDate,
    lang: Lang,
) -> Option<String> {
    let t = texts(lang);
    if asset.provider == PriceProviderId::Manual {
        return None;
    }
    match price {
        Some((_, date, PriceSource::Market)) if (today - date).num_days() > STALE_DAYS => {
            Some(format!("{} {}", t.price_stale, format_date(date, lang)))
        }
        Some((_, _, PriceSource::Market)) => None,
        Some((_, _, PriceSource::Manual)) => {
            Some(format!("{} {}", t.no_market_price, t.price_manual))
        }
        Some((_, _, PriceSource::LastTrade)) => {
            Some(format!("{} {}", t.no_market_price, t.price_last_trade))
        }
        None => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertView {
    pub id: Uuid,
    pub asset_id: Uuid,
    /// "Si sube a 230,00 USD".
    pub condition: String,
    /// "Apple: 235,10 USD · Si sube a 230,00 USD".
    pub text: String,
    pub active: bool,
    pub triggered: bool,
}

fn alert_hit(direction: AlertDirection, price: Decimal, target: Decimal) -> bool {
    match direction {
        AlertDirection::Above => price >= target,
        AlertDirection::Below => price <= target,
    }
}

fn alert_view(alert: &PriceAlert, asset: &Asset, price: Option<Decimal>, lang: Lang) -> AlertView {
    let t = texts(lang);
    let money = |value: Decimal| format_money(&Money::new(value, asset.currency.clone()), lang);
    let label = match alert.direction {
        AlertDirection::Above => t.alert_above,
        AlertDirection::Below => t.alert_below,
    };
    let condition = format!("{label} {}", money(alert.target_price));
    let triggered = alert.active
        && price.is_some_and(|price| alert_hit(alert.direction, price, alert.target_price));
    let now = price
        .map(|price| format_unit_price(price, &asset.currency, lang))
        .unwrap_or_else(|| t.no_price.to_owned());
    AlertView {
        id: alert.meta.id,
        asset_id: asset.meta.id,
        text: format!("{}: {now} · {condition}", asset.name),
        condition: if alert.active {
            condition
        } else {
            format!("{condition} ({})", t.alert_paused)
        },
        active: alert.active,
        triggered,
    }
}

fn alerts_for(data: &Data, asset: &Asset, price: Option<Decimal>, lang: Lang) -> Vec<AlertView> {
    data.alerts
        .iter()
        .filter(|alert| alert.asset_id == asset.meta.id && !alert.meta.is_deleted())
        .map(|alert| alert_view(alert, asset, price, lang))
        .collect()
}

/// Alertas activas cuyo precio ya se alcanzó, con el precio vigente de hoy.
/// Se evalúan cada vez que llegan precios nuevos y se recalculan las vistas.
pub fn triggered_alerts(data: &Data, today: NaiveDate, lang: Lang) -> Vec<AlertView> {
    let fx = FxTable::new(data.fx_rates.iter().cloned());
    data.alerts
        .iter()
        .filter(|alert| alert.active && !alert.meta.is_deleted())
        .filter_map(|alert| {
            let asset = data
                .asset(alert.asset_id)
                .filter(|a| !a.meta.is_deleted())?;
            let price = position(asset, &data.trades, &data.prices, &fx, today)
                .ok()?
                .price
                .map(|(price, _, _)| price);
            Some(alert_view(alert, asset, price, lang)).filter(|view| view.triggered)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertForm {
    pub asset_id: Uuid,
    pub direction: AlertDirection,
    pub target_price: String,
    pub active: bool,
    pub note: String,
    pub original: Option<PriceAlert>,
}

impl AlertForm {
    pub fn new(asset_id: Uuid) -> Self {
        Self {
            asset_id,
            direction: AlertDirection::Above,
            target_price: String::new(),
            active: true,
            note: String::new(),
            original: None,
        }
    }

    pub fn edit(alert: &PriceAlert, lang: Lang) -> Self {
        Self {
            asset_id: alert.asset_id,
            direction: alert.direction,
            target_price: amount_for_input(alert.target_price, lang),
            active: alert.active,
            note: alert.note.clone().unwrap_or_default(),
            original: Some(alert.clone()),
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<PriceAlert, FormError> {
        data.asset(self.asset_id).ok_or(FormError::AssetRequired)?;
        Ok(PriceAlert {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            asset_id: self.asset_id,
            direction: self.direction,
            target_price: amount(&self.target_price, lang, false)?,
            active: self.active,
            note: optional_text(&self.note),
        })
    }

    /// La misma alerta, pausada: el usuario ya la vio.
    pub fn dismissed(alert: &PriceAlert) -> PriceAlert {
        PriceAlert {
            active: false,
            ..alert.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudal_core::model::{Account, AccountKind};
    use std::str::FromStr;

    fn dec(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn data() -> Data {
        let user_id = Uuid::now_v7();
        let mut data = Data {
            user_id,
            ..Data::default()
        };
        data.accounts.push(Account {
            meta: meta(None, user_id),
            name: "Broker".into(),
            kind: AccountKind::Broker,
            currency: Currency::new("USD").unwrap(),
            opening_balance: dec("10000"),
            archived: false,
            note: None,
            color: None,
            icon: None,
        });
        data
    }

    fn add_asset(data: &mut Data, name: &str, currency: &str) -> Uuid {
        let mut form = AssetForm::new(data, today());
        form.name = name.into();
        form.currency = currency.into();
        let asset = form.validate(data, Lang::Es).unwrap();
        let id = asset.meta.id;
        data.assets.push(asset);
        id
    }

    fn trade(data: &mut Data, form: TradeForm) -> TradeDraft {
        let draft = form.validate(data, Lang::Es).unwrap();
        data.trades.push(draft.trade.clone());
        if let Some(tx) = &draft.movement {
            data.transactions.retain(|old| old.meta.id != tx.meta.id);
            data.transactions.push(tx.clone());
        }
        draft
    }

    #[test]
    fn compra_venta_y_dividendo_mueven_la_cuenta() {
        let mut data = data();
        let account = data.accounts[0].meta.id;
        let asset = add_asset(&mut data, "ACME", "USD");
        let mut buy = TradeForm::new(Some(asset), today());
        buy.quantity = "10".into();
        buy.price = "50".into();
        buy.fee = "1".into();
        buy.account_id = Some(account);
        let draft = trade(&mut data, buy);
        let movement = draft.movement.unwrap();
        assert_eq!(movement.kind, TransactionKind::Adjustment);
        assert_eq!(movement.amount, dec("-501"));
        assert_eq!(draft.trade.transaction_id, Some(movement.meta.id));

        let mut sell = TradeForm::new(Some(asset), today());
        sell.kind = TradeKind::Sell;
        sell.quantity = "4".into();
        sell.price = "60".into();
        sell.account_id = Some(account);
        assert_eq!(trade(&mut data, sell).movement.unwrap().amount, dec("240"));

        let mut dividend = TradeForm::new(Some(asset), today());
        dividend.kind = TradeKind::Dividend;
        dividend.price = "12,5".into();
        dividend.account_id = Some(account);
        let movement = trade(&mut data, dividend).movement.unwrap();
        assert_eq!(movement.kind, TransactionKind::Income);
        assert_eq!(movement.amount, dec("12.5"));

        let view = portfolio_view(&data, today(), Lang::Es);
        let acme = &view.positions[0];
        assert_eq!(acme.quantity, "6");
        // Último precio de operación: 60 × 6.
        assert_eq!(acme.value.as_deref(), Some("360,00 USD"));
        assert_eq!(acme.cost, "300,60 USD");
        assert_eq!(acme.average_cost.as_deref(), Some("50,10 USD"));
        assert_eq!(acme.return_pct.as_deref(), Some("+19,8 %"));
        // Realizado 240 - 200,4 = 39,6, más 12,5 de dividendos.
        assert_eq!(acme.realized.as_deref(), Some("52,10 USD"));
    }

    #[test]
    fn editar_conserva_el_movimiento_y_quitar_la_cuenta_lo_borra() {
        let mut data = data();
        let account = data.accounts[0].meta.id;
        let asset = add_asset(&mut data, "ACME", "USD");
        let mut buy = TradeForm::new(Some(asset), today());
        buy.quantity = "2".into();
        buy.price = "10".into();
        buy.account_id = Some(account);
        let first = trade(&mut data, buy);
        let movement_id = first.movement.unwrap().meta.id;

        let mut edit = TradeForm::edit(&first.trade, Lang::Es);
        edit.quantity = "3".into();
        let second = edit.validate(&data, Lang::Es).unwrap();
        assert_eq!(second.trade.meta.id, first.trade.meta.id);
        assert_eq!(second.movement.as_ref().unwrap().meta.id, movement_id);
        assert_eq!(second.movement.unwrap().amount, dec("-30"));

        let mut edit = TradeForm::edit(&first.trade, Lang::Es);
        edit.account_id = None;
        let third = edit.validate(&data, Lang::Es).unwrap();
        assert!(third.movement.is_none());
        assert_eq!(third.remove_movement, Some(movement_id));
        assert_eq!(third.trade.transaction_id, None);
    }

    #[test]
    fn validaciones() {
        let mut data = data();
        let account = data.accounts[0].meta.id;
        let euro = add_asset(&mut data, "Fondo", "EUR");
        let mut form = TradeForm::new(Some(euro), today());
        form.quantity = "1".into();
        form.price = "10".into();
        form.account_id = Some(account);
        assert_eq!(
            form.validate(&data, Lang::Es),
            Err(FormError::AccountCurrencyMismatch)
        );
        form.account_id = None;
        form.quantity.clear();
        assert_eq!(
            form.validate(&data, Lang::Es),
            Err(FormError::QuantityRequired)
        );
        assert_eq!(
            TradeForm::new(None, today()).validate(&data, Lang::Es),
            Err(FormError::AssetRequired)
        );

        let mut asset = AssetForm::new(&data, today());
        asset.name = "Apple".into();
        asset.provider = PriceProviderId::Eodhd;
        assert_eq!(
            asset.validate(&data, Lang::Es),
            Err(FormError::SymbolRequired)
        );
        asset.fill_from(&crate::catalog::search("aapl")[0]);
        let apple = asset.validate(&data, Lang::Es).unwrap();
        assert_eq!(apple.provider_symbol.as_deref(), Some("AAPL.US"));
        assert_eq!(apple.currency.as_str(), "USD");
    }

    #[test]
    fn precio_manual_y_activo_sin_precio() {
        let mut data = data();
        let house = add_asset(&mut data, "Casa", "USD");
        let mut transfer = TradeForm::new(Some(house), today());
        transfer.kind = TradeKind::TransferIn;
        transfer.quantity = "1".into();
        trade(&mut data, transfer);
        let view = portfolio_view(&data, today(), Lang::Es);
        assert_eq!(view.positions[0].price, "Sin precio");
        assert_eq!(
            view.note.as_deref(),
            Some("Sin precio o tipo de cambio, no incluye: Casa")
        );

        let mut edit = AssetForm::edit(&data.assets[0], today(), Lang::Es);
        edit.manual_price = "120000".into();
        data.assets[0] = edit.validate(&data, Lang::Es).unwrap();
        let view = portfolio_view(&data, today(), Lang::Es);
        assert!(view.note.is_none());
        assert_eq!(view.total, "120.000,00 USD");
        assert!(view.positions[0].price.contains("manual"));
        assert_eq!(view.allocation[0].percent, "100 %");
    }

    #[test]
    fn alertas_se_cumplen_con_el_precio_vigente_y_se_pausan() {
        let mut data = data();
        let acme = add_asset(&mut data, "ACME", "USD");
        data.assets[0].provider = PriceProviderId::Eodhd;
        data.assets[0].provider_symbol = Some("ACME.US".into());
        let mut buy = TradeForm::new(Some(acme), today());
        buy.quantity = "1".into();
        buy.price = "100".into();
        trade(&mut data, buy);
        let price = |date: &str, value: &str| caudal_core::model::AssetPrice {
            provider: PriceProviderId::Eodhd,
            symbol: "ACME.US".into(),
            price_on: date.parse().unwrap(),
            price: dec(value),
            currency: Currency::new("USD").unwrap(),
        };
        data.prices = vec![price("2026-11-09", "118")];
        let mut above = AlertForm::new(acme);
        above.target_price = "120".into();
        let above = above.validate(&data, Lang::Es).unwrap();
        let mut below = AlertForm::new(acme);
        below.direction = AlertDirection::Below;
        below.target_price = "90".into();
        let below = below.validate(&data, Lang::Es).unwrap();
        data.alerts = vec![above.clone(), below];
        assert!(triggered_alerts(&data, today(), Lang::Es).is_empty());

        // Llega un precio nuevo: la alerta de subida se cumple; la de bajada no.
        data.prices
            .push(price("2026-11-10", "121,5".replace(',', ".").as_str()));
        let hit = triggered_alerts(&data, today(), Lang::Es);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].text, "ACME: 121,50 USD · Si sube a 120,00 USD");
        let view = portfolio_view(&data, today(), Lang::Es);
        assert_eq!(view.triggered.len(), 1);
        assert_eq!(view.positions[0].alerts.len(), 2);

        data.alerts[0] = AlertForm::dismissed(&above);
        assert!(triggered_alerts(&data, today(), Lang::Es).is_empty());
        let view = portfolio_view(&data, today(), Lang::Es);
        assert!(view.positions[0].alerts[0].condition.ends_with("(pausada)"));
    }

    #[test]
    fn avisa_si_el_precio_de_mercado_quedo_atras() {
        let mut data = data();
        let acme = add_asset(&mut data, "ACME", "USD");
        data.assets[0].provider = PriceProviderId::Eodhd;
        data.assets[0].provider_symbol = Some("ACME.US".into());
        let mut buy = TradeForm::new(Some(acme), today());
        buy.quantity = "1".into();
        buy.price = "100".into();
        trade(&mut data, buy);
        let view = portfolio_view(&data, today(), Lang::Es);
        assert_eq!(
            view.positions[0].price_note.as_deref(),
            Some("Aún sin precio de mercado: se usa última operación")
        );
        data.prices = vec![caudal_core::model::AssetPrice {
            provider: PriceProviderId::Eodhd,
            symbol: "acme.us".into(),
            price_on: "2026-11-03".parse().unwrap(),
            price: dec("110"),
            currency: Currency::new("USD").unwrap(),
        }];
        let view = portfolio_view(&data, today(), Lang::Es);
        assert_eq!(
            view.positions[0].price_note.as_deref(),
            Some("Sin actualizar desde 03/11/2026")
        );
        assert_eq!(view.positions[0].value.as_deref(), Some("110,00 USD"));
        data.prices[0].price_on = "2026-11-07".parse().unwrap();
        assert!(
            portfolio_view(&data, today(), Lang::Es).positions[0]
                .price_note
                .is_none()
        );
    }

    #[test]
    fn cantidades() {
        assert_eq!(format_quantity(dec("25.000"), Lang::Es), "25");
        assert_eq!(format_quantity(dec("0.0153"), Lang::Es), "0,0153");
        assert_eq!(format_quantity(dec("1234.5"), Lang::En), "1,234.5");
        let usd = Currency::new("USD").unwrap();
        assert_eq!(format_unit_price(dec("50.1"), &usd, Lang::Es), "50,10 USD");
        assert_eq!(
            format_unit_price(dec("0.000012345"), &usd, Lang::En),
            "0.000012 USD"
        );
    }
}
