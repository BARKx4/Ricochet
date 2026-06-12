use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{Form as AxumForm, Path as AxumPath, Query as AxumQuery, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{any, delete, get, patch, post, put},
    Router,
};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hmac::{Hmac, Mac};
use ricochet_bytecode::Chunk;
use ricochet_compiler::compile_file_with_imports;
use ricochet_vm::{Value, Vm};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::active_record::{ModelMapping, MysqlDatabase, PostgresDatabase, SqliteDatabase};
use crate::ai_capability::{install_ai_capability, AiProvider};
use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::database_capability::{install_database_capability, DatabaseBackend};
use crate::manifest::{DatabaseDefault, Manifest};
use crate::revision::{AppRevision, RevisionManager};
use crate::router::{parse_routes, Route};
use crate::template::{render_template, EscapeMode};

#[derive(Clone)]
struct AppState {
    runtime: RuntimeSource,
    revisions: RevisionManager,
}

struct AppRuntime {
    root: PathBuf,
    escape: EscapeMode,
    config: BTreeMap<String, Value>,
    session_signing_key: Option<SessionSigningKey>,
    session_encryption_key: Option<SessionEncryptionKey>,
    routes: Vec<Route>,
    controllers: ControllerRegistry,
}

#[derive(Debug, Default)]
struct WebRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    path_params: BTreeMap<String, String>,
    query_params: BTreeMap<String, String>,
    form_params: BTreeMap<String, String>,
}

#[derive(Clone)]
enum RuntimeSource {
    Static(Arc<AppRuntime>),
    Watched(Arc<WatchedRuntime>),
}

struct WatchedRuntime {
    root: PathBuf,
    current: RwLock<Arc<AppRuntime>>,
    signature: Mutex<ProjectSignature>,
    builder: RuntimeBuilder,
    trace_sink: Option<WatchTraceSink>,
}

type RuntimeBuilder = Arc<dyn Fn() -> Result<AppRuntime> + Send + Sync>;
pub type WatchTraceSink = Arc<dyn Fn(&WatchTraceEvent) + Send + Sync>;
const WATCHED_FORM_BODY_LIMIT: usize = 1024 * 1024;
const SESSION_COOKIE_NAME: &str = "ricochet_session";
const SIGNED_SESSION_PREFIX: &str = "v1";
const ENCRYPTED_SESSION_PREFIX: &str = "v2";

#[derive(Clone)]
struct SessionSigningKey {
    secret: Arc<[u8]>,
}

impl SessionSigningKey {
    fn new(secret: String) -> Self {
        Self {
            secret: Arc::from(secret.into_bytes()),
        }
    }

    fn sign(&self, payload: &[u8]) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.secret)
            .expect("HMAC accepts signing keys of any length");
        mac.update(payload);
        hex_encode(&mac.finalize().into_bytes())
    }

    fn verify(&self, payload: &[u8], signature_hex: &str) -> bool {
        let Ok(signature) = hex_decode(signature_hex) else {
            return false;
        };
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.secret)
            .expect("HMAC accepts signing keys of any length");
        mac.update(payload);
        mac.verify_slice(&signature).is_ok()
    }
}

#[derive(Clone)]
struct SessionEncryptionKey {
    key: Arc<[u8; 32]>,
}

