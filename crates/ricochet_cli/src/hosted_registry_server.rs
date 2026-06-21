use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use super::{
    absolute_path_from_current, bytes_integrity, default_dependency_alias, hex_digest,
    package_tree_integrity, path_to_slash, read_package_metadata, registry_package_relative_path,
    static_registry, validate_package_integrity, validate_package_version,
    validate_registry_package_name, validate_signature_kind,
};

const PROTOCOL: &str = "ricochet-hosted-registry-v1";
const DISCOVERY_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.v1+json";
const SEARCH_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.search.v1+json";
const PACKAGE_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.package.v1+json";
const ARCHIVE_MEDIA_TYPE: &str = "application/vnd.ricochet.package.archive.v1+gzip";
const PUBLISH_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.publish.v1+json";
const ERROR_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.error.v1+json";
const REGISTRY_STATE_FILE: &str = "registry.json";
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PUBLISH_BODY_BYTES: usize = MAX_ARCHIVE_BYTES + MAX_METADATA_BYTES + 4 * 1024 * 1024;

pub(super) struct HostedRegistryServeOptions<'a> {
    pub(super) root: &'a Path,
    pub(super) host: &'a str,
    pub(super) port: u16,
    pub(super) token_envs: &'a [String],
    pub(super) publishers: &'a [String],
}

