use std::collections::BTreeMap;

use crate::result::{RicochetError, RicochetResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Result(RicochetResult),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(v) => *v,
            Value::Number(v) => *v != 0,
            Value::String(v) => !v.is_empty(),
            Value::Array(v) => !v.is_empty(),
            Value::Map(v) => !v.is_empty(),
            Value::Result(_) => true,
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
            (Value::Map(m), "empty?") => Some(Value::Bool(m.is_empty())),
            _ => None,
        }
    }
}
