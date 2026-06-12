use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use ricochet_vm::{Value, Vm, VmError};
use tokio::runtime::{Handle, RuntimeFlavor};

use crate::active_record::{ActiveRecordError, ModelMapping, PostgresDatabase};

pub trait DatabaseBackend: Send + Sync {
    fn find(&self, mapping: &ModelMapping, id: &Value) -> Result<Option<Value>, ActiveRecordError>;
    fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError>;
    fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError>;
    fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError>;
    fn limit(&self, mapping: &ModelMapping, limit: i64) -> Result<Vec<Value>, ActiveRecordError>;
    fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError>;
    fn exists_by_id(&self, mapping: &ModelMapping, id: &Value) -> Result<bool, ActiveRecordError>;
    fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError>;
    fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError>;
    fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError>;
    fn insert(
        &self,
        mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError>;
    fn update_by_id(
        &self,
        mapping: &ModelMapping,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError>;
}

impl DatabaseBackend for PostgresDatabase {
    fn find(&self, mapping: &ModelMapping, id: &Value) -> Result<Option<Value>, ActiveRecordError> {
        block_on_postgres("find", PostgresDatabase::find(self, mapping, id))
    }

    fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres("all", PostgresDatabase::all(self, mapping))
    }

    fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
        block_on_postgres("count", PostgresDatabase::count(self, mapping))
    }

    fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
        block_on_postgres("first", PostgresDatabase::first(self, mapping))
    }

    fn limit(&self, mapping: &ModelMapping, limit: i64) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres("limit", PostgresDatabase::limit(self, mapping, limit))
    }

    fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres("page", PostgresDatabase::page(self, mapping, limit, offset))
    }

    fn exists_by_id(&self, mapping: &ModelMapping, id: &Value) -> Result<bool, ActiveRecordError> {
        block_on_postgres("exists", PostgresDatabase::exists_by_id(self, mapping, id))
    }

    fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres(
            "where",
            PostgresDatabase::where_eq(self, mapping, field, value),
        )
    }

    fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres(
            "where-limit",
            PostgresDatabase::where_eq_limit(self, mapping, field, value, limit),
        )
    }

    fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres(
            "where-page",
            PostgresDatabase::where_eq_page(self, mapping, field, value, limit, offset),
        )
    }

    fn insert(
        &self,
        mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        block_on_postgres(
            "insert",
            PostgresDatabase::insert(self, mapping, attributes),
        )
    }

    fn update_by_id(
        &self,
        mapping: &ModelMapping,
        id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        block_on_postgres(
            "update",
            PostgresDatabase::update_by_id(self, mapping, id, attributes),
        )
    }
}

