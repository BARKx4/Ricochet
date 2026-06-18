use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use bytes::BytesMut;
use mysql_async::prelude::Queryable;
use mysql_async::{Pool as MysqlPool, Row as MysqlRow, Value as MysqlSqlValue};
use ricochet_vm::Value;
use rusqlite::types::{ToSqlOutput, Value as SqliteSqlValue, ValueRef};
use rusqlite::{Connection, Row as SqliteRow};
use tokio_postgres::config::{Host as PostgresHost, SslMode};
use tokio_postgres::types::{to_sql_checked, IsNull, ToSql as PostgresToSql, Type};
use tokio_postgres::{Client, Config as PostgresConfig, Row as PostgresRow};
use tokio_postgres_rustls::MakeRustlsConnect;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlDialect {
    Postgres,
    Sqlite,
    Mysql,
}

impl SqlDialect {
    fn placeholder(self, index: usize) -> String {
        match self {
            SqlDialect::Postgres => format!("${index}"),
            SqlDialect::Sqlite | SqlDialect::Mysql => "?".to_string(),
        }
    }
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
        database_type: String,
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
                write!(f, "invalid SQL {kind} identifier {name:?}")
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
                database_type,
            } => write!(
                f,
                "database field {field:?} has unsupported type {database_type}"
            ),
            ActiveRecordError::Database { operation, message } => {
                write!(f, "database {operation} failed: {message}")
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
        self.select_by_id_sql_for(SqlDialect::Postgres)
    }

    fn select_by_id_sql_for(&self, dialect: SqlDialect) -> String {
        format!(
            "select {} from {} where id = {} limit 1",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1)
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
        self.select_limit_sql_for(SqlDialect::Postgres)
    }

    fn select_limit_sql_for(&self, dialect: SqlDialect) -> String {
        format!(
            "select {} from {} limit {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1)
        )
    }

    pub fn select_page_sql(&self) -> String {
        self.select_page_sql_for(SqlDialect::Postgres)
    }

    fn select_page_sql_for(&self, dialect: SqlDialect) -> String {
        format!(
            "select {} from {} limit {} offset {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1),
            dialect.placeholder(2)
        )
    }

    pub fn select_order_page_sql(
        &self,
        field: &str,
        direction: &str,
    ) -> Result<String, ActiveRecordError> {
        self.select_order_page_sql_for(field, direction, SqlDialect::Postgres)
    }

    fn select_order_page_sql_for(
        &self,
        field: &str,
        direction: &str,
        dialect: SqlDialect,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        let direction = validate_order_direction(direction)?;
        Ok(format!(
            "select {} from {} order by {field} {direction} limit {} offset {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1),
            dialect.placeholder(2)
        ))
    }

    pub fn exists_by_id_sql(&self) -> String {
        self.exists_by_id_sql_for(SqlDialect::Postgres)
    }

    fn exists_by_id_sql_for(&self, dialect: SqlDialect) -> String {
        format!(
            "select exists(select 1 from {} where id = {})",
            self.table_name,
            dialect.placeholder(1)
        )
    }

    pub fn select_where_eq_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.select_where_eq_sql_for(field, SqlDialect::Postgres)
    }

    fn select_where_eq_sql_for(
        &self,
        field: &str,
        dialect: SqlDialect,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1)
        ))
    }

    pub fn select_where_eq_limit_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.select_where_eq_limit_sql_for(field, SqlDialect::Postgres)
    }

    fn select_where_eq_limit_sql_for(
        &self,
        field: &str,
        dialect: SqlDialect,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = {} limit {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1),
            dialect.placeholder(2)
        ))
    }

    pub fn select_where_eq_page_sql(&self, field: &str) -> Result<String, ActiveRecordError> {
        self.select_where_eq_page_sql_for(field, SqlDialect::Postgres)
    }

    fn select_where_eq_page_sql_for(
        &self,
        field: &str,
        dialect: SqlDialect,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(field)?;
        Ok(format!(
            "select {} from {} where {field} = {} limit {} offset {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1),
            dialect.placeholder(2),
            dialect.placeholder(3)
        ))
    }

    pub fn select_where_eq_order_page_sql(
        &self,
        where_field: &str,
        order_field: &str,
        direction: &str,
    ) -> Result<String, ActiveRecordError> {
        self.select_where_eq_order_page_sql_for(
            where_field,
            order_field,
            direction,
            SqlDialect::Postgres,
        )
    }

    fn select_where_eq_order_page_sql_for(
        &self,
        where_field: &str,
        order_field: &str,
        direction: &str,
        dialect: SqlDialect,
    ) -> Result<String, ActiveRecordError> {
        self.require_field(where_field)?;
        self.require_field(order_field)?;
        let direction = validate_order_direction(direction)?;
        Ok(format!(
            "select {} from {} where {where_field} = {} order by {order_field} {direction} limit {} offset {}",
            self.fields.join(", "),
            self.table_name,
            dialect.placeholder(1),
            dialect.placeholder(2),
            dialect.placeholder(3)
        ))
    }

    pub fn insert_sql(&self) -> String {
        self.insert_sql_for(SqlDialect::Postgres)
    }

    fn insert_sql_for(&self, dialect: SqlDialect) -> String {
        let fields = self.non_id_fields();
        let placeholders = (1..=fields.len())
            .map(|index| dialect.placeholder(index))
            .collect::<Vec<_>>();

        format!(
            "insert into {} ({}) values ({}) returning {}",
            self.table_name,
            fields.join(", "),
            placeholders.join(", "),
            self.fields.join(", ")
        )
    }

    fn insert_sql_without_returning_for(&self, dialect: SqlDialect) -> String {
        let fields = self.non_id_fields();
        let placeholders = (1..=fields.len())
            .map(|index| dialect.placeholder(index))
            .collect::<Vec<_>>();

        format!(
            "insert into {} ({}) values ({})",
            self.table_name,
            fields.join(", "),
            placeholders.join(", ")
        )
    }

    pub fn update_by_id_sql(&self) -> String {
        self.update_by_id_sql_for(SqlDialect::Postgres)
    }

    fn update_by_id_sql_for(&self, dialect: SqlDialect) -> String {
        let fields = self.non_id_fields();
        let assignments = fields
            .iter()
            .enumerate()
            .map(|(index, field)| format!("{field} = {}", dialect.placeholder(index + 1)))
            .collect::<Vec<_>>();
        let id_parameter = fields.len() + 1;

        format!(
            "update {} set {} where id = {} returning {}",
            self.table_name,
            assignments.join(", "),
            dialect.placeholder(id_parameter),
            self.fields.join(", ")
        )
    }

    fn update_by_id_sql_without_returning_for(&self, dialect: SqlDialect) -> String {
        let fields = self.non_id_fields();
        let assignments = fields
            .iter()
            .enumerate()
            .map(|(index, field)| format!("{field} = {}", dialect.placeholder(index + 1)))
            .collect::<Vec<_>>();
        let id_parameter = fields.len() + 1;

        format!(
            "update {} set {} where id = {}",
            self.table_name,
            assignments.join(", "),
            dialect.placeholder(id_parameter)
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
        let mut config = url
            .parse::<PostgresConfig>()
            .map_err(|error| database_error("parse connection string", error))?;
        apply_postgres_tls_policy(&mut config)?;
        let (client, connection) = config
            .connect(postgres_tls_connector())
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

    pub async fn execute_batch(&self, sql: &str) -> Result<(), ActiveRecordError> {
        self.client
            .batch_execute(sql)
            .await
            .map_err(|error| database_error("execute migration", error))?;
        Ok(())
    }

    pub async fn migration_versions(&self) -> Result<Option<Vec<String>>, ActiveRecordError> {
        let exists = self
            .client
            .query_one(
                "select exists (select 1 from information_schema.tables where table_schema = current_schema() and table_name = 'schema_migrations')",
                &[],
            )
            .await
            .map_err(|error| database_error("list migrations", error))?
            .get::<_, bool>(0);
        if !exists {
            return Ok(None);
        }
        let rows = self
            .client
            .query(
                "select version from schema_migrations order by version",
                &[],
            )
            .await
            .map_err(|error| database_error("list migrations", error))?;
        Ok(Some(rows.into_iter().map(|row| row.get(0)).collect()))
    }

    pub async fn ensure_schema_migrations_table(&self) -> Result<(), ActiveRecordError> {
        self.execute_batch(
            r#"
create table if not exists schema_migrations (
  version text primary key,
  applied_at text not null
);
"#,
        )
        .await
    }

    pub async fn record_migration(
        &self,
        version: &str,
        applied_at: &str,
    ) -> Result<(), ActiveRecordError> {
        self.client
            .execute(
                "insert into schema_migrations (version, applied_at) values ($1, $2)",
                &[&version, &applied_at],
            )
            .await
            .map_err(|error| database_error("record migration", error))?;
        Ok(())
    }

    pub async fn apply_migration(
        &self,
        version: &str,
        applied_at: &str,
        sql: &str,
    ) -> Result<(), ActiveRecordError> {
        self.execute_batch("begin").await?;
        let result = async {
            self.execute_batch(sql).await?;
            self.record_migration(version, applied_at).await
        }
        .await;
        match result {
            Ok(()) => self.execute_batch("commit").await,
            Err(error) => {
                let _ = self.execute_batch("rollback").await;
                Err(error)
            }
        }
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
            .map_err(|error| database_error("order_page", error))?;

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
            .map_err(|error| database_error("where_limit", error))?;

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
            .map_err(|error| database_error("where_page", error))?;

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
            .map_err(|error| database_error("where_order_page", error))?;

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

fn postgres_tls_connector() -> MakeRustlsConnect {
    let _ = rustls::crypto::ring::default_provider().install_default();
    MakeRustlsConnect::with_webpki_roots()
}

fn apply_postgres_tls_policy(config: &mut PostgresConfig) -> Result<(), ActiveRecordError> {
    if config.get_ssl_mode() == SslMode::Disable {
        if postgres_hosts_are_local(config) {
            return Ok(());
        }

        return Err(postgres_configuration_error(
            "PostgreSQL sslmode=disable is only allowed for localhost or loopback connections",
        ));
    }

    config.ssl_mode(SslMode::Require);
    Ok(())
}

fn postgres_hosts_are_local(config: &PostgresConfig) -> bool {
    let hostaddrs = config.get_hostaddrs();
    if !hostaddrs.is_empty() {
        return hostaddrs.iter().all(IpAddr::is_loopback);
    }

    let hosts = config.get_hosts();
    !hosts.is_empty() && hosts.iter().all(postgres_host_is_local)
}

fn postgres_host_is_local(host: &PostgresHost) -> bool {
    match host {
        PostgresHost::Tcp(host) => {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        }
        #[cfg(unix)]
        PostgresHost::Unix(_) => true,
    }
}

#[derive(Clone)]
pub struct SqliteDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl fmt::Debug for SqliteDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteDatabase").finish_non_exhaustive()
    }
}

impl SqliteDatabase {
    pub fn connect(url: &str) -> Result<Self, ActiveRecordError> {
        let connection = open_sqlite_connection(url)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| sqlite_error("configure", error))?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn ping(&self) -> Result<(), ActiveRecordError> {
        self.with_connection("ping", |connection| {
            connection
                .query_row("select 1", [], |_| Ok(()))
                .map_err(|error| sqlite_error("ping", error))
        })
    }

    pub fn find(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError> {
        self.query_optional(
            mapping,
            "find",
            mapping.select_by_id_sql_for(SqlDialect::Sqlite),
            vec![id.clone()],
        )
    }

    pub fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(mapping, "all", mapping.select_all_sql(), Vec::new())
    }

    pub fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
        self.with_connection("count", |connection| {
            connection
                .query_row(mapping.select_count_sql().as_str(), [], |row| row.get(0))
                .map_err(|error| sqlite_error("count", error))
        })
    }

    pub fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
        self.query_optional(mapping, "first", mapping.select_first_sql(), Vec::new())
    }

    pub fn limit(
        &self,
        mapping: &ModelMapping,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(
            mapping,
            "limit",
            mapping.select_limit_sql_for(SqlDialect::Sqlite),
            vec![Value::Number(limit)],
        )
    }

    pub fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(
            mapping,
            "page",
            mapping.select_page_sql_for(SqlDialect::Sqlite),
            vec![Value::Number(limit), Value::Number(offset)],
        )
    }

    pub fn order_page(
        &self,
        mapping: &ModelMapping,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql =
            mapping.select_order_page_sql_for(order.field, order.direction, SqlDialect::Sqlite)?;
        self.query_many(
            mapping,
            "order_page",
            sql,
            vec![Value::Number(order.limit), Value::Number(order.offset)],
        )
    }

    pub fn exists_by_id(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<bool, ActiveRecordError> {
        let parameter = SqliteParameter::try_from(id)?;
        self.with_connection("exists", |connection| {
            let references = vec![parameter.as_sql()];
            let exists: i64 = connection
                .query_row(
                    mapping.exists_by_id_sql_for(SqlDialect::Sqlite).as_str(),
                    references.as_slice(),
                    |row| row.get(0),
                )
                .map_err(|error| sqlite_error("exists", error))?;
            Ok(exists != 0)
        })
    }

    pub fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_sql_for(field, SqlDialect::Sqlite)?;
        self.query_many(mapping, "where", sql, vec![value.clone()])
    }

    pub fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_limit_sql_for(field, SqlDialect::Sqlite)?;
        self.query_many(
            mapping,
            "where_limit",
            sql,
            vec![value.clone(), Value::Number(limit)],
        )
    }

    pub fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_page_sql_for(field, SqlDialect::Sqlite)?;
        self.query_many(
            mapping,
            "where_page",
            sql,
            vec![value.clone(), Value::Number(limit), Value::Number(offset)],
        )
    }

    pub fn where_eq_order_page(
        &self,
        mapping: &ModelMapping,
        where_field: &str,
        value: &Value,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_order_page_sql_for(
            where_field,
            order.field,
            order.direction,
            SqlDialect::Sqlite,
        )?;
        self.query_many(
            mapping,
            "where_order_page",
            sql,
            vec![
                value.clone(),
                Value::Number(order.limit),
                Value::Number(order.offset),
            ],
        )
    }

    pub fn insert(
        &self,
        mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.insert_values(attributes)?;
        self.query_one(
            mapping,
            "insert",
            mapping.insert_sql_for(SqlDialect::Sqlite),
            values,
        )
    }

    pub fn update_by_id(
        &self,
        mapping: &ModelMapping,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.update_values(id, attributes)?;
        self.query_one(
            mapping,
            "update",
            mapping.update_by_id_sql_for(SqlDialect::Sqlite),
            values,
        )
    }

    fn query_many(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let parameters = values
            .iter()
            .map(SqliteParameter::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        self.with_connection(operation, |connection| {
            let mut statement = connection
                .prepare(sql.as_str())
                .map_err(|error| sqlite_error(operation, error))?;
            let references = parameters
                .iter()
                .map(SqliteParameter::as_sql)
                .collect::<Vec<_>>();
            let mut rows = statement
                .query(references.as_slice())
                .map_err(|error| sqlite_error(operation, error))?;
            let mut values = Vec::new();

            while let Some(row) = rows
                .next()
                .map_err(|error| sqlite_error(operation, error))?
            {
                values.push(sqlite_row_to_value(row, mapping)?);
            }

            Ok(values)
        })
    }

    fn query_optional(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Option<Value>, ActiveRecordError> {
        Ok(self
            .query_many(mapping, operation, sql, values)?
            .into_iter()
            .next())
    }

    fn query_one(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Value, ActiveRecordError> {
        self.query_optional(mapping, operation, sql, values)?
            .ok_or_else(|| ActiveRecordError::Database {
                operation,
                message: "query returned no rows".to_string(),
            })
    }

    fn with_connection<T>(
        &self,
        operation: &'static str,
        callback: impl FnOnce(&Connection) -> Result<T, ActiveRecordError>,
    ) -> Result<T, ActiveRecordError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| ActiveRecordError::Database {
                operation,
                message: "SQLite connection lock was poisoned".to_string(),
            })?;
        callback(&connection)
    }
}

