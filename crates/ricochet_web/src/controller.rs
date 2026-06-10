use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use ricochet_compiler::compile_source;
use ricochet_vm::{Value, Vm};

#[derive(Debug, Default)]
pub struct RequestContext {
    pub params: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub view_data: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    View(String),
    Text(String),
}

type Action = Box<dyn Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync>;

#[derive(Default)]
pub struct ControllerRegistry {
    actions: BTreeMap<(String, String), Action>,
}

impl ControllerRegistry {
    pub fn register_static<F>(&mut self, controller: &str, action: &str, f: F)
    where
        F: Fn(&mut RequestContext) -> Result<ActionResult> + Send + Sync + 'static,
    {
        self.actions.insert(
            (controller.to_string(), action.to_string()),
            Box::new(f),
        );
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
        let controller_name = controller.to_string();
        let action_name = action.to_string();

        self.register_static(controller, action, move |ctx| {
            let mut vm = Vm::default();
            vm.run_chunk(&chunk)
                .with_context(|| format!("failed to load controller {controller_name}"))?;
            vm.set_variable("ctx", context_value(ctx));
            let arg_values = controller_arg_values(&vm, &controller_name, &action_name, ctx);

            let instance = vm
                .new_instance(&controller_name)
                .with_context(|| format!("failed to instantiate controller {controller_name}"))?;
            let result = vm
                .call_method_value_with_args(instance, &action_name, arg_values)
                .with_context(|| format!("failed to call {controller_name}.{action_name}"))?;

            copy_view_data(&vm, ctx);
            action_result_from_value(result)
        });

        Ok(())
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

fn context_value(ctx: &RequestContext) -> Value {
    let mut context = BTreeMap::new();
    let params = ctx
        .params
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect::<BTreeMap<_, _>>();
    let query = ctx
        .query
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect::<BTreeMap<_, _>>();
    context.insert("params".to_string(), Value::Map(params));
    context.insert("query".to_string(), Value::Map(query));
    Value::Map(context)
}

fn controller_arg_values(
    vm: &Vm,
    controller: &str,
    action: &str,
    ctx: &RequestContext,
) -> Vec<Value> {
    let Some(args) = vm.method_args(controller, action) else {
        return Vec::new();
    };

    args.inputs
        .iter()
        .map(|name| controller_arg_value(name, ctx))
        .collect()
}

fn controller_arg_value(name: &str, ctx: &RequestContext) -> Value {
    if name == "ctx" {
        return context_value(ctx);
    }

    if let Some(value) = ctx.params.get(name) {
        return Value::String(value.clone());
    }

    if let Some(value) = ctx.query.get(name) {
        return Value::String(value.clone());
    }

    Value::Nil
}

fn copy_view_data(vm: &Vm, ctx: &mut RequestContext) {
    for (name, value) in vm.variables() {
        if name == "ctx" {
            continue;
        }

        if let Some(value) = view_data_string(value) {
            ctx.view_data.insert(name.clone(), value);
        }
    }
}

fn view_data_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Nil => Some(String::new()),
        _ => None,
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
                    Some(Value::String(view)) => Ok(ActionResult::View(view)),
                    _ => bail!("Ricochet view action is missing string name"),
                },
                "text" => match map.remove("body") {
                    Some(Value::String(text)) => Ok(ActionResult::Text(text)),
                    _ => bail!("Ricochet text action is missing string body"),
                },
                _ => bail!("unsupported Ricochet action result type {action_type}"),
            }
        }
        Value::String(text) => Ok(ActionResult::Text(text)),
        value => bail!("Ricochet controller returned unsupported value {value:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_controller_index_sets_title_and_returns_view() {
        let mut controllers = ControllerRegistry::default();
        controllers.register_static("HomeController", "index", |ctx| {
            ctx.view_data
                .insert("title".to_string(), "Hello Ricochet".to_string());
            Ok(ActionResult::View("home/index".to_string()))
        });

        let mut ctx = RequestContext::default();
        let result = controllers
            .call("HomeController", "index", &mut ctx)
            .expect("action should dispatch");

        assert_eq!(result, ActionResult::View("home/index".to_string()));
        assert_eq!(
            ctx.view_data.get("title"),
            Some(&"Hello Ricochet".to_string())
        );
    }

    #[test]
    fn unknown_action_fails_loudly() {
        let controllers = ControllerRegistry::default();
        let mut ctx = RequestContext::default();

        let err = controllers
            .call("HomeController", "missing", &mut ctx)
            .expect_err("unknown action should fail");

        assert!(err.to_string().contains("unknown action HomeController.missing"));
    }
}
