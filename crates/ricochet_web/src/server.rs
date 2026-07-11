use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{DefaultBodyLimit, Path as AxumPath, Query as AxumQuery, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{any, delete, get, patch, post, put},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hmac::{Hmac, Mac};
use ricochet_bytecode::Chunk;
use ricochet_compiler::{
    compile_file_with_imports, resolve_import_with_metadata, verify_runtime_import_locks_for_parent,
};
use ricochet_vm::{
    ApprovalRegistry, Capability, DynamicModuleSource, ProcessRegistry, PtyRegistry,
    UploadStreamMetadata, UploadStreamRegistry, Value, Vm,
};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::active_record::{ModelMapping, MysqlDatabase, PostgresDatabase, SqliteDatabase};
use crate::ai_capability::{install_ai_capability, AiProvider, AiProviderConfig};
use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::database_capability::{install_database_capability, DatabaseBackend};
use crate::manifest::{
    AiDefault, DatabaseDefault, Manifest, SessionSecure, StaticFiles, Uploads, WebCapabilities,
};
use crate::revision::{AppRevision, RevisionManager};
use crate::router::{parse_routes, Route};
use crate::template::{render_template, EscapeMode};
use crate::value_json::{value_to_json, SetMode};

#[derive(Clone)]
struct AppState {
    runtime: RuntimeSource,
    revisions: RevisionManager,
    request_fault_sink: Option<RequestFaultSink>,
}

struct AppRuntime {
    root: PathBuf,
    escape: EscapeMode,
    static_files: StaticFileConfig,
    uploads: Uploads,
    config: BTreeMap<String, Value>,
    session_signing_key: Option<SessionSigningKey>,
    session_encryption_key: Option<SessionEncryptionKey>,
    session_secure: SessionSecure,
    session_auto_allows_insecure_loopback: bool,
    routes: Vec<Route>,
    controllers: ControllerRegistry,
}

#[derive(Debug, Clone)]
struct StaticFileConfig {
    mount: String,
    dir: PathBuf,
}

#[derive(Default)]
struct WebRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    path_params: BTreeMap<String, String>,
    query_params: BTreeMap<String, String>,
    form_params: BTreeMap<String, String>,
    body: Option<Value>,
    json: Option<Value>,
    uploads: BTreeMap<String, Value>,
    files: Vec<Value>,
    upload_streams: UploadStreamRegistry,
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
pub type RequestFaultSink = Arc<dyn Fn(&RequestFaultPause) + Send + Sync>;
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

    fn random() -> Result<Self> {
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret)
            .map_err(|error| anyhow!("failed to generate session signing key: {error}"))?;
        Ok(Self {
            secret: Arc::from(secret.to_vec()),
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFaultStage {
    Action,
    View,
    Response,
}

impl RequestFaultStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::View => "view",
            Self::Response => "response",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFaultPause {
    pub method: String,
    pub path: String,
    pub controller: String,
    pub action: String,
    pub revision: u64,
    pub stage: RequestFaultStage,
    pub message: String,
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
    pub allow_env: bool,
    pub env_allow: Vec<String>,
    pub allow_process: bool,
    pub process_root: Option<PathBuf>,
    pub allow_pty: bool,
    pub fs_root: Option<PathBuf>,
    pub fs_readonly: bool,
    pub http_allow_hosts: Vec<String>,
    pub ai_allow_hosts: Vec<String>,
    pub database_allow_hosts: Vec<String>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            debug: false,
            watch: false,
            allow_env: false,
            env_allow: Vec::new(),
            allow_process: false,
            process_root: None,
            allow_pty: false,
            fs_root: None,
            fs_readonly: false,
            http_allow_hosts: Vec::new(),
            ai_allow_hosts: Vec::new(),
            database_allow_hosts: Vec::new(),
        }
    }
}

impl ServeOptions {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate(&self) -> Result<()> {
        if self.fs_readonly && self.fs_root.is_none() {
            bail!("--fs-readonly requires --fs-root");
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct ServeCapabilityState {
    process_registry: ProcessRegistry,
    pty_registry: PtyRegistry,
    approval_registry: ApprovalRegistry,
}

pub fn build_test_app() -> Result<Router> {
    let fixture_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");
    build_app_from_dir(fixture_root)
}

pub fn build_app_from_dir(project_root: impl AsRef<Path>) -> Result<Router> {
    crate::ServeBuilder::new(project_root).build()
}

pub fn build_app_from_dir_with_options(
    project_root: impl AsRef<Path>,
    options: &ServeOptions,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .options(options.clone())
        .build()
}

pub fn build_app_from_dir_with_options_and_request_fault_sink(
    project_root: impl AsRef<Path>,
    options: &ServeOptions,
    request_fault_sink: Option<RequestFaultSink>,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .options(options.clone())
        .request_fault_sink_option(request_fault_sink)
        .build()
}

pub fn build_app_from_dir_with_database(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .database_backend(backend)
        .build()
}

pub fn build_watched_app_from_dir(project_root: impl AsRef<Path>) -> Result<Router> {
    crate::ServeBuilder::new(project_root).watched(true).build()
}

pub fn build_watched_app_from_dir_with_options(
    project_root: impl AsRef<Path>,
    options: &ServeOptions,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .options(options.clone())
        .watched(true)
        .build()
}

pub fn build_watched_app_from_dir_with_options_and_request_fault_sink(
    project_root: impl AsRef<Path>,
    options: &ServeOptions,
    request_fault_sink: Option<RequestFaultSink>,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .options(options.clone())
        .watched(true)
        .request_fault_sink_option(request_fault_sink)
        .build()
}

pub fn build_watched_app_from_dir_with_trace(
    project_root: impl AsRef<Path>,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .watched(true)
        .trace_sink(trace_sink)
        .build()
}

pub fn build_watched_app_from_dir_with_options_and_trace(
    project_root: impl AsRef<Path>,
    options: &ServeOptions,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .options(options.clone())
        .watched(true)
        .trace_sink(trace_sink)
        .build()
}

pub fn build_watched_app_from_dir_with_database(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .database_backend(backend)
        .watched(true)
        .build()
}

pub fn build_watched_app_from_dir_with_database_and_options(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
    options: &ServeOptions,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .database_backend(backend)
        .options(options.clone())
        .watched(true)
        .build()
}

pub fn build_watched_app_from_dir_with_database_and_trace(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .database_backend(backend)
        .watched(true)
        .trace_sink(trace_sink)
        .build()
}

pub fn build_watched_app_from_dir_with_database_options_and_trace(
    project_root: impl AsRef<Path>,
    backend: Arc<dyn DatabaseBackend>,
    options: &ServeOptions,
    trace_sink: WatchTraceSink,
) -> Result<Router> {
    crate::ServeBuilder::new(project_root)
        .database_backend(backend)
        .options(options.clone())
        .watched(true)
        .trace_sink(trace_sink)
        .build()
}

pub fn build_app_from_serve_builder(builder: crate::ServeBuilder) -> Result<Router> {
    let parts = builder.into_parts();
    if let Some(options) = &parts.options {
        options.validate()?;
    }

    if parts.watched {
        return build_watched_app_from_serve_builder_parts(parts);
    }

    let vm_setup = match parts.database_backend {
        Some(backend) => Some(database_vm_setup(&parts.project_root, backend)?),
        None => {
            if parts.options.is_some() {
                model_vm_setup(&parts.project_root)?
            } else {
                None
            }
        }
    };
    let vm_setup = match &parts.options {
        Some(options) => compose_serve_capability_vm_setup(&parts.project_root, vm_setup, options)?,
        None => vm_setup,
    };
    build_app_from_dir_internal_with_options_and_request_fault_sink(
        &parts.project_root,
        vm_setup,
        parts.options.as_ref(),
        parts.request_fault_sink,
    )
}

fn build_watched_app_from_serve_builder_parts(
    parts: crate::serve_builder::ServeBuilderParts,
) -> Result<Router> {
    let project_root = parts.project_root;
    let builder_root = project_root.clone();
    let builder_options = parts.options.clone().unwrap_or_default();
    builder_options.validate()?;
    let capability_state = Arc::new(ServeCapabilityState::default());
    let request_fault_sink =
        request_fault_sink_for_options(Some(&builder_options), parts.request_fault_sink);

    let builder: RuntimeBuilder = match parts.database_backend {
        Some(backend) => Arc::new(move || {
            let vm_setup = database_vm_setup(&builder_root, backend.clone())?;
            let vm_setup = compose_serve_capability_vm_setup_with_state(
                &builder_root,
                Some(vm_setup),
                &builder_options,
                capability_state.clone(),
            )?;
            build_runtime_from_dir_internal_with_options(
                &builder_root,
                vm_setup,
                Some(&builder_options),
            )
        }),
        None => Arc::new(move || {
            let vm_setup = model_vm_setup(&builder_root)?;
            let vm_setup = compose_serve_capability_vm_setup_with_state(
                &builder_root,
                vm_setup,
                &builder_options,
                capability_state.clone(),
            )?;
            build_runtime_from_dir_internal_with_options(
                &builder_root,
                vm_setup,
                Some(&builder_options),
            )
        }),
    };

    build_watched_app_from_runtime_builder(
        project_root,
        builder,
        parts.trace_sink,
        request_fault_sink,
    )
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

fn build_app_from_dir_internal_with_options_and_request_fault_sink(
    project_root: &Path,
    vm_setup: Option<VmSetup>,
    options: Option<&ServeOptions>,
    request_fault_sink: Option<RequestFaultSink>,
) -> Result<Router> {
    let runtime = Arc::new(build_runtime_from_dir_internal_with_options(
        project_root,
        vm_setup,
        options,
    )?);
    build_static_router(
        runtime,
        request_fault_sink_for_options(options, request_fault_sink),
    )
}

fn build_runtime_from_dir_internal_with_options(
    project_root: &Path,
    vm_setup: Option<VmSetup>,
    options: Option<&ServeOptions>,
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
        .map(|config| resolve_ai_provider_config(config, options).map(AiProvider::new))
        .transpose()?;
    let vm_setup = compose_ai_vm_setup(vm_setup, ai_provider);
    let controllers = build_controller_registry(project_root, &routes, vm_setup)?;

    let session_encryption_key = manifest
        .web
        .session
        .resolved_encryption_secret()?
        .map(SessionEncryptionKey::new);
    let configured_signing_key = manifest
        .web
        .session
        .resolved_signing_secret()?
        .map(SessionSigningKey::new);
    let session_signing_key = match (configured_signing_key, session_encryption_key.is_some()) {
        (Some(signing_key), _) => Some(signing_key),
        (None, true) => None,
        (None, false) => Some(SessionSigningKey::random()?),
    };
    validate_upload_config(&manifest.web.uploads)?;
    let static_files = static_file_config(project_root, &manifest.web.static_files)?;

    Ok(AppRuntime {
        root: project_root.to_path_buf(),
        escape: manifest.web.views.escape,
        static_files,
        uploads: manifest.web.uploads.clone(),
        config: manifest_config(&manifest),
        session_signing_key,
        session_encryption_key,
        session_secure: manifest.web.session.secure,
        session_auto_allows_insecure_loopback: options
            .map(|options| host_is_local(&options.host))
            .unwrap_or(false),
        routes,
        controllers,
    })
}

fn build_watched_app_from_runtime_builder(
    project_root: PathBuf,
    builder: RuntimeBuilder,
    trace_sink: Option<WatchTraceSink>,
    request_fault_sink: Option<RequestFaultSink>,
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
        request_fault_sink,
    };

    Ok(Router::new()
        .fallback(any(render_watched_route))
        .layer(DefaultBodyLimit::disable())
        .with_state(state))
}

fn build_static_router(
    runtime: Arc<AppRuntime>,
    request_fault_sink: Option<RequestFaultSink>,
) -> Result<Router> {
    let routes = runtime.routes.clone();
    let state = AppState {
        runtime: RuntimeSource::Static(runtime),
        revisions: RevisionManager::default(),
        request_fault_sink,
    };

    let mut app = Router::new();
    for route in routes {
        let controller = route.controller.clone();
        let action = route.action.clone();
        let route_path = route.path.clone();
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
                    delete(
                        move |State(state): State<AppState>, request: Request<Body>| {
                            let controller = controller.clone();
                            let action = action.clone();
                            let route_path = route_path.clone();
                            async move {
                                render_static_route_request(
                                    state,
                                    controller,
                                    action,
                                    Method::DELETE,
                                    route_path,
                                    request,
                                )
                                .await
                            }
                        },
                    ),
                );
            }
            "PUT" => {
                app = app.route(
                    &route.path,
                    put(
                        move |State(state): State<AppState>, request: Request<Body>| {
                            let controller = controller.clone();
                            let action = action.clone();
                            let route_path = route_path.clone();
                            async move {
                                render_static_route_request(
                                    state,
                                    controller,
                                    action,
                                    Method::PUT,
                                    route_path,
                                    request,
                                )
                                .await
                            }
                        },
                    ),
                );
            }
            "PATCH" => {
                app = app.route(
                    &route.path,
                    patch(
                        move |State(state): State<AppState>, request: Request<Body>| {
                            let controller = controller.clone();
                            let action = action.clone();
                            let route_path = route_path.clone();
                            async move {
                                render_static_route_request(
                                    state,
                                    controller,
                                    action,
                                    Method::PATCH,
                                    route_path,
                                    request,
                                )
                                .await
                            }
                        },
                    ),
                );
            }
            "POST" => {
                app = app.route(
                    &route.path,
                    post(
                        move |State(state): State<AppState>, request: Request<Body>| {
                            let controller = controller.clone();
                            let action = action.clone();
                            let route_path = route_path.clone();
                            async move {
                                render_static_route_request(
                                    state,
                                    controller,
                                    action,
                                    Method::POST,
                                    route_path,
                                    request,
                                )
                                .await
                            }
                        },
                    ),
                );
            }
            _ => bail!(
                "unsupported HTTP method {} for {}",
                route.method,
                route.path
            ),
        }
    }

    Ok(app
        .fallback(any(render_static_asset_or_not_found))
        .layer(DefaultBodyLimit::disable())
        .with_state(state))
}

fn load_manifest(project_root: &Path) -> Result<Manifest> {
    let manifest_path = project_root.join("ricochet.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    toml::from_str(&manifest_source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))
}

fn validate_upload_config(config: &Uploads) -> Result<()> {
    if config.max_request_bytes == 0 {
        bail!("web.uploads.max_request_bytes must be greater than 0");
    }
    if config.max_file_bytes == 0 {
        bail!("web.uploads.max_file_bytes must be greater than 0");
    }
    if config.max_file_bytes > i64::MAX as usize {
        bail!("web.uploads.max_file_bytes must fit in a Ricochet integer");
    }
    if config.memory_threshold_bytes > config.max_file_bytes {
        bail!("web.uploads.memory_threshold_bytes must be less than or equal to max_file_bytes");
    }
    if config.max_retained_streams == 0 {
        bail!("web.uploads.max_retained_streams must be greater than 0");
    }
    Ok(())
}

fn static_file_config(project_root: &Path, config: &StaticFiles) -> Result<StaticFileConfig> {
    validate_static_mount(&config.mount)?;
    let dir = validate_project_relative_static_dir(&config.dir)?;
    Ok(StaticFileConfig {
        mount: config.mount.clone(),
        dir: project_root.join(dir),
    })
}

fn validate_static_mount(mount: &str) -> Result<()> {
    if mount != "/"
        && mount.starts_with('/')
        && !mount.ends_with('/')
        && !mount.contains('\\')
        && !mount.contains("//")
        && mount
            .split('/')
            .skip(1)
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    {
        return Ok(());
    }

    bail!("web.static.mount must be a dedicated absolute path like /assets");
}

fn validate_project_relative_static_dir(dir: &str) -> Result<PathBuf> {
    if dir.trim().is_empty() || dir.contains('\\') {
        bail!("web.static.dir must be a project-relative directory path");
    }
    let path = Path::new(dir);
    if path.is_absolute() {
        bail!("web.static.dir must be project-relative");
    }
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                if segment.to_str().is_none_or(str::is_empty) {
                    bail!("web.static.dir must contain valid UTF-8 path segments");
                }
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                bail!("web.static.dir must not contain traversal components");
            }
        }
    }
    Ok(path.to_path_buf())
}