impl SessionEncryptionKey {
    fn new(secret: String) -> Self {
        let digest = Sha256::digest(secret.as_bytes());
        let mut key = [0_u8; 32];
        key.copy_from_slice(&digest);
        Self { key: Arc::new(key) }
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let mut nonce = [0_u8; 12];
        getrandom::fill(&mut nonce)
            .map_err(|error| anyhow!("failed to generate session encryption nonce: {error}"))?;
        let cipher = ChaCha20Poly1305::new_from_slice(self.key.as_ref())
            .expect("ChaCha20-Poly1305 accepts 32 byte keys");
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| anyhow!("failed to encrypt session cookie"))?;
        Ok(format!(
            "{ENCRYPTED_SESSION_PREFIX}:{}:{}",
            hex_encode(&nonce),
            hex_encode(&ciphertext)
        ))
    }

    fn decrypt(&self, nonce_hex: &str, ciphertext_hex: &str) -> Option<Vec<u8>> {
        let nonce = hex_decode(nonce_hex).ok()?;
        if nonce.len() != 12 {
            return None;
        }
        let ciphertext = hex_decode(ciphertext_hex).ok()?;
        let cipher = ChaCha20Poly1305::new_from_slice(self.key.as_ref()).ok()?;
        cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
            .ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchTraceEvent {
    Reloaded {
        revision: u64,
        changed_files: Vec<PathBuf>,
    },
    ReloadFailed {
        changed_files: Vec<PathBuf>,
        message: String,
    },
}

enum RenderedAction {
    Html {
        body: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
    Text {
        body: String,
        status: Option<u16>,
        headers: BTreeMap<String, String>,
    },
    Json {
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

type VmSetup = Arc<dyn Fn(&mut Vm) -> Result<BTreeMap<String, Value>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSignature {
    files: BTreeMap<PathBuf, FileSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSignature {
    len: u64,
    hash: u64,
}

impl RuntimeSource {
    fn snapshot(&self, revisions: &RevisionManager) -> Result<(Arc<AppRuntime>, AppRevision)> {
        match self {
            RuntimeSource::Static(runtime) => Ok((runtime.clone(), revisions.current())),
            RuntimeSource::Watched(runtime) => runtime.snapshot(revisions),
        }
    }
}

impl WatchedRuntime {
    fn snapshot(&self, revisions: &RevisionManager) -> Result<(Arc<AppRuntime>, AppRevision)> {
        let current_signature = project_signature(&self.root)?;
        let mut signature = self
            .signature
            .lock()
            .map_err(|_| anyhow!("hot reload signature lock was poisoned"))?;

        if *signature != current_signature {
            let changed_files = changed_signature_paths(&signature, &current_signature);
            let runtime = match (self.builder)() {
                Ok(runtime) => Arc::new(runtime),
                Err(error) => {
                    self.record_trace(WatchTraceEvent::ReloadFailed {
                        changed_files,
                        message: format!("{error:#}"),
                    });
                    return Err(error);
                }
            };
            {
                let mut current = self
                    .current
                    .write()
                    .map_err(|_| anyhow!("hot reload runtime lock was poisoned"))?;
                *current = runtime.clone();
            }
            *signature = current_signature;
            let revision = revisions.publish_new_revision();
            self.record_trace(WatchTraceEvent::Reloaded {
                revision: revision.id,
                changed_files,
            });
            return Ok((runtime, revision));
        }

        let runtime = self
            .current
            .read()
            .map_err(|_| anyhow!("hot reload runtime lock was poisoned"))?
            .clone();
        Ok((runtime, revisions.current()))
    }

    fn record_trace(&self, event: WatchTraceEvent) {
        if let Some(sink) = &self.trace_sink {
            sink(&event);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub debug: bool,
    pub watch: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            debug: false,
            watch: false,
        }
    }
}

impl ServeOptions {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

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

pub fn build_watched_app_from_dir(project_root: impl AsRef<Path>) -> Result<Router> {
    let project_root = project_root.as_ref().to_path_buf();
    let builder_root = project_root.clone();
    let builder: RuntimeBuilder = Arc::new(move || {
        let vm_setup = model_vm_setup(&builder_root)?;
        build_runtime_from_dir_internal(&builder_root, vm_setup)
    });

    build_watched_app_from_runtime_builder(project_root, builder, None)
}

pub fn build_watched_app_from_dir_with_trace(
    project_root: impl AsRef<Path>,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    let project_root = project_root.as_ref().to_path_buf();
    let builder_root = project_root.clone();
    let builder: RuntimeBuilder = Arc::new(move || {
        let vm_setup = model_vm_setup(&builder_root)?;
        build_runtime_from_dir_internal(&builder_root, vm_setup)
    });

    build_watched_app_from_runtime_builder(project_root, builder, Some(trace_sink))
}

pub fn build_watched_app_from_dir_with_database(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<Router> {
    let project_root = project_root.as_ref().to_path_buf();
    let builder_root = project_root.clone();
    let builder: RuntimeBuilder = Arc::new(move || {
        let vm_setup = database_vm_setup(&builder_root, backend.clone())?;
        build_runtime_from_dir_internal(&builder_root, Some(vm_setup))
    });

    build_watched_app_from_runtime_builder(project_root, builder, None)
}

pub fn build_watched_app_from_dir_with_database_and_trace(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    let project_root = project_root.as_ref().to_path_buf();
    let builder_root = project_root.clone();
    let builder: RuntimeBuilder = Arc::new(move || {
        let vm_setup = database_vm_setup(&builder_root, backend.clone())?;
        build_runtime_from_dir_internal(&builder_root, Some(vm_setup))
    });

    build_watched_app_from_runtime_builder(project_root, builder, Some(trace_sink))
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
    let runtime = Arc::new(build_runtime_from_dir_internal(project_root, vm_setup)?);
    build_static_router(runtime)
}

fn build_runtime_from_dir_internal(
    project_root: &Path,
    vm_setup: Option<VmSetup>,
) -> Result<AppRuntime> {
    let manifest = load_manifest(project_root)?;
    let routes = routes_from_dir(project_root)?;

    let vm_setup = match vm_setup {
        Some(setup) => Some(setup),
        None => model_vm_setup(project_root)?,
    };
    let ai_provider = manifest
        .ai
        .default
        .as_ref()
        .map(|config| config.resolved_config().map(AiProvider::new))
        .transpose()?;
    let vm_setup = compose_ai_vm_setup(vm_setup, ai_provider);
    let controllers = build_controller_registry(project_root, &routes, vm_setup)?;

    Ok(AppRuntime {
        root: project_root.to_path_buf(),
        escape: manifest.web.views.escape,
        config: manifest_config(&manifest),
        session_signing_key: manifest
            .web
            .session
            .resolved_signing_secret()?
            .map(SessionSigningKey::new),
        session_encryption_key: manifest
            .web
            .session
            .resolved_encryption_secret()?
            .map(SessionEncryptionKey::new),
        routes,
        controllers,
    })
}

fn build_watched_app_from_runtime_builder(
    project_root: PathBuf,
    builder: RuntimeBuilder,
    trace_sink: Option<WatchTraceSink>,
) -> Result<Router> {
    let runtime = Arc::new(builder()?);
    let signature = project_signature(&project_root)?;
    let watched = WatchedRuntime {
        root: project_root,
        current: RwLock::new(runtime),
        signature: Mutex::new(signature),
        builder,
        trace_sink,
    };

    let state = AppState {
        runtime: RuntimeSource::Watched(Arc::new(watched)),
        revisions: RevisionManager::default(),
    };

    Ok(Router::new()
        .fallback(any(render_watched_route))
        .with_state(state))
}

fn build_static_router(runtime: Arc<AppRuntime>) -> Result<Router> {
    let routes = runtime.routes.clone();
    let state = AppState {
        runtime: RuntimeSource::Static(runtime),
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
                              headers: HeaderMap,
                              uri: Uri,
                              path_params: Option<AxumPath<HashMap<String, String>>>,
                              AxumQuery(query_params): AxumQuery<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let request_headers = headers_to_map(&headers);
                        let request_path = uri.path().to_string();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                WebRequest {
                                    method: Method::GET.to_string(),
                                    path: request_path,
                                    headers: request_headers,
                                    path_params,
                                    query_params,
                                    ..WebRequest::default()
                                },
                            )
                            .await
                        }
                    }),
                );
            }
            "DELETE" => {
                app = app.route(
                    &route.path,
                    delete(move |State(state): State<AppState>,
                                 headers: HeaderMap,
                                 uri: Uri,
                                 path_params: Option<AxumPath<HashMap<String, String>>>,
                                 AxumQuery(query_params): AxumQuery<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let request_headers = headers_to_map(&headers);
                        let request_path = uri.path().to_string();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                WebRequest {
                                    method: Method::DELETE.to_string(),
                                    path: request_path,
                                    headers: request_headers,
                                    path_params,
                                    query_params,
                                    ..WebRequest::default()
                                },
                            )
                            .await
                        }
                    }),
                );
            }
            "PUT" => {
                app = app.route(
                    &route.path,
                    put(move |State(state): State<AppState>,
                              headers: HeaderMap,
                              uri: Uri,
                              path_params: Option<AxumPath<HashMap<String, String>>>,
                              AxumQuery(query_params): AxumQuery<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let request_headers = headers_to_map(&headers);
                        let request_path = uri.path().to_string();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                WebRequest {
                                    method: Method::PUT.to_string(),
                                    path: request_path,
                                    headers: request_headers,
                                    path_params,
                                    query_params,
                                    ..WebRequest::default()
                                },
                            )
                            .await
                        }
                    }),
                );
            }
            "PATCH" => {
                app = app.route(
                    &route.path,
                    patch(move |State(state): State<AppState>,
                                headers: HeaderMap,
                                uri: Uri,
                                path_params: Option<AxumPath<HashMap<String, String>>>,
                                AxumQuery(query_params): AxumQuery<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let request_headers = headers_to_map(&headers);
                        let request_path = uri.path().to_string();
                        let path_params = path_params
                            .map(|AxumPath(params)| params.into_iter().collect::<BTreeMap<_, _>>())
                            .unwrap_or_default();
                        let query_params = query_params.into_iter().collect::<BTreeMap<_, _>>();
                        async move {
                            render_route(
                                state,
                                controller,
                                action,
                                WebRequest {
                                    method: Method::PATCH.to_string(),
                                    path: request_path,
                                    headers: request_headers,
                                    path_params,
                                    query_params,
                                    ..WebRequest::default()
                                },
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
                               headers: HeaderMap,
                               uri: Uri,
                               path_params: Option<AxumPath<HashMap<String, String>>>,
                               AxumQuery(query_params): AxumQuery<HashMap<String, String>>,
                               AxumForm(form_params): AxumForm<HashMap<String, String>>| {
                        let controller = controller.clone();
                        let action = action.clone();
                        let request_headers = headers_to_map(&headers);
                        let request_path = uri.path().to_string();
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
                                WebRequest {
                                    method: Method::POST.to_string(),
                                    path: request_path,
                                    headers: request_headers,
                                    path_params,
                                    query_params,
                                    form_params,
                                },
                            )
                            .await
                        }
                    }),
                );
            }
            _ => bail!(
                "unsupported HTTP method {} for {}",
                route.method,
                route.path
            ),
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