#[derive(Clone)]
struct AppState {
    root: Arc<PathBuf>,
    base_url: Arc<str>,
    registry: Arc<Mutex<RegistryDocument>>,
    publishers: Arc<PublisherPolicies>,
    idempotency: Arc<Mutex<BTreeMap<String, IdempotencyRecord>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryDocument {
    protocol: String,
    #[serde(default)]
    packages: BTreeMap<String, StoredPackage>,
}

impl Default for RegistryDocument {
    fn default() -> Self {
        Self {
            protocol: PROTOCOL.to_string(),
            packages: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPackage {
    name: String,
    #[serde(default)]
    versions: Vec<StoredVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredVersion {
    version: String,
    published_at: String,
    yanked: bool,
    archive: StoredArchive,
    package_integrity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<StoredProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    yanked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    yanked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredArchive {
    path: String,
    integrity: String,
    media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredProvenance {
    #[serde(skip_serializing_if = "Option::is_none")]
    attestation_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attestation_integrity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_integrity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PublishMetadata {
    protocol: String,
    package: String,
    version: String,
    package_integrity: String,
    archive_integrity: String,
    provenance_integrity: Option<String>,
    signature_integrity: Option<String>,
    signature_kind: Option<String>,
}

#[derive(Debug)]
struct PublishUpload {
    metadata: PublishMetadata,
    archive_bytes: Vec<u8>,
    provenance_bytes: Option<Vec<u8>>,
    signature_bytes: Option<Vec<u8>>,
    digest: String,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Clone)]
struct PublisherPolicies {
    policies: Vec<PublisherPolicy>,
}

#[derive(Debug, Clone)]
struct PublisherPolicy {
    pattern: PublisherPattern,
    token_hash: String,
    label: String,
}

#[derive(Debug, Clone)]
enum PublisherPattern {
    All,
    Exact(String),
    Scope(String),
}

#[derive(Debug)]
struct AuthenticatedPublisher {
    token_hash: String,
    label: String,
}

#[derive(Debug, Clone)]
struct IdempotencyRecord {
    token_hash: String,
    method: &'static str,
    path: String,
    body_digest: String,
    response: StoredHttpResponse,
}

#[derive(Debug, Clone)]
struct StoredHttpResponse {
    status: StatusCode,
    media_type: &'static str,
    body: Vec<u8>,
}

#[derive(Debug)]
struct RegistryHttpError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<serde_json::Value>,
}

pub(super) async fn serve(options: HostedRegistryServeOptions<'_>) -> Result<()> {
    let root = absolute_path_from_current(options.root)?;
    fs::create_dir_all(&root)
        .with_context(|| format!("failed to create registry root {}", root.display()))?;
    fs::create_dir_all(root.join("artifacts"))
        .with_context(|| format!("failed to create {}", root.join("artifacts").display()))?;
    fs::create_dir_all(root.join(".tmp"))
        .with_context(|| format!("failed to create {}", root.join(".tmp").display()))?;

    let registry = load_registry(&root)?;
    let publishers = PublisherPolicies::from_options(options.token_envs, options.publishers)?;
    let listener = TcpListener::bind((options.host, options.port))
        .await
        .with_context(|| {
            format!(
                "failed to bind hosted registry server on {}:{}",
                options.host, options.port
            )
        })?;
    let address = listener
        .local_addr()
        .context("failed to read hosted registry listener address")?;
    let base_url = format_base_url(address);

    let state = AppState {
        root: Arc::new(root.clone()),
        base_url: Arc::from(base_url.as_str()),
        registry: Arc::new(Mutex::new(registry)),
        publishers: Arc::new(publishers),
        idempotency: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let app = Router::new()
        .route("/v1", get(discovery))
        .route("/v1/search", get(search))
        .route("/v1/packages/:package", get(package_metadata))
        .route(
            "/v1/packages/:package/versions/:version",
            get(version_metadata).put(publish_version),
        )
        .route(
            "/v1/packages/:package/versions/:version/yank",
            post(yank_version),
        )
        .fallback(get(artifact))
        .layer(DefaultBodyLimit::max(MAX_PUBLISH_BODY_BYTES))
        .with_state(state);

    println!("serving hosted registry {} at {}", root.display(), base_url);
    let _ = std::io::stdout().flush();
    axum::serve(listener, app)
        .await
        .context("hosted registry server stopped unexpectedly")
}

async fn discovery(State(state): State<AppState>) -> Response {
    json_response(
        StatusCode::OK,
        DISCOVERY_MEDIA_TYPE,
        json!({
            "protocol": PROTOCOL,
            "base_url": state.base_url.as_ref(),
        }),
    )
}

async fn search(State(state): State<AppState>, Query(query): Query<SearchQuery>) -> Response {
    match search_response(&state, query) {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn package_metadata(
    State(state): State<AppState>,
    AxumPath(package): AxumPath<String>,
) -> Response {
    match package_response(&state, &package, None) {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn version_metadata(
    State(state): State<AppState>,
    AxumPath((package, version)): AxumPath<(String, String)>,
) -> Response {
    match package_response(&state, &package, Some(&version)) {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn publish_version(
    State(state): State<AppState>,
    AxumPath((package, version)): AxumPath<(String, String)>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Response {
    match publish_version_inner(&state, &package, &version, &headers, multipart).await {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn yank_version(
    State(state): State<AppState>,
    AxumPath((package, version)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    match yank_version_inner(&state, &package, &version, &headers) {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn artifact(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    match artifact_response(&state, uri.path()) {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

fn search_response(
    state: &AppState,
    query: SearchQuery,
) -> std::result::Result<StoredHttpResponse, RegistryHttpError> {
    let needle = query.q.unwrap_or_default().to_ascii_lowercase();
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);
    let registry = lock_registry(state)?;
    let mut packages = registry
        .packages
        .values()
        .filter_map(|package| {
            if !needle.is_empty() && !package.name.to_ascii_lowercase().contains(&needle) {
                return None;
            }
            let latest = latest_non_yanked_version(&package.versions)?;
            Some(json!({
                "name": package.name,
                "latest": latest.version,
            }))
        })
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| {
        package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    let packages = packages
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json_stored_response(
        StatusCode::OK,
        SEARCH_MEDIA_TYPE,
        json!({
            "protocol": PROTOCOL,
            "packages": packages,
        }),
    ))
}

fn package_response(
    state: &AppState,
    package: &str,
    version: Option<&str>,
) -> std::result::Result<StoredHttpResponse, RegistryHttpError> {
    validate_registry_package_name(package).map_err(bad_request)?;
    if let Some(version) = version {
        validate_package_version(version).map_err(bad_request)?;
    }
    let registry = lock_registry(state)?;
    let stored = registry.packages.get(package).ok_or_else(|| {
        not_found(format!(
            "hosted registry does not contain package {package}"
        ))
    })?;
    let mut versions = if let Some(version) = version {
        vec![stored
            .versions
            .iter()
            .find(|stored| stored.version == version)
            .cloned()
            .ok_or_else(|| {
                not_found(format!(
                    "hosted registry does not contain package {package} {version}"
                ))
            })?]
    } else {
        stored.versions.clone()
    };
    sort_versions(&mut versions);
    let latest = latest_non_yanked_version(&stored.versions).map(|version| version.version.clone());
    Ok(json_stored_response(
        StatusCode::OK,
        PACKAGE_MEDIA_TYPE,
        json!({
            "protocol": PROTOCOL,
            "package": {
                "name": stored.name,
                "latest": latest,
            },
            "versions": versions,
        }),
    ))
}

async fn publish_version_inner(
    state: &AppState,
    package: &str,
    version: &str,
    headers: &HeaderMap,
    multipart: Multipart,
) -> std::result::Result<StoredHttpResponse, RegistryHttpError> {
    validate_registry_package_name(package).map_err(bad_request)?;
    validate_package_version(version).map_err(bad_request)?;
    let publisher = state.publishers.authenticate(headers, package)?;
    let upload = read_publish_upload(multipart).await?;
    validate_publish_upload(package, version, &upload)?;
    let idempotency_key = idempotency_key(headers)?;
    let path = format!("/v1/packages/{package}/versions/{version}");
    if let Some(response) = idempotency_replay(
        state,
        &idempotency_key,
        &publisher.token_hash,
        "PUT",
        &path,
        &upload.digest,
    )? {
        return Ok(response);
    }

    let mut registry = lock_registry_mut(state)?;
    let response = {
        let package_entry = registry
            .packages
            .entry(package.to_string())
            .or_insert_with(|| StoredPackage {
                name: package.to_string(),
                versions: Vec::new(),
            });
        if package_entry
            .versions
            .iter()
            .any(|stored| stored.version == version)
        {
            let response = error_stored_response(
                StatusCode::CONFLICT,
                "version_exists",
                format!("package {package} {version} already exists"),
                None,
            );
            store_idempotency(
                state,
                idempotency_key,
                publisher.token_hash,
                "PUT",
                path,
                upload.digest,
                response.clone(),
            )?;
            return Ok(response);
        }

        let version_root = state
            .root
            .join("artifacts")
            .join(registry_package_relative_path(package))
            .join(version);
        let archive_relative = artifact_relative_path(
            package,
            version,
            &format!("{}-{version}.tar.gz", default_dependency_alias(package)),
        );
        let archive_path = state.root.join(&archive_relative);
        fs::create_dir_all(&version_root).map_err(|error| {
            internal_error(format!(
                "failed to create {}: {error}",
                version_root.display()
            ))
        })?;
        write_new_file(&archive_path, &upload.archive_bytes)?;

        let provenance =
            write_optional_publish_artifacts(state, package, version, &version_root, &upload)?;
        let stored_version = StoredVersion {
            version: version.to_string(),
            published_at: Utc::now().to_rfc3339(),
            yanked: false,
            archive: StoredArchive {
                path: path_to_slash(&archive_relative),
                integrity: upload.metadata.archive_integrity.clone(),
                media_type: ARCHIVE_MEDIA_TYPE.to_string(),
            },
            package_integrity: upload.metadata.package_integrity.clone(),
            provenance,
            publisher: Some(publisher.label.clone()),
            yanked_at: None,
            yanked_by: None,
        };
        package_entry.versions.push(stored_version);
        sort_versions(&mut package_entry.versions);
        package_response_from_stored(package_entry)
    };
    save_registry(&state.root, &registry)?;
    store_idempotency(
        state,
        idempotency_key,
        publisher.token_hash,
        "PUT",
        path,
        upload.digest,
        response.clone(),
    )?;
    Ok(response)
}

fn yank_version_inner(
    state: &AppState,
    package: &str,
    version: &str,
    headers: &HeaderMap,
) -> std::result::Result<StoredHttpResponse, RegistryHttpError> {
    validate_registry_package_name(package).map_err(bad_request)?;
    validate_package_version(version).map_err(bad_request)?;
    let publisher = state.publishers.authenticate(headers, package)?;
    let idempotency_key = idempotency_key(headers)?;
    let path = format!("/v1/packages/{package}/versions/{version}/yank");
    let body_digest = bytes_integrity(&[]);
    if let Some(response) = idempotency_replay(
        state,
        &idempotency_key,
        &publisher.token_hash,
        "POST",
        &path,
        &body_digest,
    )? {
        return Ok(response);
    }

    let mut registry = lock_registry_mut(state)?;
    let mut changed = false;
    let response = {
        let package_entry = registry.packages.get_mut(package).ok_or_else(|| {
            not_found(format!(
                "hosted registry does not contain package {package}"
            ))
        })?;
        let version_entry = package_entry
            .versions
            .iter_mut()
            .find(|stored| stored.version == version)
            .ok_or_else(|| {
                not_found(format!(
                    "hosted registry does not contain package {package} {version}"
                ))
            })?;
        if !version_entry.yanked {
            version_entry.yanked = true;
            version_entry.yanked_at = Some(Utc::now().to_rfc3339());
            version_entry.yanked_by = Some(publisher.label.clone());
            changed = true;
        }
        package_response_from_stored(package_entry)
    };
    if changed {
        save_registry(&state.root, &registry)?;
    }
    store_idempotency(
        state,
        idempotency_key,
        publisher.token_hash,
        "POST",
        path,
        body_digest,
        response.clone(),
    )?;
    Ok(response)
}

fn artifact_response(
    state: &AppState,
    path: &str,
) -> std::result::Result<StoredHttpResponse, RegistryHttpError> {
    let relative = path.trim_start_matches('/');
    if !relative.starts_with("artifacts/") {
        return Err(not_found("hosted registry route not found"));
    }
    validate_registry_relative_path(relative, "artifact").map_err(bad_request)?;
    let path = state.root.join(relative);
    let bytes = fs::read(&path)
        .map_err(|_| not_found(format!("hosted registry artifact {relative:?} not found")))?;
    let media_type = if relative.ends_with(".tar.gz") {
        ARCHIVE_MEDIA_TYPE
    } else {
        "application/octet-stream"
    };
    Ok(StoredHttpResponse {
        status: StatusCode::OK,
        media_type,
        body: bytes,
    })
}

async fn read_publish_upload(
    mut multipart: Multipart,
) -> std::result::Result<PublishUpload, RegistryHttpError> {
    let mut metadata_bytes = None;
    let mut archive_bytes = None;
    let mut provenance_bytes = None;
    let mut signature_bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| bad_request(error.into()))?
    {
        let name = field.name().map(str::to_string).unwrap_or_default();
        let content_type = field.content_type().map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|error| bad_request(error.into()))?
            .to_vec();
        match name.as_str() {
            "metadata" => {
                ensure_single_part(&metadata_bytes, "metadata")?;
                ensure_part_media_type(content_type.as_deref(), PUBLISH_MEDIA_TYPE, "metadata")?;
                if bytes.len() > MAX_METADATA_BYTES {
                    return Err(bad_request(anyhow::anyhow!(
                        "publish metadata is too large"
                    )));
                }
                metadata_bytes = Some(bytes);
            }
            "archive" => {
                ensure_single_part(&archive_bytes, "archive")?;
                ensure_part_media_type(content_type.as_deref(), ARCHIVE_MEDIA_TYPE, "archive")?;
                if bytes.len() > MAX_ARCHIVE_BYTES {
                    return Err(bad_request(anyhow::anyhow!("package archive is too large")));
                }
                archive_bytes = Some(bytes);
            }
            "provenance" => {
                ensure_single_part(&provenance_bytes, "provenance")?;
                provenance_bytes = Some(bytes);
            }
            "signature" => {
                ensure_single_part(&signature_bytes, "signature")?;
                signature_bytes = Some(bytes);
            }
            _ => {
                return Err(bad_request(anyhow::anyhow!(
                    "unsupported multipart field {name:?}"
                )))
            }
        }
    }
    let metadata_bytes = metadata_bytes
        .ok_or_else(|| bad_request(anyhow::anyhow!("publish metadata part is missing")))?;
    let archive_bytes =
        archive_bytes.ok_or_else(|| bad_request(anyhow::anyhow!("archive part is missing")))?;
    let metadata: PublishMetadata = serde_json::from_slice(&metadata_bytes).map_err(|error| {
        bad_request(anyhow::anyhow!(
            "failed to parse publish metadata JSON: {error}"
        ))
    })?;
    let digest = publish_body_digest(
        &metadata_bytes,
        &archive_bytes,
        provenance_bytes.as_deref(),
        signature_bytes.as_deref(),
    );
    Ok(PublishUpload {
        metadata,
        archive_bytes,
        provenance_bytes,
        signature_bytes,
        digest,
    })
}

fn validate_publish_upload(
    package: &str,
    version: &str,
    upload: &PublishUpload,
) -> std::result::Result<(), RegistryHttpError> {
    if upload.metadata.protocol != PROTOCOL {
        return Err(bad_request(anyhow::anyhow!(
            "publish metadata uses unsupported protocol {:?}",
            upload.metadata.protocol
        )));
    }
    validate_registry_package_name(&upload.metadata.package).map_err(bad_request)?;
    validate_package_version(&upload.metadata.version).map_err(bad_request)?;
    if upload.metadata.package != package {
        return Err(bad_request(anyhow::anyhow!(
            "publish metadata package {:?} does not match request package {:?}",
            upload.metadata.package,
            package
        )));
    }
    if upload.metadata.version != version {
        return Err(bad_request(anyhow::anyhow!(
            "publish metadata version {:?} does not match request version {:?}",
            upload.metadata.version,
            version
        )));
    }
    validate_package_integrity(&upload.metadata.archive_integrity).map_err(bad_request)?;
    validate_package_integrity(&upload.metadata.package_integrity).map_err(bad_request)?;
    let archive_integrity = bytes_integrity(&upload.archive_bytes);
    if archive_integrity != upload.metadata.archive_integrity {
        return Err(bad_request(anyhow::anyhow!(
            "archive integrity {archive_integrity} does not match publish metadata {}",
            upload.metadata.archive_integrity
        )));
    }
    validate_publish_artifact(
        "provenance",
        upload.provenance_bytes.as_deref(),
        upload.metadata.provenance_integrity.as_deref(),
    )?;
    validate_publish_artifact(
        "signature",
        upload.signature_bytes.as_deref(),
        upload.metadata.signature_integrity.as_deref(),
    )?;
    if let Some(signature_kind) = upload.metadata.signature_kind.as_deref() {
        validate_signature_kind(signature_kind).map_err(bad_request)?;
        if upload.metadata.signature_integrity.is_none() {
            return Err(bad_request(anyhow::anyhow!(
                "signature_kind requires signature_integrity"
            )));
        }
    }

    let tmp_root = tempfile::Builder::new()
        .prefix("publish-")
        .tempdir()
        .map_err(|error| {
            internal_error(format!(
                "failed to create temporary publish directory: {error}"
            ))
        })?;
    let extract_dir = tmp_root.path().join("package");
    static_registry::extract_package_archive(&upload.archive_bytes, &extract_dir)
        .map_err(bad_request)?;
    let package_metadata = read_package_metadata(&extract_dir).map_err(bad_request)?;
    if package_metadata.name.as_deref() != Some(package) {
        return Err(bad_request(anyhow::anyhow!(
            "archive manifest package name {:?} does not match request package {:?}",
            package_metadata.name,
            package
        )));
    }
    if package_metadata.version.as_deref() != Some(version) {
        return Err(bad_request(anyhow::anyhow!(
            "archive manifest version {:?} does not match request version {:?}",
            package_metadata.version,
            version
        )));
    }
    let package_integrity = package_tree_integrity(&extract_dir).map_err(bad_request)?;
    if package_integrity != upload.metadata.package_integrity {
        return Err(bad_request(anyhow::anyhow!(
            "package tree integrity {package_integrity} does not match publish metadata {}",
            upload.metadata.package_integrity
        )));
    }
    Ok(())
}

fn validate_publish_artifact(
    label: &str,
    bytes: Option<&[u8]>,
    integrity: Option<&str>,
) -> std::result::Result<(), RegistryHttpError> {
    match (bytes, integrity) {
        (None, None) => Ok(()),
        (Some(bytes), Some(integrity)) => {
            validate_package_integrity(integrity).map_err(bad_request)?;
            let actual = bytes_integrity(bytes);
            if actual != integrity {
                return Err(bad_request(anyhow::anyhow!(
                    "{label} integrity {actual} does not match publish metadata {integrity}"
                )));
            }
            Ok(())
        }
        (Some(_), None) => Err(bad_request(anyhow::anyhow!(
            "{label} part requires {label}_integrity in publish metadata"
        ))),
        (None, Some(_)) => Err(bad_request(anyhow::anyhow!(
            "{label}_integrity requires {label} multipart part"
        ))),
    }
}

fn write_optional_publish_artifacts(
    state: &AppState,
    package: &str,
    version: &str,
    version_root: &Path,
    upload: &PublishUpload,
) -> std::result::Result<Option<StoredProvenance>, RegistryHttpError> {
    if upload.provenance_bytes.is_none()
        && upload.signature_bytes.is_none()
        && upload.metadata.signature_kind.is_none()
    {
        return Ok(None);
    }
    let mut provenance = StoredProvenance {
        attestation_path: None,
        attestation_integrity: None,
        signature_path: None,
        signature_integrity: None,
        signature_kind: upload.metadata.signature_kind.clone(),
    };
    if let Some(bytes) = upload.provenance_bytes.as_deref() {
        let relative = artifact_relative_path(package, version, "provenance.attestation");
        write_new_file(&state.root.join(&relative), bytes)?;
        provenance.attestation_path = Some(path_to_slash(&relative));
        provenance.attestation_integrity = upload.metadata.provenance_integrity.clone();
    }
    if let Some(bytes) = upload.signature_bytes.as_deref() {
        let relative = artifact_relative_path(package, version, "signature.sig");
        write_new_file(&state.root.join(&relative), bytes)?;
        provenance.signature_path = Some(path_to_slash(&relative));
        provenance.signature_integrity = upload.metadata.signature_integrity.clone();
    }
    fs::create_dir_all(version_root).map_err(|error| {
        internal_error(format!(
            "failed to create {}: {error}",
            version_root.display()
        ))
    })?;
    Ok(Some(provenance))
}

fn ensure_single_part<T>(
    current: &Option<T>,
    name: &str,
) -> std::result::Result<(), RegistryHttpError> {
    if current.is_some() {
        return Err(bad_request(anyhow::anyhow!(
            "duplicate multipart field {name:?}"
        )));
    }
    Ok(())
}

fn ensure_part_media_type(
    actual: Option<&str>,
    expected: &str,
    label: &str,
) -> std::result::Result<(), RegistryHttpError> {
    let actual = actual.ok_or_else(|| {
        bad_request(anyhow::anyhow!(
            "{label} multipart part must include Content-Type {expected}"
        ))
    })?;
    let media_type = actual.split(';').next().unwrap_or("").trim();
    if media_type.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(bad_request(anyhow::anyhow!(
        "{label} multipart part has Content-Type {actual:?}, expected {expected}"
    )))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> std::result::Result<(), RegistryHttpError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            internal_error(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| internal_error(format!("failed to create {}: {error}", path.display())))?;
    file.write_all(bytes)
        .map_err(|error| internal_error(format!("failed to write {}: {error}", path.display())))
}

fn package_response_from_stored(package: &StoredPackage) -> StoredHttpResponse {
    let latest =
        latest_non_yanked_version(&package.versions).map(|version| version.version.clone());
    json_stored_response(
        StatusCode::OK,
        PACKAGE_MEDIA_TYPE,
        json!({
            "protocol": PROTOCOL,
            "package": {
                "name": package.name,
                "latest": latest,
            },
            "versions": package.versions,
        }),
    )
}

fn load_registry(root: &Path) -> Result<RegistryDocument> {
    let path = root.join(REGISTRY_STATE_FILE);
    if !path.is_file() {
        return Ok(RegistryDocument::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let registry: RegistryDocument = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_registry_document(&registry)?;
    Ok(registry)
}

fn save_registry(
    root: &Path,
    registry: &RegistryDocument,
) -> std::result::Result<(), RegistryHttpError> {
    let path = root.join(REGISTRY_STATE_FILE);
    let text = serde_json::to_string_pretty(registry)
        .map_err(|error| internal_error(format!("failed to encode registry metadata: {error}")))?;
    fs::write(&path, text)
        .map_err(|error| internal_error(format!("failed to write {}: {error}", path.display())))
}

fn validate_registry_document(registry: &RegistryDocument) -> Result<()> {
    if registry.protocol != PROTOCOL {
        bail!(
            "unsupported hosted registry state protocol {:?}",
            registry.protocol
        );
    }
    for (package_name, package) in &registry.packages {
        validate_registry_package_name(package_name)?;
        if package.name != *package_name {
            bail!(
                "hosted registry state package key {:?} does not match package name {:?}",
                package_name,
                package.name
            );
        }
        let mut seen_versions = std::collections::BTreeSet::new();
        for version in &package.versions {
            validate_package_version(&version.version)?;
            if !seen_versions.insert(version.version.clone()) {
                bail!(
                    "hosted registry state package {package_name} has duplicate version {}",
                    version.version
                );
            }
            validate_registry_relative_path(&version.archive.path, "archive")?;
            validate_package_integrity(&version.archive.integrity)?;
            validate_package_integrity(&version.package_integrity)?;
            if version.archive.media_type != ARCHIVE_MEDIA_TYPE {
                bail!(
                    "hosted registry state package {package_name} {} has unsupported archive media type {:?}",
                    version.version,
                    version.archive.media_type
                );
            }
            if let Some(provenance) = &version.provenance {
                validate_optional_artifact_pair(
                    &provenance.attestation_path,
                    &provenance.attestation_integrity,
                    "provenance attestation",
                )?;
                validate_optional_artifact_pair(
                    &provenance.signature_path,
                    &provenance.signature_integrity,
                    "signature",
                )?;
                if let Some(kind) = provenance.signature_kind.as_deref() {
                    validate_signature_kind(kind)?;
                    if provenance.signature_path.is_none()
                        || provenance.signature_integrity.is_none()
                    {
                        bail!("hosted registry state package {package_name} {} has signature_kind without signature", version.version);
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_optional_artifact_pair(
    path: &Option<String>,
    integrity: &Option<String>,
    label: &str,
) -> Result<()> {
    match (path.as_deref(), integrity.as_deref()) {
        (None, None) => Ok(()),
        (Some(path), Some(integrity)) => {
            validate_registry_relative_path(path, label)?;
            validate_package_integrity(integrity)
        }
        (Some(_), None) => bail!("{label} path is present without integrity"),
        (None, Some(_)) => bail!("{label} integrity is present without path"),
    }
}

impl PublisherPolicies {
    fn from_options(token_envs: &[String], publishers: &[String]) -> Result<Self> {
        let mut policies = Vec::new();
        for token_env in token_envs {
            policies.push(load_publisher_policy(
                PublisherPattern::All,
                token_env,
                format!("{token_env}:*"),
            )?);
        }
        for publisher in publishers {
            let (pattern, token_env) = publisher
                .split_once('=')
                .with_context(|| format!("publisher policy {publisher:?} must use PACKAGE=ENV"))?;
            let pattern = parse_publisher_pattern(pattern)?;
            policies.push(load_publisher_policy(
                pattern,
                token_env,
                publisher.to_string(),
            )?);
        }
        Ok(Self { policies })
    }

    fn authenticate(
        &self,
        headers: &HeaderMap,
        package: &str,
    ) -> std::result::Result<AuthenticatedPublisher, RegistryHttpError> {
        if self.policies.is_empty() {
            return Err(RegistryHttpError {
                status: StatusCode::UNAUTHORIZED,
                code: "publisher_auth_not_configured",
                message: "hosted registry server has no publisher token policy configured"
                    .to_string(),
                details: None,
            });
        }
        let token = bearer_token(headers)?;
        let token_hash = bytes_integrity(token.as_bytes());
        let mut token_known = false;
        for policy in &self.policies {
            if policy.token_hash != token_hash {
                continue;
            }
            token_known = true;
            if policy.pattern.matches(package) {
                return Ok(AuthenticatedPublisher {
                    token_hash,
                    label: policy.label.clone(),
                });
            }
        }
        if token_known {
            Err(RegistryHttpError {
                status: StatusCode::FORBIDDEN,
                code: "publisher_forbidden",
                message: format!("publisher token is not authorized for package {package}"),
                details: None,
            })
        } else {
            Err(RegistryHttpError {
                status: StatusCode::UNAUTHORIZED,
                code: "invalid_bearer_token",
                message: "missing or invalid hosted registry bearer token".to_string(),
                details: None,
            })
        }
    }
}

impl PublisherPattern {
    fn matches(&self, package: &str) -> bool {
        match self {
            PublisherPattern::All => true,
            PublisherPattern::Exact(expected) => expected == package,
            PublisherPattern::Scope(scope) => package
                .strip_prefix('@')
                .and_then(|rest| rest.split_once('/').map(|(scope, _)| scope))
                .is_some_and(|actual| actual == scope),
        }
    }
}

fn parse_publisher_pattern(pattern: &str) -> Result<PublisherPattern> {
    if pattern == "*" {
        return Ok(PublisherPattern::All);
    }
    if let Some(scope) = pattern
        .strip_prefix('@')
        .and_then(|rest| rest.strip_suffix("/*"))
    {
        validate_registry_package_segment(scope, "scope", pattern)?;
        return Ok(PublisherPattern::Scope(scope.to_string()));
    }
    validate_registry_package_name(pattern)?;
    Ok(PublisherPattern::Exact(pattern.to_string()))
}

fn load_publisher_policy(
    pattern: PublisherPattern,
    token_env: &str,
    label: String,
) -> Result<PublisherPolicy> {
    validate_env_name(token_env)?;
    let token = env::var(token_env).with_context(|| {
        format!("hosted registry publisher token env var {token_env} is not set")
    })?;
    if token.is_empty() {
        bail!("hosted registry publisher token env var {token_env} is empty");
    }
    Ok(PublisherPolicy {
        pattern,
        token_hash: bytes_integrity(token.as_bytes()),
        label,
    })
}

fn validate_env_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("environment variable name must not be empty");
    }
    if name.contains('=') {
        bail!("environment variable name must not contain =");
    }
    if name.contains('\0') {
        bail!("environment variable name must not contain NUL");
    }
    Ok(())
}

fn validate_registry_package_segment(segment: &str, label: &str, package: &str) -> Result<()> {
    if segment.is_empty()
        || !segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!(
            "invalid {label} in Ricochet package policy {package:?}; use letters, numbers, _ or -"
        );
    }
    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> std::result::Result<String, RegistryHttpError> {
    let header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| RegistryHttpError {
            status: StatusCode::UNAUTHORIZED,
            code: "missing_bearer_token",
            message: "Authorization: Bearer token is required".to_string(),
            details: None,
        })?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| RegistryHttpError {
            status: StatusCode::UNAUTHORIZED,
            code: "missing_bearer_token",
            message: "Authorization: Bearer token is required".to_string(),
            details: None,
        })?;
    if token.is_empty() {
        return Err(RegistryHttpError {
            status: StatusCode::UNAUTHORIZED,
            code: "missing_bearer_token",
            message: "Authorization: Bearer token is required".to_string(),
            details: None,
        });
    }
    Ok(token.to_string())
}

fn idempotency_key(headers: &HeaderMap) -> std::result::Result<String, RegistryHttpError> {
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| bad_request(anyhow::anyhow!("Idempotency-Key header is required")))?;
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_whitespace) {
        return Err(bad_request(anyhow::anyhow!(
            "Idempotency-Key must be non-empty, at most 128 characters, and contain no whitespace"
        )));
    }
    Ok(key.to_string())
}

fn idempotency_replay(
    state: &AppState,
    key: &str,
    token_hash: &str,
    method: &'static str,
    path: &str,
    body_digest: &str,
) -> std::result::Result<Option<StoredHttpResponse>, RegistryHttpError> {
    let records = state
        .idempotency
        .lock()
        .map_err(|_| internal_error("hosted registry idempotency state is poisoned"))?;
    let Some(record) = records.get(key) else {
        return Ok(None);
    };
    if record.token_hash == token_hash
        && record.method == method
        && record.path == path
        && record.body_digest == body_digest
    {
        return Ok(Some(record.response.clone()));
    }
    Err(RegistryHttpError {
        status: StatusCode::CONFLICT,
        code: "idempotency_conflict",
        message: "Idempotency-Key was replayed with a different publisher, path, method, or body"
            .to_string(),
        details: None,
    })
}

fn store_idempotency(
    state: &AppState,
    key: String,
    token_hash: String,
    method: &'static str,
    path: String,
    body_digest: String,
    response: StoredHttpResponse,
) -> std::result::Result<(), RegistryHttpError> {
    state
        .idempotency
        .lock()
        .map_err(|_| internal_error("hosted registry idempotency state is poisoned"))?
        .insert(
            key,
            IdempotencyRecord {
                token_hash,
                method,
                path,
                body_digest,
                response,
            },
        );
    Ok(())
}

fn publish_body_digest(
    metadata: &[u8],
    archive: &[u8],
    provenance: Option<&[u8]>,
    signature: Option<&[u8]>,
) -> String {
    let mut hasher = Sha256::new();
    update_digest_part(&mut hasher, b"metadata", metadata);
    update_digest_part(&mut hasher, b"archive", archive);
    if let Some(provenance) = provenance {
        update_digest_part(&mut hasher, b"provenance", provenance);
    }
    if let Some(signature) = signature {
        update_digest_part(&mut hasher, b"signature", signature);
    }
    format!("sha256:{}", hex_digest(&hasher.finalize()))
}

fn update_digest_part(hasher: &mut Sha256, label: &[u8], bytes: &[u8]) {
    hasher.update(label);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn sort_versions(versions: &mut [StoredVersion]) {
    versions.sort_by(|left, right| {
        Version::parse(&left.version)
            .expect("validated version should parse")
            .cmp(&Version::parse(&right.version).expect("validated version should parse"))
    });
}

fn latest_non_yanked_version(versions: &[StoredVersion]) -> Option<&StoredVersion> {
    versions
        .iter()
        .filter(|version| !version.yanked)
        .filter_map(|version| {
            Version::parse(&version.version)
                .ok()
                .map(|parsed| (parsed, version))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, version)| version)
}

fn artifact_relative_path(package: &str, version: &str, leaf: &str) -> PathBuf {
    PathBuf::from("artifacts")
        .join(registry_package_relative_path(package))
        .join(version)
        .join(leaf)
}

fn validate_registry_relative_path(path: &str, label: &str) -> Result<()> {
    if path.is_empty() {
        bail!("{label} must be a registry-relative path");
    }
    if path.contains(':') {
        bail!("{label} must be a registry-relative path, got {path:?}");
    }
    if path.starts_with('/') {
        bail!("{label} must not start with /");
    }
    if path.contains('\\') {
        bail!("{label} path must use forward slashes");
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            bail!("{label} path must not contain empty, . or .. path segments");
        }
    }
    Ok(())
}

fn lock_registry(
    state: &AppState,
) -> std::result::Result<std::sync::MutexGuard<'_, RegistryDocument>, RegistryHttpError> {
    state
        .registry
        .lock()
        .map_err(|_| internal_error("hosted registry state is poisoned"))
}

fn lock_registry_mut(
    state: &AppState,
) -> std::result::Result<std::sync::MutexGuard<'_, RegistryDocument>, RegistryHttpError> {
    lock_registry(state)
}

fn json_stored_response(
    status: StatusCode,
    media_type: &'static str,
    value: serde_json::Value,
) -> StoredHttpResponse {
    StoredHttpResponse {
        status,
        media_type,
        body: serde_json::to_vec(&value).expect("JSON values should serialize"),
    }
}

fn error_stored_response(
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<serde_json::Value>,
) -> StoredHttpResponse {
    json_stored_response(
        status,
        ERROR_MEDIA_TYPE,
        json!({
            "error": {
                "code": code,
                "message": message,
                "details": details.unwrap_or_else(|| json!({})),
            }
        }),
    )
}

fn json_response(
    status: StatusCode,
    media_type: &'static str,
    value: serde_json::Value,
) -> Response {
    json_stored_response(status, media_type, value).into_response()
}

impl IntoResponse for StoredHttpResponse {
    fn into_response(self) -> Response {
        Response::builder()
            .status(self.status)
            .header(header::CONTENT_TYPE, self.media_type)
            .body(Body::from(self.body))
            .expect("response builder should accept static headers")
    }
}

impl IntoResponse for RegistryHttpError {
    fn into_response(self) -> Response {
        error_stored_response(self.status, self.code, self.message, self.details).into_response()
    }
}

fn bad_request(error: anyhow::Error) -> RegistryHttpError {
    RegistryHttpError {
        status: StatusCode::BAD_REQUEST,
        code: "bad_request",
        message: error.to_string(),
        details: None,
    }
}

fn not_found(message: impl Into<String>) -> RegistryHttpError {
    RegistryHttpError {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: message.into(),
        details: None,
    }
}

fn internal_error(message: impl Into<String>) -> RegistryHttpError {
    RegistryHttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal_error",
        message: message.into(),
        details: None,
    }
}

fn format_base_url(address: SocketAddr) -> String {
    if address.ip().is_ipv6() {
        format!("http://[{}]:{}", address.ip(), address.port())
    } else {
        format!("http://{}:{}", address.ip(), address.port())
    }
}
