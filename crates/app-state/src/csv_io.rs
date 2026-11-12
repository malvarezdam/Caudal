//! CSV: exportar movimientos para una planilla e importar la cartola del banco.
//! La importación adivina columnas, muestra una vista previa y aparta los
//! movimientos que ya estaban registrados.

use crate::catalog::fold;
use crate::data::Data;
use crate::format::{format_date, parse_amount};
use crate::forms::meta;
use crate::i18n::{Lang, texts};
use crate::rules::categorize;
use caudal_core::model::{Transaction, TransactionKind, TransactionStatus};
use caudal_core::money::Currency;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Marca de orden de bytes: Excel la necesita para leer tildes en UTF-8.
const BOM: &str = "\u{feff}";

fn amount_text(amount: Decimal, lang: Lang) -> String {
    let text = amount.normalize().to_string();
    match lang {
        Lang::Es => text.replace('.', ","),
        Lang::En => text,
    }
}

/// Todos los movimientos activos, del más reciente al más antiguo. En español
/// usa `;` y coma decimal, como espera Excel con configuración regional
/// hispana; en inglés, `,` y punto.
pub fn export_transactions(data: &Data, lang: Lang) -> String {
    let t = texts(lang);
    let delimiter = if lang == Lang::Es { b';' } else { b',' };
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    let headers = match lang {
        Lang::Es => [
            "Fecha",
            "Tipo",
            "Monto",
            "Moneda",
            "Cuenta",
            "Cuenta destino",
            "Monto recibido",
            "Categoría",
            "Etiquetas",
            "Descripción",
            "Nota",
            "Estado",
        ],
        Lang::En => [
            "Date",
            "Type",
            "Amount",
            "Currency",
            "Account",
            "To account",
            "Received amount",
            "Category",
            "Tags",
            "Description",
            "Note",
            "Status",
        ],
    };
    let _ = writer.write_record(headers);
    let mut transactions: Vec<&Transaction> = data
        .transactions
        .iter()
        .filter(|tx| !tx.meta.is_deleted())
        .collect();
    transactions.sort_by(|a, b| {
        b.occurred_on
            .cmp(&a.occurred_on)
            .then(a.meta.id.cmp(&b.meta.id))
    });
    for tx in transactions {
        let tags: Vec<String> = tx
            .tag_ids
            .iter()
            .filter_map(|id| data.tag(*id).map(|tag| tag.name.clone()))
            .collect();
        let _ = writer.write_record([
            tx.occurred_on.to_string(),
            t.transaction_kind(tx.kind).to_owned(),
            amount_text(tx.amount, lang),
            tx.currency.to_string(),
            data.account_name(tx.account_id)
                .unwrap_or_default()
                .to_owned(),
            data.account_name(tx.to_account_id)
                .unwrap_or_default()
                .to_owned(),
            tx.to_amount
                .map(|v| amount_text(v, lang))
                .unwrap_or_default(),
            tx.category_id
                .and_then(|id| data.category_path(id))
                .unwrap_or_default(),
            tags.join(", "),
            tx.description.clone().unwrap_or_default(),
            tx.note.clone().unwrap_or_default(),
            match tx.status {
                TransactionStatus::Pending => t.pending.to_owned(),
                _ => String::new(),
            },
        ]);
    }
    let bytes = writer.into_inner().unwrap_or_default();
    format!("{BOM}{}", String::from_utf8_lossy(&bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    Empty,
    Unreadable,
}

/// Archivo leído: cabeceras y filas de texto.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// UTF-8 (con o sin BOM) o, si no lo es, Latin-1/Windows-1252, que es lo que
/// exportan muchos bancos.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim_start_matches(BOM).to_owned(),
        Err(_) => bytes.iter().map(|&byte| byte as char).collect(),
    }
}

/// El separador que más se repite, igual, en las primeras líneas.
fn detect_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .collect();
    b";,\t|"
        .iter()
        .copied()
        .max_by_key(|&delimiter| {
            let counts: Vec<usize> = lines
                .iter()
                .map(|line| line.bytes().filter(|&b| b == delimiter).count())
                .collect();
            let first = counts.first().copied().unwrap_or(0);
            let consistent = counts.iter().all(|&count| count == first);
            (first > 0, consistent, first)
        })
        .unwrap_or(b',')
}