#[derive(Clone)]
pub struct MysqlDatabase {
    pool: MysqlPool,
}

impl fmt::Debug for MysqlDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MysqlDatabase").finish_non_exhaustive()
    }
}

impl MysqlDatabase {
    pub async fn connect(url: &str) -> Result<Self, ActiveRecordError> {
        let pool = MysqlPool::from_url(url).map_err(|error| mysql_error("connect", error))?;
        let database = Self { pool };
        database.ping().await?;
        Ok(database)
    }

    pub async fn ping(&self) -> Result<(), ActiveRecordError> {
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("ping", error))?;
        connection
            .query_drop("select 1")
            .await
            .map_err(|error| mysql_error("ping", error))?;
        Ok(())
    }

    pub async fn execute_batch(&self, sql: &str) -> Result<(), ActiveRecordError> {
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("execute migration", error))?;
        for statement in sql
            .split(';')
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
        {
            connection
                .query_drop(statement)
                .await
                .map_err(|error| mysql_error("execute migration", error))?;
        }
        Ok(())
    }

    pub async fn migration_versions(&self) -> Result<Option<Vec<String>>, ActiveRecordError> {
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("list migrations", error))?;
        let exists: Option<u8> = connection
            .exec_first(
                "select 1 from information_schema.tables where table_schema = database() and table_name = 'schema_migrations'",
                (),
            )
            .await
            .map_err(|error| mysql_error("list migrations", error))?;
        if exists.is_none() {
            return Ok(None);
        }
        let rows: Vec<String> = connection
            .query("select version from schema_migrations order by version")
            .await
            .map_err(|error| mysql_error("list migrations", error))?;
        Ok(Some(rows))
    }

    pub async fn ensure_schema_migrations_table(&self) -> Result<(), ActiveRecordError> {
        self.execute_batch(
            r#"
create table if not exists schema_migrations (
  version varchar(255) primary key,
  applied_at varchar(64) not null
);
"#,
        )
        .await
    }

    pub async fn record_migration(
        &self,
        version: &str,
        applied_at: &str,
    ) -> Result<(), ActiveRecordError> {
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("record migration", error))?;
        connection
            .exec_drop(
                "insert into schema_migrations (version, applied_at) values (?, ?)",
                (version, applied_at),
            )
            .await
            .map_err(|error| mysql_error("record migration", error))?;
        Ok(())
    }

    pub async fn apply_migration(
        &self,
        version: &str,
        applied_at: &str,
        sql: &str,
    ) -> Result<(), ActiveRecordError> {
        self.execute_batch("start transaction").await?;
        let result = async {
            self.execute_batch(sql).await?;
            self.record_migration(version, applied_at).await
        }
        .await;
        match result {
            Ok(()) => self.execute_batch("commit").await,
            Err(error) => {
                let _ = self.execute_batch("rollback").await;
                Err(error)
            }
        }
    }

    pub async fn find(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError> {
        self.query_optional(
            mapping,
            "find",
            mapping.select_by_id_sql_for(SqlDialect::Mysql),
            vec![id.clone()],
        )
        .await
    }

    pub async fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(mapping, "all", mapping.select_all_sql(), Vec::new())
            .await
    }

    pub async fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("count", error))?;
        let count: Option<u64> = connection
            .exec_first(
                mapping.select_count_sql().as_str(),
                Vec::<MysqlSqlValue>::new(),
            )
            .await
            .map_err(|error| mysql_error("count", error))?;
        let count = count.ok_or_else(|| ActiveRecordError::Database {
            operation: "count",
            message: "query returned no rows".to_string(),
        })?;

        i64::try_from(count).map_err(|_| ActiveRecordError::Database {
            operation: "count",
            message: "COUNT(*) exceeded Ricochet i64 range".to_string(),
        })
    }

    pub async fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
        self.query_optional(mapping, "first", mapping.select_first_sql(), Vec::new())
            .await
    }

    pub async fn limit(
        &self,
        mapping: &ModelMapping,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(
            mapping,
            "limit",
            mapping.select_limit_sql_for(SqlDialect::Mysql),
            vec![Value::Number(limit)],
        )
        .await
    }

    pub async fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.query_many(
            mapping,
            "page",
            mapping.select_page_sql_for(SqlDialect::Mysql),
            vec![Value::Number(limit), Value::Number(offset)],
        )
        .await
    }

    pub async fn order_page(
        &self,
        mapping: &ModelMapping,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql =
            mapping.select_order_page_sql_for(order.field, order.direction, SqlDialect::Mysql)?;
        self.query_many(
            mapping,
            "order_page",
            sql,
            vec![Value::Number(order.limit), Value::Number(order.offset)],
        )
        .await
    }

    pub async fn exists_by_id(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<bool, ActiveRecordError> {
        let parameters = mysql_parameters([id])?;
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("exists", error))?;
        let row: Option<MysqlRow> = connection
            .exec_first(
                mapping.exists_by_id_sql_for(SqlDialect::Mysql).as_str(),
                parameters,
            )
            .await
            .map_err(|error| mysql_error("exists", error))?;
        let Some(row) = row else {
            return Ok(false);
        };
        let value = row.as_ref(0).ok_or_else(|| ActiveRecordError::Database {
            operation: "exists",
            message: "query did not return EXISTS column".to_string(),
        })?;

        match mysql_column_value(value, "exists")? {
            Value::Bool(value) => Ok(value),
            Value::Number(value) => Ok(value != 0),
            Value::Nil => Ok(false),
            value => Err(ActiveRecordError::UnsupportedColumnType {
                field: "exists".to_string(),
                database_type: value_kind(&value).to_string(),
            }),
        }
    }

    pub async fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_sql_for(field, SqlDialect::Mysql)?;
        self.query_many(mapping, "where", sql, vec![value.clone()])
            .await
    }

    pub async fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_limit_sql_for(field, SqlDialect::Mysql)?;
        self.query_many(
            mapping,
            "where_limit",
            sql,
            vec![value.clone(), Value::Number(limit)],
        )
        .await
    }

    pub async fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_page_sql_for(field, SqlDialect::Mysql)?;
        self.query_many(
            mapping,
            "where_page",
            sql,
            vec![value.clone(), Value::Number(limit), Value::Number(offset)],
        )
        .await
    }

    pub async fn where_eq_order_page(
        &self,
        mapping: &ModelMapping,
        where_field: &str,
        value: &Value,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let sql = mapping.select_where_eq_order_page_sql_for(
            where_field,
            order.field,
            order.direction,
            SqlDialect::Mysql,
        )?;
        self.query_many(
            mapping,
            "where_order_page",
            sql,
            vec![
                value.clone(),
                Value::Number(order.limit),
                Value::Number(order.offset),
            ],
        )
        .await
    }

    pub async fn insert(
        &self,
        mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.insert_values(attributes)?;
        let parameters = mysql_parameters(values.iter())?;
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("insert", error))?;
        connection
            .exec_drop(
                mapping
                    .insert_sql_without_returning_for(SqlDialect::Mysql)
                    .as_str(),
                parameters,
            )
            .await
            .map_err(|error| mysql_error("insert", error))?;
        let id = connection
            .last_insert_id()
            .ok_or_else(|| ActiveRecordError::Database {
                operation: "insert",
                message: "insert did not report an AUTO_INCREMENT id".to_string(),
            })
            .and_then(|id| {
                i64::try_from(id).map_err(|_| ActiveRecordError::Database {
                    operation: "insert",
                    message: "AUTO_INCREMENT id exceeded Ricochet i64 range".to_string(),
                })
            })?;

        Self::query_one_with_connection(
            &mut connection,
            mapping,
            "insert",
            mapping.select_by_id_sql_for(SqlDialect::Mysql),
            vec![Value::Number(id)],
        )
        .await
    }

    pub async fn update_by_id(
        &self,
        mapping: &ModelMapping,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        let values = mapping.update_values(id.clone(), attributes)?;
        let parameters = mysql_parameters(values.iter())?;
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error("update", error))?;
        connection
            .exec_drop(
                mapping
                    .update_by_id_sql_without_returning_for(SqlDialect::Mysql)
                    .as_str(),
                parameters,
            )
            .await
            .map_err(|error| mysql_error("update", error))?;

        Self::query_one_with_connection(
            &mut connection,
            mapping,
            "update",
            mapping.select_by_id_sql_for(SqlDialect::Mysql),
            vec![id],
        )
        .await
    }

    async fn query_many(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let parameters = mysql_parameters(values.iter())?;
        let mut connection = self
            .pool
            .get_conn()
            .await
            .map_err(|error| mysql_error(operation, error))?;
        let rows: Vec<MysqlRow> = connection
            .exec(sql.as_str(), parameters)
            .await
            .map_err(|error| mysql_error(operation, error))?;

        rows.iter()
            .map(|row| mysql_row_to_value(row, mapping))
            .collect()
    }

    async fn query_optional(
        &self,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Option<Value>, ActiveRecordError> {
        Ok(self
            .query_many(mapping, operation, sql, values)
            .await?
            .into_iter()
            .next())
    }

    async fn query_one_with_connection(
        connection: &mut mysql_async::Conn,
        mapping: &ModelMapping,
        operation: &'static str,
        sql: String,
        values: Vec<Value>,
    ) -> Result<Value, ActiveRecordError> {
        let parameters = mysql_parameters(values.iter())?;
        let row: Option<MysqlRow> = connection
            .exec_first(sql.as_str(), parameters)
            .await
            .map_err(|error| mysql_error(operation, error))?;

        row.as_ref()
            .map(|row| mysql_row_to_value(row, mapping))
            .transpose()?
            .ok_or_else(|| ActiveRecordError::Database {
                operation,
                message: "query returned no rows".to_string(),
            })
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
    fn as_sql(&self) -> &(dyn PostgresToSql + Sync) {
        self
    }
}

impl PostgresToSql for PostgresParameter {
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

#[derive(Debug, Clone, PartialEq)]
enum SqliteParameter {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
}

impl SqliteParameter {
    fn as_sql(&self) -> &dyn rusqlite::types::ToSql {
        self
    }
}

impl rusqlite::types::ToSql for SqliteParameter {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value = match self {
            SqliteParameter::Null => SqliteSqlValue::Null,
            SqliteParameter::Bool(value) => SqliteSqlValue::Integer(if *value { 1 } else { 0 }),
            SqliteParameter::Number(value) => SqliteSqlValue::Integer(*value),
            SqliteParameter::String(value) => SqliteSqlValue::Text(value.clone()),
        };

        Ok(ToSqlOutput::Owned(value))
    }
}

impl TryFrom<&Value> for SqliteParameter {
    type Error = ActiveRecordError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(SqliteParameter::Null),
            Value::Bool(value) => Ok(SqliteParameter::Bool(*value)),
            Value::Number(value) => Ok(SqliteParameter::Number(*value)),
            Value::String(value) => Ok(SqliteParameter::String(value.clone())),
            value => Err(ActiveRecordError::UnsupportedValue {
                operation: "SQLite parameter",
                actual: value_kind(value).to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum MysqlParameter {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
}

impl MysqlParameter {
    fn into_sql(self) -> MysqlSqlValue {
        match self {
            MysqlParameter::Null => MysqlSqlValue::NULL,
            MysqlParameter::Bool(value) => MysqlSqlValue::Int(if value { 1 } else { 0 }),
            MysqlParameter::Number(value) => MysqlSqlValue::Int(value),
            MysqlParameter::String(value) => MysqlSqlValue::Bytes(value.into_bytes()),
        }
    }
}

impl TryFrom<&Value> for MysqlParameter {
    type Error = ActiveRecordError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::Nil => Ok(MysqlParameter::Null),
            Value::Bool(value) => Ok(MysqlParameter::Bool(*value)),
            Value::Number(value) => Ok(MysqlParameter::Number(*value)),
            Value::String(value) => Ok(MysqlParameter::String(value.clone())),
            value => Err(ActiveRecordError::UnsupportedValue {
                operation: "MySQL parameter",
                actual: value_kind(value).to_string(),
            }),
        }
    }
}

fn mysql_parameters<'a>(
    values: impl IntoIterator<Item = &'a Value>,
) -> Result<Vec<MysqlSqlValue>, ActiveRecordError> {
    values
        .into_iter()
        .map(MysqlParameter::try_from)
        .map(|parameter| parameter.map(MysqlParameter::into_sql))
        .collect()
}

fn open_sqlite_connection(url: &str) -> Result<Connection, ActiveRecordError> {
    let path = sqlite_path_from_url(url)?;
    if path == ":memory:" {
        Connection::open_in_memory().map_err(|error| sqlite_error("connect", error))
    } else {
        Connection::open(path).map_err(|error| sqlite_error("connect", error))
    }
}

fn sqlite_path_from_url(url: &str) -> Result<String, ActiveRecordError> {
    let trimmed = url.trim();
    let path = trimmed
        .strip_prefix("sqlite://")
        .or_else(|| trimmed.strip_prefix("sqlite:"))
        .unwrap_or(trimmed);
    let path = normalize_sqlite_path(path);

    if path.is_empty() {
        return Err(ActiveRecordError::Database {
            operation: "connect",
            message: "SQLite database path is empty".to_string(),
        });
    }

    Ok(path)
}

#[cfg(windows)]
fn normalize_sqlite_path(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' {
        path[1..].to_string()
    } else {
        path.to_string()
    }
}

