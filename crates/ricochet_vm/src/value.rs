use std::collections::BTreeMap;

use thiserror::Error;

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
            Value::String(v) => !v.is_empty(),
            Value::Array(v) => !v.is_empty(),
            Value::Map(v) => !v.is_empty(),
            Value::Result(_) => true,
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
            (Value::Map(m), "empty?") => Some(Value::Bool(m.is_empty())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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

        let cases = [
            (Value::Nil, false),
            (Value::Bool(false), false),
            (Value::Bool(true), true),
            (Value::Number(0), false),
            (Value::Number(1), true),
            (Value::String(String::new()), false),
            (Value::String("Ada".to_string()), true),
            (Value::Array(Vec::new()), false),
            (Value::Array(vec![Value::Nil]), true),
            (Value::Map(BTreeMap::new()), false),
            (Value::Map(populated_map), true),
        ];

        for (value, expected) in cases {
            assert_eq!(value.truthy_for_condition(), Ok(expected));
        }
    }
}