fn manifest_config(manifest: &Manifest) -> BTreeMap<String, Value> {
    let mut config = BTreeMap::new();
    config.insert(
        "package".to_string(),
        Value::Map(
            BTreeMap::from([(
                "name".to_string(),
                Value::String(manifest.package.name.clone()),
            )])
            .into(),
        ),
    );
    config.insert(
        "web".to_string(),
        Value::Map(
            BTreeMap::from([
                ("mode".to_string(), Value::String(manifest.web.mode.clone())),
                (
                    "routes".to_string(),
                    Value::String(manifest.web.routes.clone()),
                ),
                (
                    "views".to_string(),
                    Value::Map(
                        BTreeMap::from([(
                            "escape".to_string(),
                            Value::String(match manifest.web.views.escape {
                                EscapeMode::Html => "html".to_string(),
                                EscapeMode::None => "none".to_string(),
                            }),
                        )])
                        .into(),
                    ),
                ),
            ])
            .into(),
        ),
    );

    if let Some(database) = &manifest.database.default {
        config.insert(
            "database".to_string(),
            Value::Map(
                BTreeMap::from([(
                    "adapter".to_string(),
                    Value::String(database.adapter.clone()),
                )])
                .into(),
            ),
        );
    }

    config
}

fn project_signature(project_root: &Path) -> Result<ProjectSignature> {
    let mut files = BTreeMap::new();
    let manifest_path = project_root.join("ricochet.toml");
    if manifest_path.exists() {
        files.insert(
            PathBuf::from("ricochet.toml"),
            file_signature(&manifest_path)?,
        );
    }

    collect_watch_files(project_root, &project_root.join("app"), &mut files)?;
    collect_watch_files(project_root, &project_root.join("config"), &mut files)?;

    Ok(ProjectSignature { files })
}

