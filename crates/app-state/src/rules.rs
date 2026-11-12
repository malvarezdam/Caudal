//! Reglas de autocategorización y plantillas de movimiento. Solo sugieren:
//! el usuario puede cambiar la categoría que proponga una regla.

use crate::catalog::fold;
use crate::data::Data;
use crate::format::{amount_for_input, format_money};
use crate::forms::{
    FormError, TransactionForm, currency, meta, optional_amount, optional_text, required_name,
};
use crate::i18n::Lang;
use caudal_core::model::{CategoryKind, CategoryRule, TransactionKind, TransactionTemplate};
use caudal_core::money::Money;
use chrono::NaiveDate;
use uuid::Uuid;

/// Categoría y etiquetas sugeridas para una descripción. Gana la regla con el
/// texto más largo que aparezca; a igual largo, la más antigua.
pub fn categorize(
    data: &Data,
    description: &str,
    kind: TransactionKind,
) -> Option<(Uuid, Vec<Uuid>)> {
    let text = fold(description);
    if text.trim().is_empty() {
        return None;
    }
    data.category_rules
        .iter()
        .filter(|rule| rule.active && !rule.meta.is_deleted())
        .filter(|rule| {
            let pattern = fold(rule.pattern.trim());
            !pattern.is_empty() && text.contains(&pattern)
        })
        .filter(|rule| {
            data.category(rule.category_id).is_some_and(|category| {
                !category.meta.is_deleted()
                    && match kind {
                        TransactionKind::Income => category.kind != CategoryKind::Expense,
                        TransactionKind::Expense => category.kind != CategoryKind::Income,
                        _ => true,
                    }
            })
        })
        .max_by_key(|rule| {
            (
                rule.pattern.trim().chars().count(),
                std::cmp::Reverse(rule.meta.id),
            )
        })
        .map(|rule| (rule.category_id, rule.tag_ids.clone()))
}

