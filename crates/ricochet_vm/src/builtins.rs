use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Match as RegexMatch;
use ricochet_bytecode::Chunk;
use serde_json::Value as JsonValue;

use super::*;
use crate::capability::Capability;
use crate::regex_value::RegexValue;
use crate::result::{RicochetError, RicochetResult};
use crate::vm::value_kind;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_MAX_RESPONSE_BYTES: usize = 1_048_576;

impl Vm {
    pub(super) fn builtin_method_exists(&self, receiver: &Value, method: &str) -> bool {
        match receiver {
            Value::String(_) => matches!(
                method,
                "length"
                    | "count"
                    | "at"
                    | "first"
                    | "last"
                    | "take"
                    | "skip"
                    | "reverse"
                    | "contains?"
                    | "starts-with?"
                    | "ends-with?"
                    | "blank?"
                    | "trim"
                    | "trim-start"
                    | "trim-end"
                    | "uppercase"
                    | "lowercase"
                    | "slice"
                    | "index-of"
                    | "last-index-of"
                    | "repeat"
                    | "lines"
                    | "chars"
                    | "split"
                    | "replace"
                    | "concat"
                    | "to-number"
            ),
            Value::Array(_) | Value::List(_) => matches!(
                method,
                "count"
                    | "at"
                    | "first"
                    | "last"
                    | "take"
                    | "skip"
                    | "reverse"
                    | "has?"
                    | "push!"
                    | "insert!"
                    | "remove!"
                    | "remove-at!"
                    | "clear!"
                    | "each"
                    | "transform"
                    | "select"
                    | "reduce"
                    | "find"
                    | "any?"
                    | "all?"
                    | "join"
            ),
            Value::Set(_) => matches!(
                method,
                "count"
                    | "first"
                    | "last"
                    | "take"
                    | "skip"
                    | "reverse"
                    | "has?"
                    | "push!"
                    | "remove!"
                    | "clear!"
                    | "each"
                    | "transform"
                    | "select"
                    | "reduce"
                    | "find"
                    | "any?"
                    | "all?"
                    | "join"
            ),
            Value::Map(_) => matches!(
                method,
                "count"
                    | "at"
                    | "has?"
                    | "keys"
                    | "values"
                    | "put!"
                    | "remove!"
                    | "clear!"
                    | "each"
                    | "transform"
                    | "select"
                    | "find"
                    | "any?"
                    | "all?"
            ),
            Value::Result(_) => {
                matches!(method, "error?" | "unwrap-or" | "map-result" | "and-then")
            }
            Value::Task(_) => {
                matches!(
                    method,
                    "id" | "status" | "pending?" | "running?" | "completed?" | "failed?"
                )
            }
            Value::Capability(Capability::FileSystem) => {
                matches!(
                    method,
                    "read-text" | "write-text!" | "exists?" | "list" | "create-dir!"
                )
            }
            Value::Capability(Capability::Http) => {
                matches!(method, "get" | "post-json" | "get-task" | "post-json-task")
            }
            Value::Regex(_) => matches!(method, "matches?" | "find" | "captures" | "replace"),
            _ => false,
        }
    }

    pub(super) fn call_builtin_method(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        let stack_before = self.stack.clone();
        let result = self.call_builtin_method_inner(receiver, method);
        if result.is_err() {
            self.stack = stack_before;
        }
        result
    }

    fn call_builtin_method_inner(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        match method {
            "count" | "length" => self.method_count(receiver, method),
            "at" => self.method_at(receiver, method),
            "first" => self.method_first(receiver, method),
            "last" => self.method_last(receiver, method),
            "take" => self.method_take(receiver, method),
            "skip" => self.method_skip(receiver, method),
            "reverse" => self.method_reverse(receiver, method),
            "has?" | "contains?" => self.method_has(receiver, method),
            "keys" => self.method_keys(receiver, method),
            "values" => self.method_values(receiver, method),
            "push!" => self.method_push(receiver, method),
            "put!" => self.method_put(receiver, method),
            "insert!" => self.method_insert(receiver, method),
            "remove!" => self.method_remove(receiver, method),
            "remove-at!" => self.method_remove_at(receiver, method),
            "clear!" => self.method_clear(receiver, method),
            "each" => self.method_each(receiver, method),
            "transform" => self.method_transform(receiver, method),
            "select" => self.method_select(receiver, method),
            "reduce" => self.method_reduce(receiver, method),
            "find" => match receiver {
                Value::Regex(_) => self.method_regex_find(receiver, method),
                receiver => self.method_find(receiver, method),
            },
            "any?" => self.method_any(receiver, method),
            "all?" => self.method_all(receiver, method),
            "blank?" => self.method_blank(receiver, method),
            "trim" => self.string_unary(receiver, method, |value| value.trim().to_string()),
            "trim-start" => {
                self.string_unary(receiver, method, |value| value.trim_start().to_string())
            }
            "trim-end" => self.string_unary(receiver, method, |value| value.trim_end().to_string()),
            "uppercase" => self.string_unary(receiver, method, |value| value.to_uppercase()),
            "lowercase" => self.string_unary(receiver, method, |value| value.to_lowercase()),
            "slice" => self.method_slice(receiver, method),
            "index-of" => self.method_index_of(receiver, method),
            "last-index-of" => self.method_last_index_of(receiver, method),
            "repeat" => self.method_repeat(receiver, method),
            "lines" => self.method_lines(receiver, method),
            "chars" => self.method_chars(receiver, method),
            "starts-with?" => {
                self.string_predicate(receiver, method, |value, needle| value.starts_with(needle))
            }
            "ends-with?" => {
                self.string_predicate(receiver, method, |value, needle| value.ends_with(needle))
            }
            "split" => self.method_split(receiver, method),
            "join" => self.method_join(receiver, method),
            "replace" => match receiver {
                Value::Regex(_) => self.method_regex_replace(receiver, method),
                receiver => self.method_replace(receiver, method),
            },
            "concat" => self.method_concat(receiver, method),
            "to-number" => self.method_to_number(receiver, method),
            "error?" => self.method_result_error(receiver, method),
            "unwrap-or" => self.method_unwrap_or(receiver, method),
            "map-result" => self.method_map_result(receiver, method),
            "and-then" => self.method_and_then(receiver, method),
            "id" => self.method_task_id(receiver, method),
            "status" => self.method_task_status(receiver, method),
            "pending?" => self.method_task_pending(receiver, method),
            "running?" => self.method_task_running(receiver, method),
            "completed?" => self.method_task_completed(receiver, method),
            "failed?" => self.method_task_failed(receiver, method),
            "read-text" => self.method_fs_read_text(receiver, method),
            "write-text!" => self.method_fs_write_text(receiver, method),
            "exists?" => self.method_fs_exists(receiver, method),
            "list" => self.method_fs_list(receiver, method),
            "create-dir!" => self.method_fs_create_dir(receiver, method),
            "get" => self.method_http_get(receiver, method),
            "post-json" => self.method_http_post_json(receiver, method),
            "get-task" => self.method_http_get_task(receiver, method),
            "post-json-task" => self.method_http_post_json_task(receiver, method),
            "matches?" => self.method_regex_matches(receiver, method),
            "captures" => self.method_regex_captures(receiver, method),
            _ => Err(VmError::UnknownMethod {
                class_name: value_kind(&receiver).to_string(),
                method: method.to_string(),
            }),
        }
    }

