use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, FixedOffset, NaiveDate, SecondsFormat,
    TimeZone, Timelike, Utc,
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{self, ClearType},
};
use regex::Match as RegexMatch;
use ricochet_bytecode::Chunk;
use serde_json::Value as JsonValue;
use wait_timeout::ChildExt;

use super::*;
use crate::approval_runtime::{ApprovalCreateRequest, ApprovalRuntimeError, ApprovalSnapshot};
use crate::capability::Capability;
use crate::http_stream_runtime::{
    HttpStreamRead, HttpStreamRequest, HttpStreamRuntimeError, HttpStreamSnapshot,
};
use crate::process_runtime::{ProcessRead, ProcessRequest, ProcessRuntimeError, ProcessSnapshot};
use crate::pty_runtime::{PtyRead, PtyRequest, PtyRuntimeError, PtySnapshot};
use crate::regex_value::RegexValue;
use crate::result::{RicochetError, RicochetResult};
use crate::vm::{
    arithmetic_overflow, display_float, finite_float_result, value_kind, NumericValue,
};

const HTTP_DEFAULT_TIMEOUT_MS: u64 = 10_000;
const HTTP_MAX_TIMEOUT_MS: u64 = 300_000;
const HTTP_DEFAULT_MAX_RESPONSE_BYTES: usize = 1_048_576;
const HTTP_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const PROCESS_DEFAULT_TIMEOUT_MS: u64 = 30_000;
const PROCESS_MAX_TIMEOUT_MS: u64 = 300_000;
const PROCESS_DEFAULT_OUTPUT_MAX_BYTES: usize = 1_048_576;
const PROCESS_MAX_OUTPUT_MAX_BYTES: usize = 16 * 1024 * 1024;
const PTY_DEFAULT_OUTPUT_MAX_BYTES: usize = 1_048_576;
const PTY_MAX_OUTPUT_MAX_BYTES: usize = 16 * 1024 * 1024;
const PTY_DEFAULT_ROWS: u16 = 24;
const PTY_DEFAULT_COLS: u16 = 80;
const WORKSPACE_DEFAULT_MAX_READ_BYTES: usize = 1_048_576;
const WORKSPACE_MAX_READ_BYTES: usize = 16 * 1024 * 1024;
const WORKSPACE_DEFAULT_MAX_LIST_ENTRIES: usize = 1_000;
const WORKSPACE_MAX_LIST_ENTRIES: usize = 10_000;
const APPROVAL_DEFAULT_TTL_MS: i64 = 10 * 60 * 1000;
const APPROVAL_MAX_TTL_MS: i64 = 24 * 60 * 60 * 1000;