fn collect_watch_files(
    project_root: &Path,
    dir: &Path,
    files: &mut BTreeMap<PathBuf, FileSignature>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();

    for path in entries {
        if path.is_dir() {
            collect_watch_files(project_root, &path, files)?;
        } else if is_watched_file(&path) {
            let relative_path = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .to_path_buf();
            files.insert(relative_path, file_signature(&path)?);
        }
    }

    Ok(())
}

fn is_watched_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("rco" | "html" | "htm")
    )
}

fn file_signature(path: &Path) -> Result<FileSignature> {
    let contents = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(FileSignature {
        len: contents.len() as u64,
        hash: hasher.finish(),
    })
}

fn changed_signature_paths(before: &ProjectSignature, after: &ProjectSignature) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    paths.extend(before.files.keys().cloned());
    paths.extend(after.files.keys().cloned());
    paths
        .into_iter()
        .filter(|path| before.files.get(path) != after.files.get(path))
        .collect()
}

fn stdout_watch_trace_sink() -> WatchTraceSink {
    Arc::new(print_watch_trace_event)
}

fn print_watch_trace_event(event: &WatchTraceEvent) {
    match event {
        WatchTraceEvent::Reloaded {
            revision,
            changed_files,
        } => {
            println!(
                "TRACE watch reload revision={revision} changed={}",
                changed_files.len()
            );
            for path in changed_files {
                println!("  changed: {}", path.display());
            }
        }
        WatchTraceEvent::ReloadFailed {
            changed_files,
            message,
        } => {
            println!(
                "FAULT watch reload changed={} {message}",
                changed_files.len()
            );
            for path in changed_files {
                println!("  changed: {}", path.display());
            }
        }
    }
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
        let chunk = compile_file_with_imports(&controller_path)
            .with_context(|| format!("failed to compile {}", controller_path.display()))?;
        registry.register_ricochet_chunk(&controller, &action, chunk);
    }

    Ok(registry)
}

