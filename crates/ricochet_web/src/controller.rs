use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use ricochet_bytecode::Chunk;
use ricochet_compiler::compile_source;
use ricochet_vm::{UploadStreamRegistry, Value, Vm};

use crate::manifest::default_controller_instruction_limit;
use crate::value_json::{value_to_json, SetMode};

#[derive(Default)]
pub struct RequestContext {
    pub method: String,
    pub path: String,
    pub params: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub form: BTreeMap<String, String>,
    pub body: Option<Value>,
    pub json: Option<Value>,
    pub uploads: BTreeMap<String, Value>,
    pub files: Vec<Value>,
    pub upload_streams: UploadStreamRegistry,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub session: BTreeMap<String, Value>,
    pub config: BTreeMap<String, Value>,
    pub view_data: BTreeMap<String, Value>,
    pub logs: Vec<LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    View(String),
    Text(String),
    Json(String),
    ViewResponse {
        view: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
    TextResponse {
        body: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
    JsonResponse {
        body: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
    Redirect {
        location: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
}

type Action = Box<dyn Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync>;
type VmSetup = Arc<dyn Fn(&mut Vm) -> Result<BTreeMap<String, Value>> + Send + Sync>;

pub struct ControllerRegistry {
    actions: BTreeMap<(String, String), Action>,
    vm_setup: Option<VmSetup>,
    instruction_limit: u64,
}

impl Default for ControllerRegistry {
    fn default() -> Self {
        Self {
            actions: BTreeMap::new(),
            vm_setup: None,
            instruction_limit: default_controller_instruction_limit(),
        }
    }
}

impl ControllerRegistry {
    pub fn with_instruction_limit(instruction_limit: u64) -> Self {
        Self {
            instruction_limit,
            ..Self::default()
        }
    }

    pub fn set_vm_setup<F>(&mut self, setup: F)
    where
        F: Fn(&mut Vm) -> Result<BTreeMap<String, Value>> + Send + Sync + 'static,
    {
        self.vm_setup = Some(Arc::new(setup));
    }

    pub fn register_static<F>(&mut self, controller: &str, action: &str, f: F)
    where
        F: Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync + 'static,
    {
        self.actions
            .insert((controller.to_string(), action.to_string()), Box::new(f));
    }

    pub fn register_ricochet_source(
        &mut self,
        controller: &str,
        action: &str,
        file: &str,
        source: &str,
    ) -> Result<()> {
        let chunk = compile_source(file, source)
            .with_context(|| format!("failed to compile controller {controller} from {file}"))?;
        self.register_ricochet_chunk(controller, action, chunk);
        Ok(())
    }

    pub fn register_ricochet_chunk(&mut self, controller: &str, action: &str, chunk: Chunk) {
        let controller_name = controller.to_string();
        let action_name = action.to_string();
        let vm_setup = self.vm_setup.clone();
        let instruction_limit = self.instruction_limit;

        self.register_static(controller, action, move |ctx| {
            let mut vm = Vm::default();
            vm.set_instruction_limit(instruction_limit);
            vm.set_upload_stream_registry(ctx.upload_streams.clone());
            let capabilities = match &vm_setup {
                Some(setup) => setup(&mut vm)?,
                None => BTreeMap::new(),
            };
            vm.set_sleep_enabled(false);
            vm.run_chunk(&chunk)
                .with_context(|| format!("failed to load controller {controller_name}"))?;
            let log_entries = Arc::new(Mutex::new(Vec::new()));
            let logger = install_logger_capability(&mut vm, log_entries.clone())?;
            let mut capabilities = capabilities;
            capabilities.insert("logger".to_string(), logger);
            let context = context_value(ctx, &capabilities);
            vm.set_variable("ctx", context.clone());
            let arg_values =
                controller_arg_values(&vm, &controller_name, &action_name, ctx, &context);

            let instance = vm
                .new_instance(&controller_name)
                .with_context(|| format!("failed to instantiate controller {controller_name}"))?;
            let result = vm
                .call_method_value_with_args(instance, &action_name, arg_values)
                .with_context(|| format!("failed to call {controller_name}.{action_name}"))?;
            let action_result = action_result_from_value(result)?;

            record_instruction_budget_warning(
                &controller_name,
                &action_name,
                vm.instructions_executed(),
                instruction_limit,
                &log_entries,
            );

            copy_session(&context, ctx)?;
            copy_logs(&log_entries, ctx)?;
            copy_view_data(&vm, ctx);
            Ok(action_result)
        });
    }

    pub fn call(
        &self,
        controller: &str,
        action: &str,
        ctx: &mut RequestContext,
    ) -> Result<ActionResult> {
        let key = (controller.to_string(), action.to_string());
        let Some(action_fn) = self.actions.get(&key) else {
            bail!("unknown action {controller}.{action}");
        };

        action_fn(ctx)
    }
}

fn record_instruction_budget_warning(
    controller: &str,
    action: &str,
    used: u64,
    limit: u64,
    entries: &Arc<Mutex<Vec<LogEntry>>>,
) {
    let Some(message) = instruction_budget_warning(controller, action, used, limit) else {
        return;
    };
    if let Ok(mut entries) = entries.lock() {
        entries.push(LogEntry {
            level: "warn".to_string(),
            message: message.clone(),
        });
    }
    eprintln!("Ricochet warning: {message}");
}

fn instruction_budget_warning(
    controller: &str,
    action: &str,
    used: u64,
    limit: u64,
) -> Option<String> {
    if limit == 0 || used < limit - limit / 5 {
        return None;
    }
    Some(format!(
        "controller {controller}.{action} used {used} of {limit} configured VM instructions (80% warning threshold)"
    ))
}

fn context_value(ctx: &RequestContext, capabilities: &BTreeMap<String, Value>) -> Value {
    let mut context = BTreeMap::new();
    let params = string_map_value(&ctx.params);
    let query = string_map_value(&ctx.query);
    let form = string_map_value(&ctx.form);
    let body = ctx.body.clone().unwrap_or(Value::Nil);
    let json = ctx.json.clone().unwrap_or(Value::Nil);
    let uploads = Value::Map(ctx.uploads.clone().into());
    let files = Value::Array(ctx.files.clone().into());
    let headers = string_map_value(&ctx.headers);
    let cookies = string_map_value(&ctx.cookies);
    let session = Value::Map(ctx.session.clone().into());
    let config = Value::Map(ctx.config.clone().into());
    let request = Value::Map(
        BTreeMap::from([
            ("method".to_string(), Value::String(ctx.method.clone())),
            ("path".to_string(), Value::String(ctx.path.clone())),
            ("params".to_string(), params.clone()),
            ("query".to_string(), query.clone()),
            ("form".to_string(), form.clone()),
            ("body".to_string(), body.clone()),
            ("json".to_string(), json.clone()),
            ("uploads".to_string(), uploads.clone()),
            ("files".to_string(), files.clone()),
            ("headers".to_string(), headers.clone()),
            ("cookies".to_string(), cookies.clone()),
            ("session".to_string(), session.clone()),
        ])
        .into(),
    );

    context.insert("params".to_string(), params);
    context.insert("query".to_string(), query);
    context.insert("form".to_string(), form);
    context.insert("body".to_string(), body);
    context.insert("json".to_string(), json);
    context.insert("uploads".to_string(), uploads);
    context.insert("files".to_string(), files);
    context.insert("headers".to_string(), headers);
    context.insert("cookies".to_string(), cookies);
    context.insert("session".to_string(), session);
    context.insert("request".to_string(), request);
    context.insert("config".to_string(), config);
    context.extend(capabilities.clone());
    Value::Map(context.into())
}

fn string_map_value(values: &BTreeMap<String, String>) -> Value {
    Value::Map(
        values
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect::<BTreeMap<_, _>>()
            .into(),
    )
}

fn controller_arg_values(
    vm: &Vm,
    controller: &str,
    action: &str,
    ctx: &RequestContext,
    context: &Value,
) -> Vec<Value> {
    let Some(args) = vm.method_args(controller, action) else {
        return Vec::new();
    };

    args.inputs
        .iter()
        .map(|name| controller_arg_value(name, ctx, context))
        .collect()
}

fn controller_arg_value(name: &str, ctx: &RequestContext, context: &Value) -> Value {
    if name == "ctx" {
        return context.clone();
    }

    if let Some(value) = ctx.params.get(name) {
        return Value::String(value.clone());
    }

    if let Some(value) = ctx.form.get(name) {
        return Value::String(value.clone());
    }

    if let Some(value) = ctx
        .json
        .as_ref()
        .and_then(|value| body_object_field(value, name))
    {
        return value;
    }

    if let Some(value) = ctx.uploads.get(name) {
        return value.clone();
    }

    if let Some(value) = ctx.query.get(name) {
        return Value::String(value.clone());
    }

    if let Value::Map(context) = context {
        if let Some(value) = context.get(name) {
            return value;
        }
    }

    Value::Nil
}

fn body_object_field(value: &Value, name: &str) -> Option<Value> {
    match value {
        Value::Map(values) => values.get(name),
        _ => None,
    }
}

fn copy_view_data(vm: &Vm, ctx: &mut RequestContext) {
    for (name, value) in vm.variables().iter().chain(vm.last_call_variables()) {
        if name == "ctx" {
            continue;
        }

        ctx.view_data.insert(name.clone(), value.clone());
    }
}

fn copy_session(context: &Value, ctx: &mut RequestContext) -> Result<()> {
    let Value::Map(context) = context else {
        return Ok(());
    };
    let Some(session) = context.get("session") else {
        return Ok(());
    };

    match session {
        Value::Map(session) => {
            ctx.session = session.snapshot();
            Ok(())
        }
        value => bail!("session context must be a map, got {value:?}"),
    }
}

fn copy_logs(entries: &Arc<Mutex<Vec<LogEntry>>>, ctx: &mut RequestContext) -> Result<()> {
    ctx.logs = entries
        .lock()
        .map_err(|_| anyhow::anyhow!("logger entries lock was poisoned"))?
        .clone();
    Ok(())
}

fn install_logger_capability(
    vm: &mut Vm,
    entries: Arc<Mutex<Vec<LogEntry>>>,
) -> Result<Value, ricochet_vm::VmError> {
    vm.define_class("LoggerCapability", "Capability")?;
    for level in ["debug", "info", "warn", "error"] {
        add_log_method(vm, level, entries.clone())?;
    }
    let entries_for_snapshot = entries.clone();
    vm.add_native_method("entries", move |_| {
        let entries = entries_for_snapshot
            .lock()
            .map_err(|_| ricochet_vm::VmError::UnknownWord("logger.entries".to_string()))?
            .iter()
            .map(|entry| {
                Value::Map(
                    BTreeMap::from([
                        ("level".to_string(), Value::String(entry.level.clone())),
                        ("message".to_string(), Value::String(entry.message.clone())),
                    ])
                    .into(),
                )
            })
            .collect::<Vec<_>>();
        Ok(Value::Array(entries.into()))
    })?;
    vm.end_class();
    vm.new_instance("LoggerCapability")
}

fn add_log_method(
    vm: &mut Vm,
    level: &'static str,
    entries: Arc<Mutex<Vec<LogEntry>>>,
) -> Result<(), ricochet_vm::VmError> {
    vm.add_native_method_with_arity(level, 1, move |arguments| {
        let message = match arguments.as_slice() {
            [Value::String(message), _receiver] => message.clone(),
            [value, _receiver] => {
                return Err(ricochet_vm::VmError::TypeError {
                    word: format!("logger.{level}"),
                    expected: "message string".to_string(),
                    actual: logger_value_kind(value).to_string(),
                });
            }
            _ => {
                return Err(ricochet_vm::VmError::StackUnderflow {
                    word: format!("logger.{level}"),
                    needed: 1,
                    available: 0,
                });
            }
        };

        entries
            .lock()
            .map_err(|_| ricochet_vm::VmError::UnknownWord(format!("logger.{level}")))?
            .push(LogEntry {
                level: level.to_string(),
                message,
            });
        Ok(Value::Nil)
    })
}

fn logger_value_kind(value: &Value) -> &'static str {
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
        Value::DeferredHttpCredentials(_) => "deferred HTTP credentials",
        Value::SecretRef(_) => "secret reference",
        Value::SecureSessionAction(_) => "secure session action",
    }
}

fn action_result_from_value(value: Value) -> Result<ActionResult> {
    match value {
        Value::Map(mut map) => {
            let action_type = match map.remove("type") {
                Some(Value::String(action_type)) => action_type,
                _ => bail!("Ricochet action result map is missing string type"),
            };

            match action_type.as_str() {
                "view" => match map.remove("name") {
                    Some(Value::String(view)) => {
                        let (status, headers) = response_meta_from_map(&mut map)?;
                        if status.is_none() && headers.is_empty() {
                            Ok(ActionResult::View(view))
                        } else {
                            Ok(ActionResult::ViewResponse {
                                view,
                                status,
                                headers,
                            })
                        }
                    }
                    _ => bail!("Ricochet view action is missing string name"),
                },
                "text" => match map.remove("body") {
                    Some(Value::String(body)) => {
                        let (status, headers) = response_meta_from_map(&mut map)?;
                        if status.is_none() && headers.is_empty() {
                            Ok(ActionResult::Text(body))
                        } else {
                            Ok(ActionResult::TextResponse {
                                body,
                                status,
                                headers,
                            })
                        }
                    }
                    _ => bail!("Ricochet text action is missing string body"),
                },
                "json" => match map.remove("body") {
                    Some(body) => {
                        let body = json_string_from_value(body)?;
                        let (status, headers) = response_meta_from_map(&mut map)?;
                        if status.is_none() && headers.is_empty() {
                            Ok(ActionResult::Json(body))
                        } else {
                            Ok(ActionResult::JsonResponse {
                                body,
                                status,
                                headers,
                            })
                        }
                    }
                    None => bail!("Ricochet json action is missing body"),
                },
                "redirect" => match map.remove("location") {
                    Some(Value::String(location)) => {
                        let (status, headers) = response_meta_from_map(&mut map)?;
                        Ok(ActionResult::Redirect {
                            location,
                            status,
                            headers,
                        })
                    }
                    _ => bail!("Ricochet redirect action is missing string location"),
                },
                _ => bail!("unsupported Ricochet action result type {action_type}"),
            }
        }
        Value::String(text) => Ok(ActionResult::Text(text)),
        value => bail!("Ricochet controller returned unsupported value {value:?}"),
    }
}

fn response_meta_from_map(
    map: &mut ricochet_vm::collection::MapValue,
) -> Result<(Option<u16>, BTreeMap<String, String>)> {
    let status = match map.remove("status") {
        Some(Value::Number(status)) if (100..=599).contains(&status) => Some(status as u16),
        Some(Value::Number(status)) => {
            bail!("HTTP status must be between 100 and 599, got {status}")
        }
        Some(value) => bail!("HTTP status must be a number, got {value:?}"),
        None => None,
    };
    let headers = match map.remove("headers") {
        Some(Value::Map(headers)) => string_map(headers.snapshot(), "response headers")?,
        Some(value) => bail!("response headers must be a map, got {value:?}"),
        None => BTreeMap::new(),
    };

    Ok((status, headers))
}

fn string_map(values: BTreeMap<String, Value>, context: &str) -> Result<BTreeMap<String, String>> {
    values
        .into_iter()
        .map(|(key, value)| match value {
            Value::String(value) => Ok((key, value)),
            value => bail!("{context} values must be strings, got {value:?}"),
        })
        .collect()
}

fn json_string_from_value(value: Value) -> Result<String> {
    Ok(serde_json::to_string(&value_to_json(
        &value,
        "$",
        SetMode::Array,
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_controller_index_sets_title_and_returns_view() {
        let mut controllers = ControllerRegistry::default();
        controllers.register_static("HomeController", "index", |ctx| {
            ctx.view_data.insert(
                "title".to_string(),
                Value::String("Hello Ricochet".to_string()),
            );
            Ok(ActionResult::View("home/index".to_string()))
        });

        let mut ctx = RequestContext::default();
        let result = controllers
            .call("HomeController", "index", &mut ctx)
            .expect("action should dispatch");

        assert_eq!(result, ActionResult::View("home/index".to_string()));
        assert_eq!(
            ctx.view_data.get("title"),
            Some(&Value::String("Hello Ricochet".to_string()))
        );
    }

    #[test]
    fn unknown_action_fails_loudly() {
        let controllers = ControllerRegistry::default();
        let mut ctx = RequestContext::default();

        let err = controllers
            .call("HomeController", "missing", &mut ctx)
            .expect_err("unknown action should fail");

        assert!(err
            .to_string()
            .contains("unknown action HomeController.missing"));
    }

    #[test]
    fn instruction_budget_warning_starts_at_eighty_percent() {
        assert!(instruction_budget_warning("HomeController", "index", 799, 1_000).is_none());
        assert_eq!(
            instruction_budget_warning("HomeController", "index", 800, 1_000).as_deref(),
            Some(
                "controller HomeController.index used 800 of 1000 configured VM instructions (80% warning threshold)"
            )
        );

        let entries = Arc::new(Mutex::new(Vec::new()));
        record_instruction_budget_warning("HomeController", "index", 800, 1_000, &entries);
        assert_eq!(
            entries.lock().expect("warning entries should lock").as_slice(),
            &[LogEntry {
                level: "warn".to_string(),
                message: "controller HomeController.index used 800 of 1000 configured VM instructions (80% warning threshold)".to_string(),
            }]
        );
    }

    #[test]
    fn ricochet_controller_receives_setup_capabilities_in_context() {
        let mut controllers = ControllerRegistry::default();
        controllers.set_vm_setup(|vm| {
            vm.define_class("GreetingCapability", "Capability")?;
            vm.add_native_method("hello", |_| {
                Ok(Value::String("hello from capability".to_string()))
            })?;
            vm.end_class();
            let capability = vm.new_instance("GreetingCapability")?;
            Ok(BTreeMap::from([("greeter".to_string(), capability)]))
        });
        controllers
            .register_ricochet_source(
                "HomeController",
                "index",
                "HomeController.rco",
                r#"
HomeController Controller Subclass
  [
    ctx get "greeter" at hello text
  ] "index" Method
end
"#,
            )
            .expect("controller registers");

        let mut ctx = RequestContext::default();
        let result = controllers
            .call("HomeController", "index", &mut ctx)
            .expect("controller dispatches");

        assert_eq!(
            result,
            ActionResult::Text("hello from capability".to_string())
        );
    }

    #[test]
    fn ricochet_controller_logs_to_request_context() {
        let mut controllers = ControllerRegistry::default();
        controllers
            .register_ricochet_source(
                "HomeController",
                "index",
                "HomeController.rco",
                r#"
HomeController Controller Subclass
  [
    "loaded" ctx get "logger" at info drop
    "ok" text
  ] "index" Method
end
"#,
            )
            .expect("controller registers");

        let mut ctx = RequestContext::default();
        let result = controllers
            .call("HomeController", "index", &mut ctx)
            .expect("controller dispatches");

        assert_eq!(result, ActionResult::Text("ok".to_string()));
        assert_eq!(
            ctx.logs,
            vec![LogEntry {
                level: "info".to_string(),
                message: "loaded".to_string()
            }]
        );
    }
}
