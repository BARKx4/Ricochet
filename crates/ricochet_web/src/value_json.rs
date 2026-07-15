use anyhow::{bail, Result};
use ricochet_vm::Value;
use serde_json::Value as JsonValue;

#[derive(Clone, Copy)]
pub(crate) enum SetMode {
    Array,
    Reject,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonVisit {
    Array(usize),
    List(usize),
    Set(usize),
    Map(usize),
}

pub(crate) fn value_to_json(value: &Value, root: &str, set_mode: SetMode) -> Result<JsonValue> {
    if value.opaque_value_kind().is_some() {
        bail!("cannot encode non-serializable value as JSON at {root}");
    }
    value_to_json_inner(value, root, set_mode, &mut Vec::new())
}

fn value_to_json_inner(
    value: &Value,
    path: &str,
    set_mode: SetMode,
    visits: &mut Vec<JsonVisit>,
) -> Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => Ok(JsonValue::Number((*value).into())),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(JsonValue::Number)
            .ok_or_else(|| anyhow::anyhow!("cannot encode non-finite float as JSON at {path}")),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::Array(values) => with_collection(
            visits,
            JsonVisit::Array(values.identity()),
            path,
            |visits| sequence_to_json(values.snapshot(), path, set_mode, visits),
        ),
        Value::List(values) => {
            with_collection(visits, JsonVisit::List(values.identity()), path, |visits| {
                sequence_to_json(values.snapshot(), path, set_mode, visits)
            })
        }
        Value::Set(values) if matches!(set_mode, SetMode::Array) => {
            with_collection(visits, JsonVisit::Set(values.identity()), path, |visits| {
                sequence_to_json(values.snapshot(), path, set_mode, visits)
            })
        }
        Value::Map(values) => {
            with_collection(visits, JsonVisit::Map(values.identity()), path, |visits| {
                let mut output = serde_json::Map::new();
                for (key, value) in values.entries() {
                    output.insert(
                        key.clone(),
                        value_to_json_inner(&value, &format!("{path}.{key}"), set_mode, visits)?,
                    );
                }
                Ok(JsonValue::Object(output))
            })
        }
        value => bail!("cannot encode {value:?} as JSON at {path}"),
    }
}

fn sequence_to_json(
    values: Vec<Value>,
    path: &str,
    set_mode: SetMode,
    visits: &mut Vec<JsonVisit>,
) -> Result<JsonValue> {
    let mut output = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        output.push(value_to_json_inner(
            value,
            &format!("{path}[{index}]"),
            set_mode,
            visits,
        )?);
    }
    Ok(JsonValue::Array(output))
}

fn with_collection<T>(
    visits: &mut Vec<JsonVisit>,
    visit: JsonVisit,
    path: &str,
    serialize: impl FnOnce(&mut Vec<JsonVisit>) -> Result<T>,
) -> Result<T> {
    if visits.contains(&visit) {
        bail!("cannot encode cyclic collection as JSON at {path}");
    }
    visits.push(visit);
    let result = serialize(visits);
    visits.pop();
    result
}

#[cfg(test)]
mod tests {
    use ricochet_vm::{ArrayValue, MapValue, SetValue};

    use super::*;

    #[test]
    fn shared_acyclic_child_serializes_in_each_branch() {
        let child = ArrayValue::from(vec![Value::Number(7)]);
        let root = MapValue::default();
        root.insert("left".to_string(), Value::Array(child.clone()));
        root.insert("right".to_string(), Value::Array(child));

        let json = value_to_json(&Value::Map(root), "$", SetMode::Array)
            .expect("shared DAG should serialize");

        assert_eq!(json["left"][0], 7);
        assert_eq!(json["right"][0], 7);
    }

    #[test]
    fn set_policy_preserves_controller_and_session_behavior() {
        let set = Value::Set(SetValue::try_from(vec![Value::Number(7)]).expect("ordinary set"));

        assert_eq!(
            value_to_json(&set, "$", SetMode::Array).expect("controller set should encode"),
            serde_json::json!([7])
        );
        assert!(value_to_json(&set, "$", SetMode::Reject).is_err());
    }

    #[test]
    fn web_json_rejects_nested_deferred_credentials_with_sanitized_error() {
        let sentinel = "synthetic-web-json-secret-that-must-not-render";
        let source = format!(
            r#""POST" "https://api.openai.com/v1/responses" http_request_new value "{sentinel}" secret_literal http_bearer_auth value"#
        );
        let chunk = ricochet_compiler::compile_source("web-json-secret-audit.rco", &source)
            .expect("fixture should compile");
        let mut vm = ricochet_vm::Vm::default();
        vm.run_chunk(&chunk).expect("fixture should execute");
        let value = Value::Array(ArrayValue::from(vec![vm.stack()[0].clone()]));

        let error = value_to_json(&value, "$.controller", SetMode::Array)
            .expect_err("Web JSON must reject nested credentials");
        assert_eq!(
            error.to_string(),
            "cannot encode non-serializable value as JSON at $.controller"
        );
        assert!(!error.to_string().contains(sentinel));
    }
}