    fn method_count(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let count = match receiver {
            Value::String(value) => value.chars().count(),
            Value::Array(value) => value.len(),
            Value::List(value) => value.len(),
            Value::Map(value) => value.len(),
            Value::Set(value) => value.len(),
            value => return Err(method_type_error(method, "countable value", &value)),
        };
        number_from_usize(method, count)
    }

    fn method_at(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => {
                let index = self.pop_index(method)?;
                Ok(value
                    .chars()
                    .nth(index)
                    .map(|value| Value::String(value.to_string()))
                    .unwrap_or(Value::Nil))
            }
            Value::Array(value) => Ok(value.get(self.pop_index(method)?).unwrap_or(Value::Nil)),
            Value::List(value) => Ok(value.get(self.pop_index(method)?).unwrap_or(Value::Nil)),
            Value::Map(value) => {
                let key = self.pop_string(method, "map key string")?;
                Ok(value.get(&key).unwrap_or(Value::Nil))
            }
            value => Err(method_type_error(
                method,
                "string, array, list, or map",
                &value,
            )),
        }
    }

    fn method_first(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(value
                .chars()
                .next()
                .map(|character| Value::String(character.to_string()))
                .unwrap_or(Value::Nil)),
            Value::Array(value) => Ok(value.get(0).unwrap_or(Value::Nil)),
            Value::List(value) => Ok(value.get(0).unwrap_or(Value::Nil)),
            Value::Set(value) => Ok(value.snapshot().first().cloned().unwrap_or(Value::Nil)),
            value => Err(method_type_error(
                method,
                "string, array, list, or set",
                &value,
            )),
        }
    }

    fn method_last(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(value
                .chars()
                .last()
                .map(|character| Value::String(character.to_string()))
                .unwrap_or(Value::Nil)),
            Value::Array(value) => Ok(value.snapshot().last().cloned().unwrap_or(Value::Nil)),
            Value::List(value) => Ok(value.snapshot().last().cloned().unwrap_or(Value::Nil)),
            Value::Set(value) => Ok(value.snapshot().last().cloned().unwrap_or(Value::Nil)),
            value => Err(method_type_error(
                method,
                "string, array, list, or set",
                &value,
            )),
        }
    }

    fn method_take(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let count = self.pop_index(method)?;
        match receiver {
            Value::String(value) => Ok(Value::String(value.chars().take(count).collect())),
            receiver => {
                let values = sequence_snapshot(&receiver, method)?
                    .into_iter()
                    .take(count)
                    .collect();
                collection_from_sequence_receiver(&receiver, values, method)
            }
        }
    }

    fn method_skip(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let count = self.pop_index(method)?;
        match receiver {
            Value::String(value) => Ok(Value::String(value.chars().skip(count).collect())),
            receiver => {
                let values = sequence_snapshot(&receiver, method)?
                    .into_iter()
                    .skip(count)
                    .collect();
                collection_from_sequence_receiver(&receiver, values, method)
            }
        }
    }

    fn method_reverse(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::String(value.chars().rev().collect())),
            receiver => {
                let mut values = sequence_snapshot(&receiver, method)?;
                values.reverse();
                collection_from_sequence_receiver(&receiver, values, method)
            }
        }
    }

    fn method_has(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let needle = self.pop(method)?;
        let result = match receiver {
            Value::String(value) => {
                let Value::String(needle) = needle else {
                    return Err(method_type_error(method, "string needle", &needle));
                };
                value.contains(&needle)
            }
            Value::Array(value) => value.snapshot().contains(&needle),
            Value::List(value) => value.snapshot().contains(&needle),
            Value::Set(value) => value.contains(&needle),
            Value::Map(value) => {
                let Value::String(key) = needle else {
                    return Err(method_type_error(method, "map key string", &needle));
                };
                value.contains_key(&key)
            }
            value => return Err(method_type_error(method, "collection or string", &value)),
        };
        Ok(Value::Bool(result))
    }

    fn method_keys(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Map(value) => Ok(Value::Array(
                value
                    .keys()
                    .into_iter()
                    .map(Value::String)
                    .collect::<Vec<_>>()
                    .into(),
            )),
            value => Err(method_type_error(method, "map", &value)),
        }
    }

    fn method_values(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Map(value) => Ok(Value::Array(value.values().into())),
            value => Err(method_type_error(method, "map", &value)),
        }
    }

    fn method_push(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let value = self.pop(method)?;
        match receiver {
            Value::Array(array) => {
                array.push(value);
                Ok(Value::Array(array))
            }
            Value::List(list) => {
                list.push(value);
                Ok(Value::List(list))
            }
            Value::Set(set) => {
                set.insert(value);
                Ok(Value::Set(set))
            }
            value => Err(method_type_error(method, "array, list, or set", &value)),
        }
    }

    fn method_put(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let value = self.pop(method)?;
        let key = self.pop_string(method, "map key string")?;
        match receiver {
            Value::Map(map) => {
                map.insert(key, value);
                Ok(Value::Map(map))
            }
            value => Err(method_type_error(method, "map", &value)),
        }
    }

    fn method_insert(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let value = self.pop(method)?;
        let index = self.pop_index(method)?;
        let inserted = match &receiver {
            Value::Array(array) => array.insert(index, value),
            Value::List(list) => list.insert(index, value),
            value => return Err(method_type_error(method, "array or list", value)),
        };
        if !inserted {
            return Err(VmError::IndexOutOfBounds {
                word: method.to_string(),
                index,
                length: collection_length(&receiver),
            });
        }
        Ok(receiver)
    }

    fn method_remove(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let target = self.pop(method)?;
        match &receiver {
            Value::Array(array) => {
                let values = array.snapshot();
                if let Some(index) = values.iter().position(|value| value == &target) {
                    array.remove(index);
                }
            }
            Value::List(list) => {
                let values = list.snapshot();
                if let Some(index) = values.iter().position(|value| value == &target) {
                    list.remove(index);
                }
            }
            Value::Set(set) => {
                set.remove(&target);
            }
            Value::Map(map) => {
                let Value::String(key) = target else {
                    return Err(method_type_error(method, "map key string", &target));
                };
                map.remove(&key);
            }
            value => {
                return Err(method_type_error(method, "array, list, set, or map", value));
            }
        }
        Ok(receiver)
    }

    fn method_remove_at(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let index = self.pop_index(method)?;
        let removed = match &receiver {
            Value::Array(array) => array.remove(index),
            Value::List(list) => list.remove(index),
            value => return Err(method_type_error(method, "array or list", value)),
        };
        if removed.is_none() {
            return Err(VmError::IndexOutOfBounds {
                word: method.to_string(),
                index,
                length: collection_length(&receiver),
            });
        }
        Ok(receiver)
    }

    fn method_clear(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match &receiver {
            Value::Array(array) => array.clear(),
            Value::List(list) => list.clear(),
            Value::Set(set) => set.clear(),
            Value::Map(map) => map.clear(),
            value => return Err(method_type_error(method, "collection", value)),
        }
        Ok(receiver)
    }

    fn method_each(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        for arguments in collection_arguments(&receiver, method)? {
            self.call_bytecode_block_with_args(method, &block, arguments)?;
        }
        Ok(receiver)
    }

    fn method_transform(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        let mut output = Vec::new();
        for arguments in collection_arguments(&receiver, method)? {
            output.push(self.call_bytecode_block_with_args(method, &block, arguments)?);
        }
        Ok(Value::Array(output.into()))
    }

    fn method_select(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        match receiver {
            Value::Map(map) => {
                let selected = MapValue::default();
                for (key, value) in map.entries() {
                    let keep = self.call_bytecode_block_with_args(
                        method,
                        &block,
                        vec![Value::String(key.clone()), value.clone()],
                    )?;
                    if condition_value(method, keep)? {
                        selected.insert(key, value);
                    }
                }
                Ok(Value::Map(selected))
            }
            receiver => {
                let values = sequence_snapshot(&receiver, method)?;
                let mut selected = Vec::new();
                for value in values {
                    let keep =
                        self.call_bytecode_block_with_args(method, &block, vec![value.clone()])?;
                    if condition_value(method, keep)? {
                        selected.push(value);
                    }
                }
                Ok(match receiver {
                    Value::Array(_) => Value::Array(selected.into()),
                    Value::List(_) => Value::List(selected.into()),
                    Value::Set(_) => Value::Set(selected.into()),
                    _ => unreachable!("sequence snapshot accepted receiver"),
                })
            }
        }
    }

    fn method_reduce(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        let mut accumulator = self.pop(method)?;
        for value in sequence_snapshot(&receiver, method)? {
            accumulator =
                self.call_bytecode_block_with_args(method, &block, vec![accumulator, value])?;
        }
        Ok(accumulator)
    }

    fn method_find(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        for arguments in collection_arguments(&receiver, method)? {
            let candidate = arguments.last().cloned().unwrap_or(Value::Nil);
            let matched = self.call_bytecode_block_with_args(method, &block, arguments)?;
            if condition_value(method, matched)? {
                return Ok(candidate);
            }
        }
        Ok(Value::Nil)
    }

    fn method_any(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        for arguments in collection_arguments(&receiver, method)? {
            let matched = self.call_bytecode_block_with_args(method, &block, arguments)?;
            if condition_value(method, matched)? {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }

    fn method_all(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        for arguments in collection_arguments(&receiver, method)? {
            let matched = self.call_bytecode_block_with_args(method, &block, arguments)?;
            if !condition_value(method, matched)? {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }

    fn string_unary(
        &self,
        receiver: Value,
        method: &str,
        transform: impl FnOnce(&str) -> String,
    ) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::String(transform(&value))),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn string_predicate(
        &mut self,
        receiver: Value,
        method: &str,
        predicate: impl FnOnce(&str, &str) -> bool,
    ) -> Result<Value, VmError> {
        let needle = self.pop_string(method, "string")?;
        match receiver {
            Value::String(value) => Ok(Value::Bool(predicate(&value, &needle))),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_blank(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::Bool(value.trim().is_empty())),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_slice(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let count = self.pop_index(method)?;
        let start = self.pop_index(method)?;
        match receiver {
            Value::String(value) => Ok(Value::String(
                value.chars().skip(start).take(count).collect(),
            )),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_index_of(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let needle = self.pop_string(method, "needle string")?;
        match receiver {
            Value::String(value) => match value.find(&needle) {
                Some(index) => number_from_usize(method, byte_to_char_index(&value, index)),
                None => Ok(Value::Nil),
            },
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_last_index_of(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let needle = self.pop_string(method, "needle string")?;
        match receiver {
            Value::String(value) => match value.rfind(&needle) {
                Some(index) => number_from_usize(method, byte_to_char_index(&value, index)),
                None => Ok(Value::Nil),
            },
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_repeat(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let count = self.pop_index(method)?;
        match receiver {
            Value::String(value) => {
                value
                    .len()
                    .checked_mul(count)
                    .ok_or_else(|| VmError::ArithmeticOverflow {
                        word: method.to_string(),
                    })?;
                Ok(Value::String(value.repeat(count)))
            }
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_lines(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::Array(
                value
                    .lines()
                    .map(|line| Value::String(line.to_string()))
                    .collect::<Vec<_>>()
                    .into(),
            )),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_chars(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::Array(
                value
                    .chars()
                    .map(|character| Value::String(character.to_string()))
                    .collect::<Vec<_>>()
                    .into(),
            )),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_split(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let delimiter = self.pop_string(method, "delimiter string")?;
        match receiver {
            Value::String(value) => {
                let parts = if delimiter.is_empty() {
                    value
                        .chars()
                        .map(|character| Value::String(character.to_string()))
                        .collect()
                } else {
                    value
                        .split(&delimiter)
                        .map(|part| Value::String(part.to_string()))
                        .collect()
                };
                Ok(Value::Array(ArrayValue::new(parts)))
            }
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_join(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let delimiter = self.pop_string(method, "delimiter string")?;
        let values = sequence_snapshot(&receiver, method)?;
        let strings = values
            .into_iter()
            .map(|value| match value {
                Value::String(value) => Ok(value),
                value => Err(method_type_error(method, "collection of strings", &value)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::String(strings.join(&delimiter)))
    }

    fn method_replace(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let replacement = self.pop_string(method, "replacement string")?;
        let pattern = self.pop_string(method, "pattern string")?;
        match receiver {
            Value::String(value) => Ok(Value::String(value.replace(&pattern, &replacement))),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_concat(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let suffix = self.pop_string(method, "string")?;
        match receiver {
            Value::String(mut value) => {
                value.push_str(&suffix);
                Ok(Value::String(value))
            }
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_to_number(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(match value.parse::<i64>() {
                Ok(value) => Value::result_ok(Value::Number(value)),
                Err(error) => Value::result_err("ParseError", error.to_string()),
            }),
            value => Err(method_type_error(method, "string", &value)),
        }
    }

    fn method_regex_matches(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let haystack = self.pop_string(method, "haystack string")?;
        match receiver {
            Value::Regex(regex) => Ok(Value::Bool(regex.regex().is_match(&haystack))),
            value => Err(method_type_error(method, "regex", &value)),
        }
    }

    fn method_regex_find(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let haystack = self.pop_string(method, "haystack string")?;
        match receiver {
            Value::Regex(regex) => Ok(regex
                .regex()
                .find(&haystack)
                .map(|matched| regex_match_map(method, &haystack, matched))
                .transpose()?
                .unwrap_or(Value::Nil)),
            value => Err(method_type_error(method, "regex", &value)),
        }
    }

    fn method_regex_captures(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let haystack = self.pop_string(method, "haystack string")?;
        match receiver {
            Value::Regex(regex) => {
                let Some(captures) = regex.regex().captures(&haystack) else {
                    return Ok(Value::Nil);
                };
                let mut values = BTreeMap::new();
                for (index, matched) in captures.iter().enumerate() {
                    if let Some(matched) = matched {
                        values.insert(
                            index.to_string(),
                            Value::String(matched.as_str().to_string()),
                        );
                    }
                }
                for name in regex.regex().capture_names().flatten() {
                    if let Some(matched) = captures.name(name) {
                        values.insert(
                            name.to_string(),
                            Value::String(matched.as_str().to_string()),
                        );
                    }
                }
                Ok(Value::Map(values.into()))
            }
            value => Err(method_type_error(method, "regex", &value)),
        }
    }

    fn method_regex_replace(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let replacement = self.pop_string(method, "replacement string")?;
        let haystack = self.pop_string(method, "haystack string")?;
        match receiver {
            Value::Regex(regex) => Ok(Value::String(
                regex
                    .regex()
                    .replace_all(&haystack, replacement.as_str())
                    .into_owned(),
            )),
            value => Err(method_type_error(method, "regex", &value)),
        }
    }

    fn method_result_error(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Result(RicochetResult::Ok(_)) => Ok(Value::Bool(false)),
            Value::Result(RicochetResult::Err(_)) => Ok(Value::Bool(true)),
            value => Err(method_type_error(method, "result", &value)),
        }
    }

    fn method_unwrap_or(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let fallback = self.pop(method)?;
        match receiver {
            Value::Result(RicochetResult::Ok(value)) => Ok(*value),
            Value::Result(RicochetResult::Err(_)) => Ok(fallback),
            value => Err(method_type_error(method, "result", &value)),
        }
    }

    fn method_map_result(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        match receiver {
            Value::Result(RicochetResult::Ok(value)) => Ok(Value::result_ok(
                self.call_bytecode_block_with_args(method, &block, vec![*value])?,
            )),
            Value::Result(RicochetResult::Err(error)) => {
                Ok(Value::Result(RicochetResult::Err(error)))
            }
            value => Err(method_type_error(method, "result", &value)),
        }
    }

    fn method_and_then(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let block = self.pop_block(method)?;
        match receiver {
            Value::Result(RicochetResult::Ok(value)) => {
                let result = self.call_bytecode_block_with_args(method, &block, vec![*value])?;
                if matches!(result, Value::Result(_)) {
                    Ok(result)
                } else {
                    Err(method_type_error(method, "block returning result", &result))
                }
            }
            Value::Result(RicochetResult::Err(error)) => {
                Ok(Value::Result(RicochetResult::Err(error)))
            }
            value => Err(method_type_error(method, "result", &value)),
        }
    }

    fn method_task_id(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => number_from_u64(method, task_id),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    fn method_task_status(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => Ok(Value::String(self.task_status(task_id).to_string())),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    fn method_task_pending(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => Ok(Value::Bool(self.task_pending(task_id))),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    fn method_task_running(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => Ok(Value::Bool(self.task_running(task_id))),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    fn method_task_completed(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => Ok(Value::Bool(self.task_completed(task_id))),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    fn method_task_failed(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => Ok(Value::Bool(self.task_failed(task_id))),
            value => Err(method_type_error(method, "task", &value)),
        }
    }

    pub(super) fn call_tasks(&mut self, word: &str) -> Result<(), VmError> {
        let tasks = self
            .pending_task_ids()
            .into_iter()
            .map(|task_id| task_info_map(word, task_id, self.task_status(task_id)))
            .collect::<Result<Vec<_>, _>>()?;
        self.stack.push(Value::Array(tasks.into()));
        Ok(())
    }

    pub(super) fn call_multiply(&mut self, word: &str) -> Result<(), VmError> {
        self.binary_checked_number(word, i64::checked_mul)
    }

    pub(super) fn call_divide(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_number_or_restore(word, &stack_before)?;
        let left = self.pop_number_or_restore(word, &stack_before)?;
        if right == 0 {
            self.stack = stack_before;
            return Err(VmError::DivisionByZero {
                word: word.to_string(),
            });
        }
        let Some(value) = left.checked_div(right) else {
            self.stack = stack_before;
            return Err(VmError::ArithmeticOverflow {
                word: word.to_string(),
            });
        };
        self.stack.push(Value::Number(value));
        Ok(())
    }

    pub(super) fn call_modulo(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_number_or_restore(word, &stack_before)?;
        let left = self.pop_number_or_restore(word, &stack_before)?;
        if right == 0 {
            self.stack = stack_before;
            return Err(VmError::DivisionByZero {
                word: word.to_string(),
            });
        }
        let Some(value) = left.checked_rem(right) else {
            self.stack = stack_before;
            return Err(VmError::ArithmeticOverflow {
                word: word.to_string(),
            });
        };
        self.stack.push(Value::Number(value));
        Ok(())
    }

    pub(super) fn call_negate(&mut self, word: &str) -> Result<(), VmError> {
        self.unary_checked_number(word, i64::checked_neg)
    }

    pub(super) fn call_abs(&mut self, word: &str) -> Result<(), VmError> {
        self.unary_checked_number(word, i64::checked_abs)
    }

    pub(super) fn call_min(&mut self, word: &str) -> Result<(), VmError> {
        self.binary_number(word, i64::min)
    }

    pub(super) fn call_max(&mut self, word: &str) -> Result<(), VmError> {
        self.binary_number(word, i64::max)
    }

    pub(super) fn call_clamp(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let stack_before = self.stack.clone();
        let maximum = self.pop_number_or_restore(word, &stack_before)?;
        let minimum = self.pop_number_or_restore(word, &stack_before)?;
        let value = self.pop_number_or_restore(word, &stack_before)?;
        if minimum > maximum {
            self.stack = stack_before;
            return Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: "minimum cannot exceed maximum".to_string(),
            });
        }
        self.stack
            .push(Value::Number(value.clamp(minimum, maximum)));
        Ok(())
    }

    pub(super) fn call_not(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        let truthy = value
            .truthy_for_condition()
            .map_err(|_| VmError::UncheckedResultCondition);
        match truthy {
            Ok(value) => {
                self.stack.push(Value::Bool(!value));
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    pub(super) fn call_boolean_binary(
        &mut self,
        word: &str,
        combine: impl FnOnce(bool, bool) -> bool,
    ) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_unchecked();
        let left = self.pop_unchecked();
        let left = left
            .truthy_for_condition()
            .map_err(|_| VmError::UncheckedResultCondition);
        let right = right
            .truthy_for_condition()
            .map_err(|_| VmError::UncheckedResultCondition);
        match (left, right) {
            (Ok(left), Ok(right)) => {
                self.stack.push(Value::Bool(combine(left, right)));
                Ok(())
            }
            (Err(error), _) | (_, Err(error)) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    pub(super) fn call_range(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let end = self.pop_number_or_restore(word, &stack_before)?;
        let start = self.pop_number_or_restore(word, &stack_before)?;
        let values = if start <= end {
            (start..end).map(Value::Number).collect()
        } else {
            ((end + 1)..=start).rev().map(Value::Number).collect()
        };
        self.stack.push(Value::Array(ArrayValue::new(values)));
        Ok(())
    }

    pub(super) fn call_to_string(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stack.push(Value::String(display_value(&value)));
        Ok(())
    }

    pub(super) fn call_json_encode(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value_to_json(&value)
            .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()))
        {
            Ok(json) => {
                self.stack.push(Value::String(json));
                Ok(())
            }
            Err(message) => {
                self.stack = stack_before;
                Err(VmError::InvalidArgument {
                    word: word.to_string(),
                    message,
                })
            }
        }
    }

    pub(super) fn call_json_decode(&mut self, word: &str) -> Result<(), VmError> {
        let json = self.pop_string(word, "JSON string")?;
        let value = match serde_json::from_str::<JsonValue>(&json) {
            Ok(value) => Value::result_ok(json_to_value(value)),
            Err(error) => Value::result_err("JsonError", error.to_string()),
        };
        self.stack.push(value);
        Ok(())
    }

    pub(super) fn call_ok(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stack.push(Value::result_ok(value));
        Ok(())
    }

    pub(super) fn call_fail(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let message = self.pop_string(word, "error message string")?;
        let kind = self.pop_string(word, "error kind string")?;
        self.stack
            .push(Value::Result(RicochetResult::Err(RicochetError {
                kind,
                message,
            })));
        Ok(())
    }

    pub(super) fn call_assert(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value.truthy_for_condition() {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.stack = stack_before;
                Err(VmError::AssertionFailed {
                    expected: "truthy".to_string(),
                    actual: format!("{value:?}"),
                })
            }
            Err(_) => {
                self.stack = stack_before;
                Err(VmError::InvalidArgument {
                    word: word.to_string(),
                    message: "Result values require ok? before assertion".to_string(),
                })
            }
        }
    }

    pub(super) fn call_assert_true(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value {
            Value::Bool(true) => Ok(()),
            Value::Bool(false) => {
                self.stack = stack_before;
                Err(VmError::AssertionFailed {
                    expected: "true".to_string(),
                    actual: "false".to_string(),
                })
            }
            value => {
                self.stack = stack_before;
                Err(method_type_error(word, "bool", &value))
            }
        }
    }

    pub(super) fn call_assert_false(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value {
            Value::Bool(false) => Ok(()),
            Value::Bool(true) => {
                self.stack = stack_before;
                Err(VmError::AssertionFailed {
                    expected: "false".to_string(),
                    actual: "true".to_string(),
                })
            }
            value => {
                self.stack = stack_before;
                Err(method_type_error(word, "bool", &value))
            }
        }
    }

    pub(super) fn call_assert_ok(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value {
            Value::Result(RicochetResult::Ok(_)) => Ok(()),
            Value::Result(RicochetResult::Err(error)) => {
                self.stack = stack_before;
                Err(VmError::AssertionFailed {
                    expected: "ok result".to_string(),
                    actual: format!("{:?}", RicochetResult::Err(error)),
                })
            }
            value => {
                self.stack = stack_before;
                Err(method_type_error(word, "result", &value))
            }
        }
    }

    pub(super) fn call_assert_error(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = self.pop(word)?;
        match value {
            Value::Result(RicochetResult::Err(_)) => Ok(()),
            Value::Result(RicochetResult::Ok(value)) => {
                self.stack = stack_before;
                Err(VmError::AssertionFailed {
                    expected: "error result".to_string(),
                    actual: format!("{:?}", RicochetResult::Ok(value)),
                })
            }
            value => {
                self.stack = stack_before;
                Err(method_type_error(word, "result", &value))
            }
        }
    }

    pub(super) fn call_inspect(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 1)?;
        let value = self
            .stack
            .last()
            .expect("stack length checked before inspect")
            .clone();
        self.stack.push(Value::String(format!("{value:?}")));
        Ok(())
    }

    pub(super) fn call_debug(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 1)?;
        let value = self
            .stack
            .last()
            .expect("stack length checked before debug")
            .clone();
        self.output_lines.push(format!("{value:?}"));
        Ok(())
    }

    pub(super) fn call_regex(&mut self, word: &str) -> Result<(), VmError> {
        let pattern = self.pop_string(word, "regex pattern string")?;
        self.stack.push(match RegexValue::try_new(pattern) {
            Ok(regex) => Value::result_ok(Value::Regex(regex)),
            Err(error) => Value::result_err("RegexError", error.to_string()),
        });
        Ok(())
    }

    pub(super) fn call_nip(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let top = self.pop_unchecked();
        self.pop_unchecked();
        self.stack.push(top);
        Ok(())
    }

    pub(super) fn call_tuck(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let top = self.pop_unchecked();
        let below = self.pop_unchecked();
        self.stack.push(top.clone());
        self.stack.push(below);
        self.stack.push(top);
        Ok(())
    }

    pub(super) fn call_pick(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let index = self.pop_index(word)?;
        if index >= self.stack.len() {
            self.stack = stack_before;
            return Err(VmError::IndexOutOfBounds {
                word: word.to_string(),
                index,
                length: self.stack.len(),
            });
        }
        let value = self.stack[self.stack.len() - 1 - index].clone();
        self.stack.push(value);
        Ok(())
    }

    pub(super) fn call_roll(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let index = self.pop_index(word)?;
        if index >= self.stack.len() {
            self.stack = stack_before;
            return Err(VmError::IndexOutOfBounds {
                word: word.to_string(),
                index,
                length: self.stack.len(),
            });
        }
        let position = self.stack.len() - 1 - index;
        let value = self.stack.remove(position);
        self.stack.push(value);
        Ok(())
    }

    pub(super) fn call_type(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stack
            .push(Value::String(value_kind(&value).to_string()));
        Ok(())
    }

    pub(super) fn call_class_of(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        let class_name = match value {
            Value::Nil => "Nil".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            Value::Number(_) => "Number".to_string(),
            Value::String(_) => "String".to_string(),
            Value::Array(_) => "Array".to_string(),
            Value::List(_) => "List".to_string(),
            Value::Map(_) => "Map".to_string(),
            Value::Set(_) => "Set".to_string(),
            Value::Class(_) => "Class".to_string(),
            Value::Instance(instance) => instance.class_name,
            Value::Member(_) => "Member".to_string(),
            Value::Block(_) => "Block".to_string(),
            Value::Task(_) => "Task".to_string(),
            Value::Result(_) => "Result".to_string(),
            Value::Regex(_) => "Regex".to_string(),
            Value::Capability(_) => "Capability".to_string(),
        };
        self.stack.push(Value::Class(class_name));
        Ok(())
    }

    pub(super) fn call_instance_of(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let class_name = match self.pop_unchecked() {
            Value::Class(value) | Value::String(value) => value,
            value => {
                return Err(method_type_error(
                    word,
                    "class or class name string",
                    &value,
                ))
            }
        };
        let value = self.pop_unchecked();
        let matches = match value {
            Value::Instance(instance) => self
                .inheritance_chain(&instance.class_name)?
                .iter()
                .any(|class| class.name == class_name),
            value => builtin_class_name(&value) == Some(class_name.as_str()),
        };
        self.stack.push(Value::Bool(matches));
        Ok(())
    }

    pub(super) fn call_responds_to(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let receiver = self.pop_unchecked();
        let method = self.pop_string(word, "method name string")?;
        let responds = if self.builtin_method_exists(&receiver, &method) {
            true
        } else {
            match &receiver {
                Value::Class(class_name) => {
                    self.resolve_native_method(class_name, &method)?.is_some()
                }
                Value::Instance(instance) => self
                    .resolve_instance_method(&instance.class_name, &method)?
                    .is_some(),
                _ => false,
            }
        };
        self.stack.push(Value::Bool(responds));
        Ok(())
    }

    pub(super) fn call_fields(&mut self, word: &str) -> Result<(), VmError> {
        let class_name = class_name_from_value(self.pop(word)?, word)?;
        let mut fields = Vec::new();
        for class in self.inheritance_chain(&class_name)?.iter().rev() {
            for field in &class.fields {
                if !fields.contains(field) {
                    fields.push(field.clone());
                }
            }
        }
        self.stack.push(Value::Array(
            fields
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>()
                .into(),
        ));
        Ok(())
    }

    pub(super) fn call_methods(&mut self, word: &str) -> Result<(), VmError> {
        let class_name = class_name_from_value(self.pop(word)?, word)?;
        let mut methods = Vec::new();
        for class in self.inheritance_chain(&class_name)?.iter().rev() {
            methods.extend(class.native_methods.keys().cloned().map(Value::String));
            methods.extend(class.bytecode_methods.keys().cloned().map(Value::String));
        }
        self.stack.push(Value::Set(methods.into()));
        Ok(())
    }

    pub(super) fn call_callable(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stack.push(Value::Bool(matches!(
            value,
            Value::Block(_) | Value::Class(_)
        )));
        Ok(())
    }

    pub(super) fn call_print(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stdout.push_str(&display_value(&value));
        Ok(())
    }

    pub(super) fn call_eprint(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stderr.push_str(&display_value(&value));
        Ok(())
    }

    pub(super) fn call_read_line(&mut self, word: &str) -> Result<(), VmError> {
        let Some(reader) = self.input_reader.clone() else {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "no input reader is installed".to_string(),
            });
        };
        let line = (reader.borrow_mut())().map_err(|message| VmError::HostError {
            word: word.to_string(),
            message,
        })?;
        self.stack
            .push(line.map(Value::String).unwrap_or(Value::Nil));
        Ok(())
    }

    pub(super) fn call_args(&mut self) -> Result<(), VmError> {
        self.stack.push(Value::Array(
            self.program_args
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>()
                .into(),
        ));
        Ok(())
    }

    pub(super) fn call_env(&mut self, word: &str) -> Result<(), VmError> {
        if !self.environment_enabled {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "environment capability is not enabled".to_string(),
            });
        }
        let name = self.pop_string(word, "environment variable name string")?;
        self.stack.push(match std::env::var(&name) {
            Ok(value) => Value::result_ok(Value::String(value)),
            Err(error) => Value::result_err("EnvironmentError", error.to_string()),
        });
        Ok(())
    }

    pub(super) fn call_cwd(&mut self) -> Result<(), VmError> {
        if !self.environment_enabled {
            return Err(VmError::HostError {
                word: "cwd".to_string(),
                message: "environment capability is not enabled".to_string(),
            });
        }
        self.stack.push(match std::env::current_dir() {
            Ok(path) => Value::result_ok(Value::String(path.to_string_lossy().into_owned())),
            Err(error) => Value::result_err("IoError", error.to_string()),
        });
        Ok(())
    }

    pub(super) fn call_now(&mut self, word: &str) -> Result<(), VmError> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| VmError::HostError {
                word: word.to_string(),
                message: error.to_string(),
            })?
            .as_millis();
        let millis = i64::try_from(millis).map_err(|_| VmError::ArithmeticOverflow {
            word: word.to_string(),
        })?;
        self.stack.push(Value::Number(millis));
        Ok(())
    }

    pub(super) fn call_sleep(&mut self, word: &str) -> Result<(), VmError> {
        if !self.sleep_enabled {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "sleep capability is not enabled".to_string(),
            });
        }
        let millis = self.pop_number(word)?;
        let millis = u64::try_from(millis).map_err(|_| VmError::InvalidArgument {
            word: word.to_string(),
            message: "sleep duration cannot be negative".to_string(),
        })?;
        thread::sleep(Duration::from_millis(millis));
        Ok(())
    }

    pub(super) fn call_random(&mut self, word: &str) -> Result<(), VmError> {
        let upper = self.pop_number(word)?;
        if upper <= 0 {
            return Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: "upper bound must be positive".to_string(),
            });
        }
        let upper = u64::try_from(upper).map_err(|_| VmError::ArithmeticOverflow {
            word: word.to_string(),
        })?;
        let value = next_random() % upper;
        self.stack.push(Value::Number(value as i64));
        Ok(())
    }

    pub(super) fn call_exit(&mut self, word: &str) -> Result<(), VmError> {
        let code = self.pop_number(word)?;
        let code = i32::try_from(code).map_err(|_| VmError::InvalidArgument {
            word: word.to_string(),
            message: "exit status must fit a 32-bit integer".to_string(),
        })?;
        Err(VmError::ExitRequested { code })
    }

    fn method_fs_read_text(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
        let path = self.pop_string(method, "path string")?;
        let path = match self.resolve_filesystem_path(method, &path) {
            Ok(path) => path,
            Err(error) => return Ok(Value::result_err("PermissionError", error.to_string())),
        };
        Ok(match fs::read_to_string(&path) {
            Ok(contents) => Value::result_ok(Value::String(contents)),
            Err(error) => Value::result_err("IoError", error.to_string()),
        })
    }

    fn method_fs_write_text(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
        let contents = self.pop_string(method, "file contents string")?;
        let path = self.pop_string(method, "path string")?;
        if !self.filesystem_writes_enabled() {
            return Ok(Value::result_err(
                "PermissionError",
                "filesystem writes are disabled",
            ));
        }
        let path = match self.resolve_filesystem_path(method, &path) {
            Ok(path) => path,
            Err(error) => return Ok(Value::result_err("PermissionError", error.to_string())),
        };
        Ok(match fs::write(&path, contents) {
            Ok(()) => Value::result_ok(Value::String(path.to_string_lossy().into_owned())),
            Err(error) => Value::result_err("IoError", error.to_string()),
        })
    }

    fn method_fs_exists(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
        let path = self.pop_string(method, "path string")?;
        let path = match self.resolve_filesystem_path(method, &path) {
            Ok(path) => path,
            Err(_) => return Ok(Value::Bool(false)),
        };
        Ok(Value::Bool(Path::new(&path).exists()))
    }

    fn method_fs_list(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
        let path = self.pop_string(method, "directory path string")?;
        let path = match self.resolve_filesystem_path(method, &path) {
            Ok(path) => path,
            Err(error) => return Ok(Value::result_err("PermissionError", error.to_string())),
        };
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => return Ok(Value::result_err("IoError", error.to_string())),
        };
        let values = entries
            .map(|entry| {
                entry
                    .map(|entry| Value::String(entry.path().to_string_lossy().into_owned()))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>();
        Ok(match values {
            Ok(values) => Value::result_ok(Value::Array(values.into())),
            Err(error) => Value::result_err("IoError", error),
        })
    }

    fn method_fs_create_dir(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
        let path = self.pop_string(method, "directory path string")?;
        if !self.filesystem_writes_enabled() {
            return Ok(Value::result_err(
                "PermissionError",
                "filesystem writes are disabled",
            ));
        }
        let path = match self.resolve_filesystem_path(method, &path) {
            Ok(path) => path,
            Err(error) => return Ok(Value::result_err("PermissionError", error.to_string())),
        };
        Ok(match fs::create_dir_all(&path) {
            Ok(()) => Value::result_ok(Value::String(path.to_string_lossy().into_owned())),
            Err(error) => Value::result_err("IoError", error.to_string()),
        })
    }

    fn method_http_get(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let url = self.pop_string(method, "URL string")?;
        if let Err(error) = self.check_http_url_allowed(method, &url) {
            return Ok(Value::result_err("PermissionError", error.to_string()));
        }
        Ok(http_in_worker(move || perform_http_get(url)))
    }

    fn method_http_post_json(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let body = self.pop(method)?;
        let url = self.pop_string(method, "URL string")?;
        let body = match value_to_json(&body) {
            Ok(value) => value,
            Err(message) => return Ok(Value::result_err("JsonError", message)),
        };
        if let Err(error) = self.check_http_url_allowed(method, &url) {
            return Ok(Value::result_err("PermissionError", error.to_string()));
        }
        Ok(http_in_worker(move || perform_http_post_json(url, body)))
    }

    fn method_http_get_task(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let url = self.pop_string(method, "URL string")?;
        let permission_error = self
            .check_http_url_allowed(method, &url)
            .err()
            .map(|error| error.to_string());
        self.spawn_value_task(method, move || match permission_error {
            Some(error) => Value::result_err("PermissionError", error),
            None => perform_http_get(url),
        })
    }

    fn method_http_post_json_task(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let body = self.pop(method)?;
        let url = self.pop_string(method, "URL string")?;
        let body = match value_to_json(&body) {
            Ok(value) => value,
            Err(message) => return Ok(Value::result_err("JsonError", message)),
        };
        let permission_error = self
            .check_http_url_allowed(method, &url)
            .err()
            .map(|error| error.to_string());
        self.spawn_value_task(method, move || match permission_error {
            Some(error) => Value::result_err("PermissionError", error),
            None => perform_http_post_json(url, body),
        })
    }

    fn pop_string(&mut self, word: &str, expected: &str) -> Result<String, VmError> {
        match self.pop(word)? {
            Value::String(value) => Ok(value),
            value => Err(method_type_error(word, expected, &value)),
        }
    }

    fn pop_index(&mut self, word: &str) -> Result<usize, VmError> {
        match self.pop(word)? {
            Value::Number(value) if value >= 0 => {
                usize::try_from(value).map_err(|_| VmError::InvalidArgument {
                    word: word.to_string(),
                    message: "index is too large".to_string(),
                })
            }
            Value::Number(value) => Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("index cannot be negative: {value}"),
            }),
            value => Err(method_type_error(word, "non-negative index", &value)),
        }
    }

    fn pop_number_or_restore(
        &mut self,
        word: &str,
        stack_before: &[Value],
    ) -> Result<i64, VmError> {
        match self.pop_number(word) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.stack = stack_before.to_vec();
                Err(error)
            }
        }
    }

    fn pop_block(&mut self, word: &str) -> Result<Chunk, VmError> {
        match self.pop(word)? {
            Value::Block(value) => Ok(value),
            value => Err(method_type_error(word, "block", &value)),
        }
    }

    fn binary_checked_number(
        &mut self,
        word: &str,
        operation: fn(i64, i64) -> Option<i64>,
    ) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_number_or_restore(word, &stack_before)?;
        let left = self.pop_number_or_restore(word, &stack_before)?;
        let Some(value) = operation(left, right) else {
            self.stack = stack_before;
            return Err(VmError::ArithmeticOverflow {
                word: word.to_string(),
            });
        };
        self.stack.push(Value::Number(value));
        Ok(())
    }

    fn unary_checked_number(
        &mut self,
        word: &str,
        operation: fn(i64) -> Option<i64>,
    ) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let input = self.pop_number_or_restore(word, &stack_before)?;
        let Some(value) = operation(input) else {
            self.stack = stack_before;
            return Err(VmError::ArithmeticOverflow {
                word: word.to_string(),
            });
        };
        self.stack.push(Value::Number(value));
        Ok(())
    }

    fn binary_number(&mut self, word: &str, operation: fn(i64, i64) -> i64) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_number_or_restore(word, &stack_before)?;
        let left = self.pop_number_or_restore(word, &stack_before)?;
        self.stack.push(Value::Number(operation(left, right)));
        Ok(())
    }
}

fn collection_arguments(receiver: &Value, method: &str) -> Result<Vec<Vec<Value>>, VmError> {
    Ok(match receiver {
        Value::Array(value) => value
            .snapshot()
            .into_iter()
            .map(|value| vec![value])
            .collect(),
        Value::List(value) => value
            .snapshot()
            .into_iter()
            .map(|value| vec![value])
            .collect(),
        Value::Set(value) => value
            .snapshot()
            .into_iter()
            .map(|value| vec![value])
            .collect(),
        Value::Map(value) => value
            .entries()
            .into_iter()
            .map(|(key, value)| vec![Value::String(key), value])
            .collect(),
        value => return Err(method_type_error(method, "collection", value)),
    })
}

fn sequence_snapshot(receiver: &Value, method: &str) -> Result<Vec<Value>, VmError> {
    match receiver {
        Value::Array(value) => Ok(value.snapshot()),
        Value::List(value) => Ok(value.snapshot()),
        Value::Set(value) => Ok(value.snapshot()),
        value => Err(method_type_error(method, "array, list, or set", value)),
    }
}

fn collection_from_sequence_receiver(
    receiver: &Value,
    values: Vec<Value>,
    method: &str,
) -> Result<Value, VmError> {
    match receiver {
        Value::Array(_) => Ok(Value::Array(values.into())),
        Value::List(_) => Ok(Value::List(values.into())),
        Value::Set(_) => Ok(Value::Set(values.into())),
        value => Err(method_type_error(method, "array, list, or set", value)),
    }
}

fn collection_length(receiver: &Value) -> usize {
    match receiver {
        Value::Array(value) => value.len(),
        Value::List(value) => value.len(),
        Value::Set(value) => value.len(),
        Value::Map(value) => value.len(),
        _ => 0,
    }
}

fn byte_to_char_index(value: &str, byte_index: usize) -> usize {
    value[..byte_index].chars().count()
}

fn regex_match_map(
    method: &str,
    haystack: &str,
    matched: RegexMatch<'_>,
) -> Result<Value, VmError> {
    Ok(Value::Map(
        BTreeMap::from([
            (
                "text".to_string(),
                Value::String(matched.as_str().to_string()),
            ),
            (
                "start".to_string(),
                number_from_usize(method, byte_to_char_index(haystack, matched.start()))?,
            ),
            (
                "end".to_string(),
                number_from_usize(method, byte_to_char_index(haystack, matched.end()))?,
            ),
        ])
        .into(),
    ))
}

fn task_info_map(word: &str, task_id: u64, status: &str) -> Result<Value, VmError> {
    Ok(Value::Map(
        BTreeMap::from([
            ("id".to_string(), number_from_u64(word, task_id)?),
            ("status".to_string(), Value::String(status.to_string())),
        ])
        .into(),
    ))
}

fn condition_value(word: &str, value: Value) -> Result<bool, VmError> {
    value
        .truthy_for_condition()
        .map_err(|_| VmError::InvalidArgument {
            word: word.to_string(),
            message: "Result values require ok? before use as a predicate".to_string(),
        })
}

fn method_type_error(word: &str, expected: &str, value: &Value) -> VmError {
    VmError::TypeError {
        word: word.to_string(),
        expected: expected.to_string(),
        actual: value_kind(value).to_string(),
    }
}

fn number_from_usize(word: &str, value: usize) -> Result<Value, VmError> {
    i64::try_from(value)
        .map(Value::Number)
        .map_err(|_| VmError::ArithmeticOverflow {
            word: word.to_string(),
        })
}

fn number_from_u64(word: &str, value: u64) -> Result<Value, VmError> {
    i64::try_from(value)
        .map(Value::Number)
        .map_err(|_| VmError::ArithmeticOverflow {
            word: word.to_string(),
        })
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Class(value) => value.clone(),
        Value::Regex(value) => format!("/{}/", value.pattern()),
        value => format!("{value:?}"),
    }
}

fn value_to_json(value: &Value) -> Result<JsonValue, String> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => Ok(JsonValue::Number((*value).into())),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::Array(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        Value::List(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        Value::Set(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        Value::Map(value) => value
            .entries()
            .into_iter()
            .map(|(key, value)| Ok((key, value_to_json(&value)?)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(JsonValue::Object),
        value => Err(format!("cannot encode {} as JSON", value_kind(value))),
    }
}

fn json_to_value(value: JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(value) => Value::Bool(value),
        JsonValue::Number(value) => Value::Number(
            value
                .as_i64()
                .unwrap_or_else(|| value.as_u64().unwrap_or(i64::MAX as u64) as i64),
        ),
        JsonValue::String(value) => Value::String(value),
        JsonValue::Array(values) => Value::Array(
            values
                .into_iter()
                .map(json_to_value)
                .collect::<Vec<_>>()
                .into(),
        ),
        JsonValue::Object(values) => Value::Map(
            values
                .into_iter()
                .map(|(key, value)| (key, json_to_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into(),
        ),
    }
}

fn builtin_class_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Nil => Some("Nil"),
        Value::Bool(_) => Some("Bool"),
        Value::Number(_) => Some("Number"),
        Value::String(_) => Some("String"),
        Value::Array(_) => Some("Array"),
        Value::List(_) => Some("List"),
        Value::Map(_) => Some("Map"),
        Value::Set(_) => Some("Set"),
        Value::Class(_) => Some("Class"),
        Value::Member(_) => Some("Member"),
        Value::Block(_) => Some("Block"),
        Value::Task(_) => Some("Task"),
        Value::Result(_) => Some("Result"),
        Value::Regex(_) => Some("Regex"),
        Value::Capability(_) => Some("Capability"),
        Value::Instance(_) => None,
    }
}

fn class_name_from_value(value: Value, word: &str) -> Result<String, VmError> {
    match value {
        Value::Class(class_name) => Ok(class_name),
        Value::Instance(instance) => Ok(instance.class_name),
        value => Err(method_type_error(word, "class or instance", &value)),
    }
}

fn require_capability(value: Value, expected: Capability, word: &str) -> Result<(), VmError> {
    if value == Value::Capability(expected) {
        Ok(())
    } else {
        Err(method_type_error(word, "matching capability", &value))
    }
}

fn http_response(response: Result<reqwest::blocking::Response, reqwest::Error>) -> Value {
    let response = match response {
        Ok(response) => response,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), Value::String(value.to_string())))
        })
        .collect::<BTreeMap<_, _>>();
    let mut body = Vec::new();
    let read_result = response
        .take((HTTP_MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body);
    if body.len() > HTTP_MAX_RESPONSE_BYTES {
        return Value::result_err(
            "HttpBodyTooLarge",
            format!("HTTP response exceeded {HTTP_MAX_RESPONSE_BYTES} bytes"),
        );
    }

    match read_result {
        Ok(_) => Value::result_ok(Value::Map(
            BTreeMap::from([
                ("status".to_string(), Value::Number(status.into())),
                (
                    "body".to_string(),
                    Value::String(String::from_utf8_lossy(&body).into_owned()),
                ),
                ("headers".to_string(), Value::Map(headers.into())),
            ])
            .into(),
        )),
        Err(error) => Value::result_err("HttpError", error.to_string()),
    }
}

fn http_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

fn perform_http_get(url: String) -> Value {
    let client = match http_client() {
        Ok(client) => client,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    http_response(client.get(url).send())
}

fn perform_http_post_json(url: String, body: JsonValue) -> Value {
    let client = match http_client() {
        Ok(client) => client,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    http_response(client.post(url).json(&body).send())
}

fn http_in_worker(request: impl FnOnce() -> Value + Send + 'static) -> Value {
    match thread::spawn(request).join() {
        Ok(value) => value,
        Err(_) => Value::result_err("HttpError", "HTTP worker thread panicked"),
    }
}

fn next_random() -> u64 {
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut current = STATE.load(Ordering::Relaxed);
    if current == 0 {
        current = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
    }
    loop {
        let mut next = current;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match STATE.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}
