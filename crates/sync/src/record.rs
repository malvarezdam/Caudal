use crate::table::Table;
use caudal_core::model::Hlc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

/// Fila tal como se guarda y se envía: un objeto JSON con sus columnas.
pub type Row = serde_json::Map<String, Value>;

#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("la fila no tiene un id válido")]
    MissingId,
    #[error("la fila no tiene updated_at")]
    MissingUpdatedAt,
    #[error("la fila no es un objeto JSON")]
    NotAnObject,
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub table: Table,
    pub id: Uuid,
    pub updated_at: Hlc,
    pub row: Row,
}

impl Record {
    pub fn from_row(table: Table, row: Row) -> Result<Self, RecordError> {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(RecordError::MissingId)?;
        let updated_at = row
            .get("updated_at")
            .and_then(Value::as_str)
            .map(|value| Hlc(value.to_owned()))
            .ok_or(RecordError::MissingUpdatedAt)?;
        Ok(Self {
            table,
            id,
            updated_at,
            row,
        })
    }

    pub fn from_model<T: Serialize>(table: Table, model: &T) -> Result<Self, RecordError> {
        match serde_json::to_value(model)? {
            Value::Object(row) => Self::from_row(table, row),
            _ => Err(RecordError::NotAnObject),
        }
    }

    pub fn to_model<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(Value::Object(self.row.clone()))
    }

    pub fn is_deleted(&self) -> bool {
        self.row
            .get("deleted_at")
            .is_some_and(|value| !value.is_null())
    }

    /// Cursor asignado por el servidor, si la fila viene de él.
    pub fn server_seq(&self) -> Option<i64> {
        match self.row.get("server_seq")? {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.parse().ok(),
            _ => None,
        }
    }

    /// Fila para subir: sin el cursor, que solo asigna el servidor.
    pub fn upload_row(&self) -> Row {
        let mut row = self.row.clone();
        row.remove("server_seq");
        row
    }

    /// Último cambio gana; a igual marca es la misma versión y no se aplica.
    pub fn wins_over(&self, existing: Option<&Record>) -> bool {
        existing.is_none_or(|existing| self.updated_at > existing.updated_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(value: Value) -> Row {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn exige_id_y_marca() {
        let missing_id = row(json!({"updated_at": "1"}));
        assert!(matches!(
            Record::from_row(Table::Tags, missing_id),
            Err(RecordError::MissingId)
        ));
        let missing_stamp = row(json!({"id": Uuid::nil().to_string()}));
        assert!(matches!(
            Record::from_row(Table::Tags, missing_stamp),
            Err(RecordError::MissingUpdatedAt)
        ));
    }

    #[test]
    fn la_marca_mayor_gana_y_la_igual_no() {
        let make = |stamp: &str| {
            Record::from_row(
                Table::Tags,
                row(json!({"id": Uuid::nil().to_string(), "updated_at": stamp})),
            )
            .unwrap()
        };
        let old = make("0001");
        let new = make("0002");
        assert!(new.wins_over(Some(&old)));
        assert!(!old.wins_over(Some(&new)));
        assert!(!new.wins_over(Some(&new.clone())));
        assert!(old.wins_over(None));
    }

    #[test]
    fn no_sube_el_cursor() {
        let record = Record::from_row(
            Table::Tags,
            row(json!({"id": Uuid::nil().to_string(), "updated_at": "1", "server_seq": 9, "deleted_at": "1"})),
        )
        .unwrap();
        assert_eq!(record.server_seq(), Some(9));
        assert!(record.is_deleted());
        assert!(!record.upload_row().contains_key("server_seq"));
    }
}
