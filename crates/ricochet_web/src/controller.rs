use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use ricochet_bytecode::Chunk;
use ricochet_compiler::compile_source;
use ricochet_vm::{Value, Vm};
use serde_json::Value as JsonValue;

#[derive(Debug, Default)]
pub struct RequestContext {
    pub method: String,
    pub path: String,
    pub params: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub form: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub config: BTreeMap<String, Value>,
    pub view_data: BTreeMap<String, Value>,
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
const WEB_CONTROLLER_INSTRUCTION_LIMIT: u64 = 50_000;

#[derive(Default)]
pub struct ControllerRegistry {
    actions: BTreeMap<(String, String), Action>,
    vm_setup: Option<VmSetup>,
}

impl ControllerRegistry {
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

        self.register_static(controller, action, move |ctx| {
            let mut vm = Vm::default();
            vm.set_instruction_limit(WEB_CONTROLLER_INSTRUCTION_LIMIT);
            let capabilities = match &vm_setup {
                Some(setup) => setup(&mut vm)?,
                None => BTreeMap::new(),
            };
            vm.run_chunk(&chunk)
                .with_context(|| format!("failed to load controller {controller_name}"))?;
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

            copy_view_data(&vm, ctx);
            action_result_from_value(result)
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

fn context_value(ctx: &RequestContext, capabilities: &BTreeMap<String, Value>) -> Value {
    let mut context = BTreeMap::new();
    let params = string_map_value(&ctx.params);
    let query = string_map_value(&ctx.query);
    let form = string_map_value(&ctx.form);
    let headers = string_map_value(&ctx.headers);
    let cookies = string_map_value(&ctx.cookies);
    let config = Value::Map(ctx.config.clone().into());
    let request = Value::Map(
        BTreeMap::from([
            ("method".to_string(), Value::String(ctx.method.clone())),
            ("path".to_string(), Value::String(ctx.path.clone())),
            ("params".to_string(), params.clone()),
            ("query".to_string(), query.clone()),
            ("form".to_string(), form.clone()),
            ("headers".to_string(), headers.clone()),
            ("cookies".to_string(), cookies.clone()),
        ])
        .into(),
    );

    context.insert("params".to_string(), params);
    context.insert("query".to_string(), query);
    context.insert("form".to_string(), form);
    context.insert("headers".to_string(), headers);
    context.insert("cookies".to_string(), cookies);
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

fn copy_view_data(vm: &Vm, ctx: &mut RequestContext) {
    for (name, value) in vm.variables() {
        if name == "ctx" {
            continue;
        }

        ctx.view_data.insert(name.clone(), value.clone());
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
    Ok(serde_json::to_string(&json_value_from_value(value)?)?)
}

fn json_value_from_value(value: Value) -> Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(value)),
        Value::Number(value) => Ok(JsonValue::Number(value.into())),
        Value::String(value) => Ok(JsonValue::String(value)),
        Value::Array(values) => values
            .snapshot()
            .into_iter()
            .map(json_value_from_value)
            .collect::<Result<Vec<_>>>()
            .map(JsonValue::Array),
        Value::List(values) => values
            .snapshot()
            .into_iter()
            .map(json_value_from_value)
            .collect::<Result<Vec<_>>>()
            .map(JsonValue::Array),
        Value::Set(values) => values
            .snapshot()
            .into_iter()
            .map(json_value_from_value)
            .collect::<Result<Vec<_>>>()
            .map(JsonValue::Array),
        Value::Map(values) => values
            .snapshot()
            .into_iter()
            .map(|(key, value)| Ok((key, json_value_from_value(value)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(JsonValue::Object),
        value => bail!("Ricochet json action cannot serialize {value:?}"),
    }
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
HomeController Controller subclass
  index method
    ctx get .greeter get .hello text
  end
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
}
