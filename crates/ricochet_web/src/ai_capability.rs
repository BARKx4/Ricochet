use std::collections::BTreeMap;
use std::io::Read;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use ricochet_vm::{Value, Vm, VmError};
use serde_json::Value as JsonValue;

const AI_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const AI_MAX_ERROR_BYTES: usize = 64 * 1024;

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
        .redirect(reqwest::redirect::Policy::none())
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
        let body = read_capped_response(response, AI_MAX_ERROR_BYTES)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|error| format!("failed to read error body: {error}"));
        let body = redact_ai_error_body(&body, &config.api_key);
        bail!("AI provider returned {status}: {body}");
    }

    let bytes = read_capped_response(response, AI_MAX_RESPONSE_BYTES)?;
    let response: JsonValue =
        serde_json::from_slice(&bytes).context("AI provider returned invalid JSON")?;
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

fn redact_ai_error_body(body: &str, api_key: &str) -> String {
    let mut redacted = body.to_string();
    if !api_key.is_empty() {
        redacted = redacted
            .replace(&format!("Bearer {api_key}"), "Bearer [redacted token]")
            .replace(api_key, "[redacted token]");
    }
    redacted
}

fn read_capped_response(
    response: reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        bail!("AI provider response exceeded {max_bytes} bytes");
    }
    let mut bytes = Vec::new();
    response
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .context("failed to read AI provider response")?;
    if bytes.len() > max_bytes {
        bail!("AI provider response exceeded {max_bytes} bytes");
    }
    Ok(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn ai_chat_does_not_follow_redirects() {
        let (address, server) = spawn_ai_response_server(
            b"HTTP/1.1 302 Found\r\nLocation: https://attacker.example/v1/chat/completions\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        );
        let error = chat_blocking(test_config(address), "hello".to_string())
            .expect_err("redirect should be returned as a provider error");
        server.join().expect("AI test server should finish");

        assert!(
            error.to_string().contains("302 Found"),
            "redirect error should expose the 302 response, got: {error:#}"
        );
    }

    #[test]
    fn ai_chat_rejects_oversized_success_response_from_content_length() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            AI_MAX_RESPONSE_BYTES + 1
        )
        .into_bytes();
        let (address, server) = spawn_ai_response_server(response);
        let error = chat_blocking(test_config(address), "hello".to_string())
            .expect_err("oversized response should fail before JSON parsing");
        server.join().expect("AI test server should finish");

        assert!(
            error.to_string().contains("AI provider response exceeded"),
            "oversized response error was: {error:#}"
        );
    }

    #[test]
    fn ai_chat_redacts_api_key_from_provider_error_body() {
        let response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: 80\r\nConnection: close\r\n\r\nprovider echoed Authorization: Bearer test-key and raw token test-key in diagnostics"
            .to_vec();
        let (address, server) = spawn_ai_response_server(response);
        let error = chat_blocking(test_config(address), "hello".to_string())
            .expect_err("provider error should fail");
        server.join().expect("AI test server should finish");
        let error = error.to_string();

        assert!(
            !error.contains("test-key"),
            "AI provider errors must redact API keys, got: {error}"
        );
        assert!(
            error.contains("Bearer [redacted token]"),
            "redacted bearer marker should remain, got: {error}"
        );
    }

    fn test_config(address: std::net::SocketAddr) -> AiProviderConfig {
        AiProviderConfig {
            provider: "openai-compatible".to_string(),
            model: "demo".to_string(),
            api_key: "test-key".to_string(),
            base_url: format!("http://{address}/v1"),
        }
    }

    fn spawn_ai_response_server(
        response: Vec<u8>,
    ) -> (std::net::SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("AI test server should bind");
        let address = listener
            .local_addr()
            .expect("AI test server should have addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("AI client should connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("AI test server read timeout should set");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        request.extend_from_slice(&buffer[..count]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(error) => panic!("AI test server read failed: {error}"),
                }
            }
            stream
                .write_all(&response)
                .expect("AI test response should write");
            stream.flush().expect("AI test response should flush");
        });
        (address, server)
    }
}
