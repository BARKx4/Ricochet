use ricochet_bytecode::Chunk;
use ricochet_secrets::{DeferredHttpCredentials, SecretRef};
use std::fmt;

use thiserror::Error;

use crate::capability::Capability;
use crate::collection::{ArrayValue, ListValue, MapValue, SetValue};
use crate::object::Instance;
use crate::regex_value::RegexValue;
use crate::result::{RicochetError, RicochetResult};
use crate::SecureSessionActionDescriptor;

#[derive(Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(i64),
    Float(f64),
    String(String),
    Array(ArrayValue),
    List(ListValue),
    Map(MapValue),
    Set(SetValue),
    Class(String),
    Instance(Instance),
    Member(String),
    Block(Chunk),
    Task(u64),
    Result(RicochetResult),
    Regex(RegexValue),
    Capability(Capability),
    DeferredHttpCredentials(DeferredHttpCredentials),
    SecretRef(SecretRef),
    SecureSessionAction(SecureSessionActionDescriptor),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpaqueValueVisit {
    Array(usize),
    List(usize),
    Map(usize),
    Set(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TruthinessError {
    #[error("result values require an explicit ok? check in conditions")]
    ResultRequiresExplicitOk,
    #[error("deferred HTTP credentials cannot be used as a condition")]
    OpaqueDeferredHttpCredentials,
    #[error("secret references cannot be used as a condition")]
    OpaqueSecretRef,
    #[error("secure session actions cannot be used as a condition")]
    OpaqueSecureSessionAction,
}

impl Value {
    /// Returns the public surface label for a nested opaque value, if present.
    ///
    /// Traversal is identity-based and cycle-safe. Callers must perform this
    /// check before any operation that would otherwise reach derived equality
    /// or serialize/inspect container contents.
    pub fn opaque_value_kind(&self) -> Option<&'static str> {
        self.opaque_value_kind_inner(&mut Vec::new())
    }

    pub fn contains_deferred_http_credential(&self) -> bool {
        self.contains_deferred_http_credential_inner(&mut Vec::new())
    }

    fn opaque_value_kind_inner(&self, visits: &mut Vec<OpaqueValueVisit>) -> Option<&'static str> {
        let (visit, values) = match self {
            Value::DeferredHttpCredentials(_) => return Some("deferred HTTP credentials"),
            Value::SecretRef(_) => return Some("secret reference"),
            Value::SecureSessionAction(_) => return Some("secure session action"),
            Value::Array(values) => (
                OpaqueValueVisit::Array(values.identity()),
                values.snapshot(),
            ),
            Value::List(values) => (OpaqueValueVisit::List(values.identity()), values.snapshot()),
            Value::Map(values) => (OpaqueValueVisit::Map(values.identity()), values.values()),
            Value::Set(values) => (OpaqueValueVisit::Set(values.identity()), values.snapshot()),
            Value::Instance(instance) => {
                return instance
                    .fields
                    .values()
                    .find_map(|value| value.opaque_value_kind_inner(visits));
            }
            Value::Result(RicochetResult::Ok(value)) => {
                return value.opaque_value_kind_inner(visits);
            }
            Value::Nil
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Class(_)
            | Value::Member(_)
            | Value::Block(_)
            | Value::Task(_)
            | Value::Result(RicochetResult::Err(_))
            | Value::Regex(_)
            | Value::Capability(_) => return None,
        };

        if visits.contains(&visit) {
            return None;
        }
        visits.push(visit);
        values
            .iter()
            .find_map(|value| value.opaque_value_kind_inner(visits))
    }

    fn contains_deferred_http_credential_inner(&self, visits: &mut Vec<OpaqueValueVisit>) -> bool {
        let (visit, values) = match self {
            Value::DeferredHttpCredentials(_) => return true,
            Value::Array(values) => (
                OpaqueValueVisit::Array(values.identity()),
                values.snapshot(),
            ),
            Value::List(values) => (OpaqueValueVisit::List(values.identity()), values.snapshot()),
            Value::Map(values) => (OpaqueValueVisit::Map(values.identity()), values.values()),
            Value::Set(values) => (OpaqueValueVisit::Set(values.identity()), values.snapshot()),
            Value::Instance(instance) => {
                return instance
                    .fields
                    .values()
                    .any(|value| value.contains_deferred_http_credential_inner(visits));
            }
            Value::Result(RicochetResult::Ok(value)) => {
                return value.contains_deferred_http_credential_inner(visits);
            }
            Value::Nil
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Float(_)
            | Value::String(_)
            | Value::Class(_)
            | Value::Member(_)
            | Value::Block(_)
            | Value::Task(_)
            | Value::Result(RicochetResult::Err(_))
            | Value::Regex(_)
            | Value::Capability(_)
            | Value::SecretRef(_)
            | Value::SecureSessionAction(_) => return false,
        };

        if visits.contains(&visit) {
            return false;
        }
        visits.push(visit);
        values
            .iter()
            .any(|value| value.contains_deferred_http_credential_inner(visits))
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(v) => *v,
            Value::Number(v) => *v != 0,
            Value::Float(v) => !v.is_nan() && *v != 0.0,
            Value::String(v) => !v.is_empty(),
            Value::Array(v) => !v.is_empty(),
            Value::List(v) => !v.is_empty(),
            Value::Map(v) => !v.is_empty(),
            Value::Set(v) => !v.is_empty(),
            Value::Class(_) => true,
            Value::Instance(_) => true,
            Value::Member(_) => true,
            Value::Block(_) => true,
            Value::Task(_) => true,
            Value::Result(_) => true,
            Value::Regex(_) => true,
            Value::Capability(_) => true,
            Value::DeferredHttpCredentials(_) => true,
            Value::SecretRef(_) => true,
            Value::SecureSessionAction(_) => true,
        }
    }

    pub fn truthy_for_condition(&self) -> Result<bool, TruthinessError> {
        match self {
            Value::Result(_) => Err(TruthinessError::ResultRequiresExplicitOk),
            Value::DeferredHttpCredentials(_) => {
                Err(TruthinessError::OpaqueDeferredHttpCredentials)
            }
            Value::SecretRef(_) => Err(TruthinessError::OpaqueSecretRef),
            Value::SecureSessionAction(_) => Err(TruthinessError::OpaqueSecureSessionAction),
            _ => Ok(self.truthy()),
        }
    }

    pub fn result_ok(value: Value) -> Self {
        Value::Result(RicochetResult::Ok(Box::new(value)))
    }

    pub fn result_err(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Value::Result(RicochetResult::Err(RicochetError {
            kind: kind.into(),
            message: message.into(),
        }))
    }

    pub fn call_predicate(&self, name: &str) -> Option<Value> {
        match (self, name) {
            (Value::Result(RicochetResult::Ok(_)), "ok?") => Some(Value::Bool(true)),
            (Value::Result(RicochetResult::Err(_)), "ok?") => Some(Value::Bool(false)),
            (Value::Nil, "nil?") => Some(Value::Bool(true)),
            (_, "nil?") => Some(Value::Bool(false)),
            (Value::String(s), "empty?") => Some(Value::Bool(s.is_empty())),
            (Value::Array(a), "empty?") => Some(Value::Bool(a.is_empty())),
            (Value::List(a), "empty?") => Some(Value::Bool(a.is_empty())),
            (Value::Map(m), "empty?") => Some(Value::Bool(m.is_empty())),
            (Value::Set(s), "empty?") => Some(Value::Bool(s.is_empty())),
            _ => None,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => formatter.write_str("Nil"),
            Self::Bool(value) => formatter.debug_tuple("Bool").field(value).finish(),
            Self::Number(value) => formatter.debug_tuple("Number").field(value).finish(),
            Self::Float(value) => formatter.debug_tuple("Float").field(value).finish(),
            Self::String(value) => formatter.debug_tuple("String").field(value).finish(),
            Self::Array(value) => formatter.debug_tuple("Array").field(value).finish(),
            Self::List(value) => formatter.debug_tuple("List").field(value).finish(),
            Self::Map(value) => formatter.debug_tuple("Map").field(value).finish(),
            Self::Set(value) => formatter.debug_tuple("Set").field(value).finish(),
            Self::Class(value) => formatter.debug_tuple("Class").field(value).finish(),
            Self::Instance(value) => formatter.debug_tuple("Instance").field(value).finish(),
            Self::Member(value) => formatter.debug_tuple("Member").field(value).finish(),
            Self::Block(value) => formatter.debug_tuple("Block").field(value).finish(),
            Self::Task(value) => formatter.debug_tuple("Task").field(value).finish(),
            Self::Result(value) => formatter.debug_tuple("Result").field(value).finish(),
            Self::Regex(value) => formatter.debug_tuple("Regex").field(value).finish(),
            Self::Capability(value) => formatter.debug_tuple("Capability").field(value).finish(),
            Self::DeferredHttpCredentials(_) => formatter.write_str("<http-credentials>"),
            Self::SecretRef(_) => formatter.write_str("<secret-ref>"),
            Self::SecureSessionAction(_) => formatter.write_str("<secure-session-action>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::collection::{ArrayValue, MapValue};
    use crate::object::Instance;

    use super::*;

    fn deferred_http_credentials_value(secret: &str) -> Value {
        let source = ricochet_secrets::DeferredSecretSource::literal(secret.to_string())
            .expect("fixture should construct");
        Value::DeferredHttpCredentials(ricochet_secrets::DeferredHttpCredentials::bearer(source))
    }

    #[test]
    fn deferred_http_credentials_values_are_redacted_and_shared_by_clone() {
        let value = deferred_http_credentials_value("synthetic-secret-value");
        let clone = value.clone();
        let separate = deferred_http_credentials_value("synthetic-secret-value");

        assert_eq!(value, clone);
        assert_ne!(value, separate);
        let rendered = format!("{value:?}");
        assert!(rendered.contains("<http-credentials>"));
        assert!(!rendered.contains("synthetic-secret-value"));
        assert_eq!(
            value.truthy_for_condition(),
            Err(TruthinessError::OpaqueDeferredHttpCredentials)
        );
    }

    #[test]
    fn vm_condition_truthiness_rejects_result_values() {
        let err = Value::result_err("ValidationError", "email required");

        assert_eq!(
            err.truthy_for_condition(),
            Err(TruthinessError::ResultRequiresExplicitOk)
        );
    }

    #[test]
    fn vm_condition_truthiness_accepts_ordinary_values() {
        let mut populated_map = BTreeMap::new();
        populated_map.insert("name".to_string(), Value::String("Ada".to_string()));
        let instance = Value::Instance(Instance::new("Widget", BTreeMap::new()));

        let cases = vec![
            (Value::Nil, false),
            (Value::Bool(false), false),
            (Value::Bool(true), true),
            (Value::Number(0), false),
            (Value::Number(1), true),
            (Value::String(String::new()), false),
            (Value::String("Ada".to_string()), true),
            (Value::Array(ArrayValue::default()), false),
            (Value::Array(ArrayValue::from(vec![Value::Nil])), true),
            (Value::Map(MapValue::default()), false),
            (Value::Map(MapValue::from(populated_map)), true),
            (instance, true),
        ];

        for (value, expected) in cases {
            assert_eq!(value.truthy_for_condition(), Ok(expected));
        }
    }

    #[test]
    fn float_truthiness_treats_zero_and_nan_as_false() {
        let cases = vec![
            (Value::Float(0.0), false),
            (Value::Float(1.25), true),
            (Value::Float(-1.25), true),
            (Value::Float(f64::NAN), false),
            (Value::Float(f64::INFINITY), true),
            (Value::Float(f64::NEG_INFINITY), true),
        ];

        for (value, expected) in cases {
            assert_eq!(value.truthy_for_condition(), Ok(expected));
        }
    }
}
