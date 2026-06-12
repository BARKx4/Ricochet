use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io;
use std::sync::Arc;

use bytes::BytesMut;
use ricochet_vm::Value;
use tokio_postgres::types::{to_sql_checked, IsNull, ToSql, Type};
use tokio_postgres::{Client, NoTls, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMapping {
    pub class_name: String,
    pub table_name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderPage<'a> {
    pub field: &'a str,
    pub direction: &'a str,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveRecordError {
    InvalidIdentifier {
        kind: &'static str,
        name: String,
    },
    UnknownField {
        class_name: String,
        field: String,
    },
    InvalidOrderDirection {
        direction: String,
    },
    UnknownModel {
        class_name: String,
    },
    MissingTable {
        class_name: String,
    },
    MissingField {
        class_name: String,
        field: String,
    },
    UnsupportedValue {
        operation: &'static str,
        actual: String,
    },
    UnsupportedColumnType {
        field: String,
        postgres_type: String,
    },
    Database {
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for ActiveRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActiveRecordError::InvalidIdentifier { kind, name } => {
                write!(f, "invalid PostgreSQL {kind} identifier {name:?}")
            }
            ActiveRecordError::UnknownField { class_name, field } => {
                write!(f, "unknown field {field:?} on model {class_name}")
            }
            ActiveRecordError::InvalidOrderDirection { direction } => {
                write!(
                    f,
                    "invalid Active Record order direction {direction:?}; expected \"asc\" or \"desc\""
                )
            }
            ActiveRecordError::UnknownModel { class_name } => {
                write!(f, "unknown Ricochet model class {class_name}")
            }
            ActiveRecordError::MissingTable { class_name } => {
                write!(
                    f,
                    "Ricochet model class {class_name} has no table declaration"
                )
            }
            ActiveRecordError::MissingField { class_name, field } => {
                write!(f, "missing field {field:?} for model {class_name}")
            }
            ActiveRecordError::UnsupportedValue { operation, actual } => {
                write!(f, "{operation} does not support Ricochet {actual} values")
            }
            ActiveRecordError::UnsupportedColumnType {
                field,
                postgres_type,
            } => write!(
                f,
                "PostgreSQL field {field:?} has unsupported type {postgres_type}"
            ),
            ActiveRecordError::Database { operation, message } => {
                write!(f, "PostgreSQL {operation} failed: {message}")
            }
        }
    }
}

impl std::error::Error for ActiveRecordError {}

impl ModelMapping {
    pub fn from_vm(vm: &ricochet_vm::Vm, class_name: &str) -> Result<Self, ActiveRecordError> {
        let fields =
            vm.class_fields(class_name)
                .ok_or_else(|| ActiveRecordError::UnknownModel {
                    class_name: class_name.to_string(),
                })?;
        let table_name =
            vm.class_table(class_name)
                .ok_or_else(|| ActiveRecordError::MissingTable {
                    class_name: class_name.to_string(),
                })?;

        Self::try_new(class_name, table_name, fields.iter().cloned())
    }