impl TransactionForm {
    /// Si no hay categoría elegida, aplica la regla que coincida. Devuelve si cambió algo.
    pub fn apply_rules(&mut self, data: &Data) -> bool {
        if self.category_id.is_some() || matches!(self.kind, TransactionKind::Transfer) {
            return false;
        }
        match categorize(data, &self.description, self.kind) {
            Some((category, tags)) => {
                self.category_id = Some(category);
                for tag in tags {
                    if !self.tag_ids.contains(&tag) {
                        self.tag_ids.push(tag);
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Movimiento nuevo con los datos de la plantilla y la fecha de hoy.
    pub fn from_template(
        template: &TransactionTemplate,
        data: &Data,
        today: NaiveDate,
        lang: Lang,
    ) -> Self {
        let mut form = Self::new(data, today);
        form.kind = template.kind;
        form.amount = template
            .amount
            .map(|amount| amount_for_input(amount, lang))
            .unwrap_or_default();
        form.currency = template.currency.to_string();
        let live = |id: Option<Uuid>| {
            id.filter(|id| {
                data.account(*id)
                    .is_some_and(|a| !a.archived && !a.meta.is_deleted())
            })
        };
        form.account_id = live(template.account_id).or(form.account_id);
        if let Some(account) = form.account_id.and_then(|id| data.account(id)) {
            form.currency = account.currency.to_string();
        }
        form.to_account_id = live(template.to_account_id);
        form.category_id = template
            .category_id
            .filter(|id| data.category(*id).is_some_and(|c| !c.meta.is_deleted()));
        form.tag_ids = template.tag_ids.clone();
        form.description = template.description.clone().unwrap_or_default();
        form
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleForm {
    pub pattern: String,
    pub category_id: Option<Uuid>,
    pub tag_ids: Vec<Uuid>,
    pub active: bool,
    pub original: Option<CategoryRule>,
}

impl RuleForm {
    pub fn new() -> Self {
        Self {
            active: true,
            ..Self::default()
        }
    }

    /// Regla a partir de un movimiento ya categorizado: "siempre que diga esto".
    pub fn from_description(description: &str, category_id: Option<Uuid>) -> Self {
        Self {
            pattern: description.trim().to_owned(),
            category_id,
            ..Self::new()
        }
    }

    pub fn edit(rule: &CategoryRule) -> Self {
        Self {
            pattern: rule.pattern.clone(),
            category_id: Some(rule.category_id),
            tag_ids: rule.tag_ids.clone(),
            active: rule.active,
            original: Some(rule.clone()),
        }
    }

    pub fn validate(&self, data: &Data) -> Result<CategoryRule, FormError> {
        Ok(CategoryRule {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            pattern: required_name(&self.pattern)?,
            category_id: self.category_id.ok_or(FormError::CategoryRequired)?,
            tag_ids: self.tag_ids.clone(),
            active: self.active,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateForm {
    pub name: String,
    pub kind: TransactionKind,
    pub amount: String,
    pub currency: String,
    pub account_id: Option<Uuid>,
    pub to_account_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub tag_ids: Vec<Uuid>,
    pub description: String,
    pub original: Option<TransactionTemplate>,
}

impl TemplateForm {
    /// Plantilla con los datos del formulario de movimiento que se está llenando.
    pub fn from_transaction(form: &TransactionForm) -> Self {
        Self {
            name: form.description.trim().to_owned(),
            kind: form.kind,
            amount: form.amount.clone(),
            currency: form.currency.clone(),
            account_id: form.account_id,
            to_account_id: form.to_account_id,
            category_id: form.category_id,
            tag_ids: form.tag_ids.clone(),
            description: form.description.clone(),
            original: None,
        }
    }

    pub fn edit(template: &TransactionTemplate, lang: Lang) -> Self {
        Self {
            name: template.name.clone(),
            kind: template.kind,
            amount: template
                .amount
                .map(|amount| amount_for_input(amount, lang))
                .unwrap_or_default(),
            currency: template.currency.to_string(),
            account_id: template.account_id,
            to_account_id: template.to_account_id,
            category_id: template.category_id,
            tag_ids: template.tag_ids.clone(),
            description: template.description.clone().unwrap_or_default(),
            original: Some(template.clone()),
        }
    }

    pub fn validate(&self, data: &Data, lang: Lang) -> Result<TransactionTemplate, FormError> {
        let amount =
            if self.kind == TransactionKind::Adjustment && self.amount.trim().starts_with('-') {
                Some(crate::forms::amount(&self.amount, lang, true)?)
            } else {
                optional_amount(&self.amount, lang)?
            };
        Ok(TransactionTemplate {
            meta: self.original.as_ref().map_or_else(
                || meta(None, data.user_id),
                |original| original.meta.clone(),
            ),
            name: required_name(&self.name)?,
            kind: self.kind,
            amount,
            currency: currency(&self.currency)?,
            account_id: self.account_id,
            to_account_id: self
                .to_account_id
                .filter(|_| self.kind == TransactionKind::Transfer),
            category_id: self.category_id,
            tag_ids: self.tag_ids.clone(),
            description: optional_text(&self.description),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleView {
    pub id: Uuid,
    pub pattern: String,
    pub category: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateView {
    pub id: Uuid,
    pub name: String,
    /// "Gasto · 3.500 CLP" o solo el tipo si no tiene monto.
    pub detail: String,
}

pub fn rules_view(data: &Data) -> Vec<RuleView> {
    let mut rules: Vec<RuleView> = data
        .category_rules
        .iter()
        .filter(|rule| !rule.meta.is_deleted())
        .map(|rule| RuleView {
            id: rule.meta.id,
            pattern: rule.pattern.clone(),
            category: data.category_path(rule.category_id).unwrap_or_default(),
            active: rule.active,
        })
        .collect();
    rules.sort_by_key(|rule| fold(&rule.pattern));
    rules
}

pub fn templates_view(data: &Data, lang: Lang) -> Vec<TemplateView> {
    let t = crate::i18n::texts(lang);
    let mut templates: Vec<TemplateView> = data
        .templates
        .iter()
        .filter(|template| !template.meta.is_deleted())
        .map(|template| TemplateView {
            id: template.meta.id,
            name: template.name.clone(),
            detail: match template.amount {
                Some(amount) => format!(
                    "{} · {}",
                    t.transaction_kind(template.kind),
                    format_money(&Money::new(amount, template.currency.clone()), lang)
                ),
                None => t.transaction_kind(template.kind).to_owned(),
            },
        })
        .collect();
    templates.sort_by_key(|template| fold(&template.name));
    templates
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudal_core::model::Category;

    fn category(data: &mut Data, name: &str, kind: CategoryKind) -> Uuid {
        let category = Category {
            meta: meta(None, data.user_id),
            name: name.into(),
            parent_id: None,
            kind,
            color: None,
            icon: None,
        };
        let id = category.meta.id;
        data.categories.push(category);
        id
    }

    fn rule(data: &mut Data, pattern: &str, category_id: Uuid) {
        let rule = RuleForm {
            pattern: pattern.into(),
            category_id: Some(category_id),
            ..RuleForm::new()
        }
        .validate(data)
        .unwrap();
        data.category_rules.push(rule);
    }

    #[test]
    fn la_regla_mas_especifica_gana_sin_mayusculas_ni_tildes() {
        let mut data = Data::default();
        let transport = category(&mut data, "Transporte", CategoryKind::Expense);
        let food = category(&mut data, "Comida", CategoryKind::Expense);
        let salary = category(&mut data, "Sueldo", CategoryKind::Income);
        rule(&mut data, "uber", transport);
        rule(&mut data, "Uber Eats", food);
        rule(&mut data, "remuneración", salary);

        assert_eq!(
            categorize(&data, "UBER *TRIP", TransactionKind::Expense).map(|c| c.0),
            Some(transport)
        );
        assert_eq!(
            categorize(&data, "uber eats santiago", TransactionKind::Expense).map(|c| c.0),
            Some(food)
        );
        assert_eq!(
            categorize(&data, "Remuneracion octubre", TransactionKind::Income).map(|c| c.0),
            Some(salary)
        );
        // Una categoría de ingresos no se sugiere para un gasto.
        assert_eq!(
            categorize(&data, "remuneracion", TransactionKind::Expense),
            None
        );
        assert_eq!(categorize(&data, "", TransactionKind::Expense), None);

        data.category_rules[0].active = false;
        assert_eq!(
            categorize(&data, "UBER *TRIP", TransactionKind::Expense),
            None
        );
    }

    #[test]
    fn el_formulario_respeta_la_categoria_elegida() {
        let mut data = Data::default();
        let transport = category(&mut data, "Transporte", CategoryKind::Expense);
        let other = category(&mut data, "Otra", CategoryKind::Expense);
        rule(&mut data, "metro", transport);
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let mut form = TransactionForm::new(&data, today);
        form.description = "Carga metro".into();
        assert!(form.apply_rules(&data));
        assert_eq!(form.category_id, Some(transport));
        form.category_id = Some(other);
        assert!(!form.apply_rules(&data));
        assert_eq!(form.category_id, Some(other));
    }

    #[test]
    fn plantilla_ida_y_vuelta() {
        let data = Data::default();
        let today = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let mut form = TransactionForm::new(&data, today);
        form.amount = "3500".into();
        form.currency = "CLP".into();
        form.description = "Café".into();
        let template = TemplateForm::from_transaction(&form)
            .validate(&data, Lang::Es)
            .unwrap();
        assert_eq!(template.name, "Café");
        let again = TransactionForm::from_template(&template, &data, today, Lang::Es);
        assert_eq!(
            (
                again.amount.as_str(),
                again.description.as_str(),
                again.date
            ),
            ("3500", "Café", today)
        );
        let mut empty = TemplateForm::from_transaction(&form);
        empty.amount.clear();
        let template = empty.validate(&data, Lang::Es).unwrap();
        assert_eq!(template.amount, None);
    }
}
