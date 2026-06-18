use std::collections::BTreeMap;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use ricochet_vm::{Value, Vm, VmError};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct AiProvider {
    config: AiProviderConfig,
}

impl AiProvider {
    pub fn new(config: AiProviderConfig) -> Self {
        Self { config }
    }

    fn chat(&self, prompt: &str) -> Result<Value> {
        let config = self.config.clone();
        let prompt = prompt.to_string();
        thread::spawn(move || chat_blocking(config, prompt))
            .join()
            .map_err(|_| anyhow!("AI provider worker panicked"))?
    }
}

fn chat_blocking(config: AiProviderConfig, prompt: String) -> Result<Value> {
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build AI HTTP client")?;
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": config.model.clone(),
        "messages": [
            {
                "role": "user",
                "content": prompt,
            }
        ],
    });

    let response = http
        .post(url)
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .context("AI provider request failed")?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|error| format!("failed to read error body: {error}"));
        bail!("AI provider returned {status}: {body}");
    }

    let response: JsonValue = response
        .json()
        .context("AI provider returned invalid JSON")?;
    let text = response
        .pointer("/choices/0/message/content")
        .and_then(JsonValue::as_str)
        .context("AI provider response is missing choices[0].message.content")?;
    let response_model = response
        .get("model")
        .and_then(JsonValue::as_str)
        .unwrap_or(&config.model);

    Ok(Value::Map(
        BTreeMap::from([
            ("provider".to_string(), Value::String(config.provider)),
            (
                "model".to_string(),
                Value::String(response_model.to_string()),
            ),
            ("text".to_string(), Value::String(text.to_string())),
        ])
        .into(),
    ))
}

pub fn install_ai_capability(vm: &mut Vm, provider: AiProvider) -> Result<Value, VmError> {
    vm.define_class("AiCapability", "Capability")?;
    vm.add_native_method_with_arity("chat", 1, move |arguments| {
        let prompt = match arguments.as_slice() {
            [Value::String(prompt), _receiver] => prompt.clone(),
            [value, _receiver] => {
                return Err(VmError::TypeError {
                    word: "ai.chat".to_string(),
                    expected: "prompt string".to_string(),
                    actual: ai_value_kind(value).to_string(),
                });
            }
            _ => {
                return Err(VmError::StackUnderflow {
                    word: "ai.chat".to_string(),
                    needed: 1,
                    available: 0,
                });
            }
        };

        Ok(match provider.chat(&prompt) {
            Ok(value) => Value::result_ok(value),
            Err(error) => Value::result_err("AiError", error.to_string()),
        })
    })?;
    vm.end_class();
    vm.new_instance("AiCapability")
}

fn ai_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::Float(_) => "float",
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
