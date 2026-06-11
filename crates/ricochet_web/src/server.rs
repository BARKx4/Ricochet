use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::{
    extract::{Form as AxumForm, Path as AxumPath, Query as AxumQuery, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use ricochet_bytecode::Chunk;
use ricochet_vm::{Value, Vm};

use crate::active_record::{ModelMapping, PostgresDatabase};
use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::database_capability::{install_database_capability, DatabaseBackend};
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

enum RenderedAction {
    Html(String),
    Text(String),
    Json(String),
}

type VmSetup = Arc<dyn Fn(&mut Vm) -> Result<BTreeMap<String, Value>> + Send + Sync>;

pub fn build_test_app() -> Result<Router> {
    let fixture_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");
    build_app_from_dir(fixture_root)
}

pub fn build_app_from_dir(project_root: impl AsRef<Path>) -> Result<Router> {
    let project_root = project_root.as_ref();
    build_app_from_dir_internal(project_root, None)
}

pub fn build_app_from_dir_with_database(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<Router> {
    let project_root = project_root.as_ref();
    let vm_setup = database_vm_setup(project_root, backend)?;
    build_app_from_dir_internal(project_root, Some(vm_setup))
}

pub fn routes_from_dir(project_root: impl AsRef<Path>) -> Result<Vec<Route>> {
    let project_root = project_root.as_ref();
    let manifest = load_manifest(project_root)?;
    let routes_path = project_root.join(&manifest.web.routes);
    let routes_source = fs::read_to_string(&routes_path)
        .with_context(|| format!("failed to read {}", routes_path.display()))?;
    parse_routes(&routes_source)
        .with_context(|| format!("failed to parse {}", routes_path.display()))
}

fn build_app_from_dir_internal(project_root: &Path, vm_setup: Option<VmSetup>) -> Result<Router> {
    let manifest = load_manifest(project_root)?;
    let routes = routes_from_dir(project_root)?;

    let vm_setup = match vm_setup {
        Some(setup) => Some(setup),
        None => model_vm_setup(project_root)?,
    };
    let controllers = build_controller_registry(project_root, &routes, vm_setup)?;
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
        let controller = route.controller.clone();
        let action = route.action.clone();
        match route.method.as_str() {
            "GET" => {
                app = app.route(
                    &route.path,
                    get(move |State(state): State<AppState>,
                              path_params: Option<AxumPath<HashMap<String, String>>>,
                              AxumQuery(query_params): AxumQuery<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                path_params,
                                query_params,
                                BTreeMap::new(),
                            )
                            .await
                        }
                    }),
                );
            }
            "POST" => {
                app = app.route(
                    &route.path,
                    post(move |State(state): State<AppState>,
                               path_params: Option<AxumPath<HashMap<String, String>>>,
                               AxumQuery(query_params): AxumQuery<HashMap<String, String>>,
                               AxumForm(form_params): AxumForm<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        let form_params = form_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                path_params,
                                query_params,
                                form_params,
                            )
                            .await
                        }
                    }),
                );
            }
            _ => bail!("unsupported HTTP method {} for {}", route.method, route.path),
        }
    }

    Ok(app.with_state(state))
}

