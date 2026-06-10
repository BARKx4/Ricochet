use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::manifest::Manifest;
use crate::revision::{AppRevision, RevisionManager};
use crate::router::{parse_routes, Route};
use crate::template::{render_template, EscapeMode};

#[derive(Clone)]
struct AppState {
    runtime: Arc<AppRuntime>,
    revisions: RevisionManager,
}

struct AppRuntime {
    root: PathBuf,
    escape: EscapeMode,
    controllers: ControllerRegistry,
}

pub fn build_test_app() -> Result<Router> {
    let fixture_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");
    build_app_from_dir(fixture_root)
}

pub fn build_app_from_dir(project_root: impl AsRef<Path>) -> Result<Router> {
    let project_root = project_root.as_ref();
    let manifest_path = project_root.join("ricochet.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&manifest_source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let routes_path = project_root.join(&manifest.web.routes);
    let routes_source = fs::read_to_string(&routes_path)
        .with_context(|| format!("failed to read {}", routes_path.display()))?;
    let routes = parse_routes(&routes_source)
        .with_context(|| format!("failed to parse {}", routes_path.display()))?;

    let controllers = build_controller_registry(project_root, &routes)?;
    let runtime = Arc::new(AppRuntime {
        root: project_root.to_path_buf(),
        escape: manifest.web.views.escape,
        controllers,
    });
    let state = AppState {
        runtime,
        revisions: RevisionManager::default(),
    };

    let mut app = Router::new();
    for route in routes {
        if route.method != "GET" {
            bail!("unsupported HTTP method {} for {}", route.method, route.path);
        }

        let controller = route.controller.clone();
        let action = route.action.clone();
        app = app.route(
            &route.path,
            get(move |State(state): State<AppState>,
                      path_params: Option<AxumPath<HashMap<String, String>>>| {
                let controller = controller.clone();
                let action = action.clone();
                let path_params = path_params
                    .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                    .unwrap_or_default();
                async move { render_route(state, controller, action, path_params).await }
            }),
        );
    }

    Ok(app.with_state(state))
}

fn build_controller_registry(project_root: &Path, routes: &[Route]) -> Result<ControllerRegistry> {
    let mut registry = ControllerRegistry::default();
    let mut registered = BTreeSet::new();

    for route in routes {
        let key = (route.controller.clone(), route.action.clone());
        if !registered.insert(key.clone()) {
            continue;
        }

        let (controller, action) = key;
        let controller_path = project_root
            .join("app")
            .join("Controllers")
            .join(format!("{controller}.rco"));
        let source = fs::read_to_string(&controller_path)
            .with_context(|| format!("failed to read {}", controller_path.display()))?;
        registry.register_ricochet_source(
            &controller,
            &action,
            controller_path.to_string_lossy().as_ref(),
            &source,
        )?;
    }

    Ok(registry)
}

async fn render_route(
    state: AppState,
    controller: String,
    action: String,
    path_params: BTreeMap<String, String>,
) -> impl IntoResponse {
    let revision = state.revisions.current();

    match render_action(&state.runtime, revision, &controller, &action, path_params) {
        Ok(body) => Html(body).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ricochet MVC error: {err}"),
        )
            .into_response(),
    }
}

fn render_action(
    runtime: &AppRuntime,
    revision: AppRevision,
    controller: &str,
    action: &str,
    path_params: BTreeMap<String, String>,
) -> Result<String> {
    let mut ctx = RequestContext {
        params: path_params,
        ..RequestContext::default()
    };
    ctx.view_data
        .insert("revision".to_string(), revision.id.to_string());

    match runtime.controllers.call(controller, action, &mut ctx)? {
        ActionResult::View(view) => render_view(runtime, &view, &ctx),
        ActionResult::Text(text) => Ok(text),
    }
}

fn render_view(runtime: &AppRuntime, view: &str, ctx: &RequestContext) -> Result<String> {
    let template_path = runtime
        .root
        .join("app")
        .join("Views")
        .join(format!("{view}.html"));
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("failed to read {}", template_path.display()))?;
    render_template(&template, &ctx.view_data, runtime.escape)
}

pub async fn serve_current_dir(debug: bool, watch: bool) -> Result<()> {
    let app = build_app_from_dir(".")?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    println!("Ricochet web server listening on http://127.0.0.1:3000 debug={debug} watch={watch}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_build_test_app_returns_ok() {
        let _ = build_test_app().expect("server test app should build");
    }
}