fn database_vm_setup(project_root: &Path, backend: Arc<dyn DatabaseBackend>) -> Result<VmSetup> {
    let (model_chunks, mappings) = load_model_runtime(project_root)?;
    let model_chunks = Arc::new(model_chunks);

    Ok(Arc::new(move |vm| {
        for chunk in model_chunks.iter() {
            vm.run_chunk(chunk)?;
        }
        let database = install_database_capability(vm, backend.clone(), mappings.clone())?;
        Ok(BTreeMap::from([("db".to_string(), database)]))
    }))
}

fn compose_ai_vm_setup(
    vm_setup: Option<VmSetup>,
    ai_provider: Option<AiProvider>,
) -> Option<VmSetup> {
    if vm_setup.is_none() && ai_provider.is_none() {
        return None;
    }

    Some(Arc::new(move |vm| {
        let mut capabilities = match &vm_setup {
            Some(setup) => setup(vm)?,
            None => BTreeMap::new(),
        };
        if let Some(provider) = &ai_provider {
            let ai = install_ai_capability(vm, provider.clone())?;
            capabilities.insert("ai".to_string(), ai);
        }
        Ok(capabilities)
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

fn load_model_runtime(project_root: &Path) -> Result<(Vec<Chunk>, BTreeMap<String, ModelMapping>)> {
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
            let chunk = compile_file_with_imports(&path)
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
    request: WebRequest,
) -> impl IntoResponse {
    let (runtime, revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    match render_action(&runtime, revision, &controller, &action, request) {
        Ok(action) => action.into_response().unwrap_or_else(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("ricochet MVC error: {err:#}"),
            )
                .into_response()
        }),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ricochet MVC error: {err:#}"),
        )
            .into_response(),
    }
}

async fn render_watched_route(
    State(state): State<AppState>,
    request: Request<Body>,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();
    let method = parts.method;
    let path = parts.uri.path().to_string();
    let headers = headers_to_map(&parts.headers);
    let query_params = parse_urlencoded_params(parts.uri.query().unwrap_or(""));
    let form_params = match form_params_from_body(&method, &parts.headers, body).await {
        Ok(params) => params,
        Err(err) => return mvc_error_response(err),
    };

    let (runtime, revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    let Some((route, path_params)) = matching_route(&runtime.routes, method.as_str(), &path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match render_action(
        &runtime,
        revision,
        &route.controller,
        &route.action,
        WebRequest {
            method: method.to_string(),
            path,
            headers,
            path_params,
            query_params,
            form_params,
        },
    ) {
        Ok(action) => action.into_response().unwrap_or_else(mvc_error_response),
        Err(err) => mvc_error_response(err),
    }
}

fn mvc_error_response(err: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("ricochet MVC error: {err:#}"),
    )
        .into_response()
}

fn render_action(
    runtime: &AppRuntime,
    revision: AppRevision,
    controller: &str,
    action: &str,
    request: WebRequest,
) -> Result<RenderedAction> {
    let cookies = cookies_from_headers(&request.headers);
    let initial_session = session_from_cookies(
        &cookies,
        runtime.session_signing_key.as_ref(),
        runtime.session_encryption_key.as_ref(),
    );
    let mut ctx = RequestContext {
        method: request.method,
        path: request.path,
        params: request.path_params,
        query: request.query_params,
        form: request.form_params,
        headers: request.headers,
        cookies,
        session: initial_session.clone(),
        config: runtime.config.clone(),
        ..RequestContext::default()
    };
    ctx.view_data
        .insert("revision".to_string(), Value::Number(revision.id as i64));

    let action = runtime.controllers.call(controller, action, &mut ctx)?;
    let mut rendered = match action {
        ActionResult::View(view) => RenderedAction::Html {
            body: render_view(runtime, &view, &ctx)?,
            status: None,
            headers: BTreeMap::new(),
        },
        ActionResult::Text(body) => RenderedAction::Text {
            body,
            status: None,
            headers: BTreeMap::new(),
        },
        ActionResult::Json(body) => RenderedAction::Json {
            body,
            status: None,
            headers: BTreeMap::new(),
        },
        ActionResult::ViewResponse {
            view,
            status,
            headers,
        } => RenderedAction::Html {
            body: render_view(runtime, &view, &ctx)?,
            status,
            headers,
        },
        ActionResult::TextResponse {
            body,
            status,
            headers,
        } => RenderedAction::Text {
            body,
            status,
            headers,
        },
        ActionResult::JsonResponse {
            body,
            status,
            headers,
        } => RenderedAction::Json {
            body,
            status,
            headers,
        },
        ActionResult::Redirect {
            location,
            status,
            headers,
        } => RenderedAction::Redirect {
            location,
            status,
            headers,
        },
    };

    if ctx.session != initial_session {
        rendered.insert_header(
            "set-cookie",
            session_cookie_header(
                &ctx.session,
                runtime.session_signing_key.as_ref(),
                runtime.session_encryption_key.as_ref(),
            )?,
        );
    }

    Ok(rendered)
}

impl RenderedAction {
    fn insert_header(&mut self, name: impl Into<String>, value: impl Into<String>) {
        match self {
            RenderedAction::Html { headers, .. }
            | RenderedAction::Text { headers, .. }
            | RenderedAction::Json { headers, .. }
            | RenderedAction::Redirect { headers, .. } => {
                headers.insert(name.into(), value.into());
            }
        }
    }

    fn into_response(self) -> Result<Response> {
        match self {
            RenderedAction::Html {
                body,
                status,
                headers,
            } => response_with_meta(Html(body), status, headers),
            RenderedAction::Text {
                body,
                status,
                headers,
            } => response_with_meta(body, status, headers),
            RenderedAction::Json {
                body,
                status,
                mut headers,
            } => {
                headers
                    .entry(header::CONTENT_TYPE.to_string())
                    .or_insert_with(|| "application/json".to_string());
                response_with_meta(body, status, headers)
            }
            RenderedAction::Redirect {
                location,
                status,
                mut headers,
            } => {
                headers.insert(header::LOCATION.to_string(), location);
                response_with_meta(
                    "",
                    Some(status.unwrap_or(StatusCode::FOUND.as_u16())),
                    headers,
                )
            }
        }
    }
}

fn response_with_meta(
    response: impl IntoResponse,
    status: Option<u16>,
    headers: BTreeMap<String, String>,
) -> Result<Response> {
    let mut response = response.into_response();
    if let Some(status) = status {
        *response.status_mut() = StatusCode::from_u16(status)
            .with_context(|| format!("invalid HTTP status {status}"))?;
    }
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid response header name {name:?}"))?;
        let value = HeaderValue::from_str(&value)
            .with_context(|| format!("invalid response header value for {name}"))?;
        response.headers_mut().insert(name, value);
    }
    Ok(response)
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

fn matching_route<'a>(
    routes: &'a [Route],
    method: &str,
    path: &str,
) -> Option<(&'a Route, BTreeMap<String, String>)> {
    routes.iter().find_map(|route| {
        if !route.method.eq_ignore_ascii_case(method) {
            return None;
        }
        route_path_params(&route.path, path).map(|params| (route, params))
    })
}

fn route_path_params(route_path: &str, request_path: &str) -> Option<BTreeMap<String, String>> {
    let route_segments = path_segments(route_path);
    let request_segments = path_segments(request_path);
    if route_segments.len() != request_segments.len() {
        return None;
    }

    let mut params = BTreeMap::new();
    for (route_segment, request_segment) in route_segments.iter().zip(request_segments) {
        if let Some(name) = route_segment.strip_prefix(':') {
            params.insert(name.to_string(), request_segment.to_string());
        } else if route_segment != &request_segment {
            return None;
        }
    }

    Some(params)
}

fn path_segments(path: &str) -> Vec<&str> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    }
}

fn headers_to_map(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect()
}

fn cookies_from_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .get("cookie")
        .map(|cookie| parse_cookie_header(cookie))
        .unwrap_or_default()
}