fn load_manifest(project_root: &Path) -> Result<Manifest> {
    let manifest_path = project_root.join("ricochet.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    toml::from_str(&manifest_source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))
}

fn build_controller_registry(
    project_root: &Path,
    routes: &[Route],
    vm_setup: Option<VmSetup>,
) -> Result<ControllerRegistry> {
    let mut registry = ControllerRegistry::default();
    if let Some(setup) = vm_setup {
        registry.set_vm_setup(move |vm| setup(vm));
    }
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

fn database_vm_setup(
    project_root: &Path,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<VmSetup> {
    let (model_chunks, mappings) = load_model_runtime(project_root)?;
    let model_chunks = Arc::new(model_chunks);

    Ok(Arc::new(move |vm| {
        for chunk in model_chunks.iter() {
            vm.run_chunk(chunk)?;
        }
        let database =
            install_database_capability(vm, backend.clone(), mappings.clone())?;
        Ok(BTreeMap::from([("db".to_string(), database)]))
    }))
}

fn model_vm_setup(project_root: &Path) -> Result<Option<VmSetup>> {
    let model_chunks = load_model_chunks(project_root)?
        .into_iter()
        .map(|model| model.chunk)
        .collect::<Vec<_>>();
    if model_chunks.is_empty() {
        return Ok(None);
    }

    let model_chunks = Arc::new(model_chunks);
    Ok(Some(Arc::new(move |vm| {
        for chunk in model_chunks.iter() {
            vm.run_chunk(chunk)?;
        }
        Ok(BTreeMap::new())
    })))
}

fn load_model_runtime(
    project_root: &Path,
) -> Result<(Vec<Chunk>, BTreeMap<String, ModelMapping>)> {
    let model_chunks = load_model_chunks(project_root)?;

    let mut vm = Vm::default();
    let mut chunks = Vec::with_capacity(model_chunks.len());
    let mut mappings = BTreeMap::new();
    for model in model_chunks {
        vm.run_chunk(&model.chunk)
            .with_context(|| format!("failed to load model {}", model.path.display()))?;
        let mapping = ModelMapping::from_vm(&vm, &model.class_name)
            .with_context(|| format!("failed to map model {}", model.path.display()))?;
        mappings.insert(model.class_name, mapping);
        chunks.push(model.chunk);
    }

    Ok((chunks, mappings))
}

struct ModelChunk {
    class_name: String,
    path: PathBuf,
    chunk: Chunk,
}

fn load_model_chunks(project_root: &Path) -> Result<Vec<ModelChunk>> {
    let models_path = project_root.join("app").join("Models");
    if !models_path.exists() {
        return Ok(Vec::new());
    }

    let mut paths = fs::read_dir(&models_path)
        .with_context(|| format!("failed to read {}", models_path.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rco"));
    paths.sort();

    paths
        .into_iter()
        .map(|path| {
            let class_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .with_context(|| {
                    format!("model filename must be valid Unicode: {}", path.display())
                })?
                .to_string();
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let file = path.to_string_lossy();
            let chunk = ricochet_compiler::compile_source(&file, &source)
                .with_context(|| format!("failed to compile model {}", path.display()))?;
            Ok(ModelChunk {
                class_name,
                path,
                chunk,
            })
        })
        .collect()
}

async fn render_route(
    state: AppState,
    controller: String,
    action: String,
    path_params: BTreeMap<String, String>,
    query_params: BTreeMap<String, String>,
    form_params: BTreeMap<String, String>,
) -> impl IntoResponse {
    let revision = state.revisions.current();

    match render_action(
        &state.runtime,
        revision,
        &controller,
        &action,
        path_params,
        query_params,
        form_params,
    ) {
        Ok(RenderedAction::Html(body)) => Html(body).into_response(),
        Ok(RenderedAction::Text(body)) => body.into_response(),
        Ok(RenderedAction::Json(body)) => {
            ([(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
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
    query_params: BTreeMap<String, String>,
    form_params: BTreeMap<String, String>,
) -> Result<RenderedAction> {
    let mut ctx = RequestContext {
        params: path_params,
        query: query_params,
        form: form_params,
        ..RequestContext::default()
    };
    ctx.view_data
        .insert("revision".to_string(), Value::Number(revision.id as i64));

    match runtime.controllers.call(controller, action, &mut ctx)? {
        ActionResult::View(view) => render_view(runtime, &view, &ctx).map(RenderedAction::Html),
        ActionResult::Text(text) => Ok(RenderedAction::Text(text)),
        ActionResult::Json(json) => Ok(RenderedAction::Json(json)),
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
    let project_root = Path::new(".");
    let manifest = load_manifest(project_root)?;
    let app = match manifest.database.default {
        Some(database) => {
            if database.adapter != "postgres" {
                bail!("unsupported database adapter {}", database.adapter);
            }
            let url = database.resolved_url()?;
            let backend = PostgresDatabase::connect(&url).await?;
            build_app_from_dir_with_database(project_root, Arc::new(backend))?
        }
        None => build_app_from_dir(project_root)?,
    };
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
