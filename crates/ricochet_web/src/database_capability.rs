use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use ricochet_vm::{Value, Vm, VmError};
use tokio::runtime::{Handle, RuntimeFlavor};

use crate::active_record::{ActiveRecordError, ModelMapping, PostgresDatabase};

pub trait DatabaseBackend: Send + Sync {
    fn find(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError>;
    fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError>;
    fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
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
    fn find(
        &self,
        mapping: &ModelMapping,
        id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError> {
        block_on_postgres("find", PostgresDatabase::find(self, mapping, id))
    }

    fn all(&self, mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        block_on_postgres("all", PostgresDatabase::all(self, mapping))
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
            Ok(values) => Value::result_ok(Value::Array(values)),
            Err(error) => database_result_error(error),
        })
    })?;

    let find_backend = backend.clone();
    let find_mappings = mappings.clone();
    vm.add_native_method_with_arity("find", 2, move |arguments| {
        let model_name = string_argument(&arguments, 0, "DatabaseCapability.find", "model name")?;
        let id = arguments
            .get(1)
            .ok_or_else(|| missing_native_argument("DatabaseCapability.find", 2, arguments.len()))?;
        let mapping = model_mapping(&find_mappings, model_name);
        Ok(match mapping.and_then(|mapping| find_backend.find(mapping, id)) {
            Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
            Err(error) => database_result_error(error),
        })
    })?;

    let where_backend = backend.clone();
    let where_mappings = mappings.clone();
    vm.add_native_method_with_arity("where", 3, move |arguments| {
        let model_name =
            string_argument(&arguments, 0, "DatabaseCapability.where", "model name")?;
        let field = string_argument(&arguments, 1, "DatabaseCapability.where", "field name")?;
        let value = arguments.get(2).ok_or_else(|| {
            missing_native_argument("DatabaseCapability.where", 3, arguments.len())
        })?;
        let mapping = model_mapping(&where_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| where_backend.where_eq(mapping, field, value)) {
                Ok(values) => Value::result_ok(Value::Array(values)),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let insert_backend = backend.clone();
    let insert_mappings = mappings.clone();
    vm.add_native_method_with_arity("insert", 2, move |arguments| {
        let model_name =
            string_argument(&arguments, 0, "DatabaseCapability.insert", "model name")?;
        let attributes =
            map_argument(&arguments, 1, "DatabaseCapability.insert", "attributes map")?;
        let mapping = model_mapping(&insert_mappings, model_name);
        Ok(
            match mapping.and_then(|mapping| insert_backend.insert(mapping, attributes)) {
                Ok(value) => Value::result_ok(value),
                Err(error) => database_result_error(error),
            },
        )
    })?;

    let update_backend = backend;
    let update_mappings = mappings;
    vm.add_native_method_with_arity("update", 3, move |arguments| {
        let model_name =
            string_argument(&arguments, 0, "DatabaseCapability.update", "model name")?;
        let id = arguments
            .get(1)
            .cloned()
            .ok_or_else(|| missing_native_argument("DatabaseCapability.update", 3, arguments.len()))?;
        let attributes =
            map_argument(&arguments, 2, "DatabaseCapability.update", "attributes map")?;
        let mapping = model_mapping(&update_mappings, model_name);
        Ok(match mapping
            .and_then(|mapping| update_backend.update_by_id(mapping, id, attributes))
        {
            Ok(value) => Value::result_ok(value),
            Err(error) => database_result_error(error),
        })
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
                    Ok(values) => Value::result_ok(Value::Array(values)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let find_backend = backend.clone();
            let find_mapping = mapping.clone();
            let find_method = format!("{}.find", mapping.class_name);
            vm.add_native_method_with_arity("find", 1, move |arguments| {
                let id = arguments.get(0).ok_or_else(|| {
                    missing_native_argument(&find_method, 1, arguments.len())
                })?;
                Ok(match find_backend.find(&find_mapping, id) {
                    Ok(value) => Value::result_ok(value.unwrap_or(Value::Nil)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let where_backend = backend.clone();
            let where_mapping = mapping.clone();
            let where_method = format!("{}.where", mapping.class_name);
            vm.add_native_method_with_arity("where", 2, move |arguments| {
                let field =
                    string_argument(&arguments, 0, &where_method, "field name")?;
                let value = arguments.get(1).ok_or_else(|| {
                    missing_native_argument(&where_method, 2, arguments.len())
                })?;
                Ok(match where_backend.where_eq(&where_mapping, field, value) {
                    Ok(values) => Value::result_ok(Value::Array(values)),
                    Err(error) => database_result_error(error),
                })
            })?;

            let insert_backend = backend.clone();
            let insert_mapping = mapping.clone();
            let insert_method = format!("{}.insert", mapping.class_name);
            vm.add_native_method_with_arity("insert", 1, move |arguments| {
                let attributes =
                    map_argument(&arguments, 0, &insert_method, "attributes map")?;
                Ok(match insert_backend.insert(&insert_mapping, attributes) {
                    Ok(value) => Value::result_ok(value),
                    Err(error) => database_result_error(error),
                })
            })?;

            let update_backend = backend.clone();
            let update_mapping = mapping.clone();
            let update_method = format!("{}.update", mapping.class_name);
            vm.add_native_method_with_arity("update", 2, move |arguments| {
                let id = arguments.get(0).cloned().ok_or_else(|| {
                    missing_native_argument(&update_method, 2, arguments.len())
                })?;
                let attributes =
                    map_argument(&arguments, 1, &update_method, "attributes map")?;
                Ok(match update_backend.update_by_id(&update_mapping, id, attributes) {
                    Ok(value) => Value::result_ok(value),
                    Err(error) => database_result_error(error),
                })
            })?;

            Ok(())
        })();
        vm.end_class();
        install_result?;
    }

    Ok(())
}

fn block_on_postgres<F, T>(
    operation: &'static str,
    future: F,
) -> Result<T, ActiveRecordError>
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

fn map_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    method: &str,
    expected: &str,
) -> Result<&'a BTreeMap<String, Value>, VmError> {
    match arguments.get(index) {
        Some(Value::Map(value)) => Ok(value),
        Some(value) => Err(VmError::TypeError {
            word: method.to_string(),
            expected: expected.to_string(),
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
        Value::Map(_) => "map",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::Member(_) => "member",
        Value::Block(_) => "block",
        Value::Result(_) => "result",
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
            Ok(vec![Value::Map(BTreeMap::from([
                ("id".to_string(), Value::Number(1)),
                ("email".to_string(), Value::String("ada@example.com".to_string())),
            ]))])
        }

        fn where_eq(
            &self,
            _mapping: &ModelMapping,
            _field: &str,
            _value: &Value,
        ) -> Result<Vec<Value>, ActiveRecordError> {
            Ok(Vec::new())
        }

        fn insert(
            &self,
            _mapping: &ModelMapping,
            attributes: &BTreeMap<String, Value>,
        ) -> Result<Value, ActiveRecordError> {
            Ok(Value::Map(attributes.clone()))
        }

        fn update_by_id(
            &self,
            _mapping: &ModelMapping,
            _id: Value,
            attributes: &BTreeMap<String, Value>,
        ) -> Result<Value, ActiveRecordError> {
            Ok(Value::Map(attributes.clone()))
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
        let chunk = ricochet_compiler::compile_source(
            "test.rco",
            "\"User\" db get .all",
        )
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
            ricochet_compiler::compile_source("test.rco", "User .all")
                .expect("source compiles");

        vm.run_chunk(&chunk).expect("active record method runs");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Ok(value))]
                if matches!(value.as_ref(), Value::Array(rows) if rows.len() == 1)
        ));
    }

    #[test]
    fn active_record_find_accepts_an_id_before_the_model_class() {
        let mut vm = vm_with_active_record();
        let chunk =
            ricochet_compiler::compile_source("test.rco", "42 User .find")
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
                Vec::new()
            ))))]
        );
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
        let chunk =
            ricochet_compiler::compile_source("test.rco", "\"Missing\" db get .all")
                .expect("source compiles");

        vm.run_chunk(&chunk).expect("database method returns result");

        assert!(matches!(
            vm.stack(),
            [Value::Result(RicochetResult::Err(error))]
                if error.kind == "DatabaseError" && error.message.contains("Missing")
        ));
    }
}
