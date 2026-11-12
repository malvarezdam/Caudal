//! Avisos dentro de la app y actividad reciente.

use crate::data::Data;
use crate::format::format_date;
use crate::i18n::{Lang, texts};
use crate::views::Tone;
use caudal_core::hlc::HlcParts;
use caudal_core::model::{Hlc, SyncMeta};
use chrono::NaiveDate;
use uuid::Uuid;

/// Pantalla a la que lleva un aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeTarget {
    Debts,
    Plans,
    Investments,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub text: String,
    pub tone: Tone,
    pub target: NoticeTarget,
}

/// Cuotas vencidas o que vencen en 3 días, presupuestos excedidos y precios
/// que no se actualizan. Las alertas de precio y los periódicos por confirmar
/// tienen su propia tarjeta.
pub fn notices(data: &Data, today: NaiveDate, lang: Lang) -> Vec<Notice> {
    let t = texts(lang);
    let mut out = Vec::new();
    for day in crate::recurring::upcoming(data, today, 3, lang) {
        for item in day
            .items
            .into_iter()
            .filter(|item| item.detail != t.recurring)
        {
            let when = if item.detail == t.overdue {
                t.overdue.to_lowercase()
            } else {
                format!("{} {}", t.due_soon, day.date_text)
            };
            out.push(Notice {
                text: format!("{}: {} · {when}", item.title, item.amount_text),
                tone: if item.detail == t.overdue {
                    Tone::Negative
                } else {
                    Tone::Neutral
                },
                target: NoticeTarget::Debts,
            });
        }
    }
    for budget in crate::planning::budgets_view(data, today, lang) {
        if budget.over && !budget.inactive {
            out.push(Notice {
                text: format!(
                    "{}: {} ({} / {})",
                    t.budget_over, budget.title, budget.spent_text, budget.limit_text
                ),
                tone: Tone::Negative,
                target: NoticeTarget::Plans,
            });
        }
    }
    let stale: Vec<String> = crate::investments::portfolio_view(data, today, lang)
        .positions
        .into_iter()
        .filter(|position| {
            position
                .price_note
                .as_deref()
                .is_some_and(|note| note.starts_with(t.price_stale))
        })
        .map(|position| position.name)
        .collect();
    if !stale.is_empty() {
        out.push(Notice {
            text: format!("{}: {}", t.prices_stale, stale.join(", ")),
            tone: Tone::Neutral,
            target: NoticeTarget::Investments,
        });
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Created,
    Edited,
    Deleted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityView {
    pub id: Uuid,
    /// Milisegundos UTC del cambio.
    pub millis: u64,
    pub when: String,
    pub action: Action,
    pub action_label: &'static str,
    pub kind_label: &'static str,
    pub label: String,
}

fn millis(stamp: &Hlc) -> u64 {
    HlcParts::decode(stamp).map_or(0, |parts| parts.millis)
}

/// Milisegundos de creación de un UUID v7.
fn created_millis(id: Uuid) -> Option<u64> {
    let (seconds, nanos) = id.get_timestamp()?.to_unix();
    Some(seconds * 1000 + u64::from(nanos) / 1_000_000)
}

fn when(millis: u64, utc_offset_minutes: i32, lang: Lang) -> String {
    let local = millis as i64 + i64::from(utc_offset_minutes) * 60_000;
    chrono::DateTime::from_timestamp_millis(local)
        .map(|moment| {
            format!(
                "{} {}",
                format_date(moment.date_naive(), lang),
                moment.format("%H:%M")
            )
        })
        .unwrap_or_default()
}

/// Los últimos cambios del usuario, del más reciente al más antiguo. El id
/// (UUID v7) dice cuándo se creó cada registro y `updated_at` cuándo cambió;
/// lo generado solo (ocurrencias de periódicos) no aparece.
pub fn activity(
    data: &Data,
    limit: usize,
    utc_offset_minutes: i32,
    lang: Lang,
) -> Vec<ActivityView> {
    let t = texts(lang);
    let mut items: Vec<(SyncMeta, &'static str, String)> = Vec::new();
    for tx in &data.transactions {
        let view = crate::views::transaction_view(data, tx, lang);
        items.push((
            tx.meta.clone(),
            t.transactions,
            format!("{} · {}", view.title, view.amount_text),
        ));
    }
    items.extend(
        data.accounts
            .iter()
            .map(|a| (a.meta.clone(), t.accounts, a.name.clone())),
    );
    items.extend(
        data.categories
            .iter()
            .map(|c| (c.meta.clone(), t.categories, c.name.clone())),
    );
    items.extend(data.debts.iter().map(|d| {
        let name = data
            .contact_name(d.contact_id)
            .map(str::to_owned)
            .or_else(|| d.description.clone())
            .unwrap_or_else(|| t.direction(d.direction).to_owned());
        (d.meta.clone(), t.debts, name)
    }));
    items.extend(
        data.goals
            .iter()
            .map(|g| (g.meta.clone(), t.goals, g.name.clone())),
    );
    items.extend(
        data.assets
            .iter()
            .map(|a| (a.meta.clone(), t.investments, a.name.clone())),
    );
    items.extend(data.rules.iter().map(|r| {
        (
            r.meta.clone(),
            t.recurring,
            r.name.clone().unwrap_or_default(),
        )
    }));
    items.extend(
        data.category_rules
            .iter()
            .map(|r| (r.meta.clone(), t.rules_title, r.pattern.clone())),
    );
    items.extend(
        data.templates
            .iter()
            .map(|r| (r.meta.clone(), t.templates_title, r.name.clone())),
    );
    for item in &data.trash {
        let kind = match item.record.table {
            caudal_sync::Table::Transactions => t.transactions,
            caudal_sync::Table::Accounts => t.accounts,
            caudal_sync::Table::Categories => t.categories,
            caudal_sync::Table::Debts => t.debts,
            caudal_sync::Table::Assets | caudal_sync::Table::Trades => t.investments,
            _ => t.trash,
        };
        let meta = SyncMeta {
            id: item.record.id,
            user_id: data.user_id,
            updated_at: item.record.updated_at.clone(),
            deleted_at: Some(item.record.updated_at.clone()),
            server_seq: None,
        };
        items.push((meta, kind, item.label.clone()));
    }

    let mut views: Vec<ActivityView> = items
        .into_iter()
        .filter_map(|(meta, kind_label, label)| {
            let changed = millis(&meta.updated_at);
            if changed == 0 {
                return None;
            }
            let action = if meta.deleted_at.is_some() {
                Action::Deleted
            } else if created_millis(meta.id)
                .is_some_and(|created| changed.abs_diff(created) < 5_000)
            {
                Action::Created
            } else {
                Action::Edited
            };
            Some(ActivityView {
                id: meta.id,
                millis: changed,
                when: when(changed, utc_offset_minutes, lang),
                action,
                action_label: match action {
                    Action::Created => t.created,
                    Action::Edited => t.edited,
                    Action::Deleted => t.deleted,
                },
                kind_label,
                label,
            })
        })
        .collect();
    views.sort_by(|a, b| b.millis.cmp(&a.millis).then(a.id.cmp(&b.id)));
    views.dedup_by_key(|view| view.id);
    views.truncate(limit);
    views
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{AccountForm, TransactionForm};
    use caudal_core::hlc::HlcParts;

    fn stamp(millis: u64) -> Hlc {
        HlcParts {
            millis,
            counter: 0,
            node: 1,
        }
        .encode()
    }

    #[test]
    fn creado_editado_y_generado() {
        let mut data = Data::default();
        let mut account = AccountForm {
            name: "Banco".into(),
            ..AccountForm::new(&data)
        }
        .validate(data.user_id, Lang::Es)
        .unwrap();
        let created = created_millis(account.meta.id).unwrap();
        account.meta.updated_at = stamp(created + 10);
        data.accounts.push(account);
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let mut tx = TransactionForm::new(&data, today);
        tx.amount = "10".into();
        let mut tx = tx.validate(&data, data.user_id, Lang::Es).unwrap();
        tx.meta.updated_at = stamp(created + 3_600_000);
        data.transactions.push(tx);
        let mut generated = data.transactions[0].clone();
        generated.meta.id = Uuid::now_v7();
        generated.meta.updated_at = stamp(0);
        data.transactions.push(generated);

        let list = activity(&data, 10, 0, Lang::Es);
        assert_eq!(list.len(), 2, "lo generado no aparece");
        assert_eq!(list[0].action, Action::Edited);
        assert_eq!(list[1].action, Action::Created);
        assert_eq!(list[1].kind_label, "Cuentas");
    }
}