#[cfg(not(windows))]
fn normalize_sqlite_path(path: &str) -> String {
    path.to_string()
}

fn row_to_value(row: &PostgresRow, mapping: &ModelMapping) -> Result<Value, ActiveRecordError> {
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
    row: &PostgresRow,
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
                database_type: postgres_type.name().to_string(),
            });
        }
    };

    result.map_err(|error| database_error("decode row", error))
}

fn sqlite_row_to_value(
    row: &SqliteRow<'_>,
    mapping: &ModelMapping,
) -> Result<Value, ActiveRecordError> {
    let mut values = BTreeMap::new();

    for (index, field) in mapping.fields.iter().enumerate() {
        let value = sqlite_column_value(row, index, field)?;
        values.insert(field.clone(), value);
    }

    Ok(Value::Map(values.into()))
}

fn sqlite_column_value(
    row: &SqliteRow<'_>,
    index: usize,
    field: &str,
) -> Result<Value, ActiveRecordError> {
    match row
        .get_ref(index)
        .map_err(|error| sqlite_error("decode row", error))?
    {
        ValueRef::Null => Ok(Value::Nil),
        ValueRef::Integer(value) => Ok(Value::Number(value)),
        ValueRef::Text(value) => Ok(Value::String(String::from_utf8_lossy(value).into_owned())),
        ValueRef::Real(_) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "REAL".to_string(),
        }),
        ValueRef::Blob(_) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "BLOB".to_string(),
        }),
    }
}

