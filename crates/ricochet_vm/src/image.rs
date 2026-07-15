use std::collections::{BTreeMap, BTreeSet};

use ricochet_bytecode::{ArgsSpec, Chunk};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::class::{BytecodeCallable, Class};
use crate::collection::{ArrayValue, ListValue, MapValue, SetValue};
use crate::object::Instance;
use crate::result::{RicochetError, RicochetResult};
use crate::value::Value;

pub const VM_IMAGE_FORMAT: &str = "ricochet-vm-image";
pub const VM_IMAGE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImageError {
    #[error("unsupported image format {found:?}; expected {expected:?}")]
    UnsupportedFormat { found: String, expected: String },
    #[error("unsupported image format version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
    #[error("cannot serialize {kind} at {path} into a Ricochet image")]
    NonSerializableValue { path: String, kind: &'static str },
    #[error("cannot serialize native method {class}.{method} into a Ricochet image")]
    NonSerializableNativeMethod { class: String, method: String },
    #[error("cannot save image while {count} retained {kind} resource(s) exist")]
    RetainedResource { kind: &'static str, count: usize },
    #[error("invalid image float {repr:?} at {path}")]
    InvalidFloat { path: String, repr: String },
    #[error("invalid image state: {message}")]
    InvalidImage { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmImage {
    pub format: String,
    pub format_version: u32,
    pub ricochet_version: String,
    pub stack: Vec<ImageValue>,
    pub variables: BTreeMap<String, ImageValue>,
    pub functions: BTreeMap<String, ImageCallable>,
    pub classes: BTreeMap<String, ImageClass>,
}

impl VmImage {
    pub fn empty() -> Self {
        Self {
            format: VM_IMAGE_FORMAT.to_string(),
            format_version: VM_IMAGE_FORMAT_VERSION,
            ricochet_version: crate::crate_version().to_string(),
            stack: Vec::new(),
            variables: BTreeMap::new(),
            functions: BTreeMap::new(),
            classes: BTreeMap::new(),
        }
    }

    pub fn validate_format(&self) -> Result<(), ImageError> {
        if self.format != VM_IMAGE_FORMAT {
            return Err(ImageError::UnsupportedFormat {
                found: self.format.clone(),
                expected: VM_IMAGE_FORMAT.to_string(),
            });
        }
        if self.format_version != VM_IMAGE_FORMAT_VERSION {
            return Err(ImageError::UnsupportedVersion {
                found: self.format_version,
                expected: VM_IMAGE_FORMAT_VERSION,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageCallable {
    pub chunk: Chunk,
    pub args: Option<ArgsSpec>,
}

impl From<&BytecodeCallable> for ImageCallable {
    fn from(callable: &BytecodeCallable) -> Self {
        Self {
            chunk: callable.chunk.clone(),
            args: callable.args.clone(),
        }
    }
}

impl From<ImageCallable> for BytecodeCallable {
    fn from(callable: ImageCallable) -> Self {
        BytecodeCallable::new(callable.chunk, callable.args)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageClass {
    pub name: String,
    pub superclass: String,
    pub table_name: Option<String>,
    pub fields: Vec<String>,
    pub accessors: Vec<String>,
    pub bytecode_methods: BTreeMap<String, ImageCallable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ImageValue {
    Nil,
    Bool(bool),
    Number(i64),
    Float(String),
    String(String),
    Array(Vec<ImageValue>),
    List(Vec<ImageValue>),
    Map(BTreeMap<String, ImageValue>),
    Set(Vec<ImageValue>),
    Class(String),
    Instance {
        class_name: String,
        fields: BTreeMap<String, ImageValue>,
    },
    Member(String),
    Block(Chunk),
    Result(ImageResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ImageResult {
    Ok(Box<ImageValue>),
    Err { kind: String, message: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImageVisit {
    Array(usize),
    List(usize),
    Map(usize),
    Set(usize),
}

pub fn class_to_image(class: &Class) -> Result<ImageClass, ImageError> {
    let accessors = class_accessors(class);
    let mut allowed_native_methods = BTreeSet::new();
    for accessor in &accessors {
        allowed_native_methods.insert(format!("{accessor}.get"));
        allowed_native_methods.insert(format!("{accessor}.set"));
    }
    for method in class.native_methods.keys() {
        if !allowed_native_methods.contains(method) {
            return Err(ImageError::NonSerializableNativeMethod {
                class: class.name.clone(),
                method: method.clone(),
            });
        }
    }

    Ok(ImageClass {
        name: class.name.clone(),
        superclass: class.superclass.clone(),
        table_name: class.table_name.clone(),
        fields: class.fields.clone(),
        accessors,
        bytecode_methods: class
            .bytecode_methods
            .iter()
            .map(|(name, callable)| (name.clone(), ImageCallable::from(callable)))
            .collect(),
    })
}

pub fn value_to_image(value: &Value, path: &str) -> Result<ImageValue, ImageError> {
    value_to_image_inner(value, path, &mut Vec::new())
}

fn value_to_image_inner(
    value: &Value,
    path: &str,
    visits: &mut Vec<ImageVisit>,
) -> Result<ImageValue, ImageError> {
    match value {
        Value::Nil => Ok(ImageValue::Nil),
        Value::Bool(value) => Ok(ImageValue::Bool(*value)),
        Value::Number(value) => Ok(ImageValue::Number(*value)),
        Value::Float(value) => Ok(ImageValue::Float(encode_float(*value))),
        Value::String(value) => Ok(ImageValue::String(value.clone())),
        Value::Array(values) => with_image_collection(
            visits,
            ImageVisit::Array(values.identity()),
            path,
            |visits| sequence_to_image(values.snapshot(), path, visits, ImageValue::Array),
        ),
        Value::List(values) => with_image_collection(
            visits,
            ImageVisit::List(values.identity()),
            path,
            |visits| sequence_to_image(values.snapshot(), path, visits, ImageValue::List),
        ),
        Value::Map(values) => {
            with_image_collection(visits, ImageVisit::Map(values.identity()), path, |visits| {
                map_to_image(values.snapshot(), path, visits)
            })
        }
        Value::Set(values) => {
            with_image_collection(visits, ImageVisit::Set(values.identity()), path, |visits| {
                sequence_to_image(values.snapshot(), path, visits, ImageValue::Set)
            })
        }
        Value::Class(name) => Ok(ImageValue::Class(name.clone())),
        Value::Instance(instance) => instance_to_image(instance, path, visits),
        Value::Member(name) => Ok(ImageValue::Member(name.clone())),
        Value::Block(chunk) => Ok(ImageValue::Block(chunk.clone())),
        Value::Result(result) => result_to_image(result, path, visits),
        Value::Task(_) => Err(non_serializable(path, "task")),
        Value::Regex(_) => Err(non_serializable(path, "regex")),
        Value::Capability(_) => Err(non_serializable(path, "capability")),
        Value::DeferredHttpCredentials(_) => {
            Err(non_serializable(path, "deferred HTTP credentials"))
        }
        Value::SecretRef(_) => Err(non_serializable(path, "secret reference")),
        Value::SecureSessionAction(_) => Err(non_serializable(path, "secure session action")),
    }
}

pub fn value_from_image(value: ImageValue) -> Result<Value, ImageError> {
    match value {
        ImageValue::Nil => Ok(Value::Nil),
        ImageValue::Bool(value) => Ok(Value::Bool(value)),
        ImageValue::Number(value) => Ok(Value::Number(value)),
        ImageValue::Float(repr) => decode_float(&repr, "image value").map(Value::Float),
        ImageValue::String(value) => Ok(Value::String(value)),
        ImageValue::Array(values) => values
            .into_iter()
            .map(value_from_image)
            .collect::<Result<Vec<_>, _>>()
            .map(ArrayValue::from)
            .map(Value::Array),
        ImageValue::List(values) => values
            .into_iter()
            .map(value_from_image)
            .collect::<Result<Vec<_>, _>>()
            .map(ListValue::from)
            .map(Value::List),
        ImageValue::Map(values) => values
            .into_iter()
            .map(|(key, value)| value_from_image(value).map(|value| (key, value)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(MapValue::from)
            .map(Value::Map),
        ImageValue::Set(values) => values
            .into_iter()
            .map(value_from_image)
            .collect::<Result<Vec<_>, _>>()
            .map(SetValue::from)
            .map(Value::Set),
        ImageValue::Class(name) => Ok(Value::Class(name)),
        ImageValue::Instance { class_name, fields } => {
            let fields = fields
                .into_iter()
                .map(|(key, value)| value_from_image(value).map(|value| (key, value)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            Ok(Value::Instance(Instance::new(class_name, fields)))
        }
        ImageValue::Member(name) => Ok(Value::Member(name)),
        ImageValue::Block(chunk) => Ok(Value::Block(chunk)),
        ImageValue::Result(ImageResult::Ok(value)) => Ok(Value::Result(RicochetResult::Ok(
            Box::new(value_from_image(*value)?),
        ))),
        ImageValue::Result(ImageResult::Err { kind, message }) => {
            Ok(Value::Result(RicochetResult::Err(RicochetError {
                kind,
                message,
            })))
        }
    }
}

fn class_accessors(class: &Class) -> Vec<String> {
    class
        .fields
        .iter()
        .filter(|field| {
            class.native_methods.contains_key(&format!("{field}.get"))
                && class.native_methods.contains_key(&format!("{field}.set"))
        })
        .cloned()
        .collect()
}

fn with_image_collection<T>(
    visits: &mut Vec<ImageVisit>,
    visit: ImageVisit,
    path: &str,
    serialize: impl FnOnce(&mut Vec<ImageVisit>) -> Result<T, ImageError>,
) -> Result<T, ImageError> {
    if visits.contains(&visit) {
        return Err(non_serializable(path, "cyclic collection"));
    }
    visits.push(visit);
    let result = serialize(visits);
    visits.pop();
    result
}

fn sequence_to_image<F>(
    values: Vec<Value>,
    path: &str,
    visits: &mut Vec<ImageVisit>,
    build: F,
) -> Result<ImageValue, ImageError>
where
    F: FnOnce(Vec<ImageValue>) -> ImageValue,
{
    values
        .iter()
        .enumerate()
        .map(|(index, value)| value_to_image_inner(value, &format!("{path}[{index}]"), visits))
        .collect::<Result<Vec<_>, _>>()
        .map(build)
}

fn map_to_image(
    values: BTreeMap<String, Value>,
    path: &str,
    visits: &mut Vec<ImageVisit>,
) -> Result<ImageValue, ImageError> {
    if is_literal_secret_reference(&values) {
        return Err(non_serializable(path, "literal secret reference"));
    }

    values
        .iter()
        .map(|(key, value)| {
            value_to_image_inner(value, &format!("{path}.{key}"), visits)
                .map(|value| (key.clone(), value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map(ImageValue::Map)
}

fn instance_to_image(
    instance: &Instance,
    path: &str,
    visits: &mut Vec<ImageVisit>,
) -> Result<ImageValue, ImageError> {
    let fields = instance
        .fields
        .iter()
        .map(|(field, value)| {
            value_to_image_inner(value, &format!("{path}.{field}"), visits)
                .map(|value| (field.clone(), value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(ImageValue::Instance {
        class_name: instance.class_name.clone(),
        fields,
    })
}

fn result_to_image(
    result: &RicochetResult,
    path: &str,
    visits: &mut Vec<ImageVisit>,
) -> Result<ImageValue, ImageError> {
    match result {
        RicochetResult::Ok(value) => value_to_image_inner(value, &format!("{path}.ok"), visits)
            .map(|value| ImageValue::Result(ImageResult::Ok(Box::new(value)))),
        RicochetResult::Err(error) => Ok(ImageValue::Result(ImageResult::Err {
            kind: error.kind.clone(),
            message: error.message.clone(),
        })),
    }
}

fn is_literal_secret_reference(values: &BTreeMap<String, Value>) -> bool {
    matches!(values.get("type"), Some(Value::String(kind)) if kind == "literal")
        && matches!(values.get("value"), Some(Value::String(_)))
}

fn non_serializable(path: &str, kind: &'static str) -> ImageError {
    ImageError::NonSerializableValue {
        path: path.to_string(),
        kind,
    }
}

fn encode_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        value.to_string()
    }
}

fn decode_float(repr: &str, path: &str) -> Result<f64, ImageError> {
    match repr {
        "NaN" => Ok(f64::NAN),
        "Infinity" | "inf" => Ok(f64::INFINITY),
        "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
        _ => repr.parse::<f64>().map_err(|_| ImageError::InvalidFloat {
            path: path.to_string(),
            repr: repr.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_rejects_self_cycles_for_every_collection_kind() {
        let array = ArrayValue::default();
        array.push(Value::Array(array.clone()));
        assert_cycle_path(Value::Array(array), "$[0]");

        let list = ListValue::default();
        list.push(Value::List(list.clone()));
        assert_cycle_path(Value::List(list), "$[0]");

        let map = MapValue::default();
        map.insert("self".to_string(), Value::Map(map.clone()));
        assert_cycle_path(Value::Map(map), "$.self");

        let set = SetValue::default();
        set.insert(Value::Set(set.clone()));
        assert_cycle_path(Value::Set(set), "$[0]");
    }

    #[test]
    fn image_rejects_cycle_through_result_and_instance_at_exact_path() {
        let array = ArrayValue::default();
        let instance = Instance::new(
            "Node",
            BTreeMap::from([("loop".to_string(), Value::Array(array.clone()))]),
        );
        array.push(Value::result_ok(Value::Instance(instance)));

        assert_cycle_path(Value::Array(array), "$[0].ok.loop");
    }

    #[test]
    fn image_serializes_shared_acyclic_child_in_each_branch() {
        let child = ArrayValue::from(vec![Value::Number(7)]);
        let root = Value::Array(ArrayValue::from(vec![
            Value::Array(child.clone()),
            Value::Array(child),
        ]));

        assert_eq!(
            value_to_image(&root, "$"),
            Ok(ImageValue::Array(vec![
                ImageValue::Array(vec![ImageValue::Number(7)]),
                ImageValue::Array(vec![ImageValue::Number(7)]),
            ]))
        );
    }

    fn assert_cycle_path(value: Value, expected_path: &str) {
        assert_eq!(
            value_to_image(&value, "$"),
            Err(ImageError::NonSerializableValue {
                path: expected_path.to_string(),
                kind: "cyclic collection",
            })
        );
    }
}