fn static_asset_response(
    runtime: &AppRuntime,
    method: &Method,
    request_path: &str,
) -> Result<Option<Response>> {
    let Some(relative_path) =
        static_request_relative_path(&runtime.static_files.mount, request_path)
    else {
        return Ok(None);
    };

    if method != Method::GET && method != Method::HEAD {
        return Ok(Some(StatusCode::METHOD_NOT_ALLOWED.into_response()));
    }

    let Some(relative_path) = relative_path else {
        return Ok(Some(StatusCode::NOT_FOUND.into_response()));
    };
    let static_root = match fs::canonicalize(&runtime.static_files.dir) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some(StatusCode::NOT_FOUND.into_response()));
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to resolve {}", runtime.static_files.dir.display())
            })
        }
    };
    let requested_asset_path = runtime.static_files.dir.join(&relative_path);
    let asset_path = match fs::canonicalize(&requested_asset_path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some(StatusCode::NOT_FOUND.into_response()));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to resolve {}", requested_asset_path.display()))
        }
    };
    if !asset_path.starts_with(&static_root) || !asset_path.is_file() {
        return Ok(Some(StatusCode::NOT_FOUND.into_response()));
    }

    let content_type = static_content_type(&asset_path);
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(
            fs::read(&asset_path)
                .with_context(|| format!("failed to read static asset {}", asset_path.display()))?,
        )
    };
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(body)
        .context("failed to build static asset response")?;
    Ok(Some(response))
}