fn parse_cookie_header(header: &str) -> BTreeMap<String, String> {
    header
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            if cookie.is_empty() {
                return None;
            }
            let (name, value) = cookie.split_once('=')?;
            Some((
                name.trim().to_string(),
                decode_urlencoded_component(value.trim()),
            ))
        })
        .collect()
}

fn session_from_cookies(
    cookies: &BTreeMap<String, String>,
    signing_key: Option<&SessionSigningKey>,
    encryption_key: Option<&SessionEncryptionKey>,
) -> BTreeMap<String, Value> {
    let Some(raw_session) = cookies.get(SESSION_COOKIE_NAME) else {
        return BTreeMap::new();
    };
    let Some(session_json) = session_json_from_cookie(raw_session, signing_key, encryption_key)
    else {
        return BTreeMap::new();
    };
    let Ok(JsonValue::Object(values)) = serde_json::from_str::<JsonValue>(&session_json) else {
        return BTreeMap::new();
    };

    values
        .into_iter()
        .filter_map(|(key, value)| json_to_session_value(value).map(|value| (key, value)))
        .collect()
}

fn json_to_session_value(value: JsonValue) -> Option<Value> {
    match value {
        JsonValue::Null => Some(Value::Nil),
        JsonValue::Bool(value) => Some(Value::Bool(value)),
        JsonValue::Number(value) => value.as_i64().map(Value::Number),
        JsonValue::String(value) => Some(Value::String(value)),
        JsonValue::Array(values) => values
            .into_iter()
            .map(json_to_session_value)
            .collect::<Option<Vec<_>>>()
            .map(|values| Value::Array(values.into())),
        JsonValue::Object(values) => values
            .into_iter()
            .map(|(key, value)| json_to_session_value(value).map(|value| (key, value)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(|values| Value::Map(values.into())),
    }
}

fn session_cookie_header(
    session: &BTreeMap<String, Value>,
    signing_key: Option<&SessionSigningKey>,
    encryption_key: Option<&SessionEncryptionKey>,
) -> Result<String> {
    if session.is_empty() {
        return Ok(format!(
            "{SESSION_COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"
        ));
    }

    let json = JsonValue::Object(
        session
            .iter()
            .map(|(key, value)| Ok((key.clone(), session_value_to_json(value)?)))
            .collect::<Result<serde_json::Map<_, _>>>()?,
    );
    let session_json = serde_json::to_string(&json)?;
    let cookie_value = match encryption_key {
        Some(encryption_key) => encrypted_session_cookie_value(&session_json, encryption_key)?,
        None => match signing_key {
            Some(signing_key) => signed_session_cookie_value(&session_json, signing_key),
            None => session_json,
        },
    };
    let encoded = encode_urlencoded_component(&cookie_value);
    Ok(format!(
        "{SESSION_COOKIE_NAME}={encoded}; Path=/; HttpOnly; SameSite=Lax"
    ))
}

fn session_json_from_cookie(
    cookie_value: &str,
    signing_key: Option<&SessionSigningKey>,
    encryption_key: Option<&SessionEncryptionKey>,
) -> Option<String> {
    if let Some(encryption_key) = encryption_key {
        if let Some(session_json) = decrypted_session_json(cookie_value, encryption_key) {
            return Some(session_json);
        }
        return signing_key
            .and_then(|signing_key| verified_signed_session_json(cookie_value, signing_key));
    }

    match signing_key {
        Some(signing_key) => verified_signed_session_json(cookie_value, signing_key),
        None => Some(cookie_value.to_string()),
    }
}

fn encrypted_session_cookie_value(
    session_json: &str,
    encryption_key: &SessionEncryptionKey,
) -> Result<String> {
    encryption_key.encrypt(session_json.as_bytes())
}

fn decrypted_session_json(
    cookie_value: &str,
    encryption_key: &SessionEncryptionKey,
) -> Option<String> {
    let mut parts = cookie_value.split(':');
    let prefix = parts.next()?;
    let nonce_hex = parts.next()?;
    let ciphertext_hex = parts.next()?;
    if parts.next().is_some() || prefix != ENCRYPTED_SESSION_PREFIX {
        return None;
    }
    let plaintext = encryption_key.decrypt(nonce_hex, ciphertext_hex)?;
    String::from_utf8(plaintext).ok()
}

fn signed_session_cookie_value(session_json: &str, signing_key: &SessionSigningKey) -> String {
    let payload_hex = hex_encode(session_json.as_bytes());
    let signature = signing_key.sign(session_json.as_bytes());
    format!("{SIGNED_SESSION_PREFIX}:{payload_hex}:{signature}")
}

fn verified_signed_session_json(
    cookie_value: &str,
    signing_key: &SessionSigningKey,
) -> Option<String> {
    let mut parts = cookie_value.split(':');
    let prefix = parts.next()?;
    let payload_hex = parts.next()?;
    let signature_hex = parts.next()?;
    if parts.next().is_some() || prefix != SIGNED_SESSION_PREFIX {
        return None;
    }
    let payload = hex_decode(payload_hex).ok()?;
    if !signing_key.verify(&payload, signature_hex) {
        return None;
    }
    String::from_utf8(payload).ok()
}

fn session_value_to_json(value: &Value) -> Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(*value)),
        Value::Number(value) => Ok(JsonValue::Number((*value).into())),
        Value::String(value) => Ok(JsonValue::String(value.clone())),
        Value::Array(values) => values
            .snapshot()
            .iter()
            .map(session_value_to_json)
            .collect::<Result<Vec<_>>>()
            .map(JsonValue::Array),
        Value::List(values) => values
            .snapshot()
            .iter()
            .map(session_value_to_json)
            .collect::<Result<Vec<_>>>()
            .map(JsonValue::Array),
        Value::Map(values) => values
            .snapshot()
            .iter()
            .map(|(key, value)| Ok((key.clone(), session_value_to_json(value)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(JsonValue::Object),
        value => bail!("session values must be JSON-serializable, got {value:?}"),
    }
}

async fn form_params_from_body(
    method: &Method,
    headers: &axum::http::HeaderMap,
    body: Body,
) -> Result<BTreeMap<String, String>> {
    if !matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) {
        return Ok(BTreeMap::new());
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !content_type.starts_with("application/x-www-form-urlencoded") {
        return Ok(BTreeMap::new());
    }

    let body = to_bytes(body, WATCHED_FORM_BODY_LIMIT)
        .await
        .map_err(|err| anyhow!("failed to read request body: {err}"))?;
    let body = std::str::from_utf8(&body).context("form body is not valid UTF-8")?;
    Ok(parse_urlencoded_params(body))
}

fn parse_urlencoded_params(source: &str) -> BTreeMap<String, String> {
    source
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (
                decode_urlencoded_component(key),
                decode_urlencoded_component(value),
            )
        })
        .collect()
}