fn mysql_row_to_value(row: &MysqlRow, mapping: &ModelMapping) -> Result<Value, ActiveRecordError> {
    let mut values = BTreeMap::new();

    for (index, field) in mapping.fields.iter().enumerate() {
        let value = row
            .as_ref(index)
            .ok_or_else(|| ActiveRecordError::Database {
                operation: "decode row",
                message: format!("query did not return mapped field {field:?}"),
            })?;
        values.insert(field.clone(), mysql_column_value(value, field)?);
    }

    Ok(Value::Map(values.into()))
}

fn mysql_column_value(value: &MysqlSqlValue, field: &str) -> Result<Value, ActiveRecordError> {
    match value {
        MysqlSqlValue::NULL => Ok(Value::Nil),
        MysqlSqlValue::Int(value) => Ok(Value::Number(*value)),
        MysqlSqlValue::UInt(value) => i64::try_from(*value).map(Value::Number).map_err(|_| {
            ActiveRecordError::UnsupportedColumnType {
                field: field.to_string(),
                database_type: "UNSIGNED BIGINT".to_string(),
            }
        }),
        MysqlSqlValue::Bytes(value) => {
            Ok(Value::String(String::from_utf8_lossy(value).into_owned()))
        }
        MysqlSqlValue::Float(_) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "FLOAT".to_string(),
        }),
        MysqlSqlValue::Double(_) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "DOUBLE".to_string(),
        }),
        MysqlSqlValue::Date(..) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "DATE/DATETIME".to_string(),
        }),
        MysqlSqlValue::Time(..) => Err(ActiveRecordError::UnsupportedColumnType {
            field: field.to_string(),
            database_type: "TIME".to_string(),
        }),
    }
}

