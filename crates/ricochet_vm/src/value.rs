use ricochet_bytecode::Chunk;
use thiserror::Error;

use crate::capability::Capability;
use crate::collection::{ArrayValue, ListValue, MapValue, SetValue};
use crate::object::Instance;
use crate::regex_value::RegexValue;
use crate::result::{RicochetError, RicochetResult};

#[derive(Debug, Clone, PartialEq)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TruthinessError {
    #[error("result values require an explicit ok? check in conditions")]
    ResultRequiresExplicitOk,
}

impl Value {
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
        }
    }

    pub fn truthy_for_condition(&self) -> Result<bool, TruthinessError> {
        match self {
            Value::Result(_) => Err(TruthinessError::ResultRequiresExplicitOk),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::collection::{ArrayValue, MapValue};
    use crate::object::Instance;

    use super::*;

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