impl CsvTable {
    pub fn parse(bytes: &[u8]) -> Result<Self, ImportError> {
        let text = decode(bytes);
        if text.trim().is_empty() {
            return Err(ImportError::Empty);
        }
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(detect_delimiter(&text))
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());
        let mut rows: Vec<Vec<String>> = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|_| ImportError::Unreadable)?;
            let row: Vec<String> = record.iter().map(|cell| cell.trim().to_owned()).collect();
            if row.iter().any(|cell| !cell.is_empty()) {
                rows.push(row);
            }
        }
        if rows.is_empty() {
            return Err(ImportError::Empty);
        }
        let headers = rows.remove(0);
        Ok(Self { headers, rows })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateOrder {
    /// 31/12/2026, como en Chile y España.
    #[default]
    DayFirst,
    /// 12/31/2026, como en Estados Unidos.
    MonthFirst,
}

/// Qué columna es cada dato. Los índices son posiciones en `CsvTable::headers`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportMapping {
    pub date: Option<usize>,
    /// Monto con signo: negativo es gasto.
    pub amount: Option<usize>,
    /// O bien columnas separadas de cargo y abono.
    pub debit: Option<usize>,
    pub credit: Option<usize>,
    pub description: Option<usize>,
    pub category: Option<usize>,
    pub date_order: DateOrder,
    /// Para bancos que exportan los gastos en positivo.
    pub invert: bool,
    pub account_id: Option<Uuid>,
    pub currency: String,
    /// Importar también lo que parece repetido.
    pub include_duplicates: bool,
}

fn find(headers: &[String], names: &[&str]) -> Option<usize> {
    let folded: Vec<String> = headers.iter().map(|h| fold(h)).collect();
    names
        .iter()
        .find_map(|name| folded.iter().position(|header| header == name))
        .or_else(|| {
            names
                .iter()
                .find_map(|name| folded.iter().position(|header| header.contains(name)))
        })
}

impl ImportMapping {
    /// Adivina las columnas por su nombre y el orden de la fecha por sus valores.
    pub fn guess(table: &CsvTable, data: &Data) -> Self {
        let headers = &table.headers;
        let date = find(
            headers,
            &["fecha", "date", "fecha operacion", "fecha contable"],
        );
        let amount = find(headers, &["monto", "importe", "amount", "valor", "total"]);
        let debit = find(
            headers,
            &["cargo", "cargos", "debito", "debe", "debit", "withdrawal"],
        );
        let credit = find(
            headers,
            &["abono", "abonos", "credito", "haber", "credit", "deposit"],
        );
        let description = find(
            headers,
            &[
                "descripcion",
                "glosa",
                "detalle",
                "concepto",
                "description",
                "memo",
                "payee",
            ],
        );
        let category = find(headers, &["categoria", "category"]);
        let accounts = data.selectable_accounts(None);
        let account = (accounts.len() == 1).then(|| accounts[0]);
        let currency = account
            .map(|a| a.currency.to_string())
            .or_else(|| data.profile.as_ref().map(|p| p.base_currency.to_string()))
            .unwrap_or_else(|| "USD".into());
        let mut mapping = Self {
            date,
            // Con cargo y abono separados no se usa una columna de monto.
            amount: if debit.is_some() && credit.is_some() {
                None
            } else {
                amount
            },
            debit,
            credit,
            description,
            category,
            account_id: account.map(|a| a.meta.id),
            currency,
            ..Self::default()
        };
        if let Some(column) = date {
            let values = table.rows.iter().filter_map(|row| row.get(column));
            let day_first = values
                .clone()
                .filter(|v| parse_date(v, DateOrder::DayFirst).is_some())
                .count();
            let month_first = values
                .filter(|v| parse_date(v, DateOrder::MonthFirst).is_some())
                .count();
            if month_first > day_first {
                mapping.date_order = DateOrder::MonthFirst;
            }
        }
        mapping
    }
}

/// Fecha en formato ISO, o con día y mes en el orden indicado; ignora la hora.
pub fn parse_date(text: &str, order: DateOrder) -> Option<NaiveDate> {
    let text = text.trim().split([' ', 'T']).next()?;
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(date);
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y/%m/%d") {
        return Some(date);
    }
    let formats: &[&str] = match order {
        DateOrder::DayFirst => &[
            "%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%d/%m/%y", "%d-%m-%y", "%d.%m.%y",
        ],
        DateOrder::MonthFirst => &["%m/%d/%Y", "%m-%d-%Y", "%m/%d/%y"],
    };
    formats.iter().find_map(|format| {
        NaiveDate::parse_from_str(text, format)
            .ok()
            // Años de dos cifras: 26 es 2026.
            .filter(|date| (1970..=2100).contains(&chrono::Datelike::year(date)))
    })
}