fn database_error(operation: &'static str, error: tokio_postgres::Error) -> ActiveRecordError {
    ActiveRecordError::Database {
        operation,
        message: error.to_string(),
    }
}

fn postgres_configuration_error(message: impl Into<String>) -> ActiveRecordError {
    ActiveRecordError::Database {
        operation: "configure postgres tls",
        message: message.into(),
    }
}

fn sqlite_error(operation: &'static str, error: rusqlite::Error) -> ActiveRecordError {
    ActiveRecordError::Database {
        operation,
        message: error.to_string(),
    }
}

fn mysql_error(operation: &'static str, error: mysql_async::Error) -> ActiveRecordError {
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
    fn postgres_tls_policy_requires_tls_for_remote_hosts() {
        let mut config = "postgres://app:secret@db.example.com/app"
            .parse::<PostgresConfig>()
            .expect("postgres config parses");

        assert_eq!(config.get_ssl_mode(), SslMode::Prefer);
        apply_postgres_tls_policy(&mut config).expect("tls policy applies");
        assert_eq!(config.get_ssl_mode(), SslMode::Require);
    }

    #[test]
    fn postgres_tls_policy_allows_disable_for_local_development() {
        for url in [
            "postgres://app:secret@localhost/app?sslmode=disable",
            "postgres://app:secret@127.0.0.1/app?sslmode=disable",
            "postgres://app:secret@[::1]/app?sslmode=disable",
        ] {
            let mut config = url
                .parse::<PostgresConfig>()
                .expect("postgres config parses");

            apply_postgres_tls_policy(&mut config).expect("loopback may disable tls");
            assert_eq!(config.get_ssl_mode(), SslMode::Disable);
        }
    }

    #[test]
    fn postgres_tls_policy_rejects_disable_for_remote_hosts() {
        let mut config = "postgres://app:secret@db.example.com/app?sslmode=disable"
            .parse::<PostgresConfig>()
            .expect("postgres config parses");

        assert_eq!(
            apply_postgres_tls_policy(&mut config),
            Err(ActiveRecordError::Database {
                operation: "configure postgres tls",
                message:
                    "PostgreSQL sslmode=disable is only allowed for localhost or loopback connections"
                        .to_string(),
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
    fn sqlite_sql_uses_question_mark_placeholders_and_returning() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_by_id_sql_for(SqlDialect::Sqlite),
            "select id, email, name from users where id = ? limit 1"
        );
        assert_eq!(
            mapping.select_page_sql_for(SqlDialect::Sqlite),
            "select id, email, name from users limit ? offset ?"
        );
        assert_eq!(
            mapping
                .select_where_eq_order_page_sql_for("email", "id", "desc", SqlDialect::Sqlite),
            Ok(
                "select id, email, name from users where email = ? order by id desc limit ? offset ?"
                    .to_string()
            )
        );
        assert_eq!(
            mapping.insert_sql_for(SqlDialect::Sqlite),
            "insert into users (email, name) values (?, ?) returning id, email, name"
        );
        assert_eq!(
            mapping.update_by_id_sql_for(SqlDialect::Sqlite),
            "update users set email = ?, name = ? where id = ? returning id, email, name"
        );
    }

    #[test]
    fn mysql_sql_uses_question_mark_placeholders_without_returning() {
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(
            mapping.select_by_id_sql_for(SqlDialect::Mysql),
            "select id, email, name from users where id = ? limit 1"
        );
        assert_eq!(
            mapping.select_page_sql_for(SqlDialect::Mysql),
            "select id, email, name from users limit ? offset ?"
        );
        assert_eq!(
            mapping.select_where_eq_order_page_sql_for("email", "id", "desc", SqlDialect::Mysql),
            Ok(
                "select id, email, name from users where email = ? order by id desc limit ? offset ?"
                    .to_string()
            )
        );
        assert_eq!(
            mapping.insert_sql_without_returning_for(SqlDialect::Mysql),
            "insert into users (email, name) values (?, ?)"
        );
        assert_eq!(
            mapping.update_by_id_sql_without_returning_for(SqlDialect::Mysql),
            "update users set email = ?, name = ? where id = ?"
        );
    }

    #[test]
    fn sqlite_database_runs_active_record_queries_against_real_database() {
        let database = SqliteDatabase::connect(":memory:").expect("sqlite connects");
        database
            .with_connection("test setup", |connection| {
                connection
                    .execute_batch(
                        r#"
                        create table users (
                            id integer primary key,
                            email text not null,
                            name text not null
                        );
                        insert into users (email, name) values
                            ('ada@example.com', 'Ada'),
                            ('grace@example.com', 'Grace');
                        "#,
                    )
                    .map_err(|error| sqlite_error("test setup", error))
            })
            .expect("schema setup works");
        let mapping = ModelMapping::try_new("User", "users", ["id", "email", "name"])
            .expect("mapping is valid");

        assert_eq!(database.count(&mapping), Ok(2));
        assert_eq!(database.exists_by_id(&mapping, &Value::Number(2)), Ok(true));
        assert_eq!(
            database.find(&mapping, &Value::Number(1)),
            Ok(Some(user_row(1, "ada@example.com", "Ada")))
        );
        assert_eq!(
            database.order_page(
                &mapping,
                OrderPage {
                    field: "id",
                    direction: "desc",
                    limit: 1,
                    offset: 0,
                },
            ),
            Ok(vec![user_row(2, "grace@example.com", "Grace")])
        );
        assert_eq!(
            database.where_eq_page(
                &mapping,
                "email",
                &Value::String("ada@example.com".to_string()),
                1,
                0,
            ),
            Ok(vec![user_row(1, "ada@example.com", "Ada")])
        );

        let inserted = database
            .insert(
                &mapping,
                &BTreeMap::from([
                    (
                        "email".to_string(),
                        Value::String("katherine@example.com".to_string()),
                    ),
                    ("name".to_string(), Value::String("Katherine".to_string())),
                ]),
            )
            .expect("insert returns row");
        assert_eq!(inserted, user_row(3, "katherine@example.com", "Katherine"));

        let updated = database
            .update_by_id(
                &mapping,
                Value::Number(3),
                &BTreeMap::from([
                    (
                        "email".to_string(),
                        Value::String("kat@example.com".to_string()),
                    ),
                    ("name".to_string(), Value::String("Kat".to_string())),
                ]),
            )
            .expect("update returns row");
        assert_eq!(updated, user_row(3, "kat@example.com", "Kat"));
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
    fn mysql_parameters_reject_non_scalar_values() {
        assert_eq!(
            MysqlParameter::try_from(&Value::Map(BTreeMap::new().into())),
            Err(ActiveRecordError::UnsupportedValue {
                operation: "MySQL parameter",
                actual: "map".to_string(),
            })
        );
    }

    #[test]
    fn mysql_column_values_decode_supported_scalar_values() {
        assert_eq!(
            mysql_column_value(&MysqlSqlValue::NULL, "deleted_at"),
            Ok(Value::Nil)
        );
        assert_eq!(
            mysql_column_value(&MysqlSqlValue::Int(42), "id"),
            Ok(Value::Number(42))
        );
        assert_eq!(
            mysql_column_value(&MysqlSqlValue::UInt(42), "id"),
            Ok(Value::Number(42))
        );
        assert_eq!(
            mysql_column_value(
                &MysqlSqlValue::Bytes("ada@example.com".as_bytes().to_vec()),
                "email",
            ),
            Ok(Value::String("ada@example.com".to_string()))
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
            "User Model Subclass\n  \"users\" Table\n  \"id\" Accessor\n  \"email\" Accessor\nend\n",
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

    fn user_row(id: i64, email: &str, name: &str) -> Value {
        Value::Map(
            BTreeMap::from([
                ("id".to_string(), Value::Number(id)),
                ("email".to_string(), Value::String(email.to_string())),
                ("name".to_string(), Value::String(name.to_string())),
            ])
            .into(),
        )
    }
}