#[cfg(windows)]
fn configure_process_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_process_window(_command: &mut Command) {}

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
                    | "starts_with?"
                    | "ends_with?"
                    | "blank?"
                    | "trim"
                    | "trim_start"
                    | "trim_end"
                    | "uppercase"
                    | "lowercase"
                    | "slice"
                    | "index_of"
                    | "last_index_of"
                    | "repeat"
                    | "lines"
                    | "chars"
                    | "split"
                    | "replace"
                    | "concat"
                    | "to_number"
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
                    | "remove_at!"
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
                matches!(method, "error?" | "unwrap_or" | "map_result" | "and_then")
            }
            Value::Task(_) => {
                matches!(
                    method,
                    "id" | "info"
                        | "task_status"
                        | "pending?"
                        | "running?"
                        | "completed?"
                        | "failed?"
                )
            }
            Value::Capability(Capability::FileSystem) => {
                matches!(
                    method,
                    "read_text" | "write_text!" | "exists?" | "list" | "create_dir!" | "delete!"
                )
            }
            Value::Capability(Capability::Http) => {
                matches!(
                    method,
                    "get"
                        | "post_json"
                        | "request"
                        | "get_task"
                        | "post_json_task"
                        | "request_task"
                )
            }
            Value::Capability(Capability::Terminal) => matches!(
                method,
                "enter!"
                    | "leave!"
                    | "clear!"
                    | "move_to!"
                    | "write!"
                    | "flush!"
                    | "size"
                    | "poll_key"
                    | "read_key"
            ),
            Value::Capability(Capability::Webview) => matches!(
                method,
                "text"
                    | "heading"
                    | "button"
                    | "input"
                    | "link"
                    | "container"
                    | "window"
                    | "document"
            ),
            Value::Regex(_) => matches!(
                method,
                "matches?" | "regex_find" | "captures" | "regex_replace"
            ),
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
            "remove_at!" => self.method_remove_at(receiver, method),
            "clear!" => match receiver {
                Value::Capability(Capability::Terminal) => self.method_tui_clear(receiver, method),
                receiver => self.method_clear(receiver, method),
            },
            "each" => self.method_each(receiver, method),
            "transform" => self.method_transform(receiver, method),
            "select" => self.method_select(receiver, method),
            "reduce" => self.method_reduce(receiver, method),
            "find" => self.method_find(receiver, method),
            "regex_find" => self.method_regex_find(receiver, method),
            "any?" => self.method_any(receiver, method),
            "all?" => self.method_all(receiver, method),
            "blank?" => self.method_blank(receiver, method),
            "trim" => self.string_unary(receiver, method, |value| value.trim().to_string()),
            "trim_start" => {
                self.string_unary(receiver, method, |value| value.trim_start().to_string())
            }
            "trim_end" => self.string_unary(receiver, method, |value| value.trim_end().to_string()),
            "uppercase" => self.string_unary(receiver, method, |value| value.to_uppercase()),
            "lowercase" => self.string_unary(receiver, method, |value| value.to_lowercase()),
            "slice" => self.method_slice(receiver, method),
            "index_of" => self.method_index_of(receiver, method),
            "last_index_of" => self.method_last_index_of(receiver, method),
            "repeat" => self.method_repeat(receiver, method),
            "lines" => self.method_lines(receiver, method),
            "chars" => self.method_chars(receiver, method),
            "starts_with?" => {
                self.string_predicate(receiver, method, |value, needle| value.starts_with(needle))
            }
            "ends_with?" => {
                self.string_predicate(receiver, method, |value, needle| value.ends_with(needle))
            }
            "split" => self.method_split(receiver, method),
            "join" => self.method_join(receiver, method),
            "replace" => self.method_replace(receiver, method),
            "regex_replace" => self.method_regex_replace(receiver, method),
            "concat" => self.method_concat(receiver, method),
            "to_number" => self.method_to_number(receiver, method),
            "error?" => self.method_result_error(receiver, method),
            "unwrap_or" => self.method_unwrap_or(receiver, method),
            "map_result" => self.method_map_result(receiver, method),
            "and_then" => self.method_and_then(receiver, method),
            "id" => self.method_task_id(receiver, method),
            "info" => self.method_task_info(receiver, method),
            "task_status" => self.method_task_status(receiver, method),
            "pending?" => self.method_task_pending(receiver, method),
            "running?" => self.method_task_running(receiver, method),
            "completed?" => self.method_task_completed(receiver, method),
            "failed?" => self.method_task_failed(receiver, method),
            "read_text" => self.method_fs_read_text(receiver, method),
            "write_text!" => self.method_fs_write_text(receiver, method),
            "exists?" => self.method_fs_exists(receiver, method),
            "list" => self.method_fs_list(receiver, method),
            "create_dir!" => self.method_fs_create_dir(receiver, method),
            "delete!" => self.method_fs_delete(receiver, method),
            "get" => self.method_http_get(receiver, method),
            "post_json" => self.method_http_post_json(receiver, method),
            "request" => self.method_http_request(receiver, method),
            "get_task" => self.method_http_get_task(receiver, method),
            "post_json_task" => self.method_http_post_json_task(receiver, method),
            "request_task" => self.method_http_request_task(receiver, method),
            "enter!" => self.method_tui_enter(receiver, method),
            "leave!" => self.method_tui_leave(receiver, method),
            "move_to!" => self.method_tui_move_to(receiver, method),
            "write!" => self.method_tui_write(receiver, method),
            "flush!" => self.method_tui_flush(receiver, method),
            "size" => self.method_tui_size(receiver, method),
            "poll_key" => self.method_tui_poll_key(receiver, method),
            "read_key" => self.method_tui_read_key(receiver, method),
            "text" => self.method_webview_text(receiver, method),
            "heading" => self.method_webview_heading(receiver, method),
            "button" => self.method_webview_button(receiver, method),
            "action" => self.method_webview_action(receiver, method),
            "input" => self.method_webview_input(receiver, method),
            "link" => self.method_webview_link(receiver, method),
            "container" => self.method_webview_container(receiver, method),
            "window_state" => self.method_webview_window_state(receiver, method),
            "window" | "document" => self.method_webview_window(receiver, method),
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
            value => Err(method_receiver_type_error(method, "string", &value)),
        }
    }

    fn string_predicate(
        &mut self,
        receiver: Value,
        method: &str,
        predicate: impl FnOnce(&str, &str) -> bool,
    ) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let needle = self.pop_string(method, "string below receiver")?;
        Ok(Value::Bool(predicate(&value, &needle)))
    }

    fn method_blank(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(Value::Bool(value.trim().is_empty())),
            value => Err(method_receiver_type_error(method, "string", &value)),
        }
    }

    fn method_slice(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let count = self.pop_index(method)?;
        let start = self.pop_index(method)?;
        Ok(Value::String(
            value.chars().skip(start).take(count).collect(),
        ))
    }

    fn method_index_of(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let needle = self.pop_string(method, "needle string below receiver")?;
        match value.find(&needle) {
            Some(index) => number_from_usize(method, byte_to_char_index(&value, index)),
            None => Ok(Value::Nil),
        }
    }

    fn method_last_index_of(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let needle = self.pop_string(method, "needle string below receiver")?;
        match value.rfind(&needle) {
            Some(index) => number_from_usize(method, byte_to_char_index(&value, index)),
            None => Ok(Value::Nil),
        }
    }

    fn method_repeat(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let count = self.pop_index(method)?;
        value
            .len()
            .checked_mul(count)
            .ok_or_else(|| VmError::ArithmeticOverflow {
                word: method.to_string(),
            })?;
        Ok(Value::String(value.repeat(count)))
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
            value => Err(method_receiver_type_error(method, "string", &value)),
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
            value => Err(method_receiver_type_error(method, "string", &value)),
        }
    }

    fn method_split(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let delimiter = self.pop_string(method, "delimiter string")?;
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
        let Value::String(value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let replacement = self.pop_string(method, "replacement string")?;
        let pattern = self.pop_string(method, "pattern string")?;
        Ok(Value::String(value.replace(&pattern, &replacement)))
    }

    fn method_concat(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::String(mut value) = receiver else {
            return Err(method_receiver_type_error(method, "string", &receiver));
        };
        let suffix = self.pop_string(method, "string")?;
        value.push_str(&suffix);
        Ok(Value::String(value))
    }

    fn method_to_number(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::String(value) => Ok(match value.parse::<i64>() {
                Ok(value) => Value::result_ok(Value::Number(value)),
                Err(error) => Value::result_err("ParseError", error.to_string()),
            }),
            value => Err(method_receiver_type_error(method, "string", &value)),
        }
    }

    fn method_regex_matches(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::Regex(regex) = receiver else {
            return Err(method_receiver_type_error(method, "regex", &receiver));
        };
        let haystack = self.pop_string(method, "haystack string")?;
        Ok(Value::Bool(regex.regex().is_match(&haystack)))
    }

    fn method_regex_find(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::Regex(regex) = receiver else {
            return Err(method_receiver_type_error(method, "regex", &receiver));
        };
        let haystack = self.pop_string(method, "haystack string")?;
        Ok(regex
            .regex()
            .find(&haystack)
            .map(|matched| regex_match_map(method, &haystack, matched))
            .transpose()?
            .unwrap_or(Value::Nil))
    }

    fn method_regex_captures(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::Regex(regex) = receiver else {
            return Err(method_receiver_type_error(method, "regex", &receiver));
        };
        let haystack = self.pop_string(method, "haystack string")?;
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

    fn method_regex_replace(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        let Value::Regex(regex) = receiver else {
            return Err(method_receiver_type_error(method, "regex", &receiver));
        };
        let replacement = self.pop_string(method, "replacement string")?;
        let haystack = self.pop_string(method, "haystack string")?;
        Ok(Value::String(
            regex
                .regex()
                .replace_all(&haystack, replacement.as_str())
                .into_owned(),
        ))
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

    fn method_task_info(&self, receiver: Value, method: &str) -> Result<Value, VmError> {
        match receiver {
            Value::Task(task_id) => task_info_map(method, task_id, self.task_status(task_id)),
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
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_multiply(word, left, right) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn call_divide(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_divide(word, left, right) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
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
        self.ensure_stack(word, 1)?;
        let stack_before = self.stack.clone();
        let value = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_negate(word, value) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn call_abs(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 1)?;
        let stack_before = self.stack.clone();
        let value = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_abs(word, value) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn call_min(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        self.stack.push(numeric_min(left, right));
        Ok(())
    }

    pub(super) fn call_max(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        self.stack.push(numeric_max(left, right));
        Ok(())
    }

    pub(super) fn call_clamp(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let stack_before = self.stack.clone();
        let maximum = self.pop_numeric_or_restore(word, &stack_before)?;
        let minimum = self.pop_numeric_or_restore(word, &stack_before)?;
        let value = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_clamp(word, value, minimum, maximum) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
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

    pub(super) fn call_numeric_conversion(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        self.stack.push(convert_numeric(word, value));
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

    pub(super) fn call_result_envelope(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let options = match self.pop(word) {
            Ok(options) => options,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = match self.pop(word) {
            Ok(Value::Result(result)) => result,
            Ok(value) => {
                self.stack = stack_before;
                return Err(method_type_error(word, "result", &value));
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let Value::Map(options) = options else {
            self.stack = stack_before;
            return Err(method_type_error(word, "map", &options));
        };
        self.stack
            .push(result_envelope_value(result, options.snapshot()));
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
            Value::Float(_) => "Float".to_string(),
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
        let stack_before = self.stack.clone();
        let name = match self.pop_string(word, "environment variable name string") {
            Ok(name) => name,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if self
            .environment_allowed_names
            .as_ref()
            .is_some_and(|names| !names.contains(&name))
        {
            self.stack = stack_before;
            return Err(VmError::HostError {
                word: word.to_string(),
                message: format!("environment variable is not allowed: {name}"),
            });
        }
        self.stack.push(match std::env::var(&name) {
            Ok(value) => Value::result_ok(Value::String(value)),
            Err(error) => Value::result_err("EnvironmentError", error.to_string()),
        });
        Ok(())
    }

    pub(super) fn call_env_set(&mut self, word: &str) -> Result<(), VmError> {
        if !self.environment_enabled {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "environment capability is not enabled".to_string(),
            });
        }
        let stack_before = self.stack.clone();
        let value = match self.pop_string(word, "environment variable value string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let name = match self.pop_string(word, "environment variable name string") {
            Ok(name) => name,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if self
            .environment_allowed_names
            .as_ref()
            .is_some_and(|names| !names.contains(&name))
        {
            self.stack = stack_before;
            return Err(VmError::HostError {
                word: word.to_string(),
                message: format!("environment variable is not allowed: {name}"),
            });
        }
        if let Some(message) = validate_environment_assignment(&name, &value) {
            self.stack
                .push(Value::result_err("EnvironmentError", message));
            return Ok(());
        }
        std::env::set_var(&name, &value);
        self.stack.push(Value::result_ok(Value::Nil));
        Ok(())
    }

    pub(super) fn call_secret_env(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let name = match self.pop_string(word, "environment variable name string") {
            Ok(name) => name,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if let Some(message) = validate_environment_name(&name) {
            self.stack = stack_before;
            return Err(VmError::InvalidArgument {
                word: word.to_string(),
                message,
            });
        }
        self.stack.push(secret_reference_value("env", "name", name));
        Ok(())
    }

    pub(super) fn call_secret_literal(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = match self.pop_string(word, "secret literal string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        self.stack
            .push(secret_reference_value("literal", "value", value));
        Ok(())
    }

    pub(super) fn call_secret_resolve(&mut self, word: &str) -> Result<(), VmError> {
        let reference = self.pop(word)?;
        let Value::Map(reference) = reference else {
            self.stack.push(Value::result_err(
                "SecretReferenceError",
                format!(
                    "secret reference must be a map, got {}",
                    value_kind(&reference)
                ),
            ));
            return Ok(());
        };
        let reference = reference.snapshot();
        let kind = match secret_reference_string(&reference, "type")
            .or_else(|| secret_reference_string(&reference, "kind"))
        {
            Some(kind) => kind,
            None => {
                self.stack.push(Value::result_err(
                    "SecretReferenceError",
                    "secret reference requires type",
                ));
                return Ok(());
            }
        };
        match kind.as_str() {
            "env" => {
                let Some(name) = secret_reference_string(&reference, "name")
                    .or_else(|| secret_reference_string(&reference, "env"))
                else {
                    self.stack.push(Value::result_err(
                        "SecretReferenceError",
                        "env secret reference requires name",
                    ));
                    return Ok(());
                };
                if let Some(message) = validate_environment_name(&name) {
                    self.stack
                        .push(Value::result_err("SecretReferenceError", message));
                    return Ok(());
                }
                let value = match self.resolve_environment_value(word, &name)? {
                    Ok(value) => Value::result_ok(Value::String(value)),
                    Err(error) => Value::result_err("EnvironmentError", error),
                };
                self.stack.push(value);
            }
            "literal" => {
                let Some(value) = secret_reference_string(&reference, "value") else {
                    self.stack.push(Value::result_err(
                        "SecretReferenceError",
                        "literal secret reference requires value",
                    ));
                    return Ok(());
                };
                self.stack.push(Value::result_ok(Value::String(value)));
            }
            _ => {
                self.stack.push(Value::result_err(
                    "SecretReferenceError",
                    format!("unsupported secret reference type: {kind}"),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn call_config_get(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let path = match self.pop(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let config = match self.pop(word) {
            Ok(Value::Map(value)) => value,
            Ok(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "config map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let path = match config_path_from_value(path) {
            Ok(path) => path,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        self.stack.push(config_get_path(&config, &path));
        Ok(())
    }

    pub(super) fn call_http_request_new(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let url = match self.pop_string(word, "HTTP request URL string") {
            Ok(url) => url,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let method = match self.pop_string(word, "HTTP request method string") {
            Ok(method) => method,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if method.parse::<reqwest::Method>().is_err() {
            self.stack.push(Value::result_err(
                "HttpRequestError",
                "invalid HTTP request method",
            ));
            return Ok(());
        }
        if let Err(error) = reqwest::Url::parse(&url) {
            self.stack.push(Value::result_err(
                "HttpRequestError",
                format!("invalid HTTP request URL: {error}"),
            ));
            return Ok(());
        }
        self.stack.push(Value::result_ok(Value::Map(
            BTreeMap::from([
                ("method".to_string(), Value::String(method)),
                ("url".to_string(), Value::String(url)),
            ])
            .into(),
        )));
        Ok(())
    }

    pub(super) fn call_http_header_put(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = match self.pop_string(word, "HTTP header value string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let name = match self.pop_string(word, "HTTP header name string") {
            Ok(name) => name,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match self.pop_map(word, "HTTP request map") {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        self.stack
            .push(http_request_header_put(request, name, value));
        Ok(())
    }

    pub(super) fn call_http_bearer_auth(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let token = match self.pop_string(word, "bearer token string") {
            Ok(token) => token,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match self.pop_map(word, "HTTP request map") {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if token.is_empty() {
            self.stack.push(Value::result_err(
                "HttpRequestError",
                "bearer token must not be empty",
            ));
            return Ok(());
        }
        self.stack.push(http_request_header_put(
            request,
            "Authorization".to_string(),
            format!("Bearer {token}"),
        ));
        Ok(())
    }

    pub(super) fn call_http_json_body(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let body = match self.pop(word) {
            Ok(body) => body,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match self.pop_map(word, "HTTP request map") {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        request.remove("body");
        request.insert("json".to_string(), body);
        self.stack.push(Value::result_ok(Value::Map(request)));
        Ok(())
    }

    pub(super) fn call_http_timeout(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let timeout_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match self.pop_map(word, "HTTP request map") {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if timeout_ms <= 0 {
            self.stack.push(Value::result_err(
                "HttpRequestError",
                format!("HTTP timeout_ms must be positive, got {timeout_ms}"),
            ));
            return Ok(());
        }
        if timeout_ms > HTTP_MAX_TIMEOUT_MS as i64 {
            self.stack.push(Value::result_err(
                "HttpRequestError",
                format!("HTTP timeout_ms must be at most {HTTP_MAX_TIMEOUT_MS}"),
            ));
            return Ok(());
        }
        request.insert("timeout_ms".to_string(), Value::Number(timeout_ms));
        self.stack.push(Value::result_ok(Value::Map(request)));
        Ok(())
    }

    pub(super) fn call_http_stream_start(&mut self, word: &str) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }

        let stack_before = self.stack.clone();
        let request = match self.pop(word) {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match http_request_from_value(request) {
            Ok(request) => request,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        if let Err(error) = self.check_http_url_allowed(word, &request.url) {
            self.stack
                .push(Value::result_err("PermissionError", error.to_string()));
            return Ok(());
        }
        if let Some(error) = http_request_policy_error(&request) {
            self.stack.push(error);
            return Ok(());
        }
        let stream_request = HttpStreamRequest {
            method: request.method,
            url: request.url,
            headers: request.headers,
            json: request.json,
            body: request.body,
            timeout: request.timeout,
            max_response_bytes: request.max_response_bytes,
        };
        let result = match self.http_stream_registry().start(stream_request) {
            Ok(snapshot) => Value::result_ok(http_stream_snapshot_value(&snapshot)),
            Err(error) => http_stream_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_http_streams(&mut self) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: "http_streams".to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }
        let streams = self
            .http_stream_registry()
            .streams()
            .iter()
            .map(http_stream_snapshot_value)
            .collect::<Vec<_>>();
        self.stack.push(Value::Array(streams.into()));
        Ok(())
    }

    pub(super) fn call_http_stream(&mut self, word: &str) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }
        let id = self.pop_http_stream_id(word)?;
        let result = self
            .http_stream_registry()
            .stream(id)
            .map(|snapshot| Value::result_ok(http_stream_snapshot_value(&snapshot)))
            .unwrap_or_else(|| unknown_http_stream_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_http_stream_cancel(&mut self, word: &str) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }
        let id = self.pop_http_stream_id(word)?;
        let result = self
            .http_stream_registry()
            .cancel(id)
            .map(|snapshot| Value::result_ok(http_stream_snapshot_value(&snapshot)))
            .unwrap_or_else(|| unknown_http_stream_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_http_stream_release(&mut self, word: &str) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }
        let id = self.pop_http_stream_id(word)?;
        let result = match self.http_stream_registry().release(id) {
            Ok(true) => Value::result_ok(Value::Bool(true)),
            Ok(false) => unknown_http_stream_value(id),
            Err(error) => http_stream_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_http_stream_read(&mut self, word: &str) -> Result<(), VmError> {
        if !self.http_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "HTTP capability is not enabled".to_string(),
            });
        }
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let id = match self.pop_http_stream_id(word) {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let offset = match http_stream_read_offset(options) {
            Ok(offset) => offset,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = self
            .http_stream_registry()
            .read(id, offset)
            .map(|read| Value::result_ok(http_stream_read_value(&read)))
            .unwrap_or_else(|| unknown_http_stream_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_process_env_put(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let value = match self.pop_string(word, "process environment variable value string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let name = match self.pop_string(word, "process environment variable name string") {
            Ok(name) => name,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match self.pop_map(word, "process options map") {
            Ok(options) => options,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if let Some(message) = validate_environment_assignment(&name, &value) {
            self.stack
                .push(Value::result_err("ProcessRequestError", message));
            return Ok(());
        }
        self.stack
            .push(process_options_env_put(options, name, value));
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

    pub(super) fn call_timestamp_now(&mut self, word: &str) -> Result<(), VmError> {
        self.call_now(word)
    }

    pub(super) fn call_timestamp_parse(&mut self, word: &str) -> Result<(), VmError> {
        let input = self.pop_string(word, "RFC3339 timestamp string")?;
        let result = match DateTime::<FixedOffset>::parse_from_rfc3339(&input) {
            Ok(value) => {
                Value::result_ok(Value::Number(value.with_timezone(&Utc).timestamp_millis()))
            }
            Err(error) => Value::result_err("DateTimeParseError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_timestamp_format(&mut self, word: &str) -> Result<(), VmError> {
        let timestamp_ms = self.pop_number(word)?;
        self.stack.push(match utc_datetime_value(timestamp_ms) {
            Ok(value) => Value::result_ok(Value::String(format_rfc3339_millis(value))),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_timestamp_format_pattern(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let pattern = match self.pop_string(word, "timestamp format pattern string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let timestamp_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        self.stack.push(match utc_datetime_value(timestamp_ms) {
            Ok(value) => Value::result_ok(Value::String(value.format(&pattern).to_string())),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_timestamp_parts(&mut self, word: &str) -> Result<(), VmError> {
        let timestamp_ms = self.pop_number(word)?;
        self.stack.push(match utc_datetime_value(timestamp_ms) {
            Ok(value) => Value::result_ok(timestamp_parts_value(value)),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_timestamp_from_parts(&mut self, word: &str) -> Result<(), VmError> {
        let parts = self.pop_map(word, "timestamp parts map")?;
        self.stack.push(match timestamp_from_parts_value(&parts) {
            Ok(value) => Value::result_ok(Value::Number(value)),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_timestamp_add(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let duration_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let timestamp_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = timestamp_ms
            .checked_add(duration_ms)
            .ok_or_else(|| Value::result_err("DateTimeRangeError", "timestamp addition overflow"))
            .and_then(|value| {
                utc_datetime_value(value).map(|_| Value::result_ok(Value::Number(value)))
            })
            .unwrap_or_else(|error| error);
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_timestamp_diff(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let end_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let start_ms = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = end_ms
            .checked_sub(start_ms)
            .map(|value| Value::result_ok(Value::Number(value)))
            .unwrap_or_else(|| Value::result_err("DurationError", "timestamp difference overflow"));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_date_from_timestamp(&mut self, word: &str) -> Result<(), VmError> {
        let timestamp_ms = self.pop_number(word)?;
        self.stack.push(match utc_datetime_value(timestamp_ms) {
            Ok(value) => Value::result_ok(date_value(value.date_naive())),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_date_to_timestamp(&mut self, word: &str) -> Result<(), VmError> {
        let date = self.pop_map(word, "date map")?;
        self.stack.push(match date_from_value(&date) {
            Ok(value) => {
                let Some(value) = value.and_hms_milli_opt(0, 0, 0, 0) else {
                    self.stack.push(Value::result_err(
                        "DateTimeRangeError",
                        "date cannot be represented as a UTC timestamp",
                    ));
                    return Ok(());
                };
                Value::result_ok(Value::Number(
                    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc).timestamp_millis(),
                ))
            }
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_date_parse(&mut self, word: &str) -> Result<(), VmError> {
        let input = self.pop_string(word, "date string")?;
        let result = match NaiveDate::parse_from_str(&input, "%Y-%m-%d") {
            Ok(value) => Value::result_ok(date_value(value)),
            Err(error) => Value::result_err("DateParseError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_date_format(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let pattern = match self.pop_string(word, "date format pattern string") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let date = match self.pop_map(word, "date map") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        self.stack.push(match date_from_value(&date) {
            Ok(value) => Value::result_ok(Value::String(value.format(&pattern).to_string())),
            Err(error) => error,
        });
        Ok(())
    }

    pub(super) fn call_date_add_days(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let days = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let date = match self.pop_map(word, "date map") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        self.stack.push(
            match date_from_value(&date).and_then(|value| {
                value
                    .checked_add_signed(ChronoDuration::days(days))
                    .map(date_value)
                    .map(Value::result_ok)
                    .ok_or_else(|| Value::result_err("DateRangeError", "date addition overflow"))
            }) {
                Ok(value) => value,
                Err(error) => error,
            },
        );
        Ok(())
    }

    pub(super) fn call_date_diff_days(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let end = match self.pop_map(word, "end date map") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let start = match self.pop_map(word, "start date map") {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = date_from_value(&start).and_then(|start| {
            date_from_value(&end)
                .map(|end| Value::result_ok(Value::Number((end - start).num_days())))
        });
        self.stack.push(result.unwrap_or_else(|error| error));
        Ok(())
    }

    pub(super) fn call_duration_unit(
        &mut self,
        word: &str,
        multiplier: i64,
    ) -> Result<(), VmError> {
        let value = self.pop_number(word)?;
        let result = value
            .checked_mul(multiplier)
            .map(|value| Value::result_ok(Value::Number(value)))
            .unwrap_or_else(|| Value::result_err("DurationError", "duration overflow"));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_duration_parts(&mut self, word: &str) -> Result<(), VmError> {
        let duration_ms = self.pop_number(word)?;
        self.stack
            .push(Value::result_ok(duration_parts_value(duration_ms)));
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

    pub(super) fn call_runtime_capabilities(&mut self) -> Result<(), VmError> {
        self.stack.push(self.runtime_capabilities_value());
        Ok(())
    }

    pub(super) fn call_process_spawn(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }

        let stack_before = self.stack.clone();
        let request = match self.pop_process_request(word) {
            Ok(Ok(request)) => request,
            Ok(Err(error)) => {
                self.stack.push(error);
                return Ok(());
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        self.stack.push(perform_process_spawn(request));
        Ok(())
    }

    pub(super) fn call_process_spawn_task(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }

        let stack_before = self.stack.clone();
        let request = match self.pop_process_request(word) {
            Ok(Ok(request)) => request,
            Ok(Err(error)) => {
                self.stack.push(error);
                return Ok(());
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        let task = self.spawn_value_task(word, move || perform_process_spawn(request))?;
        self.stack.push(task);
        Ok(())
    }

    pub(super) fn call_process_start(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }

        let stack_before = self.stack.clone();
        let request = match self.pop_process_request(word) {
            Ok(Ok(request)) => request,
            Ok(Err(error)) => {
                self.stack.push(error);
                return Ok(());
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        let result = match self.process_registry().start(request) {
            Ok(snapshot) => Value::result_ok(process_snapshot_value(&snapshot)),
            Err(error) => process_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_process_jobs(&mut self) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: "process_jobs".to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }
        let jobs = self
            .process_registry()
            .jobs()
            .iter()
            .map(process_snapshot_value)
            .collect::<Vec<_>>();
        self.stack.push(Value::Array(jobs.into()));
        Ok(())
    }

    pub(super) fn call_process_job(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }
        let id = self.pop_process_id(word)?;
        let result = self
            .process_registry()
            .job(id)
            .map(|snapshot| Value::result_ok(process_snapshot_value(&snapshot)))
            .unwrap_or_else(|| unknown_process_job_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_process_cancel(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }
        let id = self.pop_process_id(word)?;
        let result = self
            .process_registry()
            .cancel(id)
            .map(|snapshot| Value::result_ok(process_snapshot_value(&snapshot)))
            .unwrap_or_else(|| unknown_process_job_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_process_release(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }
        let id = self.pop_process_id(word)?;
        let result = match self.process_registry().release(id) {
            Ok(true) => Value::result_ok(Value::Bool(true)),
            Ok(false) => unknown_process_job_value(id),
            Err(error) => process_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_process_read(&mut self, word: &str) -> Result<(), VmError> {
        if !self.process_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "process capability is not enabled".to_string(),
            });
        }
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let id = match self.pop_process_id(word) {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let (stdout_offset, stderr_offset) = match process_read_offsets(options) {
            Ok(offsets) => offsets,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = self
            .process_registry()
            .read(id, stdout_offset, stderr_offset)
            .map(|read| Value::result_ok(process_read_value(&read)))
            .unwrap_or_else(|| unknown_process_job_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_start(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }

        let stack_before = self.stack.clone();
        let request = match self.pop_pty_request(word) {
            Ok(Ok(request)) => request,
            Ok(Err(error)) => {
                self.stack.push(error);
                return Ok(());
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        let result = match self.pty_registry().start(request) {
            Ok(snapshot) => Value::result_ok(pty_snapshot_value(&snapshot)),
            Err(error) => pty_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_write(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let input = self.pop_string(word, "PTY input string")?;
        let id = self.pop_pty_id(word)?;
        let result = self
            .pty_registry()
            .write(id, &input)
            .map(|result| match result {
                Ok(snapshot) => Value::result_ok(pty_snapshot_value(&snapshot)),
                Err(error) => pty_runtime_error_value(error),
            })
            .unwrap_or_else(|| unknown_pty_session_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_read(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let id = match self.pop_pty_id(word) {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let offset = match pty_read_offset(options) {
            Ok(offset) => offset,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = self
            .pty_registry()
            .read(id, offset)
            .map(|read| Value::result_ok(pty_read_value(&read)))
            .unwrap_or_else(|| unknown_pty_session_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_resize(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let rows = self.pop_pty_u16(word, "PTY rows")?;
        let cols = self.pop_pty_u16(word, "PTY columns")?;
        let id = self.pop_pty_id(word)?;
        let result = self
            .pty_registry()
            .resize(id, cols, rows)
            .map(|result| match result {
                Ok(snapshot) => Value::result_ok(pty_snapshot_value(&snapshot)),
                Err(error) => pty_runtime_error_value(error),
            })
            .unwrap_or_else(|| unknown_pty_session_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_stop(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let options = self.pop(word)?;
        if let Err(error) = pty_empty_options(options, "PTY stop options") {
            self.stack.push(error);
            return Ok(());
        }
        let id = self.pop_pty_id(word)?;
        let result = self
            .pty_registry()
            .stop(id)
            .map(|result| match result {
                Ok(snapshot) => Value::result_ok(pty_snapshot_value(&snapshot)),
                Err(error) => pty_runtime_error_value(error),
            })
            .unwrap_or_else(|| unknown_pty_session_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_release(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let id = self.pop_pty_id(word)?;
        let result = match self.pty_registry().release(id) {
            Ok(true) => Value::result_ok(Value::Bool(true)),
            Ok(false) => unknown_pty_session_value(id),
            Err(error) => pty_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_pty_list(&mut self) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: "pty_list".to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let sessions = self
            .pty_registry()
            .sessions()
            .iter()
            .map(pty_snapshot_value)
            .collect::<Vec<_>>();
        self.stack.push(Value::Array(sessions.into()));
        Ok(())
    }

    pub(super) fn call_pty_detail(&mut self, word: &str) -> Result<(), VmError> {
        if !self.pty_enabled() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "PTY capability is not enabled".to_string(),
            });
        }
        let id = self.pop_pty_id(word)?;
        let result = self
            .pty_registry()
            .session(id)
            .map(|snapshot| Value::result_ok(pty_snapshot_value(&snapshot)))
            .unwrap_or_else(|| unknown_pty_session_value(id));
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_approval_create(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let options = match self.pop(word) {
            Ok(options) => options,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let operation = match self.pop(word) {
            Ok(operation) => operation,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let request = match approval_create_request(operation, options) {
            Ok(request) => request,
            Err(error) => {
                self.stack = stack_before;
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = match self.approval_registry().create(request) {
            Ok(snapshot) => Value::result_ok(approval_snapshot_value(&snapshot)),
            Err(error) => approval_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_approval_claim(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let token = match self.pop_string(word, "approval token string") {
            Ok(token) => token,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let id = match self.pop_string(word, "approval id string") {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = match self.approval_registry().claim(&id, &token) {
            Ok(snapshot) => Value::result_ok(approval_snapshot_value(&snapshot)),
            Err(error) => approval_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_approval_complete(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let completed_result = match self.pop(word) {
            Ok(result) => result,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let id = match self.pop_string(word, "approval id string") {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = match self.approval_registry().complete(&id, completed_result) {
            Ok(snapshot) => Value::result_ok(approval_snapshot_value(&snapshot)),
            Err(error) => approval_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_approval_reject(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let reason = match self.pop_string(word, "approval rejection reason string") {
            Ok(reason) => reason,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let id = match self.pop_string(word, "approval id string") {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = match self.approval_registry().reject(&id, reason) {
            Ok(snapshot) => Value::result_ok(approval_snapshot_value(&snapshot)),
            Err(error) => approval_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_approval_detail(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let id = match self.pop_string(word, "approval id string") {
            Ok(id) => id,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let result = match self.approval_registry().detail(&id) {
            Ok(snapshot) => Value::result_ok(approval_snapshot_value(&snapshot)),
            Err(error) => approval_runtime_error_value(error),
        };
        self.stack.push(result);
        Ok(())
    }

    fn pop_pty_request(&mut self, word: &str) -> Result<Result<PtyRequest, Value>, VmError> {
        let options = self.pop(word)?;
        let args = self.pop(word)?;
        let command = self.pop_string(word, "PTY command string")?;
        let args = match process_args_from_value(args) {
            Ok(args) => args,
            Err(error) => return Ok(Err(error)),
        };
        Ok(pty_request_from_values(self, word, command, args, options))
    }

    fn pop_pty_id(&mut self, word: &str) -> Result<u64, VmError> {
        match self.pop_number(word)? {
            value if value >= 0 => u64::try_from(value).map_err(|_| VmError::InvalidArgument {
                word: word.to_string(),
                message: "PTY session id is too large".to_string(),
            }),
            value => Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("PTY session id cannot be negative: {value}"),
            }),
        }
    }

    fn pop_pty_u16(&mut self, word: &str, expected: &str) -> Result<u16, VmError> {
        let value = self.pop_number(word)?;
        u16::try_from(value).map_err(|_| VmError::InvalidArgument {
            word: word.to_string(),
            message: format!("{expected} must be between 0 and {}", u16::MAX),
        })
    }

    fn pop_process_request(
        &mut self,
        word: &str,
    ) -> Result<Result<ProcessRequest, Value>, VmError> {
        let options = self.pop(word)?;
        let args = self.pop(word)?;
        let command = self.pop_string(word, "process command string")?;
        let args = match process_args_from_value(args) {
            Ok(args) => args,
            Err(error) => return Ok(Err(error)),
        };
        Ok(process_request_from_values(
            self, word, command, args, options,
        ))
    }

    fn pop_process_id(&mut self, word: &str) -> Result<u64, VmError> {
        match self.pop_number(word)? {
            value if value >= 0 => u64::try_from(value).map_err(|_| VmError::InvalidArgument {
                word: word.to_string(),
                message: "process job id is too large".to_string(),
            }),
            value => Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("process job id cannot be negative: {value}"),
            }),
        }
    }

    fn pop_http_stream_id(&mut self, word: &str) -> Result<u64, VmError> {
        match self.pop_number(word)? {
            value if value >= 0 => u64::try_from(value).map_err(|_| VmError::InvalidArgument {
                word: word.to_string(),
                message: "HTTP stream id is too large".to_string(),
            }),
            value => Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("HTTP stream id cannot be negative: {value}"),
            }),
        }
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

    fn method_fs_delete(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::FileSystem, method)?;
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
        if is_filesystem_root_path(&path, self.filesystem_root_path()) {
            return Ok(Value::result_err(
                "PermissionError",
                "refusing to delete filesystem root",
            ));
        }
        Ok(match delete_filesystem_path(&path, false) {
            Ok(()) => Value::result_ok(Value::String(path.to_string_lossy().into_owned())),
            Err(error) => Value::result_err("IoError", error.to_string()),
        })
    }

    pub(super) fn call_workspace_resolve(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let source = match self.pop_string(word, "workspace path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        if let Err(error) = workspace_options_map(options, &[]) {
            self.stack.push(error);
            return Ok(());
        }
        let result = match self.resolve_filesystem_path(word, &source) {
            Ok(path) => {
                let exists = path.exists();
                Value::result_ok(workspace_resolved_value(
                    &source,
                    &path,
                    self.filesystem_root_path(),
                    exists,
                ))
            }
            Err(error) => Value::result_err("PermissionError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_contains(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let path = self.pop_string(word, "workspace path string")?;
        let root = self.pop_string(word, "workspace root string")?;
        let contains = match (
            self.resolve_filesystem_path(word, &root),
            self.resolve_filesystem_path(word, &path),
        ) {
            (Ok(root), Ok(path)) => path.starts_with(root),
            _ => false,
        };
        self.stack.push(Value::Bool(contains));
        Ok(())
    }

    pub(super) fn call_workspace_metadata(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let source = self.pop_string(word, "workspace path string")?;
        let result = match self.resolve_filesystem_path(word, &source) {
            Ok(path) => workspace_metadata_result(&source, &path, self.filesystem_root_path()),
            Err(error) => Value::result_err("PermissionError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_list(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let source = match self.pop_string(word, "workspace directory path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match workspace_list_options(options) {
            Ok(options) => options,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = match self.resolve_filesystem_path(word, &source) {
            Ok(path) => workspace_list_result(&path, self.filesystem_root_path(), &options),
            Err(error) => Value::result_err("PermissionError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_read_text(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let source = match self.pop_string(word, "workspace file path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let max_bytes = match workspace_read_max_bytes(options) {
            Ok(max_bytes) => max_bytes,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = match self.resolve_filesystem_path(word, &source) {
            Ok(path) => workspace_read_text_result(&path, max_bytes),
            Err(error) => Value::result_err("PermissionError", error.to_string()),
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_write_text(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let contents = match self.pop_string(word, "workspace file contents string") {
            Ok(contents) => contents,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let source = match self.pop_string(word, "workspace file path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match workspace_write_options(options) {
            Ok(options) => options,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = if !self.filesystem_writes_enabled() {
            Value::result_err("PermissionError", "filesystem writes are disabled")
        } else {
            match self.resolve_filesystem_path(word, &source) {
                Ok(path) => workspace_write_text_result(
                    &source,
                    &path,
                    &contents,
                    self.filesystem_root_path(),
                    &options,
                ),
                Err(error) => Value::result_err("PermissionError", error.to_string()),
            }
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_mkdir(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let source = match self.pop_string(word, "workspace directory path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match workspace_mkdir_options(options) {
            Ok(options) => options,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = if !self.filesystem_writes_enabled() {
            Value::result_err("PermissionError", "filesystem writes are disabled")
        } else {
            match self.resolve_filesystem_path(word, &source) {
                Ok(path) => {
                    let created = if options.recursive {
                        fs::create_dir_all(&path)
                    } else {
                        fs::create_dir(&path)
                    };
                    match created {
                        Ok(()) => {
                            workspace_metadata_result(&source, &path, self.filesystem_root_path())
                        }
                        Err(error) => Value::result_err("IoError", error.to_string()),
                    }
                }
                Err(error) => Value::result_err("PermissionError", error.to_string()),
            }
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_delete(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let source = match self.pop_string(word, "workspace path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match workspace_delete_options(options) {
            Ok(options) => options,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = if !self.filesystem_writes_enabled() {
            Value::result_err("PermissionError", "filesystem writes are disabled")
        } else {
            match self.resolve_filesystem_path(word, &source) {
                Ok(path) => {
                    workspace_delete_result(&source, &path, self.filesystem_root_path(), &options)
                }
                Err(error) => Value::result_err("PermissionError", error.to_string()),
            }
        };
        self.stack.push(result);
        Ok(())
    }

    pub(super) fn call_workspace_copy(&mut self, word: &str) -> Result<(), VmError> {
        self.call_workspace_copy_or_move(word, false)
    }

    pub(super) fn call_workspace_move(&mut self, word: &str) -> Result<(), VmError> {
        self.call_workspace_copy_or_move(word, true)
    }

    fn call_workspace_copy_or_move(
        &mut self,
        word: &str,
        move_source: bool,
    ) -> Result<(), VmError> {
        self.ensure_workspace_enabled(word)?;
        let stack_before = self.stack.clone();
        let options = self.pop(word)?;
        let destination_source = match self.pop_string(word, "workspace destination path string") {
            Ok(destination) => destination,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let source = match self.pop_string(word, "workspace source path string") {
            Ok(source) => source,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let options = match workspace_write_options(options) {
            Ok(options) => options,
            Err(error) => {
                self.stack.push(error);
                return Ok(());
            }
        };
        let result = if !self.filesystem_writes_enabled() {
            Value::result_err("PermissionError", "filesystem writes are disabled")
        } else {
            match (
                self.resolve_filesystem_path(word, &source),
                self.resolve_filesystem_path(word, &destination_source),
            ) {
                (Ok(source_path), Ok(destination_path)) => workspace_copy_or_move_result(
                    &destination_source,
                    &source_path,
                    &destination_path,
                    self.filesystem_root_path(),
                    &options,
                    move_source,
                ),
                (Err(error), _) | (_, Err(error)) => {
                    Value::result_err("PermissionError", error.to_string())
                }
            }
        };
        self.stack.push(result);
        Ok(())
    }

    fn ensure_workspace_enabled(&self, word: &str) -> Result<(), VmError> {
        if self.filesystem_enabled() {
            Ok(())
        } else {
            Err(VmError::HostError {
                word: word.to_string(),
                message: "workspace filesystem capability is not enabled".to_string(),
            })
        }
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

    fn method_http_request(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let request = self.pop(method)?;
        let request = match http_request_from_value(request) {
            Ok(request) => request,
            Err(error) => return Ok(error),
        };
        if let Err(error) = self.check_http_url_allowed(method, &request.url) {
            return Ok(Value::result_err("PermissionError", error.to_string()));
        }
        if let Some(error) = http_request_policy_error(&request) {
            return Ok(error);
        }
        Ok(http_in_worker(move || perform_http_request(request)))
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

    fn method_http_request_task(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Http, method)?;
        let request = self.pop(method)?;
        let request = match http_request_from_value(request) {
            Ok(request) => request,
            Err(error) => return Ok(error),
        };
        let permission_error = self
            .check_http_url_allowed(method, &request.url)
            .err()
            .map(|error| Value::result_err("PermissionError", error.to_string()))
            .or_else(|| http_request_policy_error(&request));
        self.spawn_value_task(method, move || match permission_error {
            Some(error) => error,
            None => perform_http_request(request),
        })
    }

    fn method_tui_enter(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        Ok(tui_io_result((|| {
            terminal::enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(
                stdout,
                terminal::EnterAlternateScreen,
                cursor::Hide,
                terminal::Clear(ClearType::All),
                cursor::MoveTo(0, 0)
            )?;
            stdout.flush()
        })()))
    }

    fn method_tui_leave(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        Ok(tui_io_result((|| {
            let mut stdout = io::stdout();
            execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
            stdout.flush()?;
            terminal::disable_raw_mode()
        })()))
    }

    fn method_tui_clear(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        Ok(tui_io_result((|| {
            let mut stdout = io::stdout();
            execute!(
                stdout,
                terminal::Clear(ClearType::All),
                cursor::MoveTo(0, 0)
            )?;
            stdout.flush()
        })()))
    }

    fn method_tui_move_to(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        let row = self.pop_terminal_coordinate(method, "terminal row")?;
        let column = self.pop_terminal_coordinate(method, "terminal column")?;
        Ok(tui_io_result((|| {
            let mut stdout = io::stdout();
            queue!(stdout, cursor::MoveTo(column, row))?;
            Ok(())
        })()))
    }

    fn method_tui_write(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        let text = self.pop_string(method, "terminal text string")?;
        Ok(tui_io_result((|| {
            let mut stdout = io::stdout();
            queue!(stdout, Print(text))?;
            Ok(())
        })()))
    }

    fn method_tui_flush(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        Ok(tui_io_result(io::stdout().flush()))
    }

    fn method_tui_size(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        Ok(match terminal::size() {
            Ok((columns, rows)) => Value::result_ok(Value::Map(
                BTreeMap::from([
                    ("columns".to_string(), Value::Number(i64::from(columns))),
                    ("rows".to_string(), Value::Number(i64::from(rows))),
                ])
                .into(),
            )),
            Err(error) => Value::result_err("TerminalError", error.to_string()),
        })
    }

    fn method_tui_poll_key(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        let timeout = self.pop_number(method)?;
        let timeout = u64::try_from(timeout).map_err(|_| VmError::InvalidArgument {
            word: method.to_string(),
            message: "poll timeout cannot be negative".to_string(),
        })?;
        Ok(match event::poll(Duration::from_millis(timeout)) {
            Ok(false) => Value::result_ok(Value::Nil),
            Ok(true) => match event::read() {
                Ok(Event::Key(key)) => Value::result_ok(terminal_key_event_value(key)),
                Ok(_) => Value::result_ok(Value::Nil),
                Err(error) => Value::result_err("TerminalError", error.to_string()),
            },
            Err(error) => Value::result_err("TerminalError", error.to_string()),
        })
    }

    fn method_tui_read_key(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Terminal, method)?;
        loop {
            match event::read() {
                Ok(Event::Key(key)) => return Ok(Value::result_ok(terminal_key_event_value(key))),
                Ok(_) => continue,
                Err(error) => return Ok(Value::result_err("TerminalError", error.to_string())),
            }
        }
    }

    fn method_webview_text(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let text = self.pop_string(method, "text string")?;
        Ok(Value::String(escape_html_text(&text)))
    }

    fn method_webview_heading(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let level = self.pop_number(method)?;
        let text = self.pop_string(method, "heading text string")?;
        if !(1..=6).contains(&level) {
            return Err(VmError::InvalidArgument {
                word: method.to_string(),
                message: format!("heading level must be between 1 and 6, got {level}"),
            });
        }
        Ok(Value::String(format!(
            "<h{level}>{}</h{level}>",
            escape_html_text(&text)
        )))
    }

    fn method_webview_button(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let action = self.pop_string(method, "action name string")?;
        let label = self.pop_string(method, "button label string")?;
        Ok(Value::String(format!(
            r#"<button type="button" data-rco-action="{}">{}</button>"#,
            escape_html_attribute(&action),
            escape_html_text(&label)
        )))
    }

    fn method_webview_action(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let callback = self.pop_string(method, "callback word string")?;
        let action = self.pop_string(method, "action name string")?;
        let label = self.pop_string(method, "button label string")?;
        Ok(Value::Map(
            BTreeMap::from([
                ("type".to_string(), Value::String("action".to_string())),
                ("label".to_string(), Value::String(label)),
                ("action".to_string(), Value::String(action)),
                ("callback".to_string(), Value::String(callback)),
            ])
            .into(),
        ))
    }

    fn method_webview_input(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let value = self.pop_string(method, "input value string")?;
        let name = self.pop_string(method, "input name string")?;
        Ok(Value::String(format!(
            r#"<input type="text" name="{}" value="{}">"#,
            escape_html_attribute(&name),
            escape_html_attribute(&value)
        )))
    }

    fn method_webview_link(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let href = self.pop_string(method, "link href string")?;
        let label = self.pop_string(method, "link label string")?;
        Ok(Value::String(format!(
            r#"<a href="{}">{}</a>"#,
            escape_html_attribute(&href),
            escape_html_text(&label)
        )))
    }

    fn method_webview_container(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let body = self.pop_string(method, "HTML body string")?;
        Ok(Value::String(format!(
            r#"<div data-rco-container="true">{body}</div>"#
        )))
    }

    fn method_webview_window(&mut self, receiver: Value, method: &str) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let body = self.pop_string(method, "webview body HTML string")?;
        let title = self.pop_string(method, "webview title string")?;
        let state = Value::Map(BTreeMap::new().into());
        let actions = Value::Array(Vec::new().into());
        let html = webview_document_html(&title, &body, &state, &actions)?;
        Ok(Value::result_ok(Value::Map(
            BTreeMap::from([
                ("type".to_string(), Value::String("webview".to_string())),
                ("title".to_string(), Value::String(title)),
                ("body".to_string(), Value::String(body)),
                ("html".to_string(), Value::String(html)),
                ("width".to_string(), Value::Number(800)),
                ("height".to_string(), Value::Number(600)),
                ("state".to_string(), state),
                ("actions".to_string(), actions),
            ])
            .into(),
        )))
    }

    fn method_webview_window_state(
        &mut self,
        receiver: Value,
        method: &str,
    ) -> Result<Value, VmError> {
        require_capability(receiver, Capability::Webview, method)?;
        let actions = self.pop_webview_actions(method)?;
        let state = self.pop_webview_state(method)?;
        let body = self.pop_string(method, "webview body HTML string")?;
        let title = self.pop_string(method, "webview title string")?;
        let html = webview_document_html(&title, &body, &state, &actions)?;
        Ok(Value::result_ok(Value::Map(
            BTreeMap::from([
                ("type".to_string(), Value::String("webview".to_string())),
                ("title".to_string(), Value::String(title)),
                ("body".to_string(), Value::String(body)),
                ("html".to_string(), Value::String(html)),
                ("width".to_string(), Value::Number(800)),
                ("height".to_string(), Value::Number(600)),
                ("state".to_string(), state),
                ("actions".to_string(), actions),
            ])
            .into(),
        )))
    }

    fn pop_webview_state(&mut self, word: &str) -> Result<Value, VmError> {
        match self.pop(word)? {
            state @ Value::Map(_) => Ok(state),
            value => Err(method_type_error(word, "state map", &value)),
        }
    }

    fn pop_webview_actions(&mut self, word: &str) -> Result<Value, VmError> {
        match self.pop(word)? {
            actions @ Value::Array(_) | actions @ Value::List(_) => Ok(actions),
            value => Err(method_type_error(word, "actions array or list", &value)),
        }
    }

    fn pop_string(&mut self, word: &str, expected: &str) -> Result<String, VmError> {
        match self.pop(word)? {
            Value::String(value) => Ok(value),
            value => Err(method_type_error(word, expected, &value)),
        }
    }

    fn pop_map(&mut self, word: &str, expected: &str) -> Result<MapValue, VmError> {
        match self.pop(word)? {
            Value::Map(value) => Ok(value),
            value => Err(method_type_error(word, expected, &value)),
        }
    }

    fn resolve_environment_value(
        &self,
        word: &str,
        name: &str,
    ) -> Result<Result<String, String>, VmError> {
        if !self.environment_enabled {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: "environment capability is not enabled".to_string(),
            });
        }
        if self
            .environment_allowed_names
            .as_ref()
            .is_some_and(|names| !names.contains(name))
        {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: format!("environment variable is not allowed: {name}"),
            });
        }
        Ok(std::env::var(name).map_err(|error| error.to_string()))
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

    fn pop_terminal_coordinate(&mut self, word: &str, expected: &str) -> Result<u16, VmError> {
        match self.pop(word)? {
            Value::Number(value) if value >= 0 => {
                u16::try_from(value).map_err(|_| VmError::InvalidArgument {
                    word: word.to_string(),
                    message: format!("{expected} is too large"),
                })
            }
            Value::Number(value) => Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("{expected} cannot be negative: {value}"),
            }),
            value => Err(method_type_error(word, expected, &value)),
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
            (
                "pending".to_string(),
                Value::Bool(matches!(status, "running")),
            ),
            (
                "running".to_string(),
                Value::Bool(matches!(status, "running")),
            ),
            (
                "completed".to_string(),
                Value::Bool(matches!(status, "completed")),
            ),
            (
                "failed".to_string(),
                Value::Bool(matches!(status, "failed")),
            ),
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

fn method_receiver_type_error(word: &str, expected: &str, value: &Value) -> VmError {
    VmError::TypeError {
        word: word.to_string(),
        expected: format!("receiver {expected}"),
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
        Value::Float(value) => display_float(*value),
        Value::String(value) => value.clone(),
        Value::Class(value) => value.clone(),
        Value::Regex(value) => format!("/{}/", value.pattern()),
        value => format!("{value:?}"),
    }
}

fn convert_numeric(word: &str, value: Value) -> Value {
    match word {
        "to_float" | "to_float64" | "to_double" | "to_real" => {
            conversion_float_result(input_to_float(value))
        }
        "to_float32" => conversion_float_result(input_to_float(value).and_then(|value| {
            let narrowed = value as f32;
            if narrowed.is_finite() {
                Ok(f64::from(narrowed))
            } else {
                Err(("RangeError", format!("{value} is outside float32 range")))
            }
        })),
        "to_number" | "to_integer" | "to_bigint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, i64::MIN, i64::MAX, word)),
        ),
        "to_int" => conversion_integer_result(input_to_integer(value).and_then(|value| {
            checked_integer_range(value, i64::from(i32::MIN), i64::from(i32::MAX), word)
        })),
        "to_mediumint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, -8_388_608, 8_388_607, word)),
        ),
        "to_smallint" => conversion_integer_result(input_to_integer(value).and_then(|value| {
            checked_integer_range(value, i64::from(i16::MIN), i64::from(i16::MAX), word)
        })),
        "to_tinyint" => conversion_integer_result(input_to_integer(value).and_then(|value| {
            checked_integer_range(value, i64::from(i8::MIN), i64::from(i8::MAX), word)
        })),
        "to_bit" => conversion_integer_result(input_to_bit(value)),
        "to_unsigned_int" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, 0, u32::MAX.into(), word)),
        ),
        "to_unsigned_mediumint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, 0, 16_777_215, word)),
        ),
        "to_unsigned_smallint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, 0, u16::MAX.into(), word)),
        ),
        "to_unsigned_tinyint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, 0, u8::MAX.into(), word)),
        ),
        "to_unsigned_bigint" => conversion_integer_result(
            input_to_integer(value)
                .and_then(|value| checked_integer_range(value, 0, i64::MAX, word)),
        ),
        _ => unreachable!("numeric conversion caller restricts words"),
    }
}

fn conversion_integer_result(result: Result<i64, (&'static str, String)>) -> Value {
    match result {
        Ok(value) => Value::result_ok(Value::Number(value)),
        Err((kind, message)) => Value::result_err(kind, message),
    }
}

fn conversion_float_result(result: Result<f64, (&'static str, String)>) -> Value {
    match result {
        Ok(value) => Value::result_ok(Value::Float(value)),
        Err((kind, message)) => Value::result_err(kind, message),
    }
}

fn input_to_integer(value: Value) -> Result<i64, (&'static str, String)> {
    match value {
        Value::Number(value) => Ok(value),
        Value::Float(value) if value.is_finite() && value.fract() == 0.0 => {
            if value < i64::MIN as f64 || value > i64::MAX as f64 {
                Err(("RangeError", format!("{value} is outside integer range")))
            } else {
                Ok(value as i64)
            }
        }
        Value::Float(value) if value.is_finite() => {
            Err(("RangeError", format!("{value} is not an integer")))
        }
        Value::Float(value) => Err(("RangeError", format!("{value} is not finite"))),
        Value::String(value) => value
            .parse::<i64>()
            .map_err(|error| ("ParseError", error.to_string())),
        value => Err((
            "TypeError",
            format!("cannot convert {} to integer", value_kind(&value)),
        )),
    }
}

fn input_to_bit(value: Value) -> Result<i64, (&'static str, String)> {
    match value {
        Value::Bool(value) => Ok(if value { 1 } else { 0 }),
        value => {
            let value = input_to_integer(value)?;
            checked_integer_range(value, 0, 1, "to_bit")
        }
    }
}

fn input_to_float(value: Value) -> Result<f64, (&'static str, String)> {
    match value {
        Value::Number(value) => Ok(value as f64),
        Value::Float(value) if value.is_finite() => Ok(value),
        Value::Float(value) => Err(("RangeError", format!("{value} is not finite"))),
        Value::String(value) => value
            .parse::<f64>()
            .map_err(|error| ("ParseError", error.to_string()))
            .and_then(|value| {
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err(("RangeError", format!("{value} is not finite")))
                }
            }),
        value => Err((
            "TypeError",
            format!("cannot convert {} to float", value_kind(&value)),
        )),
    }
}

fn checked_integer_range(
    value: i64,
    minimum: i64,
    maximum: i64,
    word: &str,
) -> Result<i64, (&'static str, String)> {
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err((
            "RangeError",
            format!("{value} is outside {word} range {minimum}..{maximum}"),
        ))
    }
}

fn numeric_multiply(word: &str, left: NumericValue, right: NumericValue) -> Result<Value, VmError> {
    match (left, right) {
        (NumericValue::Integer(left), NumericValue::Integer(right)) => left
            .checked_mul(right)
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        _ => finite_float_result(word, left.as_f64() * right.as_f64()),
    }
}

fn numeric_divide(word: &str, left: NumericValue, right: NumericValue) -> Result<Value, VmError> {
    match (left, right) {
        (_, NumericValue::Integer(0)) | (_, NumericValue::Float(0.0)) => {
            Err(VmError::DivisionByZero {
                word: word.to_string(),
            })
        }
        (NumericValue::Integer(left), NumericValue::Integer(right)) => left
            .checked_div(right)
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        _ => finite_float_result(word, left.as_f64() / right.as_f64()),
    }
}

fn numeric_negate(word: &str, value: NumericValue) -> Result<Value, VmError> {
    match value {
        NumericValue::Integer(value) => value
            .checked_neg()
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        NumericValue::Float(value) => finite_float_result(word, -value),
    }
}

fn numeric_abs(word: &str, value: NumericValue) -> Result<Value, VmError> {
    match value {
        NumericValue::Integer(value) => value
            .checked_abs()
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        NumericValue::Float(value) => finite_float_result(word, value.abs()),
    }
}

fn numeric_min(left: NumericValue, right: NumericValue) -> Value {
    match (left, right) {
        (NumericValue::Integer(left), NumericValue::Integer(right)) => {
            Value::Number(left.min(right))
        }
        _ => Value::Float(left.as_f64().min(right.as_f64())),
    }
}

fn numeric_max(left: NumericValue, right: NumericValue) -> Value {
    match (left, right) {
        (NumericValue::Integer(left), NumericValue::Integer(right)) => {
            Value::Number(left.max(right))
        }
        _ => Value::Float(left.as_f64().max(right.as_f64())),
    }
}

fn numeric_clamp(
    word: &str,
    value: NumericValue,
    minimum: NumericValue,
    maximum: NumericValue,
) -> Result<Value, VmError> {
    if minimum.as_f64() > maximum.as_f64() {
        return Err(VmError::InvalidArgument {
            word: word.to_string(),
            message: "minimum cannot exceed maximum".to_string(),
        });
    }

    match (value, minimum, maximum) {
        (
            NumericValue::Integer(value),
            NumericValue::Integer(minimum),
            NumericValue::Integer(maximum),
        ) => Ok(Value::Number(value.clamp(minimum, maximum))),
        _ => finite_float_result(
            word,
            value.as_f64().clamp(minimum.as_f64(), maximum.as_f64()),
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonVisit {
    Array(usize),
    List(usize),
    Set(usize),
    Map(usize),
}

fn value_to_json(value: &Value) -> Result<JsonValue, String> {
    let mut visits = Vec::new();
    let mut path = Vec::new();
    value_to_json_inner(value, &mut visits, &mut path)
}

fn value_to_json_inner(
    value: &Value,
    visits: &mut Vec<JsonVisit>,
    path: &mut Vec<String>,
) -> Result<JsonValue, String> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => Ok(JsonValue::Number((*value).into())),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(JsonValue::Number)
            .ok_or_else(|| "cannot encode non-finite float as JSON".to_string()),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::Array(value) => {
            enter_json_collection(visits, JsonVisit::Array(value.identity()), path)?;
            let mut output = Vec::new();
            for (index, value) in value.snapshot().iter().enumerate() {
                path.push(format!("[{index}]"));
                output.push(value_to_json_inner(value, visits, path)?);
                path.pop();
            }
            visits.pop();
            Ok(JsonValue::Array(output))
        }
        Value::List(value) => {
            enter_json_collection(visits, JsonVisit::List(value.identity()), path)?;
            let mut output = Vec::new();
            for (index, value) in value.snapshot().iter().enumerate() {
                path.push(format!("[{index}]"));
                output.push(value_to_json_inner(value, visits, path)?);
                path.pop();
            }
            visits.pop();
            Ok(JsonValue::Array(output))
        }
        Value::Set(value) => {
            enter_json_collection(visits, JsonVisit::Set(value.identity()), path)?;
            let mut output = Vec::new();
            for (index, value) in value.snapshot().iter().enumerate() {
                path.push(format!("[{index}]"));
                output.push(value_to_json_inner(value, visits, path)?);
                path.pop();
            }
            visits.pop();
            Ok(JsonValue::Array(output))
        }
        Value::Map(value) => {
            enter_json_collection(visits, JsonVisit::Map(value.identity()), path)?;
            let mut output = serde_json::Map::new();
            for (key, value) in value.entries() {
                path.push(format!(".{key}"));
                output.insert(key, value_to_json_inner(&value, visits, path)?);
                path.pop();
            }
            visits.pop();
            Ok(JsonValue::Object(output))
        }
        value => Err(format!("cannot encode {} as JSON", value_kind(value))),
    }
}

fn enter_json_collection(
    visits: &mut Vec<JsonVisit>,
    visit: JsonVisit,
    path: &[String],
) -> Result<(), String> {
    if visits.contains(&visit) {
        return Err(format!(
            "cannot encode cyclic collection as JSON at {}",
            json_path(path)
        ));
    }
    visits.push(visit);
    Ok(())
}

fn json_path(path: &[String]) -> String {
    if path.is_empty() {
        "$".to_string()
    } else {
        format!("${}", path.join(""))
    }
}

fn json_to_value(value: JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(value) => Value::Bool(value),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::Number(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                Value::Number(value)
            } else if let Some(value) = value.as_f64() {
                Value::Float(value)
            } else {
                Value::Nil
            }
        }
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

fn result_envelope_value(result: RicochetResult, meta: BTreeMap<String, Value>) -> Value {
    let capability = match meta.get("capability") {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    };
    match result {
        RicochetResult::Ok(value) => Value::Map(
            BTreeMap::from([
                ("ok".to_string(), Value::Bool(true)),
                ("data".to_string(), *value),
                ("error".to_string(), Value::Nil),
                ("meta".to_string(), Value::Map(meta.into())),
            ])
            .into(),
        ),
        RicochetResult::Err(error) => {
            let mut error_map = BTreeMap::from([
                ("kind".to_string(), Value::String(error.kind.clone())),
                ("code".to_string(), Value::String(error.kind)),
                ("message".to_string(), Value::String(error.message)),
            ]);
            if let Some(capability) = capability {
                error_map.insert("capability".to_string(), Value::String(capability));
            }
            Value::Map(
                BTreeMap::from([
                    ("ok".to_string(), Value::Bool(false)),
                    ("data".to_string(), Value::Nil),
                    ("error".to_string(), Value::Map(error_map.into())),
                    ("meta".to_string(), Value::Map(meta.into())),
                ])
                .into(),
            )
        }
    }
}

fn escape_html_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn escape_html_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn webview_document_html(
    title: &str,
    body: &str,
    state: &Value,
    actions: &Value,
) -> Result<String, VmError> {
    let state_json = webview_json_literal("state", state)?;
    let actions_json = webview_json_literal("actions", actions)?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{}</title>
  <style>
    :root {{
      color-scheme: light dark;
      font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }}
    body {{
      margin: 0;
      padding: 24px;
    }}
    button,
    input {{
      font: inherit;
    }}
  </style>
</head>
<body>
{}
<script>
(() => {{
  window.__RICOCHET_STATE__ = {};
  window.__RICOCHET_ACTIONS__ = {};
  document.addEventListener("click", (event) => {{
    const target = event.target.closest("[data-rco-action]");
    if (!target) return;
    const message = {{
      type: "action",
      action: target.getAttribute("data-rco-action"),
      state: window.__RICOCHET_STATE__
    }};
    if (window.ipc && typeof window.ipc.postMessage === "function") {{
      window.ipc.postMessage(JSON.stringify(message));
    }}
  }});
}})();
</script>
</body>
</html>"#,
        escape_html_text(title),
        body,
        state_json,
        actions_json
    ))
}

fn webview_json_literal(name: &str, value: &Value) -> Result<String, VmError> {
    value_to_json(value)
        .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()))
        .map_err(|message| VmError::InvalidArgument {
            word: "webview_window_state".to_string(),
            message: format!("webview {name} cannot be encoded as JSON: {message}"),
        })
}

fn builtin_class_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Nil => Some("Nil"),
        Value::Bool(_) => Some("Bool"),
        Value::Number(_) => Some("Number"),
        Value::Float(_) => Some("Float"),
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

fn tui_io_result(result: io::Result<()>) -> Value {
    match result {
        Ok(()) => Value::result_ok(Value::Nil),
        Err(error) => Value::result_err("TerminalError", error.to_string()),
    }
}

fn terminal_key_event_value(key: KeyEvent) -> Value {
    let (code, character) = terminal_key_code_value(key.code);
    Value::Map(
        BTreeMap::from([
            ("type".to_string(), Value::String("key".to_string())),
            ("code".to_string(), Value::String(code)),
            ("char".to_string(), character),
            (
                "modifiers".to_string(),
                Value::Array(terminal_modifier_values(key.modifiers).into()),
            ),
        ])
        .into(),
    )
}

fn terminal_key_code_value(code: KeyCode) -> (String, Value) {
    match code {
        KeyCode::Backspace => ("backspace".to_string(), Value::Nil),
        KeyCode::Enter => ("enter".to_string(), Value::Nil),
        KeyCode::Left => ("left".to_string(), Value::Nil),
        KeyCode::Right => ("right".to_string(), Value::Nil),
        KeyCode::Up => ("up".to_string(), Value::Nil),
        KeyCode::Down => ("down".to_string(), Value::Nil),
        KeyCode::Home => ("home".to_string(), Value::Nil),
        KeyCode::End => ("end".to_string(), Value::Nil),
        KeyCode::PageUp => ("page-up".to_string(), Value::Nil),
        KeyCode::PageDown => ("page-down".to_string(), Value::Nil),
        KeyCode::Tab => ("tab".to_string(), Value::Nil),
        KeyCode::BackTab => ("back-tab".to_string(), Value::Nil),
        KeyCode::Delete => ("delete".to_string(), Value::Nil),
        KeyCode::Insert => ("insert".to_string(), Value::Nil),
        KeyCode::F(number) => (format!("f{number}"), Value::Nil),
        KeyCode::Char(character) => ("char".to_string(), Value::String(character.to_string())),
        KeyCode::Null => ("null".to_string(), Value::Nil),
        KeyCode::Esc => ("escape".to_string(), Value::Nil),
        KeyCode::CapsLock => ("caps-lock".to_string(), Value::Nil),
        KeyCode::ScrollLock => ("scroll-lock".to_string(), Value::Nil),
        KeyCode::NumLock => ("num-lock".to_string(), Value::Nil),
        KeyCode::PrintScreen => ("print-screen".to_string(), Value::Nil),
        KeyCode::Pause => ("pause".to_string(), Value::Nil),
        KeyCode::Menu => ("menu".to_string(), Value::Nil),
        KeyCode::KeypadBegin => ("keypad-begin".to_string(), Value::Nil),
        KeyCode::Media(key) => (format!("media:{key:?}"), Value::Nil),
        KeyCode::Modifier(key) => (format!("modifier:{key:?}"), Value::Nil),
    }
}

fn terminal_modifier_values(modifiers: KeyModifiers) -> Vec<Value> {
    [
        (KeyModifiers::SHIFT, "shift"),
        (KeyModifiers::CONTROL, "control"),
        (KeyModifiers::ALT, "alt"),
        (KeyModifiers::SUPER, "super"),
        (KeyModifiers::HYPER, "hyper"),
        (KeyModifiers::META, "meta"),
    ]
    .into_iter()
    .filter(|(modifier, _)| modifiers.contains(*modifier))
    .map(|(_, name)| Value::String(name.to_string()))
    .collect()
}

fn http_response(
    response: Result<reqwest::blocking::Response, reqwest::Error>,
    max_response_bytes: usize,
) -> Value {
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
        .take((max_response_bytes + 1) as u64)
        .read_to_end(&mut body);
    if body.len() > max_response_bytes {
        return Value::result_err(
            "HttpBodyTooLarge",
            format!("HTTP response exceeded {max_response_bytes} bytes"),
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

fn http_client(timeout: Duration) -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[derive(Clone)]
struct HttpRequest {
    method: reqwest::Method,
    url: String,
    headers: reqwest::header::HeaderMap,
    json: Option<JsonValue>,
    body: Option<String>,
    timeout: Duration,
    max_response_bytes: usize,
    allowed_hosts: Option<BTreeSet<String>>,
    allowed_schemes: Option<BTreeSet<String>>,
}

fn process_args_from_value(value: Value) -> Result<Vec<String>, Value> {
    let values = match value {
        Value::Array(values) => values.snapshot(),
        Value::List(values) => values.snapshot(),
        value => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process args must be an array or list, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::String(value) => Ok(value),
            value => Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process args[{index}] must be a string, got {}",
                    value_kind(&value)
                ),
            )),
        })
        .collect()
}

fn process_request_from_values(
    vm: &Vm,
    word: &str,
    command: String,
    args: Vec<String>,
    options: Value,
) -> Result<ProcessRequest, Value> {
    if command.trim().is_empty() {
        return Err(Value::result_err(
            "ProcessRequestError",
            "process command must not be empty",
        ));
    }

    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "ProcessRequestError",
            format!(
                "process options must be a map, got {}",
                value_kind(&options)
            ),
        ));
    };
    let mut options = options.snapshot();

    let cwd = match options.remove("cwd") {
        Some(Value::String(path)) => Some(
            vm.resolve_process_path(word, &path)
                .map_err(|error| Value::result_err("PermissionError", error.to_string()))?,
        ),
        Some(Value::Nil) | None => None,
        Some(value) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option cwd must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let stdin = match options.remove("stdin") {
        Some(Value::String(value)) => Some(value),
        Some(Value::Nil) | None => None,
        Some(value) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option stdin must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let timeout_ms = match options.remove("timeout_ms") {
        Some(Value::Number(value)) if value > 0 => u64::try_from(value).map_err(|_| {
            Value::result_err("ProcessRequestError", "process timeout_ms is too large")
        })?,
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!("process timeout_ms must be positive, got {value}"),
            ));
        }
        Some(Value::Nil) | None => PROCESS_DEFAULT_TIMEOUT_MS,
        Some(value) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option timeout_ms must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if timeout_ms > PROCESS_MAX_TIMEOUT_MS {
        return Err(Value::result_err(
            "ProcessRequestError",
            format!("process timeout_ms must be at most {PROCESS_MAX_TIMEOUT_MS}"),
        ));
    }

    let clear_env = match options.remove("clear_env") {
        Some(Value::Bool(value)) => value,
        Some(Value::Nil) | None => false,
        Some(value) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option clear_env must be a bool, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let env = match options.remove("env") {
        Some(Value::Map(values)) => process_env_from_map(values.snapshot())?,
        Some(Value::Nil) | None => BTreeMap::new(),
        Some(value) => {
            return Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option env must be a map, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let stdout_max_bytes = match options.remove("stdout_max_bytes") {
        Some(value) => process_max_bytes_from_value("stdout_max_bytes", value)?,
        None => PROCESS_DEFAULT_OUTPUT_MAX_BYTES,
    };
    let stderr_max_bytes = match options.remove("stderr_max_bytes") {
        Some(value) => process_max_bytes_from_value("stderr_max_bytes", value)?,
        None => PROCESS_DEFAULT_OUTPUT_MAX_BYTES,
    };

    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "ProcessRequestError",
            format!("unknown process option: {key}"),
        ));
    }

    Ok(ProcessRequest {
        command,
        args,
        cwd,
        stdin,
        timeout: Duration::from_millis(timeout_ms),
        clear_env,
        env,
        stdout_max_bytes,
        stderr_max_bytes,
    })
}

fn process_max_bytes_from_value(name: &str, value: Value) -> Result<usize, Value> {
    match value {
        Value::Number(value) if value >= 0 => {
            let value = usize::try_from(value).map_err(|_| {
                Value::result_err(
                    "ProcessRequestError",
                    format!("process option {name} is too large"),
                )
            })?;
            if value > PROCESS_MAX_OUTPUT_MAX_BYTES {
                Err(Value::result_err(
                    "ProcessRequestError",
                    format!("process option {name} must be at most {PROCESS_MAX_OUTPUT_MAX_BYTES}"),
                ))
            } else {
                Ok(value)
            }
        }
        Value::Number(value) => Err(Value::result_err(
            "ProcessRequestError",
            format!("process option {name} cannot be negative: {value}"),
        )),
        Value::Nil => Ok(PROCESS_DEFAULT_OUTPUT_MAX_BYTES),
        value => Err(Value::result_err(
            "ProcessRequestError",
            format!(
                "process option {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn pty_request_from_values(
    vm: &Vm,
    word: &str,
    command: String,
    args: Vec<String>,
    options: Value,
) -> Result<PtyRequest, Value> {
    if command.trim().is_empty() {
        return Err(Value::result_err(
            "PtyRequestError",
            "PTY command must not be empty",
        ));
    }

    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "PtyRequestError",
            format!("PTY options must be a map, got {}", value_kind(&options)),
        ));
    };
    let mut options = options.snapshot();

    let cwd = match options.remove("cwd") {
        Some(Value::String(path)) => Some(
            vm.resolve_process_path(word, &path)
                .map_err(|error| Value::result_err("PermissionError", error.to_string()))?,
        ),
        Some(Value::Nil) | None => None,
        Some(value) => {
            return Err(Value::result_err(
                "PtyRequestError",
                format!(
                    "PTY option cwd must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let clear_env = match options.remove("clear_env") {
        Some(Value::Bool(value)) => value,
        Some(Value::Nil) | None => false,
        Some(value) => {
            return Err(Value::result_err(
                "PtyRequestError",
                format!(
                    "PTY option clear_env must be a bool, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };

    let env = match options.remove("env") {
        Some(Value::Map(values)) => process_env_from_map(values.snapshot())?,
        Some(Value::Nil) | None => BTreeMap::new(),
        Some(value) => {
            return Err(Value::result_err(
                "PtyRequestError",
                format!("PTY option env must be a map, got {}", value_kind(&value)),
            ));
        }
    };

    let rows = match options.remove("rows") {
        Some(value) => pty_u16_from_value("rows", value)?,
        None => PTY_DEFAULT_ROWS,
    };
    let cols = match options.remove("cols") {
        Some(value) => pty_u16_from_value("cols", value)?,
        None => PTY_DEFAULT_COLS,
    };
    let output_max_bytes = match options.remove("output_max_bytes") {
        Some(value) => pty_output_max_bytes_from_value("output_max_bytes", value)?,
        None => PTY_DEFAULT_OUTPUT_MAX_BYTES,
    };

    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "PtyRequestError",
            format!("unknown PTY option: {key}"),
        ));
    }

    Ok(PtyRequest {
        command,
        args,
        cwd,
        clear_env,
        env,
        rows,
        cols,
        output_max_bytes,
    })
}

fn pty_u16_from_value(name: &str, value: Value) -> Result<u16, Value> {
    match value {
        Value::Number(value) if value > 0 => u16::try_from(value).map_err(|_| {
            Value::result_err(
                "PtyRequestError",
                format!("PTY option {name} must be at most {}", u16::MAX),
            )
        }),
        Value::Number(value) => Err(Value::result_err(
            "PtyRequestError",
            format!("PTY option {name} must be positive, got {value}"),
        )),
        Value::Nil => Ok(if name == "rows" {
            PTY_DEFAULT_ROWS
        } else {
            PTY_DEFAULT_COLS
        }),
        value => Err(Value::result_err(
            "PtyRequestError",
            format!(
                "PTY option {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn pty_output_max_bytes_from_value(name: &str, value: Value) -> Result<usize, Value> {
    match value {
        Value::Number(value) if value >= 0 => {
            let value = usize::try_from(value).map_err(|_| {
                Value::result_err("PtyRequestError", format!("PTY option {name} is too large"))
            })?;
            if value > PTY_MAX_OUTPUT_MAX_BYTES {
                Err(Value::result_err(
                    "PtyRequestError",
                    format!("PTY option {name} must be at most {PTY_MAX_OUTPUT_MAX_BYTES}"),
                ))
            } else {
                Ok(value)
            }
        }
        Value::Number(value) => Err(Value::result_err(
            "PtyRequestError",
            format!("PTY option {name} cannot be negative: {value}"),
        )),
        Value::Nil => Ok(PTY_DEFAULT_OUTPUT_MAX_BYTES),
        value => Err(Value::result_err(
            "PtyRequestError",
            format!(
                "PTY option {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn pty_read_offset(options: Value) -> Result<usize, Value> {
    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "PtyRequestError",
            format!(
                "PTY read options must be a map, got {}",
                value_kind(&options)
            ),
        ));
    };
    let mut options = options.snapshot();
    let offset = match options.remove("offset") {
        Some(Value::Number(value)) if value >= 0 => usize::try_from(value).map_err(|_| {
            Value::result_err("PtyRequestError", "PTY read option offset is too large")
        })?,
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "PtyRequestError",
                format!("PTY read option offset cannot be negative: {value}"),
            ));
        }
        Some(Value::Nil) | None => 0,
        Some(value) => {
            return Err(Value::result_err(
                "PtyRequestError",
                format!(
                    "PTY read option offset must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "PtyRequestError",
            format!("unknown PTY read option: {key}"),
        ));
    }
    Ok(offset)
}

fn http_stream_read_offset(options: Value) -> Result<usize, Value> {
    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "HttpStreamRequestError",
            format!(
                "HTTP stream read options must be a map, got {}",
                value_kind(&options)
            ),
        ));
    };
    let mut options = options.snapshot();
    let offset = match options.remove("offset") {
        Some(Value::Number(value)) if value >= 0 => usize::try_from(value).map_err(|_| {
            Value::result_err(
                "HttpStreamRequestError",
                "HTTP stream read option offset is too large",
            )
        })?,
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "HttpStreamRequestError",
                format!("HTTP stream read option offset cannot be negative: {value}"),
            ));
        }
        Some(Value::Nil) | None => 0,
        Some(value) => {
            return Err(Value::result_err(
                "HttpStreamRequestError",
                format!(
                    "HTTP stream read option offset must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "HttpStreamRequestError",
            format!("unknown HTTP stream read option: {key}"),
        ));
    }
    Ok(offset)
}

fn pty_empty_options(options: Value, expected: &str) -> Result<(), Value> {
    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "PtyRequestError",
            format!("{expected} must be a map, got {}", value_kind(&options)),
        ));
    };
    let options = options.snapshot();
    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "PtyRequestError",
            format!("unknown PTY option: {key}"),
        ));
    }
    Ok(())
}

fn approval_create_request(
    operation: Value,
    options: Value,
) -> Result<ApprovalCreateRequest, Value> {
    if !matches!(operation, Value::Map(_)) {
        return Err(Value::result_err(
            "ApprovalRequestError",
            format!(
                "approval operation must be a map, got {}",
                value_kind(&operation)
            ),
        ));
    }
    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "ApprovalRequestError",
            format!(
                "approval options must be a map, got {}",
                value_kind(&options)
            ),
        ));
    };
    let mut options = options.snapshot();
    let id = approval_optional_string(&mut options, "id")?;
    let token = approval_optional_string(&mut options, "token")?;
    let metadata = options.remove("metadata").unwrap_or(Value::Nil);
    let ttl_ms = match options.remove("ttl_ms") {
        Some(Value::Number(value)) if value > 0 && value <= APPROVAL_MAX_TTL_MS => Some(value),
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "ApprovalRequestError",
                format!("approval ttl_ms must be between 1 and {APPROVAL_MAX_TTL_MS}, got {value}"),
            ));
        }
        Some(Value::Nil) | None => None,
        Some(value) => {
            return Err(Value::result_err(
                "ApprovalRequestError",
                format!(
                    "approval ttl_ms must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    let expires_at_ms = match options.remove("expires_at_ms") {
        Some(Value::Number(value)) if value > 0 => Some(value),
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "ApprovalRequestError",
                format!("approval expires_at_ms must be positive, got {value}"),
            ));
        }
        Some(Value::Nil) | None => None,
        Some(value) => {
            return Err(Value::result_err(
                "ApprovalRequestError",
                format!(
                    "approval expires_at_ms must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "ApprovalRequestError",
            format!("unknown approval option: {key}"),
        ));
    }
    Ok(ApprovalCreateRequest {
        id,
        token,
        operation,
        metadata,
        ttl_ms: Some(ttl_ms.unwrap_or(APPROVAL_DEFAULT_TTL_MS)),
        expires_at_ms,
    })
}

fn approval_optional_string(
    options: &mut BTreeMap<String, Value>,
    name: &str,
) -> Result<Option<String>, Value> {
    match options.remove(name) {
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(Value::String(_)) => Err(Value::result_err(
            "ApprovalRequestError",
            format!("approval option {name} must not be empty"),
        )),
        Some(Value::Nil) | None => Ok(None),
        Some(value) => Err(Value::result_err(
            "ApprovalRequestError",
            format!(
                "approval option {name} must be a string, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn approval_snapshot_value(snapshot: &ApprovalSnapshot) -> Value {
    let token = snapshot
        .token
        .as_ref()
        .map(|token| Value::String(token.clone()))
        .unwrap_or(Value::Nil);
    let expires_at_ms = snapshot
        .expires_at_ms
        .map(Value::Number)
        .unwrap_or(Value::Nil);
    let claimed_at_ms = snapshot
        .claimed_at_ms
        .map(Value::Number)
        .unwrap_or(Value::Nil);
    let completed_at_ms = snapshot
        .completed_at_ms
        .map(Value::Number)
        .unwrap_or(Value::Nil);
    let rejected_at_ms = snapshot
        .rejected_at_ms
        .map(Value::Number)
        .unwrap_or(Value::Nil);
    let completed_result = snapshot.completed_result.clone().unwrap_or(Value::Nil);
    let rejection_reason = snapshot
        .rejection_reason
        .as_ref()
        .map(|reason| Value::String(reason.clone()))
        .unwrap_or(Value::Nil);
    Value::Map(
        BTreeMap::from([
            ("id".to_string(), Value::String(snapshot.id.clone())),
            ("token".to_string(), token),
            ("operation".to_string(), snapshot.operation.clone()),
            ("metadata".to_string(), snapshot.metadata.clone()),
            ("status".to_string(), Value::String(snapshot.status.clone())),
            ("pending".to_string(), Value::Bool(snapshot.pending)),
            ("claimed".to_string(), Value::Bool(snapshot.claimed)),
            ("completed".to_string(), Value::Bool(snapshot.completed)),
            ("rejected".to_string(), Value::Bool(snapshot.rejected)),
            ("expired".to_string(), Value::Bool(snapshot.expired)),
            (
                "created_at_ms".to_string(),
                Value::Number(snapshot.created_at_ms),
            ),
            ("expires_at_ms".to_string(), expires_at_ms),
            ("claimed_at_ms".to_string(), claimed_at_ms),
            ("completed_at_ms".to_string(), completed_at_ms),
            ("rejected_at_ms".to_string(), rejected_at_ms),
            ("result".to_string(), completed_result),
            ("rejection_reason".to_string(), rejection_reason),
        ])
        .into(),
    )
}

fn approval_runtime_error_value(error: ApprovalRuntimeError) -> Value {
    Value::result_err(error.kind, error.message)
}

fn http_stream_snapshot_value(snapshot: &HttpStreamSnapshot) -> Value {
    let status_code = snapshot
        .status_code
        .map(Value::Number)
        .unwrap_or(Value::Nil);
    let error = snapshot
        .error
        .as_ref()
        .map(|error| Value::String(error.clone()))
        .unwrap_or(Value::Nil);
    let headers = snapshot
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(value.clone())))
        .collect::<BTreeMap<_, _>>();
    Value::Map(
        BTreeMap::from([
            ("id".to_string(), Value::Number(snapshot.id as i64)),
            ("method".to_string(), Value::String(snapshot.method.clone())),
            ("url".to_string(), Value::String(snapshot.url.clone())),
            (
                "started_at_ms".to_string(),
                Value::Number(snapshot.started_at_ms),
            ),
            ("status".to_string(), Value::String(snapshot.status.clone())),
            ("running".to_string(), Value::Bool(snapshot.running)),
            ("success".to_string(), Value::Bool(snapshot.success)),
            ("status_code".to_string(), status_code),
            ("headers".to_string(), Value::Map(headers.into())),
            ("error".to_string(), error),
            (
                "body_len".to_string(),
                Value::Number(snapshot.body_len as i64),
            ),
            (
                "body_truncated".to_string(),
                Value::Bool(snapshot.body_truncated),
            ),
            ("cancelled".to_string(), Value::Bool(snapshot.cancelled)),
        ])
        .into(),
    )
}

fn http_stream_read_value(read: &HttpStreamRead) -> Value {
    let mut values = match http_stream_snapshot_value(&read.snapshot) {
        Value::Map(map) => map.snapshot(),
        _ => BTreeMap::new(),
    };
    values.insert("body".to_string(), Value::String(read.body.clone()));
    values.insert("offset".to_string(), Value::Number(read.offset as i64));
    Value::Map(values.into())
}

fn http_stream_runtime_error_value(error: HttpStreamRuntimeError) -> Value {
    Value::result_err(error.kind, error.message)
}

fn unknown_http_stream_value(id: u64) -> Value {
    Value::result_err("UnknownHttpStream", format!("unknown HTTP stream: {id}"))
}

fn pty_snapshot_value(snapshot: &PtySnapshot) -> Value {
    let cwd = snapshot
        .cwd
        .as_ref()
        .map(|path| Value::String(path.to_string_lossy().into_owned()))
        .unwrap_or(Value::Nil);
    let exit_code = snapshot.exit_code.map(Value::Number).unwrap_or(Value::Nil);
    let error = snapshot
        .error
        .as_ref()
        .map(|error| Value::String(error.clone()))
        .unwrap_or(Value::Nil);
    let process_id = snapshot
        .process_id
        .map(|id| Value::Number(id.into()))
        .unwrap_or(Value::Nil);
    Value::Map(
        BTreeMap::from([
            ("id".to_string(), Value::Number(snapshot.id as i64)),
            (
                "command".to_string(),
                Value::String(snapshot.command.clone()),
            ),
            (
                "args".to_string(),
                Value::Array(
                    snapshot
                        .args
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                ),
            ),
            ("cwd".to_string(), cwd),
            (
                "started_at_ms".to_string(),
                Value::Number(snapshot.started_at_ms),
            ),
            ("status".to_string(), Value::String(snapshot.status.clone())),
            ("running".to_string(), Value::Bool(snapshot.running)),
            ("success".to_string(), Value::Bool(snapshot.success)),
            ("exit_code".to_string(), exit_code.clone()),
            ("status_code".to_string(), exit_code),
            ("error".to_string(), error),
            (
                "output_len".to_string(),
                Value::Number(snapshot.output_len as i64),
            ),
            (
                "output_truncated".to_string(),
                Value::Bool(snapshot.output_truncated),
            ),
            ("rows".to_string(), Value::Number(snapshot.rows.into())),
            ("cols".to_string(), Value::Number(snapshot.cols.into())),
            ("process_id".to_string(), process_id),
            ("stopped".to_string(), Value::Bool(snapshot.stopped)),
        ])
        .into(),
    )
}

fn pty_read_value(read: &PtyRead) -> Value {
    let mut values = match pty_snapshot_value(&read.snapshot) {
        Value::Map(map) => map.snapshot(),
        _ => BTreeMap::new(),
    };
    values.insert("output".to_string(), Value::String(read.output.clone()));
    values.insert("offset".to_string(), Value::Number(read.offset as i64));
    Value::Map(values.into())
}

fn pty_runtime_error_value(error: PtyRuntimeError) -> Value {
    Value::result_err(error.kind, error.message)
}

fn unknown_pty_session_value(id: u64) -> Value {
    Value::result_err("PtyNotFound", format!("unknown PTY session: {id}"))
}

fn process_env_from_map(
    values: BTreeMap<String, Value>,
) -> Result<BTreeMap<String, String>, Value> {
    values
        .into_iter()
        .map(|(key, value)| match value {
            Value::String(value) => Ok((key, value)),
            value => Err(Value::result_err(
                "ProcessRequestError",
                format!(
                    "process env value for {key} must be a string, got {}",
                    value_kind(&value)
                ),
            )),
        })
        .collect()
}

fn validate_environment_assignment(name: &str, value: &str) -> Option<String> {
    if let Some(message) = validate_environment_name(name) {
        return Some(message);
    }
    if value.contains('\0') {
        return Some("environment variable value must not contain NUL".to_string());
    }
    None
}

fn validate_environment_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("environment variable name must not be empty".to_string());
    }
    if name.contains('=') {
        return Some("environment variable name must not contain =".to_string());
    }
    if name.contains('\0') {
        return Some("environment variable name must not contain NUL".to_string());
    }
    None
}

fn secret_reference_value(kind: &str, key: &str, value: String) -> Value {
    Value::Map(
        BTreeMap::from([
            ("type".to_string(), Value::String(kind.to_string())),
            (key.to_string(), Value::String(value)),
        ])
        .into(),
    )
}

fn secret_reference_string(reference: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match reference.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn config_path_from_value(path: Value) -> Result<Vec<String>, Value> {
    match path {
        Value::String(value) if !value.is_empty() => Ok(vec![value]),
        Value::String(_) => Err(Value::result_err(
            "ConfigError",
            "config path string must not be empty",
        )),
        Value::Array(values) => config_path_from_values(values.snapshot()),
        Value::List(values) => config_path_from_values(values.snapshot()),
        value => Err(Value::result_err(
            "ConfigError",
            format!(
                "config path must be a string, array, or list, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn config_path_from_values(values: Vec<Value>) -> Result<Vec<String>, Value> {
    if values.is_empty() {
        return Err(Value::result_err(
            "ConfigError",
            "config path must not be empty",
        ));
    }
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::String(value) if !value.is_empty() => Ok(value),
            Value::String(_) => Err(Value::result_err(
                "ConfigError",
                format!("config path[{index}] must not be empty"),
            )),
            value => Err(Value::result_err(
                "ConfigError",
                format!(
                    "config path[{index}] must be a string, got {}",
                    value_kind(&value)
                ),
            )),
        })
        .collect()
}

fn config_get_path(config: &MapValue, path: &[String]) -> Value {
    let mut current = Value::Map(config.clone());
    let mut traversed = Vec::new();
    for segment in path {
        traversed.push(segment.clone());
        let Value::Map(map) = current else {
            return Value::result_err(
                "ConfigError",
                format!("config path {} does not contain a map", traversed.join(".")),
            );
        };
        match map.get(segment) {
            Some(Value::Nil) | None => {
                return Value::result_err(
                    "ConfigError",
                    format!("missing config value: {}", traversed.join(".")),
                );
            }
            Some(value) => current = value,
        }
    }
    Value::result_ok(current)
}

fn http_request_header_put(request: MapValue, name: String, value: String) -> Value {
    if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
        return Value::result_err(
            "HttpHeaderError",
            format!("invalid HTTP header name: {name}"),
        );
    }
    if reqwest::header::HeaderValue::from_str(&value).is_err() {
        return Value::result_err(
            "HttpHeaderError",
            format!("invalid HTTP header value for {name}"),
        );
    }
    let headers = match request.get("headers") {
        Some(Value::Map(headers)) => headers,
        Some(Value::Nil) | None => {
            let headers = MapValue::default();
            request.insert("headers".to_string(), Value::Map(headers.clone()));
            headers
        }
        Some(value) => {
            return Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP request headers must be a map, got {}",
                    value_kind(&value)
                ),
            );
        }
    };
    headers.insert(name, Value::String(value));
    Value::result_ok(Value::Map(request))
}

fn process_options_env_put(options: MapValue, name: String, value: String) -> Value {
    let env = match options.get("env") {
        Some(Value::Map(env)) => env,
        Some(Value::Nil) | None => {
            let env = MapValue::default();
            options.insert("env".to_string(), Value::Map(env.clone()));
            env
        }
        Some(value) => {
            return Value::result_err(
                "ProcessRequestError",
                format!(
                    "process option env must be a map, got {}",
                    value_kind(&value)
                ),
            );
        }
    };
    env.insert(name, Value::String(value));
    Value::result_ok(Value::Map(options))
}

fn process_read_offsets(options: Value) -> Result<(usize, usize), Value> {
    let Value::Map(options) = options else {
        return Err(Value::result_err(
            "ProcessRequestError",
            format!(
                "process read options must be a map, got {}",
                value_kind(&options)
            ),
        ));
    };
    let mut options = options.snapshot();
    let stdout_offset = match options.remove("stdout_offset") {
        Some(value) => process_offset_from_value("stdout_offset", value)?,
        None => 0,
    };
    let stderr_offset = match options.remove("stderr_offset") {
        Some(value) => process_offset_from_value("stderr_offset", value)?,
        None => 0,
    };
    if let Some(key) = options.keys().next() {
        return Err(Value::result_err(
            "ProcessRequestError",
            format!("unknown process read option: {key}"),
        ));
    }
    Ok((stdout_offset, stderr_offset))
}

fn process_offset_from_value(name: &str, value: Value) -> Result<usize, Value> {
    match value {
        Value::Number(value) if value >= 0 => usize::try_from(value).map_err(|_| {
            Value::result_err(
                "ProcessRequestError",
                format!("process read option {name} is too large"),
            )
        }),
        Value::Number(value) => Err(Value::result_err(
            "ProcessRequestError",
            format!("process read option {name} cannot be negative: {value}"),
        )),
        Value::Nil => Ok(0),
        value => Err(Value::result_err(
            "ProcessRequestError",
            format!(
                "process read option {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn process_snapshot_value(snapshot: &ProcessSnapshot) -> Value {
    let cwd = snapshot
        .cwd
        .as_ref()
        .map(|path| Value::String(path.to_string_lossy().into_owned()))
        .unwrap_or(Value::Nil);
    let exit_code = snapshot
        .exit_code
        .map(|code| Value::Number(code.into()))
        .unwrap_or(Value::Nil);
    let error = snapshot
        .error
        .as_ref()
        .map(|error| Value::String(error.clone()))
        .unwrap_or(Value::Nil);
    Value::Map(
        BTreeMap::from([
            ("id".to_string(), Value::Number(snapshot.id as i64)),
            (
                "command".to_string(),
                Value::String(snapshot.command.clone()),
            ),
            (
                "args".to_string(),
                Value::Array(
                    snapshot
                        .args
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                ),
            ),
            ("cwd".to_string(), cwd),
            (
                "started_at_ms".to_string(),
                Value::Number(snapshot.started_at_ms),
            ),
            ("status".to_string(), Value::String(snapshot.status.clone())),
            ("running".to_string(), Value::Bool(snapshot.running)),
            ("success".to_string(), Value::Bool(snapshot.success)),
            ("exit_code".to_string(), exit_code.clone()),
            ("status_code".to_string(), exit_code),
            ("error".to_string(), error),
            (
                "stdout_len".to_string(),
                Value::Number(snapshot.stdout_len as i64),
            ),
            (
                "stderr_len".to_string(),
                Value::Number(snapshot.stderr_len as i64),
            ),
            (
                "stdout_truncated".to_string(),
                Value::Bool(snapshot.stdout_truncated),
            ),
            (
                "stderr_truncated".to_string(),
                Value::Bool(snapshot.stderr_truncated),
            ),
            ("timed_out".to_string(), Value::Bool(snapshot.timed_out)),
            ("cancelled".to_string(), Value::Bool(snapshot.cancelled)),
        ])
        .into(),
    )
}

fn process_read_value(read: &ProcessRead) -> Value {
    let mut values = match process_snapshot_value(&read.snapshot) {
        Value::Map(map) => map.snapshot(),
        _ => BTreeMap::new(),
    };
    values.insert("stdout".to_string(), Value::String(read.stdout.clone()));
    values.insert("stderr".to_string(), Value::String(read.stderr.clone()));
    values.insert(
        "stdout_offset".to_string(),
        Value::Number(read.stdout_offset as i64),
    );
    values.insert(
        "stderr_offset".to_string(),
        Value::Number(read.stderr_offset as i64),
    );
    Value::Map(values.into())
}

fn process_runtime_error_value(error: ProcessRuntimeError) -> Value {
    Value::result_err(error.kind, error.message)
}

fn unknown_process_job_value(id: u64) -> Value {
    Value::result_err("ProcessNotFound", format!("unknown process job: {id}"))
}

#[derive(Clone, Copy)]
struct WorkspaceListOptions {
    recursive: bool,
    include_files: bool,
    include_dirs: bool,
    max_entries: usize,
}

#[derive(Clone, Copy)]
struct WorkspaceWriteOptions {
    overwrite: bool,
    create_parent_dirs: bool,
}

#[derive(Clone, Copy)]
struct WorkspaceMkdirOptions {
    recursive: bool,
}

#[derive(Clone, Copy)]
struct WorkspaceDeleteOptions {
    recursive: bool,
    missing_ok: bool,
}

fn workspace_options_map(
    value: Value,
    allowed_keys: &[&str],
) -> Result<BTreeMap<String, Value>, Value> {
    let Value::Map(map) = value else {
        return Err(Value::result_err(
            "WorkspaceRequestError",
            format!(
                "workspace options must be a map, got {}",
                value_kind(&value)
            ),
        ));
    };
    let options = map.snapshot();
    if let Some(key) = options
        .keys()
        .find(|key| !allowed_keys.contains(&key.as_str()))
    {
        return Err(Value::result_err(
            "WorkspaceRequestError",
            format!("unknown workspace option: {key}"),
        ));
    }
    Ok(options)
}

fn workspace_list_options(value: Value) -> Result<WorkspaceListOptions, Value> {
    let mut options = workspace_options_map(
        value,
        &["recursive", "include_files", "include_dirs", "max_entries"],
    )?;
    let recursive = workspace_bool_option(&mut options, "recursive", false)?;
    let include_files = workspace_bool_option(&mut options, "include_files", true)?;
    let include_dirs = workspace_bool_option(&mut options, "include_dirs", true)?;
    let max_entries = workspace_usize_option(
        &mut options,
        "max_entries",
        WORKSPACE_DEFAULT_MAX_LIST_ENTRIES,
        WORKSPACE_MAX_LIST_ENTRIES,
    )?;
    Ok(WorkspaceListOptions {
        recursive,
        include_files,
        include_dirs,
        max_entries,
    })
}

fn workspace_read_max_bytes(value: Value) -> Result<usize, Value> {
    let mut options = workspace_options_map(value, &["max_bytes"])?;
    workspace_usize_option(
        &mut options,
        "max_bytes",
        WORKSPACE_DEFAULT_MAX_READ_BYTES,
        WORKSPACE_MAX_READ_BYTES,
    )
}

fn workspace_write_options(value: Value) -> Result<WorkspaceWriteOptions, Value> {
    let mut options = workspace_options_map(value, &["overwrite", "create_parent_dirs"])?;
    Ok(WorkspaceWriteOptions {
        overwrite: workspace_bool_option(&mut options, "overwrite", false)?,
        create_parent_dirs: workspace_bool_option(&mut options, "create_parent_dirs", false)?,
    })
}

fn workspace_mkdir_options(value: Value) -> Result<WorkspaceMkdirOptions, Value> {
    let mut options = workspace_options_map(value, &["recursive"])?;
    Ok(WorkspaceMkdirOptions {
        recursive: workspace_bool_option(&mut options, "recursive", true)?,
    })
}

fn workspace_delete_options(value: Value) -> Result<WorkspaceDeleteOptions, Value> {
    let mut options = workspace_options_map(value, &["recursive", "missing_ok"])?;
    Ok(WorkspaceDeleteOptions {
        recursive: workspace_bool_option(&mut options, "recursive", false)?,
        missing_ok: workspace_bool_option(&mut options, "missing_ok", false)?,
    })
}

fn workspace_bool_option(
    options: &mut BTreeMap<String, Value>,
    name: &str,
    default: bool,
) -> Result<bool, Value> {
    match options.remove(name) {
        Some(Value::Bool(value)) => Ok(value),
        Some(Value::Nil) | None => Ok(default),
        Some(value) => Err(Value::result_err(
            "WorkspaceRequestError",
            format!(
                "workspace option {name} must be a bool, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn workspace_usize_option(
    options: &mut BTreeMap<String, Value>,
    name: &str,
    default: usize,
    max: usize,
) -> Result<usize, Value> {
    match options.remove(name) {
        Some(Value::Number(value)) if value >= 0 => {
            let value = usize::try_from(value).map_err(|_| {
                Value::result_err(
                    "WorkspaceRequestError",
                    format!("workspace option {name} is too large"),
                )
            })?;
            if value > max {
                Err(Value::result_err(
                    "WorkspaceRequestError",
                    format!("workspace option {name} must be at most {max}"),
                ))
            } else {
                Ok(value)
            }
        }
        Some(Value::Number(value)) => Err(Value::result_err(
            "WorkspaceRequestError",
            format!("workspace option {name} cannot be negative: {value}"),
        )),
        Some(Value::Nil) | None => Ok(default),
        Some(value) => Err(Value::result_err(
            "WorkspaceRequestError",
            format!(
                "workspace option {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn workspace_resolved_value(source: &str, path: &Path, root: Option<&Path>, exists: bool) -> Value {
    let mut fields = workspace_path_fields(source, path, root);
    fields.insert("exists".to_string(), Value::Bool(exists));
    Value::Map(fields.into())
}

fn workspace_metadata_result(source: &str, path: &Path, root: Option<&Path>) -> Value {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Value::result_ok(workspace_metadata_value(source, path, root, &metadata)),
        Err(error) => Value::result_err("IoError", error.to_string()),
    }
}

fn workspace_metadata_value(
    source: &str,
    path: &Path,
    root: Option<&Path>,
    metadata: &fs::Metadata,
) -> Value {
    let mut fields = workspace_path_fields(source, path, root);
    let kind = workspace_entry_kind(metadata);
    let file_type = metadata.file_type();
    fields.insert("exists".to_string(), Value::Bool(true));
    fields.insert("kind".to_string(), Value::String(kind.to_string()));
    fields.insert("is_file".to_string(), Value::Bool(metadata.is_file()));
    fields.insert("is_dir".to_string(), Value::Bool(metadata.is_dir()));
    fields.insert(
        "is_symlink".to_string(),
        Value::Bool(file_type.is_symlink()),
    );
    fields.insert("len".to_string(), Value::Number(metadata.len() as i64));
    fields.insert(
        "readonly".to_string(),
        Value::Bool(metadata.permissions().readonly()),
    );
    fields.insert(
        "modified_at_ms".to_string(),
        metadata
            .modified()
            .ok()
            .map(system_time_value)
            .unwrap_or(Value::Nil),
    );
    Value::Map(fields.into())
}

fn utc_datetime_value(timestamp_ms: i64) -> Result<DateTime<Utc>, Value> {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .ok_or_else(|| {
            Value::result_err(
                "DateTimeRangeError",
                format!("timestamp {timestamp_ms} is outside the supported UTC range"),
            )
        })
}

fn format_rfc3339_millis(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn timestamp_parts_value(value: DateTime<Utc>) -> Value {
    let weekday = value.weekday();
    Value::Map(
        BTreeMap::from([
            (
                "timestamp_ms".to_string(),
                Value::Number(value.timestamp_millis()),
            ),
            ("timezone".to_string(), Value::String("UTC".to_string())),
            ("year".to_string(), Value::Number(value.year().into())),
            ("month".to_string(), Value::Number(value.month().into())),
            ("day".to_string(), Value::Number(value.day().into())),
            ("hour".to_string(), Value::Number(value.hour().into())),
            ("minute".to_string(), Value::Number(value.minute().into())),
            ("second".to_string(), Value::Number(value.second().into())),
            (
                "millisecond".to_string(),
                Value::Number(value.timestamp_subsec_millis().into()),
            ),
            ("ordinal".to_string(), Value::Number(value.ordinal().into())),
            ("weekday".to_string(), Value::String(weekday.to_string())),
            (
                "weekday_number".to_string(),
                Value::Number(weekday.number_from_monday().into()),
            ),
        ])
        .into(),
    )
}

fn date_value(value: NaiveDate) -> Value {
    let weekday = value.weekday();
    Value::Map(
        BTreeMap::from([
            ("year".to_string(), Value::Number(value.year().into())),
            ("month".to_string(), Value::Number(value.month().into())),
            ("day".to_string(), Value::Number(value.day().into())),
            ("ordinal".to_string(), Value::Number(value.ordinal().into())),
            ("weekday".to_string(), Value::String(weekday.to_string())),
            (
                "weekday_number".to_string(),
                Value::Number(weekday.number_from_monday().into()),
            ),
        ])
        .into(),
    )
}

fn timestamp_from_parts_value(parts: &MapValue) -> Result<i64, Value> {
    let date = date_from_value(parts)?;
    let hour = date_part_u32(parts, "hour", 0)?;
    let minute = date_part_u32(parts, "minute", 0)?;
    let second = date_part_u32(parts, "second", 0)?;
    let millisecond = date_part_u32(parts, "millisecond", 0)?;
    let Some(value) = date.and_hms_milli_opt(hour, minute, second, millisecond) else {
        return Err(Value::result_err(
            "DateTimeValueError",
            "timestamp parts do not form a valid UTC time",
        ));
    };
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(value, Utc).timestamp_millis())
}

fn date_from_value(value: &MapValue) -> Result<NaiveDate, Value> {
    let year = date_part_i32(value, "year")?;
    let month = date_part_u32(value, "month", 0)?;
    let day = date_part_u32(value, "day", 0)?;
    NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        Value::result_err(
            "DateValueError",
            format!("date parts do not form a valid date: {year:04}-{month:02}-{day:02}"),
        )
    })
}

fn date_part_i32(value: &MapValue, name: &str) -> Result<i32, Value> {
    let raw = required_date_part(value, name)?;
    i32::try_from(raw).map_err(|_| {
        Value::result_err(
            "DateValueError",
            format!("date field {name} is outside the supported range"),
        )
    })
}

fn date_part_u32(value: &MapValue, name: &str, default: i64) -> Result<u32, Value> {
    let raw = match value.get(name) {
        Some(Value::Nil) | None => default,
        Some(Value::Number(value)) => value,
        Some(value) => {
            return Err(Value::result_err(
                "DateValueError",
                format!(
                    "date field {name} must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if raw < 0 {
        return Err(Value::result_err(
            "DateValueError",
            format!("date field {name} must not be negative"),
        ));
    }
    u32::try_from(raw).map_err(|_| {
        Value::result_err(
            "DateValueError",
            format!("date field {name} is outside the supported range"),
        )
    })
}

fn required_date_part(value: &MapValue, name: &str) -> Result<i64, Value> {
    match value.get(name) {
        Some(Value::Number(value)) => Ok(value),
        Some(value) => Err(Value::result_err(
            "DateValueError",
            format!(
                "date field {name} must be a number, got {}",
                value_kind(&value)
            ),
        )),
        None => Err(Value::result_err(
            "DateValueError",
            format!("date field {name} is required"),
        )),
    }
}

fn duration_parts_value(duration_ms: i64) -> Value {
    let negative = duration_ms < 0;
    let mut remaining = if negative {
        -(duration_ms as i128)
    } else {
        duration_ms as i128
    };
    let days = remaining / 86_400_000;
    remaining %= 86_400_000;
    let hours = remaining / 3_600_000;
    remaining %= 3_600_000;
    let minutes = remaining / 60_000;
    remaining %= 60_000;
    let seconds = remaining / 1_000;
    let milliseconds = remaining % 1_000;
    Value::Map(
        BTreeMap::from([
            ("total_ms".to_string(), Value::Number(duration_ms)),
            ("negative".to_string(), Value::Bool(negative)),
            ("days".to_string(), Value::Number(days as i64)),
            ("hours".to_string(), Value::Number(hours as i64)),
            ("minutes".to_string(), Value::Number(minutes as i64)),
            ("seconds".to_string(), Value::Number(seconds as i64)),
            (
                "milliseconds".to_string(),
                Value::Number(milliseconds as i64),
            ),
        ])
        .into(),
    )
}

fn workspace_entry_kind(metadata: &fs::Metadata) -> &'static str {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    }
}

fn workspace_path_fields(
    source: &str,
    path: &Path,
    root: Option<&Path>,
) -> BTreeMap<String, Value> {
    let relative = root
        .and_then(|root| path.strip_prefix(root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    BTreeMap::from([
        (
            "requested_path".to_string(),
            Value::String(source.to_string()),
        ),
        (
            "path".to_string(),
            Value::String(path.to_string_lossy().into_owned()),
        ),
        ("relative_path".to_string(), Value::String(relative)),
        (
            "inside_root".to_string(),
            Value::Bool(root.is_none_or(|root| path.starts_with(root))),
        ),
    ])
}

fn workspace_list_result(
    path: &Path,
    root: Option<&Path>,
    options: &WorkspaceListOptions,
) -> Value {
    let mut values = Vec::new();
    match workspace_collect_entries(path, root, options, &mut values) {
        Ok(()) => Value::result_ok(Value::Array(values.into())),
        Err(error) => Value::result_err("IoError", error),
    }
}

fn workspace_collect_entries(
    path: &Path,
    root: Option<&Path>,
    options: &WorkspaceListOptions,
    values: &mut Vec<Value>,
) -> Result<(), String> {
    let entries = fs::read_dir(path).map_err(|error| error.to_string())?;
    for entry in entries {
        if values.len() >= options.max_entries {
            return Err(format!(
                "workspace list exceeded max_entries {}",
                options.max_entries
            ));
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if root.is_some_and(|root| !path.starts_with(root)) {
            return Err(format!("workspace entry escaped root: {}", path.display()));
        }
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        let include = (file_type.is_dir() && options.include_dirs)
            || (file_type.is_file() && options.include_files)
            || (!file_type.is_dir() && !file_type.is_file());
        if include {
            values.push(workspace_metadata_value(
                &path.to_string_lossy(),
                &path,
                root,
                &metadata,
            ));
        }
        if options.recursive && file_type.is_dir() {
            workspace_collect_entries(&path, root, options, values)?;
        }
    }
    Ok(())
}

fn workspace_read_text_result(path: &Path, max_bytes: usize) -> Value {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => return Value::result_err("IoError", error.to_string()),
    };
    let mut bytes = Vec::new();
    let read_limit = max_bytes as u64 + 1;
    if let Err(error) = Read::by_ref(&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
    {
        return Value::result_err("IoError", error.to_string());
    }
    if bytes.len() > max_bytes {
        return Value::result_err(
            "FileTooLarge",
            format!("workspace read exceeded max_bytes {max_bytes}"),
        );
    }
    match String::from_utf8(bytes) {
        Ok(contents) => Value::result_ok(Value::String(contents)),
        Err(error) => Value::result_err("Utf8Error", error.to_string()),
    }
}

fn workspace_write_text_result(
    source: &str,
    path: &Path,
    contents: &str,
    root: Option<&Path>,
    options: &WorkspaceWriteOptions,
) -> Value {
    if path.exists() && !options.overwrite {
        return Value::result_err(
            "AlreadyExists",
            format!("workspace path already exists: {}", path.display()),
        );
    }
    if options.create_parent_dirs {
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Value::result_err("IoError", error.to_string());
            }
        }
    }
    match fs::write(path, contents) {
        Ok(()) => workspace_metadata_result(source, path, root),
        Err(error) => Value::result_err("IoError", error.to_string()),
    }
}

fn workspace_delete_result(
    source: &str,
    path: &Path,
    root: Option<&Path>,
    options: &WorkspaceDeleteOptions,
) -> Value {
    if is_filesystem_root_path(path, root) {
        return Value::result_err("PermissionError", "refusing to delete filesystem root");
    }

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound && options.missing_ok => {
            return Value::result_ok(workspace_deleted_value(
                source,
                path,
                root,
                Value::Nil,
                false,
                options.recursive,
            ));
        }
        Err(error) => return Value::result_err("IoError", error.to_string()),
    };
    let kind = Value::String(workspace_entry_kind(&metadata).to_string());
    match delete_filesystem_path(path, options.recursive) {
        Ok(()) => Value::result_ok(workspace_deleted_value(
            source,
            path,
            root,
            kind,
            true,
            options.recursive,
        )),
        Err(error) => Value::result_err("IoError", error.to_string()),
    }
}

fn workspace_deleted_value(
    source: &str,
    path: &Path,
    root: Option<&Path>,
    kind: Value,
    deleted: bool,
    recursive: bool,
) -> Value {
    let mut fields = workspace_path_fields(source, path, root);
    fields.insert("exists".to_string(), Value::Bool(false));
    fields.insert("deleted".to_string(), Value::Bool(deleted));
    fields.insert("kind".to_string(), kind);
    fields.insert("recursive".to_string(), Value::Bool(recursive));
    Value::Map(fields.into())
}

fn workspace_copy_or_move_result(
    destination_source: &str,
    source: &Path,
    destination: &Path,
    root: Option<&Path>,
    options: &WorkspaceWriteOptions,
    move_source: bool,
) -> Value {
    if destination.exists() && !options.overwrite {
        return Value::result_err(
            "AlreadyExists",
            format!(
                "workspace destination already exists: {}",
                destination.display()
            ),
        );
    }
    if options.create_parent_dirs {
        if let Some(parent) = destination.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Value::result_err("IoError", error.to_string());
            }
        }
    }
    if move_source {
        if destination.exists() {
            return Value::result_err(
                "AlreadyExists",
                "workspace_move cannot overwrite an existing destination",
            );
        }
        match fs::rename(source, destination) {
            Ok(()) => workspace_metadata_result(destination_source, destination, root),
            Err(error) => Value::result_err("IoError", error.to_string()),
        }
    } else {
        match fs::copy(source, destination) {
            Ok(_) => workspace_metadata_result(destination_source, destination, root),
            Err(error) => Value::result_err("IoError", error.to_string()),
        }
    }
}

fn is_filesystem_root_path(path: &Path, root: Option<&Path>) -> bool {
    root.is_some_and(|root| path == root)
}

fn delete_filesystem_path(path: &Path, recursive: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        remove_symlink(path)
    } else if metadata.is_dir() {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    } else {
        fs::remove_file(path)
    }
}

fn remove_symlink(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(file_error) => fs::remove_dir(path).map_err(|_| file_error),
    }
}

fn system_time_value(value: SystemTime) -> Value {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| duration.as_millis().try_into().ok())
        .map(Value::Number)
        .unwrap_or(Value::Nil)
}

fn http_request_from_value(value: Value) -> Result<HttpRequest, Value> {
    let Value::Map(map) = value else {
        return Err(Value::result_err(
            "HttpRequestError",
            format!("HTTP request must be a map, got {}", value_kind(&value)),
        ));
    };
    let mut fields = map.snapshot();
    let url = match fields.remove("url") {
        Some(Value::String(value)) => value,
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP request url must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
        None => {
            return Err(Value::result_err(
                "HttpRequestError",
                "HTTP request requires a url string",
            ));
        }
    };
    let method = match fields.remove("method") {
        Some(Value::String(value)) => value
            .parse::<reqwest::Method>()
            .map_err(|_| Value::result_err("HttpRequestError", "invalid HTTP request method"))?,
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP request method must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
        None => reqwest::Method::GET,
    };
    let headers = match fields.remove("headers") {
        Some(Value::Map(headers)) => http_headers_from_map(headers.snapshot())?,
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP request headers must be a map, got {}",
                    value_kind(&value)
                ),
            ));
        }
        None => reqwest::header::HeaderMap::new(),
    };
    let json = match fields.remove("json") {
        Some(value) => match value_to_json(&value) {
            Ok(value) => Some(value),
            Err(message) => return Err(Value::result_err("JsonError", message)),
        },
        None => None,
    };
    let body = match fields.remove("body") {
        Some(Value::String(value)) => Some(value),
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP request body must be a string, got {}",
                    value_kind(&value)
                ),
            ));
        }
        None => None,
    };
    if json.is_some() && body.is_some() {
        return Err(Value::result_err(
            "HttpRequestError",
            "HTTP request cannot include both json and body",
        ));
    }
    let timeout_ms = match fields.remove("timeout_ms") {
        Some(Value::Number(value)) if value > 0 => u64::try_from(value)
            .map_err(|_| Value::result_err("HttpRequestError", "HTTP timeout_ms is too large"))?,
        Some(Value::Number(value)) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!("HTTP timeout_ms must be positive, got {value}"),
            ));
        }
        Some(Value::Nil) | None => HTTP_DEFAULT_TIMEOUT_MS,
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP timeout_ms must be a number, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    if timeout_ms > HTTP_MAX_TIMEOUT_MS {
        return Err(Value::result_err(
            "HttpRequestError",
            format!("HTTP timeout_ms must be at most {HTTP_MAX_TIMEOUT_MS}"),
        ));
    }
    let max_response_bytes = match fields.remove("max_response_bytes") {
        Some(value) => http_max_response_bytes_from_value(value)?,
        None => HTTP_DEFAULT_MAX_RESPONSE_BYTES,
    };
    let allowed_hosts = match fields.remove("allowed_hosts") {
        Some(value) => Some(http_string_set_from_value("allowed_hosts", value)?),
        None => None,
    };
    let allowed_schemes = match fields.remove("allowed_schemes") {
        Some(value) => Some(http_string_set_from_value("allowed_schemes", value)?),
        None => None,
    };
    match fields.remove("follow_redirects") {
        Some(Value::Bool(false)) | Some(Value::Nil) | None => {}
        Some(Value::Bool(true)) => {
            return Err(Value::result_err(
                "HttpRequestError",
                "HTTP follow_redirects=true is not supported yet; redirects stay disabled",
            ));
        }
        Some(value) => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP follow_redirects must be a bool, got {}",
                    value_kind(&value)
                ),
            ));
        }
    }

    Ok(HttpRequest {
        method,
        url,
        headers,
        json,
        body,
        timeout: Duration::from_millis(timeout_ms),
        max_response_bytes,
        allowed_hosts,
        allowed_schemes,
    })
}

fn http_max_response_bytes_from_value(value: Value) -> Result<usize, Value> {
    match value {
        Value::Number(value) if value >= 0 => {
            let value = usize::try_from(value).map_err(|_| {
                Value::result_err("HttpRequestError", "HTTP max_response_bytes is too large")
            })?;
            if value > HTTP_MAX_RESPONSE_BYTES {
                Err(Value::result_err(
                    "HttpRequestError",
                    format!("HTTP max_response_bytes must be at most {HTTP_MAX_RESPONSE_BYTES}"),
                ))
            } else {
                Ok(value)
            }
        }
        Value::Number(value) => Err(Value::result_err(
            "HttpRequestError",
            format!("HTTP max_response_bytes must be non-negative, got {value}"),
        )),
        Value::Nil => Ok(HTTP_DEFAULT_MAX_RESPONSE_BYTES),
        value => Err(Value::result_err(
            "HttpRequestError",
            format!(
                "HTTP max_response_bytes must be a number, got {}",
                value_kind(&value)
            ),
        )),
    }
}

fn http_string_set_from_value(name: &str, value: Value) -> Result<BTreeSet<String>, Value> {
    let values = match value {
        Value::Array(values) => values.snapshot(),
        Value::List(values) => values.snapshot(),
        Value::Nil => return Ok(BTreeSet::new()),
        value => {
            return Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP {name} must be an array or list, got {}",
                    value_kind(&value)
                ),
            ));
        }
    };
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::String(value) if !value.is_empty() => Ok(value.to_ascii_lowercase()),
            Value::String(_) => Err(Value::result_err(
                "HttpRequestError",
                format!("HTTP {name}[{index}] must not be empty"),
            )),
            value => Err(Value::result_err(
                "HttpRequestError",
                format!(
                    "HTTP {name}[{index}] must be a string, got {}",
                    value_kind(&value)
                ),
            )),
        })
        .collect()
}

fn http_headers_from_map(
    headers: BTreeMap<String, Value>,
) -> Result<reqwest::header::HeaderMap, Value> {
    let mut output = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            Value::result_err(
                "HttpHeaderError",
                format!("invalid HTTP header name: {name}"),
            )
        })?;
        let value = match value {
            Value::String(value) => value,
            value => {
                return Err(Value::result_err(
                    "HttpHeaderError",
                    format!(
                        "HTTP header values must be strings, got {} for {name}",
                        value_kind(&value)
                    ),
                ));
            }
        };
        let value = reqwest::header::HeaderValue::from_str(&value).map_err(|_| {
            Value::result_err(
                "HttpHeaderError",
                format!("invalid HTTP header value for {name}"),
            )
        })?;
        output.insert(name, value);
    }
    Ok(output)
}

fn http_request_policy_error(request: &HttpRequest) -> Option<Value> {
    let parsed = match reqwest::Url::parse(&request.url) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Some(Value::result_err(
                "HttpRequestError",
                format!("invalid HTTP URL {:?}: {error}", request.url),
            ));
        }
    };
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Some(Value::result_err(
            "HttpRequestError",
            format!("unsupported HTTP URL scheme: {scheme}"),
        ));
    }
    if let Some(allowed_schemes) = &request.allowed_schemes {
        if !allowed_schemes.contains(&scheme) {
            return Some(Value::result_err(
                "PermissionError",
                format!("HTTP scheme is not allowed by request policy: {scheme}"),
            ));
        }
    }
    let host = match parsed.host_str() {
        Some(host) => host.to_ascii_lowercase(),
        None => {
            return Some(Value::result_err(
                "HttpRequestError",
                format!("HTTP URL has no host: {:?}", request.url),
            ));
        }
    };
    if let Some(allowed_hosts) = &request.allowed_hosts {
        if !allowed_hosts.contains(&host) {
            return Some(Value::result_err(
                "PermissionError",
                format!("HTTP host is not allowed by request policy: {host}"),
            ));
        }
    }
    None
}

fn perform_process_spawn(request: ProcessRequest) -> Value {
    let mut command = Command::new(&request.command);
    command.args(&request.args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(if request.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }
    if request.clear_env {
        command.env_clear();
    }
    for (name, value) in &request.env {
        command.env(name, value);
    }
    configure_process_window(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Value::result_err("ProcessError", error.to_string()),
    };

    let stdout = child
        .stdout
        .take()
        .map(|stdout| read_process_output(stdout, request.stdout_max_bytes));
    let stderr = child
        .stderr
        .take()
        .map(|stderr| read_process_output(stderr, request.stderr_max_bytes));

    if let Some(input) = request.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(input.as_bytes()) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_process_output(stdout);
                let _ = join_process_output(stderr);
                return Value::result_err(
                    "ProcessError",
                    format!("failed to write stdin: {error}"),
                );
            }
        }
    }

    let status = match child.wait_timeout(request.timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_process_output(stdout);
            let _ = join_process_output(stderr);
            return Value::result_err(
                "ProcessTimeout",
                format!("process timed out after {} ms", request.timeout.as_millis()),
            );
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_process_output(stdout);
            let _ = join_process_output(stderr);
            return Value::result_err("ProcessError", error.to_string());
        }
    };

    let (stdout, stdout_truncated) = match join_process_output(stdout) {
        Ok(output) => output,
        Err(error) => return Value::result_err("ProcessError", error),
    };
    let (stderr, stderr_truncated) = match join_process_output(stderr) {
        Ok(output) => output,
        Err(error) => return Value::result_err("ProcessError", error),
    };
    let status_code = status
        .code()
        .map(|code| Value::Number(code.into()))
        .unwrap_or(Value::Nil);

    Value::result_ok(Value::Map(
        BTreeMap::from([
            ("success".to_string(), Value::Bool(status.success())),
            ("status".to_string(), status_code),
            ("stdout".to_string(), Value::String(stdout)),
            ("stderr".to_string(), Value::String(stderr)),
            (
                "stdout_truncated".to_string(),
                Value::Bool(stdout_truncated),
            ),
            (
                "stderr_truncated".to_string(),
                Value::Bool(stderr_truncated),
            ),
        ])
        .into(),
    ))
}

fn read_process_output<R>(
    mut reader: R,
    max_bytes: usize,
) -> thread::JoinHandle<Result<(String, bool), String>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if output.len() >= max_bytes {
                        truncated = true;
                        continue;
                    }
                    let available = max_bytes - output.len();
                    let take = count.min(available);
                    output.extend_from_slice(&buffer[..take]);
                    if take < count {
                        truncated = true;
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok((String::from_utf8_lossy(&output).into_owned(), truncated))
    })
}

fn join_process_output(
    handle: Option<thread::JoinHandle<Result<(String, bool), String>>>,
) -> Result<(String, bool), String> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| "process output reader thread panicked".to_string())?,
        None => Ok((String::new(), false)),
    }
}

fn perform_http_get(url: String) -> Value {
    let client = match http_client(Duration::from_millis(HTTP_DEFAULT_TIMEOUT_MS)) {
        Ok(client) => client,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    http_response(client.get(url).send(), HTTP_DEFAULT_MAX_RESPONSE_BYTES)
}

fn perform_http_post_json(url: String, body: JsonValue) -> Value {
    let client = match http_client(Duration::from_millis(HTTP_DEFAULT_TIMEOUT_MS)) {
        Ok(client) => client,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    http_response(
        client.post(url).json(&body).send(),
        HTTP_DEFAULT_MAX_RESPONSE_BYTES,
    )
}

fn perform_http_request(request: HttpRequest) -> Value {
    let client = match http_client(request.timeout) {
        Ok(client) => client,
        Err(error) => return Value::result_err("HttpError", error.to_string()),
    };
    let max_response_bytes = request.max_response_bytes;
    let mut builder = client
        .request(request.method, request.url)
        .headers(request.headers);
    if let Some(json) = request.json {
        builder = builder.json(&json);
    } else if let Some(body) = request.body {
        builder = builder.body(body);
    }
    http_response(builder.send(), max_response_bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_key_events_are_ricochet_maps() {
        let value = terminal_key_event_value(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        let Value::Map(map) = value else {
            panic!("terminal key event should be a map");
        };

        assert_eq!(map.get("type"), Some(Value::String("key".to_string())));
        assert_eq!(map.get("code"), Some(Value::String("char".to_string())));
        assert_eq!(map.get("char"), Some(Value::String("q".to_string())));
        assert_eq!(
            map.get("modifiers"),
            Some(Value::Array(
                vec![
                    Value::String("control".to_string()),
                    Value::String("alt".to_string())
                ]
                .into()
            ))
        );
    }
}