/// Monto como lo escriben los bancos: con símbolo de moneda, espacios o
/// negativos entre paréntesis.
pub fn parse_bank_amount(text: &str, lang: Lang) -> Option<Decimal> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parenthesis = trimmed.starts_with('(') && trimmed.ends_with(')');
    let trailing_minus = trimmed.ends_with('-');
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+'))
        .collect();
    let cleaned = cleaned.trim_end_matches('-');
    let value = parse_amount(cleaned, lang).ok()?;
    Some(if parenthesis || trailing_minus {
        -value.abs()
    } else {
        value
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowStatus {
    New,
    /// Ya hay un movimiento con la misma fecha, cuenta y monto.
    Duplicate,
    /// Sin fecha o sin monto legible.
    Invalid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRow {
    /// Número de fila en el archivo, contando la cabecera como 1.
    pub line: usize,
    pub date: String,
    pub amount: String,
    pub kind: TransactionKind,
    pub description: String,
    pub category: Option<String>,
    pub status: RowStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportPreview {
    pub rows: Vec<PreviewRow>,
    pub new: usize,
    pub duplicates: usize,
    pub invalid: usize,
    /// Movimientos listos para guardar, según la configuración.
    pub transactions: Vec<Transaction>,
}

/// Clave para detectar repetidos: fecha, cuenta y monto con signo.
fn duplicate_key(tx: &Transaction) -> (NaiveDate, Option<Uuid>, Decimal) {
    let signed = match tx.kind {
        TransactionKind::Expense | TransactionKind::Transfer => -tx.amount,
        _ => tx.amount,
    };
    (tx.occurred_on, tx.account_id, signed.normalize())
}

fn category_by_name(data: &Data, name: &str) -> Option<Uuid> {
    let target = fold(name);
    if target.is_empty() {
        return None;
    }
    data.categories
        .iter()
        .filter(|c| !c.meta.is_deleted())
        .find(|c| {
            fold(&c.name) == target
                || data
                    .category_path(c.meta.id)
                    .is_some_and(|p| fold(&p) == target)
        })
        .map(|c| c.meta.id)
}

pub fn preview(
    data: &Data,
    table: &CsvTable,
    mapping: &ImportMapping,
    lang: Lang,
) -> ImportPreview {
    let currency = Currency::new(&mapping.currency.trim().to_uppercase()).ok();
    // Cuántas veces está ya registrado cada movimiento: dos cafés iguales el
    // mismo día en el archivo y uno ya guardado dejan uno nuevo.
    let mut existing: BTreeMap<(NaiveDate, Option<Uuid>, Decimal), usize> = BTreeMap::new();
    for tx in data.transactions.iter().filter(|tx| !tx.meta.is_deleted()) {
        *existing.entry(duplicate_key(tx)).or_default() += 1;
    }
    let cell = |row: &Vec<String>, column: Option<usize>| -> String {
        column.and_then(|c| row.get(c)).cloned().unwrap_or_default()
    };
    let mut out = ImportPreview {
        rows: Vec::new(),
        new: 0,
        duplicates: 0,
        invalid: 0,
        transactions: Vec::new(),
    };
    for (index, row) in table.rows.iter().enumerate() {
        let date = parse_date(&cell(row, mapping.date), mapping.date_order);
        let signed = match (mapping.amount, mapping.debit, mapping.credit) {
            (Some(column), _, _) => parse_bank_amount(&cell(row, Some(column)), lang),
            (None, debit, credit) => {
                let out = parse_bank_amount(&cell(row, debit), lang).map(|v| -v.abs());
                let into = parse_bank_amount(&cell(row, credit), lang).map(|v| v.abs());
                match (out.filter(|v| !v.is_zero()), into.filter(|v| !v.is_zero())) {
                    (Some(out), None) => Some(out),
                    (None, Some(into)) => Some(into),
                    (Some(out), Some(into)) => Some(into + out),
                    (None, None) => None,
                }
            }
        }
        .map(|value| if mapping.invert { -value } else { value })
        .filter(|value| !value.is_zero());
        let description = cell(row, mapping.description);
        let kind = if signed.is_some_and(|v| v.is_sign_negative()) {
            TransactionKind::Expense
        } else {
            TransactionKind::Income
        };
        let category_id = category_by_name(data, &cell(row, mapping.category))
            .or_else(|| categorize(data, &description, kind).map(|(id, _)| id));
        let tags = categorize(data, &description, kind)
            .map(|(_, tags)| tags)
            .unwrap_or_default();

        let (status, transaction) = match (date, signed, &currency) {
            (Some(date), Some(signed), Some(currency)) => {
                let tx = Transaction {
                    meta: meta(None, data.user_id),
                    kind,
                    status: TransactionStatus::Confirmed,
                    amount: signed.abs(),
                    currency: currency.clone(),
                    occurred_on: date,
                    account_id: mapping.account_id,
                    to_account_id: None,
                    account_amount: None,
                    to_amount: None,
                    base_amount: None,
                    base_currency: None,
                    category_id,
                    tag_ids: tags,
                    contact_id: None,
                    recurring_rule_id: None,
                    occurrence_date: None,
                    description: (!description.is_empty()).then(|| description.clone()),
                    note: None,
                };
                let key = duplicate_key(&tx);
                let seen = existing.get_mut(&key).filter(|count| **count > 0);
                match seen {
                    Some(count) => {
                        *count -= 1;
                        (RowStatus::Duplicate, Some(tx))
                    }
                    None => (RowStatus::New, Some(tx)),
                }
            }
            _ => (RowStatus::Invalid, None),
        };
        match status {
            RowStatus::New => out.new += 1,
            RowStatus::Duplicate => out.duplicates += 1,
            RowStatus::Invalid => out.invalid += 1,
        }
        if let Some(tx) = &transaction
            && (status == RowStatus::New || mapping.include_duplicates)
        {
            out.transactions.push(tx.clone());
        }
        out.rows.push(PreviewRow {
            line: index + 2,
            date: date
                .map(|d| format_date(d, lang))
                .unwrap_or_else(|| cell(row, mapping.date)),
            amount: signed
                .map(|v| amount_text(v, lang))
                .unwrap_or_else(|| cell(row, mapping.amount.or(mapping.debit))),
            kind,
            description,
            category: category_id.and_then(|id| data.category_path(id)),
            status,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::{AccountForm, ProfileForm, TransactionForm};
    use crate::rules::RuleForm;
    use caudal_core::model::{Category, CategoryKind};

    fn data() -> Data {
        let user = Uuid::from_u128(1);
        let profile = ProfileForm {
            base_currency: "CLP".into(),
            lang: Lang::Es,
            display_name: String::new(),
        }
        .validate(user)
        .unwrap();
        let mut data = Data {
            user_id: user,
            profile: Some(profile),
            ..Data::default()
        };
        let account = AccountForm {
            name: "Cuenta corriente".into(),
            currency: "CLP".into(),
            ..AccountForm::new(&data)
        }
        .validate(user, Lang::Es)
        .unwrap();
        data.accounts.push(account);
        data
    }

    fn day(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    #[test]
    fn exporta_con_punto_y_coma_coma_decimal_y_bom() {
        let mut data = data();
        let account = data.accounts[0].meta.id;
        let mut form = TransactionForm::new(&data, day("2026-11-10"));
        form.amount = "1234,5".into();
        form.description = "Almuerzo; con \"amigos\"".into();
        form.account_id = Some(account);
        data.transactions
            .push(form.validate(&data, data.user_id, Lang::Es).unwrap());
        let text = export_transactions(&data, Lang::Es);
        assert!(text.starts_with(BOM));
        let lines: Vec<&str> = text.trim_start_matches(BOM).lines().collect();
        assert_eq!(
            lines[0],
            "Fecha;Tipo;Monto;Moneda;Cuenta;Cuenta destino;Monto recibido;Categoría;Etiquetas;Descripción;Nota;Estado"
        );
        assert_eq!(
            lines[1],
            "2026-11-10;Gasto;1234,5;CLP;Cuenta corriente;;;;;\"Almuerzo; con \"\"amigos\"\"\";;"
        );

        // Lo exportado se vuelve a leer igual.
        let table = CsvTable::parse(text.as_bytes()).unwrap();
        assert_eq!(table.rows[0][9], "Almuerzo; con \"amigos\"");
    }

    #[test]
    fn cartola_de_banco_con_cargo_y_abono_en_latin1() {
        let mut data = data();
        let transport = Category {
            meta: meta(None, data.user_id),
            name: "Transporte".into(),
            parent_id: None,
            kind: CategoryKind::Expense,
            color: None,
            icon: None,
        };
        let transport_id = transport.meta.id;
        data.categories.push(transport);
        data.category_rules.push(
            RuleForm {
                pattern: "uber".into(),
                category_id: Some(transport_id),
                ..RuleForm::new()
            }
            .validate(&data)
            .unwrap(),
        );
        // Latin-1: "Descripción" con ó = 0xF3.
        let mut bytes = b"Fecha;Descripci\xf3n;Cargos;Abonos;Saldo\n".to_vec();
        bytes.extend_from_slice(b"10/11/2026;UBER *TRIP;$ 5.990;;100.000\n");
        bytes.extend_from_slice(b"10/11/2026;UBER *TRIP;$ 5.990;;94.010\n");
        bytes.extend_from_slice(b"11/11/2026;Transferencia recibida;;150.000;244.010\n");
        bytes.extend_from_slice(b"sin fecha;Fila rota;1;;\n");
        let table = CsvTable::parse(&bytes).unwrap();
        assert_eq!(table.headers[1], "Descripción");
        let mapping = ImportMapping::guess(&table, &data);
        assert_eq!(
            (
                mapping.date,
                mapping.debit,
                mapping.credit,
                mapping.description
            ),
            (Some(0), Some(2), Some(3), Some(1))
        );
        assert_eq!(mapping.amount, None);

        // Uno de los dos viajes ya estaba registrado.
        let mut existing = TransactionForm::new(&data, day("2026-11-10"));
        existing.amount = "5990".into();
        existing.account_id = mapping.account_id;
        data.transactions
            .push(existing.validate(&data, data.user_id, Lang::Es).unwrap());

        let preview = preview(&data, &table, &mapping, Lang::Es);
        assert_eq!(
            (preview.new, preview.duplicates, preview.invalid),
            (2, 1, 1)
        );
        assert_eq!(preview.rows[0].status, RowStatus::Duplicate);
        assert_eq!(preview.rows[1].status, RowStatus::New);
        assert_eq!(preview.rows[1].category.as_deref(), Some("Transporte"));
        assert_eq!(preview.rows[3].status, RowStatus::Invalid);
        assert_eq!(preview.transactions.len(), 2);
        let income = &preview.transactions[1];
        assert_eq!(
            (income.kind, income.amount.to_string().as_str()),
            (TransactionKind::Income, "150000")
        );

        let all = ImportMapping {
            include_duplicates: true,
            ..mapping
        };
        assert_eq!(
            super::preview(&data, &table, &all, Lang::Es)
                .transactions
                .len(),
            3
        );
    }

    #[test]
    fn formato_estadounidense_con_columna_de_monto() {
        let data = data();
        let text = "Date,Description,Amount\n12/31/2026,Coffee,-4.50\n01/02/2027,Refund,(10.00)\n01/03/2027,Salary,\"1,250.00\"\n";
        let table = CsvTable::parse(text.as_bytes()).unwrap();
        let mapping = ImportMapping::guess(&table, &data);
        assert_eq!(mapping.date_order, DateOrder::MonthFirst);
        let preview = preview(&data, &table, &mapping, Lang::En);
        let amounts: Vec<String> = preview
            .transactions
            .iter()
            .map(|t| format!("{:?} {}", t.kind, t.amount))
            .collect();
        assert_eq!(amounts, ["Expense 4.50", "Expense 10.00", "Income 1250.00"]);
        assert_eq!(preview.transactions[0].occurred_on, day("2026-12-31"));
    }

    #[test]
    fn montos_y_fechas_de_banco() {
        assert_eq!(
            parse_bank_amount("$ 1.234", Lang::Es).unwrap().to_string(),
            "1234"
        );
        assert_eq!(
            parse_bank_amount("1.234,56 €", Lang::Es)
                .unwrap()
                .to_string(),
            "1234.56"
        );
        assert_eq!(
            parse_bank_amount("(12,50)", Lang::Es).unwrap().to_string(),
            "-12.50"
        );
        assert_eq!(
            parse_bank_amount("500-", Lang::Es).unwrap().to_string(),
            "-500"
        );
        assert_eq!(parse_bank_amount("", Lang::Es), None);
        assert_eq!(
            parse_date("2026-11-10 13:45:00", DateOrder::DayFirst),
            Some(day("2026-11-10"))
        );
        assert_eq!(
            parse_date("10.11.26", DateOrder::DayFirst),
            Some(day("2026-11-10"))
        );
        assert_eq!(parse_date("31/12/2026", DateOrder::MonthFirst), None);
        assert_eq!(CsvTable::parse(b"  \n"), Err(ImportError::Empty));
    }
}