fn static_request_relative_path(mount: &str, request_path: &str) -> Option<Option<PathBuf>> {
    let rest = if request_path == mount {
        ""
    } else {
        request_path
            .strip_prefix(mount)
            .and_then(|rest| rest.strip_prefix('/'))?
    };
    if rest.is_empty() {
        return Some(None);
    }

    let mut path = PathBuf::new();
    for segment in rest.split('/') {
        let Ok(segment) = percent_decode_path_segment(segment) else {
            return Some(None);
        };
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains('\\')
            || segment.contains('/')
            || segment.contains('\0')
        {
            return Some(None);
        }
        path.push(segment);
    }
    Some(Some(path))
}

fn percent_decode_path_segment(segment: &str) -> Result<String> {
    let bytes = segment.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                bail!("invalid percent-encoded path segment");
            }
            let high = hex_value(bytes[index + 1]).context("invalid percent-encoded path")?;
            let low = hex_value(bytes[index + 2]).context("invalid percent-encoded path")?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).context("static asset path is not UTF-8")
}

fn static_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "html" | "htm" => "text/html; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
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
                (
                    "session".to_string(),
                    Value::Map(
                        BTreeMap::from([(
                            "secure".to_string(),
                            Value::String(match manifest.web.session.secure {
                                SessionSecure::Auto => "auto".to_string(),
                                SessionSecure::Always => "always".to_string(),
                                SessionSecure::Never => "never".to_string(),
                            }),
                        )])
                        .into(),
                    ),
                ),
                (
                    "static".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            (
                                "dir".to_string(),
                                Value::String(manifest.web.static_files.dir.clone()),
                            ),
                            (
                                "mount".to_string(),
                                Value::String(manifest.web.static_files.mount.clone()),
                            ),
                        ])
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

fn stdout_request_fault_sink() -> RequestFaultSink {
    Arc::new(print_request_fault_pause)
}