fn decode_urlencoded_component(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let high = hex_value(bytes[index + 1]);
                let low = hex_value(bytes[index + 2]);
                if let (Some(high), Some(low)) = (high, low) {
                    decoded.push((high << 4) | low);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn encode_urlencoded_component(source: &str) -> String {
    let mut encoded = String::new();
    for byte in source.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn hex_decode(source: &str) -> Result<Vec<u8>, ()> {
    let bytes = source.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(());
    }

    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let high = hex_value(chunk[0]).ok_or(())?;
        let low = hex_value(chunk[1]).ok_or(())?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub async fn serve_current_dir(options: ServeOptions) -> Result<()> {
    options.validate()?;
    let project_root = Path::new(".");
    let manifest = load_manifest(project_root)?;
    let watch_trace_sink = (options.watch && options.debug).then(stdout_watch_trace_sink);
    let app = match (options.watch, manifest.database.default) {
        (true, Some(database)) => {
            let backend = connect_database_backend(&database).await?;
            if let Some(trace_sink) = watch_trace_sink.clone() {
                build_watched_app_from_dir_with_database_and_trace(
                    project_root,
                    backend,
                    trace_sink,
                )?
            } else {
                build_watched_app_from_dir_with_database(project_root, backend)?
            }
        }
        (true, None) => {
            if let Some(trace_sink) = watch_trace_sink.clone() {
                build_watched_app_from_dir_with_trace(project_root, trace_sink)?
            } else {
                build_watched_app_from_dir(project_root)?
            }
        }
        (false, Some(database)) => {
            let backend = connect_database_backend(&database).await?;
            build_app_from_dir_with_database(project_root, backend)?
        }
        (false, None) => build_app_from_dir(project_root)?,
    };
    let bind_addr = options.bind_addr();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    println!(
        "Ricochet web server listening on http://{bind_addr} debug={} watch={}",
        options.debug, options.watch
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn connect_database_backend(database: &DatabaseDefault) -> Result<Arc<dyn DatabaseBackend>> {
    let adapter = database.adapter.trim().to_ascii_lowercase();
    let url = database.resolved_url()?;

    match adapter.as_str() {
        "postgres" | "postgresql" => Ok(Arc::new(PostgresDatabase::connect(&url).await?)),
        "mysql" | "mariadb" => Ok(Arc::new(MysqlDatabase::connect(&url).await?)),
        "sqlite" => Ok(Arc::new(SqliteDatabase::connect(&url)?)),
        _ => bail!(
            "unsupported database adapter {}; expected postgres, sqlite, or mysql",
            database.adapter
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_build_test_app_returns_ok() {
        let _ = build_test_app().expect("server test app should build");
    }

    #[tokio::test]
    async fn database_backend_dispatch_accepts_sqlite_adapter() {
        let backend = connect_database_backend(&DatabaseDefault {
            adapter: "sqlite".to_string(),
            url: ":memory:".to_string(),
        })
        .await
        .expect("sqlite adapter should connect");
        let mapping = ModelMapping::try_new("User", "users", ["id"]).expect("mapping is valid");

        let error = backend
            .count(&mapping)
            .expect_err("empty in-memory database has no users table");

        assert!(error.to_string().contains("no such table"));
    }

    #[test]
    fn serve_options_build_configured_socket_addr() {
        let options = ServeOptions {
            host: "0.0.0.0".to_string(),
            port: 4100,
            debug: true,
            watch: true,
        };

        assert_eq!(options.bind_addr(), "0.0.0.0:4100");
    }

    #[test]
    fn serve_options_accept_watch_for_hot_reload() {
        let options = ServeOptions {
            watch: true,
            ..ServeOptions::default()
        };

        options
            .validate()
            .expect("watch should be a valid serve option");
    }
}
