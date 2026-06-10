use std::collections::BTreeMap;

use anyhow::{bail, Result};

#[derive(Debug, Default)]
pub struct RequestContext {
    pub params: BTreeMap<String, String>,
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