fn request_fault_sink_for_options(
    options: Option<&ServeOptions>,
    request_fault_sink: Option<RequestFaultSink>,
) -> Option<RequestFaultSink> {
    if options.is_some_and(|options| options.debug) {
        request_fault_sink.or_else(|| Some(stdout_request_fault_sink()))
    } else {
        None
    }
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

fn print_request_fault_pause(pause: &RequestFaultPause) {
    println!(
        "FAULT request {} {} {}.{} revision={} stage={} {}",
        pause.method,
        pause.path,
        pause.controller,
        pause.action,
        pause.revision,
        pause.stage.as_str(),
        pause.message
    );
}

fn install_dynamic_module_loader(vm: &mut Vm, parent: PathBuf) {
    vm.set_dynamic_module_loader(move |specifier| {
        dynamic_module_source(&parent, specifier).map_err(|error| format!("{error:#}"))
    });
}

fn dynamic_module_source(parent: &Path, specifier: &str) -> Result<DynamicModuleSource> {
    let resolved = resolve_import_with_metadata(parent, specifier)?;
    verify_runtime_import_locks_for_parent(parent)?;
    let chunk = compile_file_with_imports(&resolved.path)
        .with_context(|| format!("failed to compile dynamic import {specifier:?}"))?;
    let canonical = fs::canonicalize(&resolved.path).with_context(|| {
        format!(
            "failed to resolve dynamic import {}",
            resolved.path.display()
        )
    })?;
    let module_id = path_to_slash(&canonical);
    Ok(DynamicModuleSource::new(
        specifier.to_string(),
        module_id,
        Some(canonical),
        chunk,
    ))
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn build_controller_registry(
    project_root: &Path,
    routes: &[Route],
    vm_setup: Option<VmSetup>,
) -> Result<ControllerRegistry> {
    let mut registry = ControllerRegistry::default();
    let dynamic_import_parent = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve project root {}", project_root.display()))?;
    registry.set_vm_setup(move |vm| {
        install_dynamic_module_loader(vm, dynamic_import_parent.clone());
        match &vm_setup {
            Some(setup) => setup(vm),
            None => Ok(BTreeMap::new()),
        }
    });
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

pub fn install_project_database_runtime(
    vm: &mut Vm,
    project_root: &Path,
    backend: Arc<dyn DatabaseBackend>,
) -> Result<BTreeMap<String, Value>> {
    let (model_chunks, mappings) = load_model_runtime(project_root)?;
    for chunk in model_chunks.iter() {
        vm.run_chunk(chunk)?;
    }
    let database = install_database_capability(vm, backend, mappings)?;
    Ok(BTreeMap::from([("db".to_string(), database)]))
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

fn resolve_ai_provider_config(
    config: &AiDefault,
    options: Option<&ServeOptions>,
) -> Result<AiProviderConfig> {
    let Some(options) = options else {
        return config.resolved_config();
    };
    let allowed_env = allowed_env_set(options);
    let resolved = match allowed_env.as_ref() {
        Some(allowed_env) => config.resolved_config_with_env_policy(Some(allowed_env))?,
        None => config.resolved_config_with_env_policy(None)?,
    };
    ensure_allowed_http_endpoint(
        &resolved.base_url,
        &options.ai_allow_hosts,
        "AI provider",
        "--ai-allow-host",
    )?;
    Ok(resolved)
}

fn allowed_env_set(options: &ServeOptions) -> Option<BTreeSet<String>> {
    if options.allow_env {
        None
    } else {
        Some(options.env_allow.iter().cloned().collect())
    }
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
) -> Response {
    let (runtime, revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    let fault_method = request.method.clone();
    let fault_path = request.path.clone();
    match render_action(&runtime, revision, &controller, &action, request) {
        Ok(action_result) => action_result.into_response().unwrap_or_else(|err| {
            mvc_request_fault_response(
                &state,
                RequestFaultMeta {
                    method: &fault_method,
                    path: &fault_path,
                    controller: &controller,
                    action: &action,
                    revision,
                    stage: RequestFaultStage::Response,
                },
                err,
            )
        }),
        Err(err) => {
            let (stage, error) = err.into_parts();
            mvc_request_fault_response(
                &state,
                RequestFaultMeta {
                    method: &fault_method,
                    path: &fault_path,
                    controller: &controller,
                    action: &action,
                    revision,
                    stage,
                },
                error,
            )
        }
    }
}

async fn render_static_route_request(
    state: AppState,
    controller: String,
    action: String,
    method: Method,
    route_path: String,
    request: Request<Body>,
) -> Response {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let headers = headers_to_map(&parts.headers);
    let query_params = parse_urlencoded_params(parts.uri.query().unwrap_or(""));
    let path_params = route_path_params(&route_path, &path).unwrap_or_default();

    let (runtime, _revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    let request_body =
        match request_body_from_body(&method, &parts.headers, body, &runtime.uploads).await {
            Ok(params) => params,
            Err(err) => return mvc_bad_request_response(err),
        };

    render_route(
        state,
        controller,
        action,
        WebRequest {
            method: method.to_string(),
            path,
            headers,
            path_params,
            query_params,
            form_params: request_body.form_params,
            body: request_body.body,
            json: request_body.json,
            uploads: request_body.uploads,
            files: request_body.files,
            upload_streams: request_body.upload_streams,
        },
    )
    .await
    .into_response()
}

async fn render_static_asset_or_not_found(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Response {
    let (parts, _body) = request.into_parts();
    let (runtime, _revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    match static_asset_response(&runtime, &parts.method, parts.uri.path()) {
        Ok(Some(response)) => response,
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => mvc_error_response(err),
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

    let (runtime, revision) = match state.runtime.snapshot(&state.revisions) {
        Ok(snapshot) => snapshot,
        Err(err) => return mvc_error_response(err),
    };

    match static_asset_response(&runtime, &method, &path) {
        Ok(Some(response)) => return response,
        Ok(None) => {}
        Err(err) => return mvc_error_response(err),
    }

    let request_body =
        match request_body_from_body(&method, &parts.headers, body, &runtime.uploads).await {
            Ok(params) => params,
            Err(err) => return mvc_bad_request_response(err),
        };

    let Some((route, path_params)) = matching_route(&runtime.routes, method.as_str(), &path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let fault_method = method.to_string();
    let fault_path = path.clone();
    match render_action(
        &runtime,
        revision,
        &route.controller,
        &route.action,
        WebRequest {
            method: fault_method.clone(),
            path,
            headers,
            path_params,
            query_params,
            form_params: request_body.form_params,
            body: request_body.body,
            json: request_body.json,
            uploads: request_body.uploads,
            files: request_body.files,
            upload_streams: request_body.upload_streams,
        },
    ) {
        Ok(action_result) => action_result.into_response().unwrap_or_else(|err| {
            mvc_request_fault_response(
                &state,
                RequestFaultMeta {
                    method: &fault_method,
                    path: &fault_path,
                    controller: &route.controller,
                    action: &route.action,
                    revision,
                    stage: RequestFaultStage::Response,
                },
                err,
            )
        }),
        Err(err) => {
            let (stage, error) = err.into_parts();
            mvc_request_fault_response(
                &state,
                RequestFaultMeta {
                    method: &fault_method,
                    path: &fault_path,
                    controller: &route.controller,
                    action: &route.action,
                    revision,
                    stage,
                },
                error,
            )
        }
    }
}

fn mvc_error_response(err: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("ricochet MVC error: {err:#}"),
    )
        .into_response()
}

struct RequestFaultMeta<'a> {
    method: &'a str,
    path: &'a str,
    controller: &'a str,
    action: &'a str,
    revision: AppRevision,
    stage: RequestFaultStage,
}

fn mvc_request_fault_response(
    state: &AppState,
    meta: RequestFaultMeta<'_>,
    err: anyhow::Error,
) -> Response {
    let message = format!("{err:#}");
    if let Some(sink) = &state.request_fault_sink {
        sink(&RequestFaultPause {
            method: meta.method.to_string(),
            path: meta.path.to_string(),
            controller: meta.controller.to_string(),
            action: meta.action.to_string(),
            revision: meta.revision.id,
            stage: meta.stage,
            message: message.clone(),
        });
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("ricochet MVC error: {message}"),
    )
        .into_response()
}

struct RequestFaultError {
    stage: RequestFaultStage,
    error: anyhow::Error,
}

impl RequestFaultError {
    fn new(stage: RequestFaultStage, error: anyhow::Error) -> Self {
        Self { stage, error }
    }

    fn into_parts(self) -> (RequestFaultStage, anyhow::Error) {
        (self.stage, self.error)
    }
}

fn mvc_bad_request_response(err: anyhow::Error) -> Response {
    (
        StatusCode::BAD_REQUEST,
        format!("ricochet request error: {err:#}"),
    )
        .into_response()
}

fn render_action(
    runtime: &AppRuntime,
    revision: AppRevision,
    controller: &str,
    action: &str,
    request: WebRequest,
) -> std::result::Result<RenderedAction, RequestFaultError> {
    let cookies = cookies_from_headers(&request.headers);
    let initial_session = session_from_cookies(
        &cookies,
        runtime.session_signing_key.as_ref(),
        runtime.session_encryption_key.as_ref(),
    );
    let initial_session_json = session_json_value(&initial_session)
        .map_err(|error| RequestFaultError::new(RequestFaultStage::Response, error))?;
    let mut ctx = RequestContext {
        method: request.method,
        path: request.path,
        params: request.path_params,
        query: request.query_params,
        form: request.form_params,
        body: request.body,
        json: request.json,
        uploads: request.uploads,
        files: request.files,
        upload_streams: request.upload_streams,
        headers: request.headers,
        cookies,
        session: initial_session.clone(),
        config: runtime.config.clone(),
        ..RequestContext::default()
    };
    ctx.view_data
        .insert("revision".to_string(), Value::Number(revision.id as i64));

    let action = runtime
        .controllers
        .call(controller, action, &mut ctx)
        .map_err(|error| RequestFaultError::new(RequestFaultStage::Action, error))?;
    let mut rendered = match action {
        ActionResult::View(view) => RenderedAction::Html {
            body: render_view(runtime, &view, &ctx)
                .map_err(|error| RequestFaultError::new(RequestFaultStage::View, error))?,
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
            body: render_view(runtime, &view, &ctx)
                .map_err(|error| RequestFaultError::new(RequestFaultStage::View, error))?,
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

    let current_session_json = session_json_value(&ctx.session)
        .map_err(|error| RequestFaultError::new(RequestFaultStage::Response, error))?;
    if current_session_json != initial_session_json {
        rendered.insert_header(
            "set-cookie",
            session_cookie_header(
                &current_session_json,
                ctx.session.is_empty(),
                runtime.session_signing_key.as_ref(),
                runtime.session_encryption_key.as_ref(),
                runtime.session_secure,
                runtime.session_auto_allows_insecure_loopback,
                &ctx.headers,
            )
            .map_err(|error| RequestFaultError::new(RequestFaultStage::Response, error))?,
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
    let template_path = resolve_view_template_path(runtime, view)?;
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("failed to read {}", template_path.display()))?;
    render_template(&template, &ctx.view_data, runtime.escape)
}

fn resolve_view_template_path(runtime: &AppRuntime, view: &str) -> Result<PathBuf> {
    validate_view_name(view)?;
    let views_root =
        fs::canonicalize(runtime.root.join("app").join("Views")).with_context(|| {
            format!(
                "failed to resolve views root for {}",
                runtime.root.display()
            )
        })?;
    let template_path = runtime
        .root
        .join("app")
        .join("Views")
        .join(format!("{view}.html"));
    let template_path = fs::canonicalize(&template_path)
        .with_context(|| format!("failed to resolve {}", template_path.display()))?;
    if !template_path.starts_with(&views_root) {
        bail!("view {view:?} resolves outside app/Views");
    }
    Ok(template_path)
}

fn validate_view_name(view: &str) -> Result<()> {
    if view.is_empty() || view.contains('\\') {
        bail!("invalid view name {view:?}");
    }
    let path = Path::new(view);
    if path.is_absolute() {
        bail!("invalid view name {view:?}");
    }
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let Some(segment) = segment.to_str() else {
                    bail!("invalid view name {view:?}");
                };
                if segment.is_empty()
                    || !segment
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                {
                    bail!("invalid view name {view:?}");
                }
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                bail!("invalid view name {view:?}");
            }
        }
    }
    Ok(())
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
        JsonValue::Number(value) => json_number_to_value(value),
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

fn json_number_to_value(value: serde_json::Number) -> Option<Value> {
    if let Some(value) = value.as_i64() {
        Some(Value::Number(value))
    } else if let Some(value) = value.as_u64() {
        match i64::try_from(value) {
            Ok(value) => Some(Value::Number(value)),
            Err(_) => Some(Value::String(value.to_string())),
        }
    } else {
        value.as_f64().map(Value::Float)
    }
}

fn session_json_value(session: &BTreeMap<String, Value>) -> Result<JsonValue> {
    value_to_json(&Value::Map(session.clone().into()), "$", SetMode::Reject)
        .context("failed to serialize session")
}

fn session_cookie_header(
    session: &JsonValue,
    session_is_empty: bool,
    signing_key: Option<&SessionSigningKey>,
    encryption_key: Option<&SessionEncryptionKey>,
    secure_policy: SessionSecure,
    auto_allows_insecure_loopback: bool,
    request_headers: &BTreeMap<String, String>,
) -> Result<String> {
    let attributes = session_cookie_attributes(
        secure_policy,
        auto_allows_insecure_loopback,
        request_headers,
    );
    if session_is_empty {
        return Ok(format!("{SESSION_COOKIE_NAME}=; Max-Age=0; {attributes}"));
    }

    let session_json = serde_json::to_string(session)?;
    let cookie_value = match encryption_key {
        Some(encryption_key) => encrypted_session_cookie_value(&session_json, encryption_key)?,
        None => match signing_key {
            Some(signing_key) => signed_session_cookie_value(&session_json, signing_key),
            None => session_json,
        },
    };
    let encoded = encode_urlencoded_component(&cookie_value);
    Ok(format!("{SESSION_COOKIE_NAME}={encoded}; {attributes}"))
}

fn session_cookie_attributes(
    secure_policy: SessionSecure,
    auto_allows_insecure_loopback: bool,
    request_headers: &BTreeMap<String, String>,
) -> String {
    let secure = match secure_policy {
        SessionSecure::Always => true,
        SessionSecure::Never => false,
        SessionSecure::Auto => {
            session_request_requires_secure_cookie(auto_allows_insecure_loopback, request_headers)
        }
    };
    if secure {
        "Path=/; HttpOnly; SameSite=Lax; Secure".to_string()
    } else {
        "Path=/; HttpOnly; SameSite=Lax".to_string()
    }
}

fn session_request_requires_secure_cookie(
    auto_allows_insecure_loopback: bool,
    request_headers: &BTreeMap<String, String>,
) -> bool {
    if request_headers
        .get("x-forwarded-proto")
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
    {
        return true;
    }
    !auto_allows_insecure_loopback
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

    signing_key.and_then(|signing_key| verified_signed_session_json(cookie_value, signing_key))
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

#[derive(Default)]
struct ParsedRequestBody {
    form_params: BTreeMap<String, String>,
    body: Option<Value>,
    json: Option<Value>,
    uploads: BTreeMap<String, Value>,
    files: Vec<Value>,
    upload_streams: UploadStreamRegistry,
}

async fn request_body_from_body(
    method: &Method,
    headers: &axum::http::HeaderMap,
    body: Body,
    uploads_config: &Uploads,
) -> Result<ParsedRequestBody> {
    let mut parsed = ParsedRequestBody::default();
    if !matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) {
        return Ok(parsed);
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let normalized_content_type = content_type.to_ascii_lowercase();

    if normalized_content_type.starts_with("application/x-www-form-urlencoded") {
        let body = to_bytes(body, uploads_config.max_request_bytes)
            .await
            .map_err(|err| anyhow!("failed to read request body: {err}"))?;
        let body = std::str::from_utf8(&body).context("form body is not valid UTF-8")?;
        parsed.form_params = parse_urlencoded_params(body);
        parsed.body = Some(string_map_value(&parsed.form_params));
        return Ok(parsed);
    }

    if normalized_content_type.starts_with("application/json") {
        let body = to_bytes(body, uploads_config.max_request_bytes)
            .await
            .map_err(|err| anyhow!("failed to read request body: {err}"))?;
        let json = serde_json::from_slice::<JsonValue>(&body)
            .map_err(|err| anyhow!("invalid JSON request body: {err}"))?;
        let value = json_to_request_value(json);
        parsed.body = Some(value.clone());
        parsed.json = Some(value);
        return Ok(parsed);
    }

    if normalized_content_type.starts_with("multipart/form-data") {
        parse_multipart_body(content_type, body, uploads_config, &mut parsed).await?;
        parsed.body = Some(multipart_body_value(&parsed));
        return Ok(parsed);
    }

    Ok(parsed)
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

fn multipart_body_value(parsed: &ParsedRequestBody) -> Value {
    Value::Map(
        BTreeMap::from([
            ("form".to_string(), string_map_value(&parsed.form_params)),
            (
                "uploads".to_string(),
                Value::Map(parsed.uploads.clone().into()),
            ),
            (
                "files".to_string(),
                Value::Array(parsed.files.clone().into()),
            ),
        ])
        .into(),
    )
}

fn json_to_request_value(value: JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(value) => Value::Bool(value),
        JsonValue::Number(value) => json_number_to_value(value).unwrap_or(Value::Nil),
        JsonValue::String(value) => Value::String(value),
        JsonValue::Array(values) => Value::Array(
            values
                .into_iter()
                .map(json_to_request_value)
                .collect::<Vec<_>>()
                .into(),
        ),
        JsonValue::Object(values) => Value::Map(
            values
                .into_iter()
                .map(|(key, value)| (key, json_to_request_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into(),
        ),
    }
}

async fn parse_multipart_body(
    content_type: &str,
    body: Body,
    config: &Uploads,
    parsed: &mut ParsedRequestBody,
) -> Result<()> {
    parsed.upload_streams = UploadStreamRegistry::with_max_retained(config.max_retained_streams);
    let boundary = multipart_boundary(content_type)
        .context("multipart/form-data request is missing boundary")?;
    let mut multipart = multer::Multipart::new(body.into_data_stream(), boundary);
    let mut request_bytes = 0usize;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| anyhow!("failed to read multipart field: {err}"))?
    {
        let name = field
            .name()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .context("multipart form-data part is missing name")?;
        let filename = field.file_name().map(str::to_string);
        let content_type = field.content_type().map(ToString::to_string);

        if filename.is_some() {
            let mut temp =
                tempfile::NamedTempFile::new().context("failed to create temporary upload file")?;
            let mut file_bytes = 0usize;
            let mut memory = Some(Vec::new());
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|err| anyhow!("failed to read multipart file {name}: {err}"))?
            {
                add_multipart_request_bytes(&mut request_bytes, chunk.len(), config)?;
                file_bytes = file_bytes
                    .checked_add(chunk.len())
                    .context("multipart file size overflow")?;
                if file_bytes > config.max_file_bytes {
                    bail!(
                        "multipart file {name} exceeds web.uploads.max_file_bytes of {} bytes",
                        config.max_file_bytes
                    );
                }
                temp.write_all(&chunk)
                    .context("failed to write temporary upload file")?;
                if let Some(buffer) = &mut memory {
                    match buffer.len().checked_add(chunk.len()) {
                        Some(next_len) if next_len <= config.memory_threshold_bytes => {
                            buffer.extend_from_slice(&chunk);
                        }
                        _ => {
                            memory = None;
                        }
                    }
                }
            }
            temp.flush()
                .context("failed to flush temporary upload file")?;
            let snapshot = parsed
                .upload_streams
                .retain_temp_file(
                    UploadStreamMetadata {
                        field: name.clone(),
                        filename,
                        content_type,
                        size: file_bytes as u64,
                    },
                    temp.into_temp_path(),
                )
                .map_err(|err| anyhow!("{}", err.message))?;
            let upload = upload_value_from_snapshot(&snapshot, memory.as_deref());
            parsed.files.push(upload.clone());
            insert_upload(&mut parsed.uploads, name, upload);
        } else {
            let mut data = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|err| anyhow!("failed to read multipart field {name}: {err}"))?
            {
                add_multipart_request_bytes(&mut request_bytes, chunk.len(), config)?;
                data.extend_from_slice(&chunk);
            }
            let value = String::from_utf8(data)
                .with_context(|| format!("multipart field {name} is not valid UTF-8"))?;
            parsed.form_params.insert(name, value);
        }
    }

    Ok(())
}

fn multipart_boundary(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("boundary") {
            return None;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn add_multipart_request_bytes(total: &mut usize, bytes: usize, config: &Uploads) -> Result<()> {
    *total = total
        .checked_add(bytes)
        .context("multipart request size overflow")?;
    if *total > config.max_request_bytes {
        bail!(
            "multipart request exceeds web.uploads.max_request_bytes of {} bytes",
            config.max_request_bytes
        );
    }
    Ok(())
}

fn upload_value_from_snapshot(
    snapshot: &ricochet_vm::upload_runtime::UploadStreamSnapshot,
    data: Option<&[u8]>,
) -> Value {
    let text = data
        .and_then(|data| std::str::from_utf8(data).ok())
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Nil);
    let data_base64 = data
        .map(|data| Value::String(BASE64_STANDARD.encode(data)))
        .unwrap_or(Value::Nil);
    let filename = snapshot
        .filename
        .as_ref()
        .map(|value| Value::String(value.clone()))
        .unwrap_or(Value::Nil);
    let content_type = snapshot
        .content_type
        .as_ref()
        .map(|value| Value::String(value.clone()))
        .unwrap_or(Value::Nil);
    Value::Map(
        BTreeMap::from([
            ("name".to_string(), Value::String(snapshot.field.clone())),
            ("field".to_string(), Value::String(snapshot.field.clone())),
            ("stream_id".to_string(), Value::Number(snapshot.id as i64)),
            ("filename".to_string(), filename),
            ("content_type".to_string(), content_type),
            ("size_known".to_string(), Value::Bool(snapshot.size_known)),
            ("size".to_string(), Value::Number(snapshot.size as i64)),
            ("data_base64".to_string(), data_base64),
            ("text".to_string(), text),
        ])
        .into(),
    )
}

fn insert_upload(uploads: &mut BTreeMap<String, Value>, name: String, upload: Value) {
    match uploads.remove(&name) {
        None => {
            uploads.insert(name, upload);
        }
        Some(Value::Array(existing)) => {
            let mut values = existing.snapshot();
            values.push(upload);
            uploads.insert(name, Value::Array(values.into()));
        }
        Some(existing) => {
            uploads.insert(name, Value::Array(vec![existing, upload].into()));
        }
    }
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

pub async fn serve_current_dir(options: ServeOptions) -> Result<()> {
    options.validate()?;
    let project_root = Path::new(".");
    let app =
        build_served_app_from_dir(project_root, options.debug, options.watch, &options).await?;
    let bind_addr = options.bind_addr();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    println!(
        "Ricochet web server listening on http://{bind_addr} debug={} watch={}",
        options.debug, options.watch
    );
    serve_app_on_listener(listener, app).await
}

pub async fn build_served_app_from_dir(
    project_root: impl AsRef<Path>,
    debug: bool,
    watch: bool,
    options: &ServeOptions,
) -> Result<Router> {
    let project_root = project_root.as_ref();
    let manifest = load_manifest(project_root)?;
    let effective_options = apply_manifest_capabilities(project_root, &manifest, options)?;
    effective_options.validate()?;
    let watch_trace_sink = (watch && debug).then(stdout_watch_trace_sink);
    let mut builder = crate::ServeBuilder::new(project_root)
        .options(effective_options.clone())
        .watched(watch);
    if let Some(database) = manifest.database.default {
        let backend =
            connect_served_database_backend(project_root, &database, &effective_options).await?;
        builder = builder.database_backend(backend);
    }
    if let Some(trace_sink) = watch_trace_sink {
        builder = builder.trace_sink(trace_sink);
    }

    builder.build()
}

fn apply_manifest_capabilities(
    project_root: &Path,
    manifest: &Manifest,
    options: &ServeOptions,
) -> Result<ServeOptions> {
    let mut effective = options.clone();
    let capabilities = &manifest.web.capabilities;

    apply_manifest_paths(project_root, capabilities, options, &mut effective)?;
    if capabilities.fs_readonly {
        effective.fs_readonly = true;
    }
    if capabilities.allow_env {
        if !options.allow_env {
            bail!(
                "manifest requests web.capabilities.allow_env; rerun with --allow-env to grant it"
            );
        }
        effective.allow_env = true;
    }
    if !capabilities.env_allow.is_empty() {
        ensure_manifest_env_allowed(&capabilities.env_allow, options)?;
        if !options.allow_env {
            effective.env_allow = capabilities.env_allow.clone();
        }
    }
    if capabilities.allow_process {
        if !options.allow_process {
            bail!("manifest requests web.capabilities.allow_process; rerun with --allow-process to grant it");
        }
        effective.allow_process = true;
    }
    if capabilities.allow_pty {
        if !options.allow_pty {
            bail!(
                "manifest requests web.capabilities.allow_pty; rerun with --allow-pty to grant it"
            );
        }
        effective.allow_pty = true;
    }
    if !capabilities.http_allow_hosts.is_empty() {
        ensure_manifest_hosts_allowed(
            &capabilities.http_allow_hosts,
            &options.http_allow_hosts,
            "HTTP",
            "--http-allow-host",
        )?;
        effective.http_allow_hosts = capabilities.http_allow_hosts.clone();
    }

    Ok(effective)
}

fn apply_manifest_paths(
    project_root: &Path,
    capabilities: &WebCapabilities,
    operator: &ServeOptions,
    options: &mut ServeOptions,
) -> Result<()> {
    if let Some(root) = capabilities.fs_root.as_deref() {
        let Some(operator_root) = operator.fs_root.as_deref() else {
            bail!("manifest requests web.capabilities.fs_root; rerun with --fs-root to grant filesystem access");
        };
        let manifest_root = resolve_manifest_relative_path(project_root, root, "manifest fs_root")?;
        ensure_manifest_path_within_operator_root(
            operator_root,
            &manifest_root,
            "manifest fs_root",
            "--fs-root",
        )?;
        options.fs_root = Some(manifest_root);
    }
    if let Some(root) = capabilities.process_root.as_deref() {
        let Some(operator_root) = operator.process_root.as_deref() else {
            bail!("manifest requests web.capabilities.process_root; rerun with --process-root to grant process access");
        };
        let manifest_root =
            resolve_manifest_relative_path(project_root, root, "manifest process_root")?;
        ensure_manifest_path_within_operator_root(
            operator_root,
            &manifest_root,
            "manifest process_root",
            "--process-root",
        )?;
        options.process_root = Some(manifest_root);
    }
    Ok(())
}

fn resolve_manifest_relative_path(
    project_root: &Path,
    value: &str,
    label: &str,
) -> Result<PathBuf> {
    if value.contains('\\') {
        bail!("{label} must use forward slashes");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        bail!("{label} must be relative to the project root");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("{label} must not contain .. components"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("{label} must be relative to the project root")
            }
        }
    }
    Ok(project_root.join(path))
}

fn ensure_manifest_path_within_operator_root(
    operator_root: &Path,
    candidate: &Path,
    label: &str,
    grant: &str,
) -> Result<()> {
    let operator_root = fs::canonicalize(operator_root)
        .with_context(|| format!("failed to resolve {grant} {}", operator_root.display()))?;
    let existing = nearest_existing_ancestor(candidate);
    let existing = fs::canonicalize(existing)
        .with_context(|| format!("failed to resolve {label} {}", candidate.display()))?;
    if !existing.starts_with(&operator_root) {
        bail!("{label} must stay within {grant}: {}", candidate.display());
    }
    Ok(())
}

fn nearest_existing_ancestor(path: &Path) -> &Path {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or_else(|| Path::new("."))
}

fn ensure_manifest_env_allowed(names: &[String], options: &ServeOptions) -> Result<()> {
    if options.allow_env {
        return Ok(());
    }
    for name in names {
        if !options.env_allow.iter().any(|allowed| allowed == name) {
            bail!("manifest requests environment variable {name}; rerun with --env-allow {name} to grant it");
        }
    }
    Ok(())
}

fn ensure_manifest_hosts_allowed(
    requested: &[String],
    allowed: &[String],
    label: &str,
    flag: &str,
) -> Result<()> {
    for host in requested {
        if !host_allowed_by_policy(host, allowed) {
            bail!("manifest requests {label} host {host}; rerun with {flag} {host} to grant it");
        }
    }
    Ok(())
}

fn host_allowed_by_policy(host: &str, allowed: &[String]) -> bool {
    let host = host.trim().to_ascii_lowercase();
    !host.is_empty()
        && allowed
            .iter()
            .any(|allowed| allowed.trim().eq_ignore_ascii_case(&host))
}

fn compose_serve_capability_vm_setup(
    project_root: &Path,
    vm_setup: Option<VmSetup>,
    options: &ServeOptions,
) -> Result<Option<VmSetup>> {
    compose_serve_capability_vm_setup_with_state(
        project_root,
        vm_setup,
        options,
        Arc::new(ServeCapabilityState::default()),
    )
}

fn compose_serve_capability_vm_setup_with_state(
    project_root: &Path,
    vm_setup: Option<VmSetup>,
    options: &ServeOptions,
    capability_state: Arc<ServeCapabilityState>,
) -> Result<Option<VmSetup>> {
    let root = if let Some(root) = &options.fs_root {
        let root = if root.is_absolute() {
            root.clone()
        } else {
            project_root.join(root)
        };
        let root = fs::canonicalize(&root)
            .with_context(|| format!("failed to resolve --fs-root {}", root.display()))?;
        if !root.is_dir() {
            bail!("--fs-root must be a directory: {}", root.display());
        }
        Some(Arc::new(root))
    } else {
        None
    };
    let readonly = options.fs_readonly;
    let allow_all_env = options.allow_env;
    let allow_env = allow_all_env || !options.env_allow.is_empty();
    let env_allow = Arc::new(options.env_allow.clone());
    let allow_process = options.allow_process;
    let process_root = if let Some(root) = options.process_root.clone() {
        let root = fs::canonicalize(&root)
            .with_context(|| format!("failed to resolve --process-root {}", root.display()))?;
        if !root.is_dir() {
            bail!("--process-root must be a directory: {}", root.display());
        }
        Some(Arc::new(root))
    } else {
        None
    };
    let allow_pty = options.allow_pty;
    let http_allow_hosts = Arc::new(options.http_allow_hosts.clone());
    Ok(Some(Arc::new(move |vm| {
        let mut capabilities = match &vm_setup {
            Some(setup) => setup(vm)?,
            None => BTreeMap::new(),
        };
        vm.set_approval_registry(capability_state.approval_registry.clone());
        vm.set_environment_enabled(allow_env);
        if allow_all_env || env_allow.is_empty() {
            vm.clear_environment_allowed_names();
        } else {
            vm.set_environment_allowed_names((*env_allow).clone());
        }
        vm.set_host_capabilities(root.is_some(), !http_allow_hosts.is_empty());
        vm.set_process_enabled(allow_process);
        vm.set_pty_enabled(allow_pty);
        if let Some(process_root) = &process_root {
            vm.set_process_root((**process_root).clone());
        }
        if allow_process {
            vm.set_process_registry(capability_state.process_registry.clone());
            capabilities.insert(
                "process".to_string(),
                Value::Capability(Capability::Process),
            );
        }
        if allow_pty {
            vm.set_pty_registry(capability_state.pty_registry.clone());
        }
        if let Some(root) = &root {
            vm.set_filesystem_root((**root).clone());
            if readonly {
                vm.set_filesystem_writes_enabled(false);
            }
            capabilities.insert("fs".to_string(), Value::Capability(Capability::FileSystem));
        }
        if !http_allow_hosts.is_empty() {
            vm.set_http_allowed_hosts((*http_allow_hosts).clone());
            capabilities.insert("http".to_string(), Value::Capability(Capability::Http));
        }
        Ok(capabilities)
    })))
}

pub async fn serve_app_on_listener(listener: tokio::net::TcpListener, app: Router) -> Result<()> {
    axum::serve(listener, app).await?;
    Ok(())
}

async fn connect_served_database_backend(
    project_root: &Path,
    database: &DatabaseDefault,
    options: &ServeOptions,
) -> Result<Arc<dyn DatabaseBackend>> {
    let adapter = database.adapter.trim().to_ascii_lowercase();
    let url = resolve_served_database_url(project_root, database, options, adapter.as_str())?;

    connect_database_backend_url(adapter.as_str(), &url).await
}

#[cfg(test)]
async fn connect_database_backend(database: &DatabaseDefault) -> Result<Arc<dyn DatabaseBackend>> {
    let adapter = database.adapter.trim().to_ascii_lowercase();
    let url = database.resolved_url()?;

    connect_database_backend_url(adapter.as_str(), &url).await
}

async fn connect_database_backend_url(
    adapter: &str,
    url: &str,
) -> Result<Arc<dyn DatabaseBackend>> {
    match adapter {
        "postgres" | "postgresql" => Ok(Arc::new(PostgresDatabase::connect(url).await?)),
        "mysql" | "mariadb" => Ok(Arc::new(MysqlDatabase::connect(url).await?)),
        "sqlite" => Ok(Arc::new(SqliteDatabase::connect(url)?)),
        _ => bail!("unsupported database adapter {adapter}; expected postgres, sqlite, or mysql"),
    }
}

fn resolve_served_database_url(
    project_root: &Path,
    database: &DatabaseDefault,
    options: &ServeOptions,
    adapter: &str,
) -> Result<String> {
    let allowed_env = allowed_env_set(options);
    let url = match allowed_env.as_ref() {
        Some(allowed_env) => database.resolved_url_with_env_policy(Some(allowed_env))?,
        None => database.resolved_url_with_env_policy(None)?,
    };

    match adapter {
        "postgres" | "postgresql" | "mysql" | "mariadb" => {
            ensure_database_endpoint_allowed(&url, &options.database_allow_hosts)?;
            Ok(url)
        }
        "sqlite" => resolve_served_sqlite_url(project_root, &url),
        _ => bail!(
            "unsupported database adapter {}; expected postgres, sqlite, or mysql",
            database.adapter
        ),
    }
}

fn resolve_served_sqlite_url(project_root: &Path, url: &str) -> Result<String> {
    let path = sqlite_path_from_url_text(url)?;
    if path == ":memory:" {
        return Ok(path);
    }
    let path = Path::new(&path);
    if path.is_absolute() {
        bail!("SQLite database path must be relative to the project root");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("SQLite database path must not contain .. components"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("SQLite database path must be relative to the project root")
            }
        }
    }
    let resolved = project_root.join(path);
    let existing = nearest_existing_ancestor(&resolved);
    let canonical_root = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve project root {}", project_root.display()))?;
    let canonical_existing = fs::canonicalize(existing).with_context(|| {
        format!(
            "failed to resolve SQLite database path {}",
            resolved.display()
        )
    })?;
    if !canonical_existing.starts_with(canonical_root) {
        bail!(
            "SQLite database path is outside the project root: {}",
            resolved.display()
        );
    }
    Ok(resolved.to_string_lossy().into_owned())
}

fn sqlite_path_from_url_text(url: &str) -> Result<String> {
    let trimmed = url.trim();
    let path = trimmed
        .strip_prefix("sqlite://")
        .or_else(|| trimmed.strip_prefix("sqlite:"))
        .unwrap_or(trimmed);
    let path = normalize_sqlite_url_path(path);
    if path.is_empty() {
        bail!("SQLite database path is empty");
    }
    Ok(path)
}

#[cfg(windows)]
fn normalize_sqlite_url_path(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' {
        path[1..].to_string()
    } else {
        path.to_string()
    }
}

#[cfg(not(windows))]
fn normalize_sqlite_url_path(path: &str) -> String {
    path.to_string()
}

fn ensure_database_endpoint_allowed(url: &str, allowed_hosts: &[String]) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("database URL must be absolute")?;
    let Some(host) = parsed.host_str() else {
        return Ok(());
    };
    if host_is_local(host) {
        return Ok(());
    }
    if host_allowed_by_policy(host, allowed_hosts) {
        Ok(())
    } else {
        bail!(
            "database host {host} is not allowed; rerun with --database-allow-host {host} to grant it"
        )
    }
}

fn ensure_allowed_http_endpoint(
    url: &str,
    allowed_hosts: &[String],
    label: &str,
    flag: &str,
) -> Result<()> {
    let parsed =
        reqwest::Url::parse(url).with_context(|| format!("{label} URL must be absolute"))?;
    let scheme = parsed.scheme();
    let host = parsed
        .host_str()
        .with_context(|| format!("{label} URL must include a host"))?;
    if scheme != "https" && !(scheme == "http" && host_is_local(host)) {
        bail!("{label} URL must use https, except for localhost development endpoints");
    }
    if host_allowed_by_policy(host, allowed_hosts) {
        Ok(())
    } else {
        bail!("{label} host {host} is not allowed; rerun with {flag} {host} to grant it")
    }
}

fn host_is_local(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn server_build_test_app_returns_ok() {
        let _ = build_test_app().expect("server test app should build");
    }

    #[tokio::test]
    async fn serve_builder_builds_static_router() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");

        let _ = crate::ServeBuilder::new(fixture_root)
            .build()
            .expect("ServeBuilder should build a static router");
    }

    #[tokio::test]
    async fn serve_builder_builds_watched_router_with_options() {
        let fixture_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");
        let options = ServeOptions {
            watch: true,
            ..ServeOptions::default()
        };

        let _ = crate::ServeBuilder::new(fixture_root)
            .options(options)
            .watched(true)
            .build()
            .expect("ServeBuilder should build a watched router with serve options");
    }

    #[tokio::test]
    async fn mvc_controller_can_use_dynamic_import() {
        let temp = tempfile::tempdir().expect("temp app dir should be created");
        let root = temp.path();
        fs::create_dir_all(root.join("config")).expect("config dir should be created");
        fs::create_dir_all(root.join("app").join("Controllers"))
            .expect("controllers dir should be created");
        fs::create_dir_all(root.join("app").join("Support"))
            .expect("support dir should be created");
        fs::write(
            root.join("ricochet.toml"),
            r#"
[package]
name = "dynamic_mvc"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
        )
        .expect("manifest should be written");
        fs::write(
            root.join("config").join("routes.rco"),
            r#"GET "/" HomeController "index" route
"#,
        )
        .expect("routes should be written");
        fs::write(
            root.join("app").join("Support").join("Greeter.rco"),
            r#"
greeting function
  "dynamic mvc"
end
"#,
        )
        .expect("support module should be written");
        fs::write(
            root.join("app")
                .join("Controllers")
                .join("HomeController.rco"),
            r#"
HomeController Controller Subclass
  [
    "app/Support/Greeter" import_dynamic value helper var
    args array
    helper get "greeting" args get module_call value text
  ] "index" Method
end
"#,
        )
        .expect("controller should be written");

        let app = build_app_from_dir(root).expect("dynamic MVC app should build");
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should complete");
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should be readable");
        let body = String::from_utf8(body.to_vec()).expect("body should be utf8");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "dynamic mvc");
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
            ..ServeOptions::default()
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

    #[test]
    fn serve_options_accept_watch_with_capability_flags() {
        let options = ServeOptions {
            watch: true,
            allow_env: true,
            allow_process: true,
            allow_pty: true,
            fs_root: Some(PathBuf::from(".")),
            http_allow_hosts: vec!["127.0.0.1".to_string()],
            ..ServeOptions::default()
        };

        options
            .validate()
            .expect("watch should support serve capability flags");
    }

    #[test]
    fn manifest_capabilities_cannot_self_grant_process_access() {
        let manifest = test_manifest_with_capabilities("allow_process = true");
        let error =
            apply_manifest_capabilities(Path::new("."), &manifest, &ServeOptions::default())
                .expect_err("manifest must not grant process access by itself");

        assert!(
            error.to_string().contains("--allow-process"),
            "error should point at the required operator grant, got: {error:#}"
        );
    }

    #[test]
    fn manifest_capabilities_must_narrow_http_hosts() {
        let manifest = test_manifest_with_capabilities("http_allow_hosts = [\"metadata.test\"]");
        let options = ServeOptions {
            http_allow_hosts: vec!["api.example.test".to_string()],
            ..ServeOptions::default()
        };

        let error = apply_manifest_capabilities(Path::new("."), &manifest, &options)
            .expect_err("manifest host outside operator policy should fail");

        assert!(
            error.to_string().contains("--http-allow-host"),
            "error should point at the required host grant, got: {error:#}"
        );
    }

    #[test]
    fn ai_provider_policy_requires_allowed_env_and_endpoint_host() {
        std::env::set_var("RICOCHET_TEST_AI_KEY", "secret");
        let config = AiDefault {
            provider: "openai-compatible".to_string(),
            model: "demo".to_string(),
            api_key: "${RICOCHET_TEST_AI_KEY}".to_string(),
            base_url: Some("https://ai.example.test/v1".to_string()),
        };

        let denied_env = resolve_ai_provider_config(&config, Some(&ServeOptions::default()))
            .expect_err("ungranted env variable should fail");
        assert!(
            denied_env.to_string().contains("RICOCHET_TEST_AI_KEY"),
            "error should identify denied env variable, got: {denied_env:#}"
        );

        let denied_host = resolve_ai_provider_config(
            &config,
            Some(&ServeOptions {
                env_allow: vec!["RICOCHET_TEST_AI_KEY".to_string()],
                ..ServeOptions::default()
            }),
        )
        .expect_err("ungranted AI host should fail");
        assert!(
            denied_host.to_string().contains("--ai-allow-host"),
            "error should identify denied AI host, got: {denied_host:#}"
        );

        resolve_ai_provider_config(
            &config,
            Some(&ServeOptions {
                env_allow: vec!["RICOCHET_TEST_AI_KEY".to_string()],
                ai_allow_hosts: vec!["ai.example.test".to_string()],
                ..ServeOptions::default()
            }),
        )
        .expect("allowed env and host should pass");
    }

    #[test]
    fn database_policy_requires_allowed_env_and_remote_host() {
        std::env::set_var(
            "RICOCHET_TEST_DATABASE_URL",
            "postgres://user:pass@db.example.test/app",
        );
        let database = DatabaseDefault {
            adapter: "postgres".to_string(),
            url: "${RICOCHET_TEST_DATABASE_URL}".to_string(),
        };

        let denied_env = resolve_served_database_url(
            Path::new("."),
            &database,
            &ServeOptions::default(),
            "postgres",
        )
        .expect_err("ungranted database URL env variable should fail");
        assert!(
            denied_env
                .to_string()
                .contains("RICOCHET_TEST_DATABASE_URL"),
            "error should identify denied env variable, got: {denied_env:#}"
        );

        let denied_host = resolve_served_database_url(
            Path::new("."),
            &database,
            &ServeOptions {
                env_allow: vec!["RICOCHET_TEST_DATABASE_URL".to_string()],
                ..ServeOptions::default()
            },
            "postgres",
        )
        .expect_err("ungranted database host should fail");
        assert!(
            denied_host.to_string().contains("--database-allow-host"),
            "error should identify denied database host, got: {denied_host:#}"
        );

        resolve_served_database_url(
            Path::new("."),
            &database,
            &ServeOptions {
                env_allow: vec!["RICOCHET_TEST_DATABASE_URL".to_string()],
                database_allow_hosts: vec!["db.example.test".to_string()],
                ..ServeOptions::default()
            },
            "postgres",
        )
        .expect("allowed env and database host should pass");
    }

    #[test]
    fn served_sqlite_urls_must_stay_under_project_root() {
        let project_root = std::env::current_dir().expect("test cwd should exist");

        let relative = resolve_served_sqlite_url(&project_root, "sqlite:db/development.sqlite3")
            .expect("relative SQLite URL should resolve under project root");
        assert!(Path::new(&relative).starts_with(&project_root));

        let parent_error = resolve_served_sqlite_url(&project_root, "../outside.sqlite3")
            .expect_err("parent traversal should be rejected");
        assert!(
            parent_error.to_string().contains("must not contain .."),
            "parent traversal error was: {parent_error:#}"
        );

        let absolute_error = resolve_served_sqlite_url(
            &project_root,
            &std::env::temp_dir()
                .join("outside.sqlite3")
                .to_string_lossy(),
        )
        .expect_err("absolute SQLite path should be rejected");
        assert!(
            absolute_error.to_string().contains("must be relative"),
            "absolute path error was: {absolute_error:#}"
        );
    }

    #[test]
    fn json_number_to_value_preserves_unsigned_values_above_i64() {
        let value = i64::MAX as u64 + 1;

        assert_eq!(
            json_number_to_value(serde_json::Number::from(value)),
            Some(Value::String(value.to_string()))
        );
    }

    #[test]
    fn json_request_values_preserve_large_unsigned_ids() {
        let id = u64::MAX;
        let value = json_to_request_value(serde_json::json!({ "id": id }));
        let Value::Map(values) = value else {
            panic!("JSON object should decode as a Ricochet map");
        };

        assert_eq!(values.get("id"), Some(Value::String(id.to_string())));
    }

    #[test]
    fn session_auto_secure_does_not_trust_host_header_for_downgrade() {
        let headers = BTreeMap::from([("host".to_string(), "localhost".to_string())]);

        assert!(
            session_request_requires_secure_cookie(false, &headers),
            "Auto secure sessions should not let a request Host header disable Secure"
        );
        assert!(
            !session_request_requires_secure_cookie(true, &headers),
            "Auto secure sessions should preserve explicit loopback development behavior"
        );
    }

    fn test_manifest_with_capabilities(capabilities: &str) -> Manifest {
        toml::from_str(&format!(
            r#"
[package]
name = "policy_test"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.capabilities]
{capabilities}
"#
        ))
        .expect("test manifest should parse")
    }
}