    pub fn try_new(
        class_name: impl Into<String>,
        table_name: impl Into<String>,
        fields: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ActiveRecordError> {
        let table_name = table_name.into();
        validate_table_identifier(&table_name)?;

        let fields = fields
            .into_iter()
            .map(Into::into)
            .map(|field| {
                validate_field_identifier(&field)?;
                Ok(field)
            })
            .collect::<Result<Vec<_>, ActiveRecordError>>()?;

        Ok(Self {
            class_name: class_name.into(),
            table_name,
            fields,
        })
    }

    pub fn select_by_id_sql(&self) -> String {
        format!(
            "select {} from {} where id = $1 limit 1",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_all_sql(&self) -> String {
        format!("select {} from {}", self.fields.join(", "), self.table_name)
    }

    pub fn select_count_sql(&self) -> String {
        format!("select count(*) from {}", self.table_name)
    }

    pub fn select_first_sql(&self) -> String {
        format!(
            "select {} from {} limit 1",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_limit_sql(&self) -> String {
        format!(
            "select {} from {} limit $1",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_page_sql(&self) -> String {
        format!(
            "select {} from {} limit $1 offset $2",
            self.fields.join(", "),
            self.table_name
        )
    }

    pub fn select_order_page_sql(
        &self,
        field: &str,
        direction: &str,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        let direction = validate_order_direction(direction)?;
        Ok(format!(
            "select {} from {} order by {field} {direction} limit $1 offset $2",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn exists_by_id_sql(&self) -> String {
        format!(
            "select exists(select 1 from {} where id = $1)",
            self.table_name
        )
    }

    pub fn select_where_eq_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = $1",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn select_where_eq_limit_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = $1 limit $2",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn select_where_eq_page_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = $1 limit $2 offset $3",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn select_where_eq_order_page_sql(
        &self,
        where_field: &str,
        order_field: &str,
        direction: &str,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(where_field)?;
        self.require_field(order_field)?;
        let direction = validate_order_direction(direction)?;
        Ok(format!(
            "select {} from {} where {where_field} = $1 order by {order_field} {direction} limit $2 offset $3",
            self.fields.join(", "),
            self.table_name
        ))
    }

    pub fn insert_sql(&self) -> String {
        let fields = self.non_id_fields();
        let placeholders = (1..=fields.len())
            .map(|index| format!("${index}"))
            .collect::<Vec<_>>();

        format!(
            "insert into {} ({}) values ({}) returning {}",
            self.table_name,
            fields.join(", "),
            placeholders.join(", "),
            self.fields.join(", ")
        )
    }

    pub fn update_by_id_sql(&self) -> String {
        let fields = self.non_id_fields();
        let assignments = fields
            .iter()
            .enumerate()
            .map(|(index, field)| format!("{field} = ${}", index + 1))
            .collect::<Vec<_>>();
        let id_parameter = fields.len() + 1;

        format!(
            "update {} set {} where id = ${id_parameter} returning {}",
            self.table_name,
            assignments.join(", "),
            self.fields.join(", ")
        )
    }

    pub fn insert_values(
        &self,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.values_for_fields(&self.non_id_fields(), attributes)
    }

    pub fn update_values(
        &self,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let mut values = self.values_for_fields(&self.non_id_fields(), attributes)?;
        values.push(id);
        Ok(values)
    }

    fn require_field(&self, field: &str) -> Result<(), ActiveRecordError> {
        validate_field_identifier(field)?;
        if self.fields.iter().any(|known| known == field) {
            Ok(())
        } else {
            Err(ActiveRecordError::UnknownField {
                class_name: self.class_name.clone(),
                field: field.to_string(),
            })
        }
    }

    fn non_id_fields(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter(|field| field.as_str() != "id")
            .cloned()
            .collect()
    }

    fn values_for_fields(
        &self,
        fields: &[String],
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        fields
            .iter()
            .map(|field| {
                attributes
                    .get(field)
                    .cloned()
                    .ok_or_else(|| ActiveRecordError::MissingField {
                        class_name: self.class_name.clone(),
                        field: field.clone(),
                    })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct PostgresDatabase {
    client: Arc<Client>,
}

impl fmt::Debug for PostgresDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresDatabase").finish_non_exhaustive()
    }
}

impl PostgresDatabase {
    pub async fn connect(url: &str) -> Result<Self, ActiveRecordError> {
        let (client, connection) = tokio_postgres::connect(url, NoTls)
            .await
            .map_err(|error| database_error("connect", error))?;

        tokio::spawn(async move {
            if let Err(error) = connection.await {
                eprintln!("Ricochet PostgreSQL connection failed: {error}");
            }
        });

        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn ping(&self) -> Result<(), ActiveRecordError> {
        self.client
            .simple_query("select 1")
            .await
            .map_err(|error| database_error("ping", error))?;
        Ok(())
    }

    pub async fn find(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError> {
        let parameter = PostgresParameter::try_from(id)?;
        let rows = self
            .client
            .query(mapping.select_by_id_sql().as_str(), &[parameter.as_sql()])
            .await
            .map_err(|error| database_error("find", error))?;

        rows.first()
            .map(|row| row_to_value(row, mapping))
            .transpose()
    }

    pub async fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        let rows = self
            .client
            .query(mapping.select_all_sql().as_str(), &[])
            .await
            .map_err(|error| database_error("all", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
        let row = self
            .client
            .query_one(mapping.select_count_sql().as_str(), &[])
            .await
            .map_err(|error| database_error("count", error))?;

        Ok(row.get(0))
    }

    pub async fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
        let rows = self
            .client
            .query(mapping.select_first_sql().as_str(), &[])
            .await
            .map_err(|error| database_error("first", error))?;

        rows.first()
            .map(|row| row_to_value(row, mapping))
            .transpose()
    }

    pub async fn limit(
        &self,
        mapping: &ModelMapping,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let limit = Value::Number(limit);
        let parameter = PostgresParameter::try_from(&limit)?;
        let rows = self
            .client
            .query(mapping.select_limit_sql().as_str(), &[parameter.as_sql()])
            .await
            .map_err(|error| database_error("limit", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let limit = Value::Number(limit);
        let offset = Value::Number(offset);
        let limit = PostgresParameter::try_from(&limit)?;
        let offset = PostgresParameter::try_from(&offset)?;
        let rows = self
            .client
            .query(
                mapping.select_page_sql().as_str(),
                &[limit.as_sql(), offset.as_sql()],
            )
            .await
            .map_err(|error| database_error("page", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn order_page(
        &self,
        mapping: &ModelMapping,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_order_page_sql(order.field, order.direction)?;
        let limit = Value::Number(order.limit);
        let offset = Value::Number(order.offset);
        let limit = PostgresParameter::try_from(&limit)?;
        let offset = PostgresParameter::try_from(&offset)?;
        let rows = self
            .client
            .query(sql.as_str(), &[limit.as_sql(), offset.as_sql()])
            .await
            .map_err(|error| database_error("order-page", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn exists_by_id(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<bool, ActiveRecordError> {
        let parameter = PostgresParameter::try_from(id)?;
        let row = self
            .client
            .query_one(mapping.exists_by_id_sql().as_str(), &[parameter.as_sql()])
            .await
            .map_err(|error| database_error("exists", error))?;

        Ok(row.get(0))
    }

    pub async fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_sql(field)?;
        let parameter = PostgresParameter::try_from(value)?;
        let rows = self
            .client
            .query(sql.as_str(), &[parameter.as_sql()])
            .await
            .map_err(|error| database_error("where", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_limit_sql(field)?;
        let value = PostgresParameter::try_from(value)?;
        let limit = Value::Number(limit);
        let limit = PostgresParameter::try_from(&limit)?;
        let rows = self
            .client
            .query(sql.as_str(), &[value.as_sql(), limit.as_sql()])
            .await
            .map_err(|error| database_error("where-limit", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_page_sql(field)?;
        let value = PostgresParameter::try_from(value)?;
        let limit = Value::Number(limit);
        let offset = Value::Number(offset);
        let limit = PostgresParameter::try_from(&limit)?;
        let offset = PostgresParameter::try_from(&offset)?;
        let rows = self
            .client
            .query(
                sql.as_str(),
                &[value.as_sql(), limit.as_sql(), offset.as_sql()],
            )
            .await
            .map_err(|error| database_error("where-page", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn where_eq_order_page(
        &self,
        mapping: &ModelMapping,
        where_field: &str,
        value: &Value,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql =
            mapping.select_where_eq_order_page_sql(where_field, order.field, order.direction)?;
        let value = PostgresParameter::try_from(value)?;
        let limit = Value::Number(order.limit);
        let offset = Value::Number(order.offset);
        let limit = PostgresParameter::try_from(&limit)?;
        let offset = PostgresParameter::try_from(&offset)?;
        let rows = self
            .client
            .query(
                sql.as_str(),
                &[value.as_sql(), limit.as_sql(), offset.as_sql()],
            )
            .await
            .map_err(|error| database_error("where-order-page", error))?;

        rows.iter().map(|row| row_to_value(row, mapping)).collect()
    }

    pub async fn insert(
        &self,
        mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.insert_values(attributes)?;
        self.query_one(mapping, "insert", mapping.insert_sql(), values)
            .await
    }

    pub async fn update_by_id(
        &self,
        mapping: &ModelMapping,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.update_values(id, attributes)?;
        self.query_one(mapping, "update", mapping.update_by_id_sql(), values)
            .await
    }

    async fn query_one(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Value, ActiveRecordError> {
        let parameters = values
            .iter()
            .map(PostgresParameter::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let references = parameters
            .iter()
            .map(PostgresParameter::as_sql)
            .collect::<Vec<_>>();
        let row = self
            .client
            .query_one(sql.as_str(), &references)
            .await
            .map_err(|error| database_error(operation, error))?;
        row_to_value(&row, mapping)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PostgresParameter {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
}

impl PostgresParameter {
    fn as_sql(&self) -> &(dyn ToSql + Sync) {
        self
    }
}

impl ToSql for PostgresParameter {
    fn to_sql(
        &self,
        postgres_type: &Type,
        output: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        match self {
            PostgresParameter::Null => Ok(IsNull::Yes),
            PostgresParameter::Bool(value) if *postgres_type == Type::BOOL => {
                value.to_sql(postgres_type, output)
            }
            PostgresParameter::Number(value) if *postgres_type == Type::INT2 => {
                i16::try_from(*value)
                    .map_err(|_| parameter_range_error(*value, postgres_type))?
                    .to_sql(postgres_type, output)
            }
            PostgresParameter::Number(value) if *postgres_type == Type::INT4 => {
                i32::try_from(*value)
                    .map_err(|_| parameter_range_error(*value, postgres_type))?
                    .to_sql(postgres_type, output)
            }
            PostgresParameter::Number(value) if *postgres_type == Type::INT8 => {
                value.to_sql(postgres_type, output)
            }
            PostgresParameter::String(value)
                if matches!(
                    *postgres_type,
                    Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN
                ) =>
            {
                value.as_str().to_sql(postgres_type, output)
            }
            value => Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "cannot encode Ricochet {} as PostgreSQL {}",
                    value.parameter_kind(),
                    postgres_type.name()
                ),
            ))),
        }
    }

    fn accepts(_postgres_type: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

impl PostgresParameter {
    fn parameter_kind(&self) -> &'static str {
        match self {
            PostgresParameter::Null => "nil",
            PostgresParameter::Bool(_) => "bool",
            PostgresParameter::Number(_) => "number",
            PostgresParameter::String(_) => "string",
        }
    }
}

impl TryFrom<&Value> for PostgresParameter {
    type Error = ActiveRecordError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(PostgresParameter::Null),
            Value::Bool(value) => Ok(PostgresParameter::Bool(*value)),
            Value::Number(value) => Ok(PostgresParameter::Number(*value)),
            Value::String(value) => Ok(PostgresParameter::String(value.clone())),
            value => Err(ActiveRecordError::UnsupportedValue {
                operation: "PostgreSQL parameter",
                actual: value_kind(value).to_string(),
            }),
        }
    }
}

fn parameter_range_error(value: i64, postgres_type: &Type) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "Ricochet number {value} is outside PostgreSQL {} range",
            postgres_type.name()
        ),
    )
}

fn row_to_value(row: &Row, mapping: &ModelMapping) -> Result<Value, ActiveRecordError> {
    let mut values = BTreeMap::new();

    for (index, field) in mapping.fields.iter().enumerate() {
        let column = row
            .columns()
            .get(index)
            .ok_or_else(|| ActiveRecordError::Database {
                operation: "decode row",
                message: format!("query did not return mapped field {field:?}"),
            })?;
        let value = column_value(row, index, field, column.type_())?;
        values.insert(field.clone(), value);
    }

    Ok(Value::Map(values.into()))
}

fn column_value(
    row: &Row,
    index: usize,
    field: &str,
    postgres_type: &Type,
) -> Result<Value, ActiveRecordError> {
    let result = match *postgres_type {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(index)
            .map(|value| value.map(Value::Bool).unwrap_or(Value::Nil)),
        Type::INT2 => row.try_get::<_, Option<i16>>(index).map(|value| {
            value
                .map(|value| Value::Number(value.into()))
                .unwrap_or(Value::Nil)
        }),
        Type::INT4 => row.try_get::<_, Option<i32>>(index).map(|value| {
            value
                .map(|value| Value::Number(value.into()))
                .unwrap_or(Value::Nil)
        }),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(index)
            .map(|value| value.map(Value::Number).unwrap_or(Value::Nil)),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => row
            .try_get::<_, Option<String>>(index)
            .map(|value| value.map(Value::String).unwrap_or(Value::Nil)),
        _ => {
            return Err(ActiveRecordError::UnsupportedColumnType {
                field: field.to_string(),
                postgres_type: postgres_type.name().to_string(),
            });
        }
    };

    result.map_err(|error| database_error("decode row", error))
}

fn database_error(operation: &'static str, error: tokio_postgres::Error) -> ActiveRecordError {
    ActiveRecordError::Database {
        operation,
        message: error.to_string(),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::Member(_) => "member",
        Value::Block(_) => "block",
        Value::Task(_) => "task",
        Value::Result(_) => "result",
        Value::Regex(_) => "regex",
        Value::Capability(_) => "capability",
    }
}

fn validate_table_identifier(name: &str) -> Result<(), ActiveRecordError> {
    if name.split('.').all(is_postgres_identifier) {
        Ok(())
    } else {
        Err(ActiveRecordError::InvalidIdentifier {
            kind: "table",
            name: name.to_string(),
        })
    }
}

fn validate_field_identifier(name: &str) -> Result<(), ActiveRecordError> {
    if is_postgres_identifier(name) {
        Ok(())
    } else {
        Err(ActiveRecordError::InvalidIdentifier {
            kind: "field",
            name: name.to_string(),
        })
    }
}

fn validate_order_direction(direction: &str) -> Result<&'static str, ActiveRecordError> {
    match direction.to_ascii_lowercase().as_str() {
        "asc" => Ok("asc"),
        "desc" => Ok("desc"),
        _ => Err(ActiveRecordError::InvalidOrderDirection {
            direction: direction.to_string(),
        }),
    }
}

fn is_postgres_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('_' | 'a'..='z' | 'A'..='Z'))
        && chars.all(|ch| matches!(ch, '_' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ricochet_vm::Value;
    use std::collections::BTreeMap;

    #[test]
    fn select_by_id_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string(), "name".to_string()],
        };

        assert_eq!(
            mapping.select_by_id_sql(),
            "select id, email, name from users where id = $1 limit 1"
        );
    }

    #[test]
    fn select_all_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(mapping.select_all_sql(), "select id, email from users");
    }

    #[test]
    fn select_limit_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(
            mapping.select_limit_sql(),
            "select id, email from users limit $1"
        );
    }

    #[test]
    fn select_page_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(
            mapping.select_page_sql(),
            "select id, email from users limit $1 offset $2"
        );
    }

    #[test]
    fn select_order_page_sql_requires_mapped_field_and_valid_direction() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_order_page_sql("email", "DESC"),
            Ok(
                "select id, email, name from public.users order by email desc limit $1 offset $2"
                    .to_string()
            )
        );
        assert_eq!(
            mapping.select_order_page_sql("password_hash", "asc"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
        assert_eq!(
            mapping.select_order_page_sql("email", "sideways"),
            Err(ActiveRecordError::InvalidOrderDirection {
                direction: "sideways".to_string(),
            })
        );
    }

    #[test]
    fn select_count_sql_uses_existing_table() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(mapping.select_count_sql(), "select count(*) from users");
    }

    #[test]
    fn select_first_sql_uses_existing_table_and_fields() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(
            mapping.select_first_sql(),
            "select id, email from users limit 1"
        );
    }

    #[test]
    fn exists_by_id_sql_uses_existing_table() {
        let mapping = ModelMapping {
            class_name: "User".to_string(),
            table_name: "users".to_string(),
            fields: vec!["id".to_string(), "email".to_string()],
        };

        assert_eq!(
            mapping.exists_by_id_sql(),
            "select exists(select 1 from users where id = $1)"
        );
    }

    #[test]
    fn try_new_rejects_unsafe_postgres_identifiers() {
        assert_eq!(
            ModelMapping::try_new("User", "users; drop table users", ["id", "email"]),
            Err(ActiveRecordError::InvalidIdentifier {
                kind: "table",
                name: "users; drop table users".to_string(),
            })
        );
        assert_eq!(
            ModelMapping::try_new("User", "users", ["id", "email-address"]),
            Err(ActiveRecordError::InvalidIdentifier {
                kind: "field",
                name: "email-address".to_string(),
            })
        );
    }

    #[test]
    fn select_where_eq_sql_requires_mapped_field() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_where_eq_sql("email"),
            Ok("select id, email, name from public.users where email = $1".to_string())
        );
        assert_eq!(
            mapping.select_where_eq_sql("password_hash"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
    }

    #[test]
    fn select_where_eq_limit_sql_requires_mapped_field() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_where_eq_limit_sql("email"),
            Ok("select id, email, name from public.users where email = $1 limit $2".to_string())
        );
        assert_eq!(
            mapping.select_where_eq_limit_sql("password_hash"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
    }

    #[test]
    fn select_where_eq_page_sql_requires_mapped_field() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_where_eq_page_sql("email"),
            Ok(
                "select id, email, name from public.users where email = $1 limit $2 offset $3"
                    .to_string()
            )
        );
        assert_eq!(
            mapping.select_where_eq_page_sql("password_hash"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
    }

    #[test]
    fn select_where_eq_order_page_sql_requires_mapped_fields_and_valid_direction() {
        let mapping = ModelMapping::try_new("User", "public.users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_where_eq_order_page_sql("email", "id", "asc"),
            Ok(
                "select id, email, name from public.users where email = $1 order by id asc limit $2 offset $3"
                    .to_string()
            )
        );
        assert_eq!(
            mapping.select_where_eq_order_page_sql("password_hash", "id", "asc"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
        assert_eq!(
            mapping.select_where_eq_order_page_sql("email", "password_hash", "asc"),
            Err(ActiveRecordError::UnknownField {
                class_name: "User".to_string(),
                field: "password_hash".to_string(),
            })
        );
        assert_eq!(
            mapping.select_where_eq_order_page_sql("email", "id", "down"),
            Err(ActiveRecordError::InvalidOrderDirection {
                direction: "down".to_string(),
            })
        );
    }

    #[test]
    fn insert_and_update_sql_use_non_id_fields_and_return_all_fields() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.insert_sql(),
            "insert into users (email, name) values ($1, $2) returning id, email, name"
        );
        assert_eq!(
            mapping.update_by_id_sql(),
            "update users set email = $1, name = $2 where id = $3 returning id, email, name"
        );
    }

    #[test]
    fn insert_values_follow_model_field_order() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");
        let attributes = BTreeMap::from([
            ("name".to_string(), Value::String("Ada".to_string())),
            (
                "email".to_string(),
                Value::String("ada@example.com".to_string()),
            ),
        ]);

        assert_eq!(
            mapping.insert_values(&attributes),
            Ok(vec![
                Value::String("ada@example.com".to_string()),
                Value::String("Ada".to_string()),
            ])
        );
    }

    #[test]
    fn insert_values_require_every_mapped_non_id_field() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");
        let attributes = BTreeMap::from([(
            "email".to_string(),
            Value::String("ada@example.com".to_string()),
        )]);

        assert_eq!(
            mapping.insert_values(&attributes),
            Err(ActiveRecordError::MissingField {
                class_name: "User".to_string(),
                field: "name".to_string(),
            })
        );
    }

    #[test]
    fn postgres_parameters_reject_non_scalar_values() {
        assert_eq!(
            PostgresParameter::try_from(&Value::Array(Vec::new().into())),
            Err(ActiveRecordError::UnsupportedValue {
                operation: "PostgreSQL parameter",
                actual: "array".to_string(),
            })
        );
    }

    #[test]
    fn postgres_number_parameter_encodes_for_integer_columns() {
        let parameter = PostgresParameter::try_from(&Value::Number(42))
            .expect("number parameter should convert");
        let mut output = bytes::BytesMut::new();

        let null = parameter
            .to_sql(&Type::INT4, &mut output)
            .expect("number should encode as int4");

        assert!(matches!(null, tokio_postgres::types::IsNull::No));
        assert_eq!(output.as_ref(), &[0, 0, 0, 42]);
    }

    #[test]
    fn postgres_nil_parameter_encodes_as_sql_null() {
        let parameter =
            PostgresParameter::try_from(&Value::Nil).expect("nil parameter should convert");
        let mut output = bytes::BytesMut::new();

        let null = parameter
            .to_sql(&Type::TEXT, &mut output)
            .expect("nil should encode");

        assert!(matches!(null, tokio_postgres::types::IsNull::Yes));
        assert!(output.is_empty());
    }

    #[test]
    fn model_mapping_comes_from_loaded_ricochet_model_class() {
        let chunk = ricochet_compiler::compile_source(
            "app/Models/User.rco",
            "User Model subclass\n  users table\n  id field\n  email field\nend\n",
        )
        .expect("model compiles");
        let mut vm = ricochet_vm::Vm::default();
        vm.run_chunk(&chunk).expect("model loads");

        let mapping = ModelMapping::from_vm(&vm, "User").expect("mapping should derive");

        assert_eq!(
            mapping,
            ModelMapping {
                class_name: "User".to_string(),
                table_name: "users".to_string(),
                fields: vec!["id".to_string(), "email".to_string()],
            }
        );
    }
}