pub fn install_database_capability(
    vm: &mut Vm,
    backend: Arc<dyn DatabaseBackend>,
    mappings: BTreeMap<String, ModelMapping>,
) -> Result<Value, VmError> {
    install_model_active_record_methods(vm, backend.clone(), &mappings)?;
    let mappings = Arc::new(mappings);
    vm.define_class("DatabaseCapability", "Capability")?;

    let all_backend = backend.clone();
    let all_mappings = mappings.clone();
    vm.add_native_method_with_arity("all", 1, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.all", "model name")?;
        let mapping = model_mapping(&all_mappings, model_name);
        Ok(match mapping.and_then(|mapping| all_backend.all(mapping)) {
            Ok(values) => Value::result_ok(Value::Array(values.into())),
            Err(error) => database_result_error(error),
        })
    })?;

    let count_backend = backend.clone();
    let count_mappings = mappings.clone();
    vm.add_native_method_with_arity("count", 1, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.count", "model name")?;
        let mapping = model_mapping(&count_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| count_backend.count(mapping)) {
                Ok(value) => Value::result_ok(Value::Number(value)),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let first_backend = backend.clone();
    let first_mappings = mappings.clone();
    vm.add_native_method_with_arity("first", 1, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.first", "model name")?;
        let mapping = model_mapping(&first_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| first_backend.first(mapping)) {
                Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let limit_backend = backend.clone();
    let limit_mappings = mappings.clone();
    vm.add_native_method_with_arity("limit", 2, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.limit", "model name")?;
        let limit = limit_argument(&arguments, 1, "DatabaseCapability.limit")?;
        let mapping = model_mapping(&limit_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| limit_backend.limit(mapping, limit)) {
                Ok(values) => Value::result_ok(Value::Array(values.into())),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let page_backend = backend.clone();
    let page_mappings = mappings.clone();
    vm.add_native_method_with_arity("page", 3, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.page", "model name")?;
        let limit = limit_argument(&arguments, 1, "DatabaseCapability.page")?;
        let offset = offset_argument(&arguments, 2, "DatabaseCapability.page")?;
        let mapping = model_mapping(&page_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| page_backend.page(mapping, limit, offset)) {
                Ok(values) => Value::result_ok(Value::Array(values.into())),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let exists_backend = backend.clone();
    let exists_mappings = mappings.clone();
    vm.add_native_method_with_arity("exists?", 2, move |arguments| {
        let model_name =
            string_argument(&arguments, 0, "DatabaseCapability.exists?", "model name")?;
        let id = arguments.get(1).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.exists?", 2, arguments.len())
        })?;
        let mapping = model_mapping(&exists_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| exists_backend.exists_by_id(mapping, id)) {
                Ok(value) => Value::result_ok(Value::Bool(value)),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let find_backend = backend.clone();
    let find_mappings = mappings.clone();
    vm.add_native_method_with_arity("find", 2, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.find", "model name")?;
        let id = arguments.get(1).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.find", 2, arguments.len())
        })?;
        let mapping = model_mapping(&find_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| find_backend.find(mapping, id)) {
                Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let where_backend = backend.clone();
    let where_mappings = mappings.clone();
    vm.add_native_method_with_arity("where", 3, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.where", "model name")?;
        let field = string_argument(&arguments, 1, "DatabaseCapability.where", "field name")?;
        let value = arguments.get(2).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.where", 3, arguments.len())
        })?;
        let mapping = model_mapping(&where_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| where_backend.where_eq(mapping, field, value)) {
                Ok(values) => Value::result_ok(Value::Array(values.into())),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let where_limit_backend = backend.clone();
    let where_limit_mappings = mappings.clone();
    vm.add_native_method_with_arity("where-limit", 4, move |arguments| {
        let model_name = string_argument(
            &arguments,
            0,
            "DatabaseCapability.where-limit",
            "model name",
        )?;
        let field = string_argument(
            &arguments,
            1,
            "DatabaseCapability.where-limit",
            "field name",
        )?;
        let value = arguments.get(2).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.where-limit", 4, arguments.len())
        })?;
        let limit = limit_argument(&arguments, 3, "DatabaseCapability.where-limit")?;
        let mapping = model_mapping(&where_limit_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| {
                where_limit_backend.where_eq_limit(mapping, field, value, limit)
            }) {
                Ok(values) => Value::result_ok(Value::Array(values.into())),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let where_page_backend = backend.clone();
    let where_page_mappings = mappings.clone();
    vm.add_native_method_with_arity("where-page", 5, move |arguments| {
        let model_name =
            string_argument(&arguments, 0, "DatabaseCapability.where-page", "model name")?;
        let field = string_argument(&arguments, 1, "DatabaseCapability.where-page", "field name")?;
        let value = arguments.get(2).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.where-page", 5, arguments.len())
        })?;
        let limit = limit_argument(&arguments, 3, "DatabaseCapability.where-page")?;
        let offset = offset_argument(&arguments, 4, "DatabaseCapability.where-page")?;
        let mapping = model_mapping(&where_page_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| {
                where_page_backend.where_eq_page(mapping, field, value, limit, offset)
            }) {
                Ok(values) => Value::result_ok(Value::Array(values.into())),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let insert_backend = backend.clone();
    let insert_mappings = mappings.clone();
    vm.add_native_method_with_arity("insert", 2, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.insert", "model name")?;
        let attributes =
            map_argument(&arguments, 1, "DatabaseCapability.insert", "attributes map")?;
        let mapping = model_mapping(&insert_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| insert_backend.insert(mapping, &attributes)) {
                Ok(value) => Value::result_ok(value),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let update_backend = backend;
    let update_mappings = mappings;
    vm.add_native_method_with_arity("update", 3, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.update", "model name")?;
        let id = arguments.get(1).cloned().ok_or_else(|| {
            missing_native_argument("DatabaseCapability.update", 3, arguments.len())
        })?;
        let attributes =
            map_argument(&arguments, 2, "DatabaseCapability.update", "attributes map")?;
        let mapping = model_mapping(&update_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| update_backend.update_by_id(mapping, id, &attributes))
            {
                Ok(value) => Value::result_ok(value),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    vm.end_class();
    vm.new_instance("DatabaseCapability")
}

fn install_model_active_record_methods(
    vm: &mut Vm,
    backend: Arc<dyn DatabaseBackend>,
    mappings: &BTreeMap<String, ModelMapping>,
) -> Result<(), VmError> {
    for mapping in mappings.values() {
        vm.define_class(mapping.class_name.clone(), "Model")?;
        let install_result = (|| {
            let all_backend = backend.clone();
            let all_mapping = mapping.clone();
            vm.add_native_method_with_arity("all", 0, move |_| {
                Ok(match all_backend.all(&all_mapping) {
                    Ok(values) => Value::result_ok(Value::Array(values.into())),
                    Err(error) => database_result_error(error),
                })
            })?;

            let count_backend = backend.clone();
            let count_mapping = mapping.clone();
            vm.add_native_method_with_arity("count", 0, move |_| {
                Ok(match count_backend.count(&count_mapping) {
                    Ok(value) => Value::result_ok(Value::Number(value)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let first_backend = backend.clone();
            let first_mapping = mapping.clone();
            vm.add_native_method_with_arity("first", 0, move |_| {
                Ok(match first_backend.first(&first_mapping) {
                    Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let limit_backend = backend.clone();
            let limit_mapping = mapping.clone();
            let limit_method = format!("{}.limit", mapping.class_name);
            vm.add_native_method_with_arity("limit", 1, move |arguments| {
                let limit = limit_argument(&arguments, 0, &limit_method)?;
                Ok(match limit_backend.limit(&limit_mapping, limit) {
                    Ok(values) => Value::result_ok(Value::Array(values.into())),
                    Err(error) => database_result_error(error),
                })
            })?;

            let page_backend = backend.clone();
            let page_mapping = mapping.clone();
            let page_method = format!("{}.page", mapping.class_name);
            vm.add_native_method_with_arity("page", 2, move |arguments| {
                let limit = limit_argument(&arguments, 0, &page_method)?;
                let offset = offset_argument(&arguments, 1, &page_method)?;
                Ok(match page_backend.page(&page_mapping, limit, offset) {
                    Ok(values) => Value::result_ok(Value::Array(values.into())),
                    Err(error) => database_result_error(error),
                })
            })?;

            let exists_backend = backend.clone();
            let exists_mapping = mapping.clone();
            let exists_method = format!("{}.exists?", mapping.class_name);
            vm.add_native_method_with_arity("exists?", 1, move |arguments| {
                let id = arguments
                    .first()
                    .ok_or_else(|| missing_native_argument(&exists_method, 1, arguments.len()))?;
                Ok(match exists_backend.exists_by_id(&exists_mapping, id) {
                    Ok(value) => Value::result_ok(Value::Bool(value)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let find_backend = backend.clone();
            let find_mapping = mapping.clone();
            let find_method = format!("{}.find", mapping.class_name);
            vm.add_native_method_with_arity("find", 1, move |arguments| {
                let id = arguments
                    .first()
                    .ok_or_else(|| missing_native_argument(&find_method, 1, arguments.len()))?;
                Ok(match find_backend.find(&find_mapping, id) {
                    Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let where_backend = backend.clone();
            let where_mapping = mapping.clone();
            let where_method = format!("{}.where", mapping.class_name);
            vm.add_native_method_with_arity("where", 2, move |arguments| {
                let field = string_argument(&arguments, 0, &where_method, "field name")?;
                let value = arguments
                    .get(1)
                    .ok_or_else(|| missing_native_argument(&where_method, 2, arguments.len()))?;
                Ok(match where_backend.where_eq(&where_mapping, field, value) {
                    Ok(values) => Value::result_ok(Value::Array(values.into())),
                    Err(error) => database_result_error(error),
                })
            })?;

            let where_limit_backend = backend.clone();
            let where_limit_mapping = mapping.clone();
            let where_limit_method = format!("{}.where-limit", mapping.class_name);
            vm.add_native_method_with_arity("where-limit", 3, move |arguments| {
                let field = string_argument(&arguments, 0, &where_limit_method, "field name")?;
                let value = arguments.get(1).ok_or_else(|| {
                    missing_native_argument(&where_limit_method, 3, arguments.len())
                })?;
                let limit = limit_argument(&arguments, 2, &where_limit_method)?;
                Ok(
                    match where_limit_backend.where_eq_limit(
                        &where_limit_mapping,
                        field,
                        value,
                        limit,
                    ) {
                        Ok(values) => Value::result_ok(Value::Array(values.into())),
                        Err(error) => database_result_error(error),
                    },
                )
            })?;

            let where_page_backend = backend.clone();
            let where_page_mapping = mapping.clone();
            let where_page_method = format!("{}.where-page", mapping.class_name);
            vm.add_native_method_with_arity("where-page", 4, move |arguments| {
                let field = string_argument(&arguments, 0, &where_page_method, "field name")?;
                let value = arguments.get(1).ok_or_else(|| {
                    missing_native_argument(&where_page_method, 4, arguments.len())
                })?;
                let limit = limit_argument(&arguments, 2, &where_page_method)?;
                let offset = offset_argument(&arguments, 3, &where_page_method)?;
                Ok(
                    match where_page_backend.where_eq_page(
                        &where_page_mapping,
                        field,
                        value,
                        limit,
                        offset,
                    ) {
                        Ok(values) => Value::result_ok(Value::Array(values.into())),
                        Err(error) => database_result_error(error),
                    },
                )
            })?;

            let insert_backend = backend.clone();
            let insert_mapping = mapping.clone();
            let insert_method = format!("{}.insert", mapping.class_name);
            vm.add_native_method_with_arity("insert", 1, move |arguments| {
                let attributes = map_argument(&arguments, 0, &insert_method, "attributes map")?;
                Ok(match insert_backend.insert(&insert_mapping, &attributes) {
                    Ok(value) => Value::result_ok(value),
                    Err(error) => database_result_error(error),
                })
            })?;

            let update_backend = backend.clone();
            let update_mapping = mapping.clone();
            let update_method = format!("{}.update", mapping.class_name);
            vm.add_native_method_with_arity("update", 2, move |arguments| {
                let id = arguments
                    .first()
                    .cloned()
                    .ok_or_else(|| missing_native_argument(&update_method, 2, arguments.len()))?;
                let attributes = map_argument(&arguments, 1, &update_method, "attributes map")?;
                Ok(
                    match update_backend.update_by_id(&update_mapping, id, &attributes) {
                        Ok(value) => Value::result_ok(value),
                        Err(error) => database_result_error(error),
                    },
                )
            })?;

            Ok(())
        })();
        vm.end_class();
        install_result?;
    }

    Ok(())
}

fn block_on_postgres<F, T>(operation: &'static str, future: F) -> Result<T, ActiveRecordError>
where
    F: Future<Output = Result<T, ActiveRecordError>>,
{
    let handle = Handle::try_current().map_err(|error| ActiveRecordError::Database {
        operation,
        message: format!("no Tokio runtime is available: {error}"),
    })?;
    if handle.runtime_flavor() == RuntimeFlavor::CurrentThread {
        return Err(ActiveRecordError::Database {
            operation,
            message: "database capability requires a multithreaded Tokio runtime".to_string(),
        });
    }

    tokio::task::block_in_place(|| handle.block_on(future))
}

fn model_mapping<'a>(
    mappings: &'a BTreeMap<String, ModelMapping>,
    model_name: &str,
) -> Result<&'a ModelMapping, ActiveRecordError> {
    mappings
        .get(model_name)
        .ok_or_else(|| ActiveRecordError::UnknownModel {
            class_name: model_name.to_string(),
        })
}

fn database_result_error(error: ActiveRecordError) -> Value {
    Value::result_err("DatabaseError", error.to_string())
}

fn string_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    method: &str,
    expected: &str,
) -> Result<&'a str, VmError> {
    match arguments.get(index) {
        Some(Value::String(value)) => Ok(value),
        Some(value) => Err(VmError::TypeError {
            word: method.to_string(),
            expected: expected.to_string(),
            actual: value_kind(value).to_string(),
        }),
        None => Err(missing_native_argument(method, index + 1, arguments.len())),
    }
}

fn map_argument(
    arguments: &[Value],
    index: usize,
    method: &str,
    expected: &str,
) -> Result<BTreeMap<String, Value>, VmError> {
    match arguments.get(index) {
        Some(Value::Map(value)) => Ok(value.snapshot()),
        Some(value) => Err(VmError::TypeError {
            word: method.to_string(),
            expected: expected.to_string(),
            actual: value_kind(value).to_string(),
        }),
        None => Err(missing_native_argument(method, index + 1, arguments.len())),
    }
}

fn limit_argument(arguments: &[Value], index: usize, method: &str) -> Result<i64, VmError> {
    match arguments.get(index) {
        Some(Value::Number(value)) if *value >= 0 => Ok(*value),
        Some(Value::Number(value)) => Err(VmError::InvalidArgument {
            word: method.to_string(),
            message: format!("limit must be non-negative, got {value}"),
        }),
        Some(value) => Err(VmError::TypeError {
            word: method.to_string(),
            expected: "non-negative number".to_string(),
            actual: value_kind(value).to_string(),
        }),
        None => Err(missing_native_argument(method, index + 1, arguments.len())),
    }
}

fn offset_argument(arguments: &[Value], index: usize, method: &str) -> Result<i64, VmError> {
    match arguments.get(index) {
        Some(Value::Number(value)) if *value >= 0 => Ok(*value),
        Some(Value::Number(value)) => Err(VmError::InvalidArgument {
            word: method.to_string(),
            message: format!("offset must be non-negative, got {value}"),
        }),
        Some(value) => Err(VmError::TypeError {
            word: method.to_string(),
            expected: "non-negative number".to_string(),
            actual: value_kind(value).to_string(),
        }),
        None => Err(missing_native_argument(method, index + 1, arguments.len())),
    }
}

fn missing_native_argument(method: &str, needed: usize, available: usize) -> VmError {
    VmError::StackUnderflow {
        word: method.to_string(),
        needed,
        available,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ricochet_vm::RicochetResult;

    struct FixtureDatabase;

    impl DatabaseBackend for FixtureDatabase {
        fn find(
            &self,
            _mapping: &ModelMapping,
            _id: &Value,
        ) -> Result<Option<Value>, ActiveRecordError> {
            Ok(None)
        }

        fn all(&self, _mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
            Ok(vec![Value::Map(
                BTreeMap::from([
                    ("id".to_string(), Value::Number(1)),
                    (
                        "email".to_string(),
                        Value::String("ada@example.com".to_string()),
                    ),
                ])
                .into(),
            )])
        }

        fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
            Ok(self.all(mapping)?.len() as i64)
        }

        fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
            Ok(self.all(mapping)?.into_iter().next())
        }

        fn limit(
            &self,
            mapping: &ModelMapping,
            limit: i64,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            let mut rows = self.all(mapping)?;
            rows.truncate(limit as usize);
            Ok(rows)
        }

        fn page(
            &self,
            mapping: &ModelMapping,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            Ok(self
                .all(mapping)?
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect())
        }

        fn exists_by_id(
            &self,
            mapping: &ModelMapping,
            id: &Value,
        ) -> Result<bool, ActiveRecordError> {
            Ok(self.all(mapping)?.iter().any(|row| match row {
                Value::Map(row) => row.get("id").as_ref() == Some(id),
                _ => false,
            }))
        }

        fn where_eq(
            &self,
            mapping: &ModelMapping,
            field: &str,
            value: &Value,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            Ok(self
                .all(mapping)?
                .into_iter()
                .filter(|row| match row {
                    Value::Map(row) => row.get(field).as_ref() == Some(value),
                    _ => false,
                })
                .collect())
        }

        fn where_eq_limit(
            &self,
            mapping: &ModelMapping,
            field: &str,
            value: &Value,
            limit: i64,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            let mut rows = self.where_eq(mapping, field, value)?;
            rows.truncate(limit as usize);
            Ok(rows)
        }

        fn where_eq_page(
            &self,
            mapping: &ModelMapping,
            field: &str,
            value: &Value,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            Ok(self
                .where_eq(mapping, field, value)?
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect())
        }

        fn insert(
            &self,
            _mapping: &ModelMapping,
            attributes: &BTreeMap<String, Value>,
        ) -> Result<Value, ActiveRecordError> {
            Ok(Value::Map(attributes.clone().into()))
        }

        fn update_by_id(
            &self,
            _mapping: &ModelMapping,
            _id: Value,
            attributes: &BTreeMap<String, Value>,
        ) -> Result<Value, ActiveRecordError> {
            Ok(Value::Map(attributes.clone().into()))
        }
    }

    fn user_mapping() -> ModelMapping {
        ModelMapping::try_new("User", "users", ["id", "email"]).expect("mapping is valid")
    }

    fn vm_with_active_record() -> Vm {
        let mut vm = Vm::default();
        let model = ricochet_compiler::compile_source(
            "app/Models/User.rco",
            "User Model subclass\n  users table\n  id field\n  email field\nend\n",
        )
        .expect("model compiles");
        vm.run_chunk(&model).expect("model loads");
        install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm
    }

    #[test]
    fn database_all_is_callable_from_ricochet_code() {
        let mut vm = Vm::default();
        let capability = install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source("test.rco", "\"User\" db get .all")
            .expect("source compiles");

        vm.run_chunk(&chunk).expect("database method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn active_record_all_is_callable_on_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk =
            ricochet_compiler::compile_source("test.rco", "User .all").expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn active_record_limit_accepts_a_count_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source("test.rco", "1 User .limit")
            .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn active_record_page_accepts_limit_and_offset_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source("test.rco", "1 1 User .page")
            .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.is_empty())
        ));
    }

    #[test]
    fn active_record_count_returns_a_number_result() {
        let mut vm = vm_with_active_record();
        let chunk =
            ricochet_compiler::compile_source("test.rco", "User .count").expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Number(
                1
            ))))]
        );
    }

    #[test]
    fn active_record_first_returns_first_row_or_nil() {
        let mut vm = vm_with_active_record();
        let chunk =
            ricochet_compiler::compile_source("test.rco", "User .first").expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Map(row) if row.get("email") == Some(Value::String("ada@example.com".to_string())))
        ));
    }

    #[test]
    fn active_record_exists_accepts_an_id_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source("test.rco", "1 User .exists?")
            .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Bool(
                true
            ))))]
        );
    }

    #[test]
    fn database_capability_limit_accepts_model_name_and_count() {
        let mut vm = Vm::default();
        let capability = install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source("test.rco", "\"User\" 1 db get .limit")
            .expect("source compiles");

        vm.run_chunk(&chunk)
            .expect("database capability limit method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn database_capability_page_accepts_model_name_limit_and_offset() {
        let mut vm = Vm::default();
        let capability = install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source("test.rco", "\"User\" 1 1 db get .page")
            .expect("source compiles");

        vm.run_chunk(&chunk)
            .expect("database capability page method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.is_empty())
        ));
    }

    #[test]
    fn database_capability_supports_count_first_and_exists() {
        let mut vm = Vm::default();
        let capability = install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"User\" db get .count value\n\"User\" db get .first value \"email\" swap .at\n\"User\" 1 db get .exists? value",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk)
            .expect("database capability methods run");

        assert_eq!(
            vm.stack(),
            &[
                Value::Number(1),
                Value::String("ada@example.com".to_string()),
                Value::Bool(true),
            ]
        );
    }

    #[test]
    fn active_record_find_accepts_an_id_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source("test.rco", "42 User .find")
            .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Nil)))]
        );
    }

    #[test]
    fn active_record_where_accepts_field_and_value_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"email\" \"ada@example.com\" User .where",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Array(
                vec![Value::Map(
                    BTreeMap::from([
                        ("id".to_string(), Value::Number(1)),
                        (
                            "email".to_string(),
                            Value::String("ada@example.com".to_string())
                        )
                    ])
                    .into()
                )]
                .into()
            ))))]
        );
    }

    #[test]
    fn active_record_where_limit_accepts_field_value_and_limit_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"email\" \"ada@example.com\" 1 User .where-limit",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn active_record_where_page_accepts_field_value_limit_and_offset_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"email\" \"ada@example.com\" 1 1 User .where-page",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.is_empty())
        ));
    }

    #[test]
    fn database_capability_supports_bounded_where_queries() {
        let mut vm = Vm::default();
        let capability = install_database_capability(
            &mut vm,
            Arc::new(FixtureDatabase),
            BTreeMap::from([("User".to_string(), user_mapping())]),
        )
        .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"User\" \"email\" \"ada@example.com\" 1 db get .where-limit\n\"User\" \"email\" \"ada@example.com\" 1 1 db get .where-page",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk)
            .expect("database capability bounded where methods run");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(first)), Value::Result(RicochetResult::Ok(second))]
                if matches!(first.as_ref(), Value::Array(rows) if rows.len() == 1)
                    && matches!(second.as_ref(), Value::Array(rows) if rows.is_empty())
        ));
    }

    #[test]
    fn active_record_insert_accepts_an_attributes_map_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "map \"email\" \"ada@example.com\" !put User .insert",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Map(
                BTreeMap::from([(
                    "email".to_string(),
                    Value::String("ada@example.com".to_string())
                )])
                .into()
            ))))]
        );
    }

    #[test]
    fn active_record_update_accepts_id_and_attributes_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "42 map \"email\" \"grace@example.com\" !put User .update",
        )
        .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert_eq!(
            vm.stack(),
            &[Value::Result(RicochetResult::Ok(Box::new(Value::Map(
                BTreeMap::from([(
                    "email".to_string(),
                    Value::String("grace@example.com".to_string())
                )])
                .into()
            ))))]
        );
    }

    #[test]
    fn unknown_model_is_an_expected_database_result_error() {
        let mut vm = Vm::default();
        let capability =
            install_database_capability(&mut vm, Arc::new(FixtureDatabase), BTreeMap::new())
                .expect("capability installs");
        vm.set_variable("db", capability);
        let chunk = ricochet_compiler::compile_source("test.rco", "\"Missing\" db get .all")
            .expect("source compiles");

        vm.run_chunk(&chunk)
            .expect("database method returns result");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Err(error))]
                if error.kind == "DatabaseError" && error.message.contains("Missing")
        ));
    }
}
