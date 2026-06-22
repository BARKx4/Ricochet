use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::blocking::Response;
use reqwest::Method;
use reqwest::Url;
use semver::{Version, VersionReq};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, Item, Table};

use super::{
    bytes_integrity, hex_digest, package_tree_integrity, package_version_satisfies, path_to_slash,
    project_dependency_path, read_package_metadata, static_registry, validate_package_integrity,
    validate_package_version, validate_registry_package_name, validate_signature_kind,
    DependencySpec, LockedPackage, PublishArtifact,
};

const PROTOCOL: &str = "ricochet-hosted-registry-v1";
const DISCOVERY_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.v1+json";
const SEARCH_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.search.v1+json";
const PACKAGE_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.package.v1+json";
const ARCHIVE_MEDIA_TYPE: &str = "application/vnd.ricochet.package.archive.v1+gzip";
const PUBLISH_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.publish.v1+json";
const ERROR_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.error.v1+json";
const PUBLISH_ACCEPT: &str = "application/vnd.ricochet.registry.package.v1+json, application/vnd.ricochet.registry.v1+json, application/vnd.ricochet.registry.error.v1+json";
const YANK_ACCEPT: &str = "application/vnd.ricochet.registry.package.v1+json, application/vnd.ricochet.registry.error.v1+json";
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct HostedRegistryDiscovery {
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct HostedDiscoveryResponse {
    protocol: String,
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HostedSearchResponse {
    protocol: String,
    packages: Option<Vec<HostedSearchPackage>>,
    results: Option<Vec<HostedSearchPackage>>,
}

#[derive(Debug, Clone, Deserialize)]
struct HostedSearchPackage {
    name: String,
    latest: String,
}

#[derive(Debug, Deserialize)]
struct HostedPackageMetadata {
    protocol: String,
    package: HostedPackageInfo,
    versions: Vec<HostedVersion>,
}

#[derive(Debug, Deserialize)]
struct HostedPackageInfo {
    name: String,
    latest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HostedVersion {
    version: String,
    yanked: bool,
    archive: HostedArchive,
    package_integrity: String,
    provenance: Option<HostedProvenance>,
}

#[derive(Debug, Deserialize)]
struct HostedArchive {
    path: String,
    integrity: String,
    media_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HostedProvenance {
    attestation_path: Option<String>,
    attestation_integrity: Option<String>,
    signature_path: Option<String>,
    signature_integrity: Option<String>,
    signature_kind: Option<String>,
}

enum HostedCacheValidation<'a> {
    Existing { dependency_name: &'a str },
    Extracted,
}

#[derive(Debug)]
pub(super) struct HostedPublishOptions<'a> {
    pub(super) package_root: &'a Path,
    pub(super) package: &'a str,
    pub(super) version: &'a str,
    pub(super) package_integrity: &'a str,
    pub(super) registry_url: &'a str,
    pub(super) token_env: Option<&'a str>,
    pub(super) dry_run: bool,
    pub(super) provenance: Option<&'a PublishArtifact>,
    pub(super) signature: Option<&'a PublishArtifact>,
    pub(super) signature_kind: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct HostedPublishMetadata<'a> {
    protocol: &'static str,
    package: &'a str,
    version: &'a str,
    package_integrity: &'a str,
    archive_integrity: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance_integrity: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_integrity: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_kind: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct HostedErrorEnvelope {
    error: Option<HostedErrorBody>,
}

#[derive(Debug, Deserialize)]
struct HostedErrorBody {
    code: Option<String>,
    message: Option<String>,
    details: Option<serde_json::Value>,
}

struct HostedMutationRequest {
    method: Method,
    url: Url,
    accept: &'static str,
    token: String,
    idempotency_key: String,
    content_type: Option<String>,
    body: Vec<u8>,
    label: &'static str,
    duplicate_conflict_hint: bool,
}

pub(super) fn is_hosted_source(registry: &str) -> bool {
    registry.starts_with("https://") || registry.starts_with("http://")
}

pub(super) fn validate_base_url(registry_url: &str) -> Result<String> {
    let mut url = Url::parse(registry_url)
        .with_context(|| format!("invalid hosted registry URL {registry_url:?}"))?;
    if url.cannot_be_a_base() {
        bail!("hosted registry URL {registry_url:?} must be a base URL");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("hosted registry URL {registry_url:?} must not include query or fragment");
    }
    match url.scheme() {
        "https" => {}
        "http" if is_allowed_loopback_http(&url) => {}
        "http" => {
            bail!("hosted registry URL {registry_url:?} must use https:// outside loopback tests")
        }
        scheme => bail!("hosted registry URL {registry_url:?} has unsupported scheme {scheme:?}"),
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_string())
}

pub(super) fn publish(options: HostedPublishOptions<'_>) -> Result<()> {
    validate_registry_package_name(options.package)?;
    validate_package_version(options.version)?;
    validate_base_url(options.registry_url)?;
    if let Some(token_env) = options.token_env {
        validate_token_env_name(token_env)?;
    }

    if options.dry_run {
        println!(
            "would publish {} {} to {} with integrity {}",
            options.package, options.version, options.registry_url, options.package_integrity
        );
        if let Some(provenance) = options.provenance {
            println!("would attach provenance {}", provenance.integrity);
        }
        if let Some(signature) = options.signature {
            println!(
                "would attach {} signature {}",
                options.signature_kind.unwrap_or("detached"),
                signature.integrity
            );
        }
        return Ok(());
    }

    let token_env = options
        .token_env
        .context("--token-env is required for hosted publish unless --dry-run is used")?;
    let token = resolve_bearer_token(token_env)?;
    let archive_bytes = static_registry::create_package_archive_bytes(options.package_root)?;
    let archive_integrity = bytes_integrity(&archive_bytes);
    let provenance_bytes = options
        .provenance
        .map(read_publish_artifact_bytes)
        .transpose()?;
    let signature_bytes = options
        .signature
        .map(read_publish_artifact_bytes)
        .transpose()?;
    let discovery = discover(options.registry_url)?;
    let publish_url = endpoint_url(
        &discovery.base_url,
        &[
            "v1",
            "packages",
            options.package,
            "versions",
            options.version,
        ],
    )?;
    let metadata = HostedPublishMetadata {
        protocol: PROTOCOL,
        package: options.package,
        version: options.version,
        package_integrity: options.package_integrity,
        archive_integrity: &archive_integrity,
        provenance_integrity: options
            .provenance
            .map(|artifact| artifact.integrity.as_str()),
        signature_integrity: options
            .signature
            .map(|artifact| artifact.integrity.as_str()),
        signature_kind: options.signature_kind,
    };
    let metadata_bytes =
        serde_json::to_vec(&metadata).context("failed to encode hosted publish metadata")?;
    let idempotency_key = generate_idempotency_key()?;
    let boundary = format!("ricochet-{idempotency_key}");
    let mut body = Vec::new();
    push_multipart_part(
        &mut body,
        &boundary,
        "metadata",
        None,
        PUBLISH_MEDIA_TYPE,
        &metadata_bytes,
    );
    push_multipart_part(
        &mut body,
        &boundary,
        "archive",
        Some("package.tar.gz"),
        ARCHIVE_MEDIA_TYPE,
        &archive_bytes,
    );
    if let (Some(provenance), Some(bytes)) = (options.provenance, provenance_bytes.as_deref()) {
        push_multipart_part(
            &mut body,
            &boundary,
            "provenance",
            Some(provenance.target),
            "application/octet-stream",
            bytes,
        );
    }
    if let (Some(signature), Some(bytes)) = (options.signature, signature_bytes.as_deref()) {
        push_multipart_part(
            &mut body,
            &boundary,
            "signature",
            Some(signature.target),
            "application/octet-stream",
            bytes,
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    send_mutation(HostedMutationRequest {
        method: Method::PUT,
        url: publish_url,
        accept: PUBLISH_ACCEPT,
        token,
        idempotency_key,
        content_type: Some(content_type),
        body,
        label: "hosted registry publish response",
        duplicate_conflict_hint: true,
    })?;

    println!(
        "published {} {} to {} with integrity {}",
        options.package, options.version, discovery.base_url, options.package_integrity
    );
    Ok(())
}

pub(super) fn yank(
    package: &str,
    version: &str,
    registry_url: &str,
    token_env: &str,
) -> Result<()> {
    validate_registry_package_name(package)?;
    validate_package_version(version)?;
    validate_base_url(registry_url)?;
    validate_token_env_name(token_env)?;
    let token = resolve_bearer_token(token_env)?;
    let discovery = discover(registry_url)?;
    let yank_url = endpoint_url(
        &discovery.base_url,
        &["v1", "packages", package, "versions", version, "yank"],
    )?;
    let idempotency_key = generate_idempotency_key()?;
    send_mutation(HostedMutationRequest {
        method: Method::POST,
        url: yank_url,
        accept: YANK_ACCEPT,
        token,
        idempotency_key,
        content_type: None,
        body: Vec::new(),
        label: "hosted registry yank response",
        duplicate_conflict_hint: false,
    })?;
    println!("yanked {package} {version} from {}", discovery.base_url);
    Ok(())
}

pub(super) fn search(query: &str, registry_url: &str) -> Result<()> {
    let discovery = discover(registry_url)?;
    let packages = search_packages(&discovery, query, 50, 0)?;

    let mut found = 0usize;
    for package in packages {
        validate_registry_package_name(&package.name)?;
        validate_package_version(&package.latest)?;
        println!("{} {}", package.name, package.latest);
        found += 1;
    }
    if found == 0 {
        println!("no packages found");
    }
    Ok(())
}

pub(super) fn mirror(registry_url: &str, output_path: &Path) -> Result<()> {
    let discovery = discover(registry_url)?;
    let output_root = super::absolute_path_from_current(output_path)?;
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let mut package_names = BTreeSet::new();
    let mut offset = 0usize;
    let limit = 100usize;
    loop {
        let packages = search_packages(&discovery, "", limit, offset)?;
        let count = packages.len();
        for package in packages {
            validate_registry_package_name(&package.name)?;
            package_names.insert(package.name);
        }
        if count < limit {
            break;
        }
        offset += limit;
    }

    let mut index_packages = BTreeMap::new();
    let mut mirrored_versions = 0usize;
    for package in &package_names {
        let metadata = load_package(&discovery, package)?;
        let metadata_relative = mirror_metadata_relative_path(package);
        write_mirror_package_metadata(&discovery, &output_root, &metadata, &metadata_relative)?;
        index_packages.insert(package.clone(), path_to_slash(&metadata_relative));
        mirrored_versions += metadata.versions.len();
    }
    write_mirror_index(&output_root, &index_packages)?;
    println!(
        "mirrored {} hosted registry packages ({} versions) from {} to {}",
        package_names.len(),
        mirrored_versions,
        discovery.base_url,
        output_root.display()
    );
    Ok(())
}

pub(super) fn install_dependency(
    project_root: &Path,
    spec: &mut DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<()> {
    let registry = spec
        .registry
        .as_deref()
        .expect("install_hosted_registry_dependency only handles registry dependencies");
    let discovery = discover(registry)?;
    refresh_spec_registry(spec, &discovery.base_url);

    let package_name = spec.registry_package_name().to_string();
    let metadata = load_package(&discovery, &package_name)?;
    let version = select_version(&metadata, spec, locked)?;
    validate_locked_version(&metadata, spec, locked, version)?;

    let package_cache =
        project_dependency_path(project_root, &spec.path, "hosted registry package cache")?;
    if package_cache.exists() {
        validate_package_cache(
            &package_cache,
            &metadata,
            version,
            HostedCacheValidation::Existing {
                dependency_name: &spec.name,
            },
        )?;
    } else {
        if let Some(parent) = package_cache.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
            super::ensure_existing_project_dir(
                project_root,
                parent,
                "hosted registry package cache parent",
            )?;
        }
        let archive_url = resolve_archive_url(&discovery.base_url, &version.archive.path)?;
        let archive_bytes = read_bytes(
            archive_url,
            MAX_ARCHIVE_BYTES,
            ARCHIVE_MEDIA_TYPE,
            "hosted registry package archive",
        )?;
        let actual_archive_integrity = bytes_integrity(&archive_bytes);
        if actual_archive_integrity != version.archive.integrity {
            bail!(
                "hosted registry archive for {} {} has integrity {}, expected {}",
                metadata.package.name,
                version.version,
                actual_archive_integrity,
                version.archive.integrity
            );
        }
        static_registry::extract_package_archive(&archive_bytes, &package_cache)?;
        validate_package_cache(
            &package_cache,
            &metadata,
            version,
            HostedCacheValidation::Extracted,
        )?;
    }

    spec.package_version = Some(version.version.clone());
    spec.archive_integrity = Some(version.archive.integrity.clone());
    spec.integrity = Some(version.package_integrity.clone());
    spec.provenance = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.attestation_integrity.clone());
    spec.signature = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.signature_integrity.clone());
    spec.signature_kind = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.signature_kind.clone());
    Ok(())
}

fn validate_package_cache(
    package_cache: &Path,
    metadata: &HostedPackageMetadata,
    version: &HostedVersion,
    validation: HostedCacheValidation<'_>,
) -> Result<()> {
    let cached_integrity = package_tree_integrity(package_cache)?;
    if cached_integrity != version.package_integrity {
        match validation {
            HostedCacheValidation::Existing { dependency_name } => {
                bail!(
                    "hosted registry package cache for {} already exists with integrity {cached_integrity}, expected {}; remove {} or choose a different dependency name",
                    dependency_name,
                    version.package_integrity,
                    package_cache.display()
                );
            }
            HostedCacheValidation::Extracted => {
                bail!(
                    "hosted registry archive for {} {} unpacked to integrity {}, expected {}",
                    metadata.package.name,
                    version.version,
                    cached_integrity,
                    version.package_integrity
                );
            }
        }
    }

    let cached_metadata = read_package_metadata(package_cache)?;
    if cached_metadata.name.as_deref() != Some(&metadata.package.name) {
        bail!(
            "hosted registry package cache for {} {} has manifest package name {:?}",
            metadata.package.name,
            version.version,
            cached_metadata.name
        );
    }
    if cached_metadata.version.as_deref() != Some(&version.version) {
        bail!(
            "hosted registry package cache for {} {} has manifest version {:?}",
            metadata.package.name,
            version.version,
            cached_metadata.version
        );
    }
    Ok(())
}

fn discover(registry_url: &str) -> Result<HostedRegistryDiscovery> {
    let requested_base = validate_base_url(registry_url)?;
    let discovery_url = endpoint_url(&requested_base, &["v1"])?;
    let response: HostedDiscoveryResponse = read_json(
        discovery_url,
        MAX_METADATA_BYTES,
        DISCOVERY_MEDIA_TYPE,
        "hosted registry discovery response",
    )?;
    ensure_protocol(&response.protocol, "hosted registry discovery response")?;
    let base_url = response
        .base_url
        .as_deref()
        .map(validate_base_url)
        .transpose()?
        .unwrap_or_else(|| requested_base.clone());
    ensure_discovered_base_same_origin(&requested_base, &base_url)?;
    Ok(HostedRegistryDiscovery { base_url })
}

fn ensure_discovered_base_same_origin(requested_base: &str, discovered_base: &str) -> Result<()> {
    let requested = Url::parse(requested_base)
        .with_context(|| format!("invalid hosted registry base URL {requested_base:?}"))?;
    let discovered = Url::parse(discovered_base)
        .with_context(|| format!("invalid hosted registry base URL {discovered_base:?}"))?;
    if !same_origin(&requested, &discovered) {
        bail!(
            "hosted registry discovery base_url {discovered_base:?} must stay on requested registry origin {requested_base:?}"
        );
    }
    Ok(())
}

fn search_packages(
    discovery: &HostedRegistryDiscovery,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<HostedSearchPackage>> {
    let mut search_url = endpoint_url(&discovery.base_url, &["v1", "search"])?;
    search_url
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string());
    let response: HostedSearchResponse = read_json(
        search_url,
        MAX_METADATA_BYTES,
        SEARCH_MEDIA_TYPE,
        "hosted registry search response",
    )?;
    ensure_protocol(&response.protocol, "hosted registry search response")?;
    response
        .packages
        .or(response.results)
        .with_context(|| "hosted registry search response must include packages or results array")
}

fn load_package(
    discovery: &HostedRegistryDiscovery,
    expected_package: &str,
) -> Result<HostedPackageMetadata> {
    let metadata_url = endpoint_url(&discovery.base_url, &["v1", "packages", expected_package])?;
    let metadata: HostedPackageMetadata = read_json(
        metadata_url,
        MAX_METADATA_BYTES,
        PACKAGE_MEDIA_TYPE,
        "hosted registry package metadata",
    )?;
    validate_package_metadata(metadata, expected_package)
}

fn validate_package_metadata(
    metadata: HostedPackageMetadata,
    expected_package: &str,
) -> Result<HostedPackageMetadata> {
    ensure_protocol(&metadata.protocol, "hosted registry package metadata")?;
    validate_registry_package_name(&metadata.package.name)?;
    if metadata.package.name != expected_package {
        bail!(
            "hosted registry package metadata name {:?} does not match requested package {:?}",
            metadata.package.name,
            expected_package
        );
    }
    if let Some(latest) = metadata.package.latest.as_deref() {
        validate_package_version(latest)?;
    }

    let mut seen_versions = BTreeSet::new();
    for version in &metadata.versions {
        validate_package_version(&version.version)?;
        if !seen_versions.insert(version.version.clone()) {
            bail!(
                "hosted registry package {} lists duplicate version {}",
                metadata.package.name,
                version.version
            );
        }
        validate_registry_relative_path(&version.archive.path, "archive")?;
        validate_package_integrity(&version.archive.integrity)?;
        validate_package_integrity(&version.package_integrity)?;
        if let Some(media_type) = version.archive.media_type.as_deref() {
            if media_type != ARCHIVE_MEDIA_TYPE {
                bail!(
                    "hosted registry archive for {} {} has unsupported media type {:?}",
                    metadata.package.name,
                    version.version,
                    media_type
                );
            }
        }
        if let Some(provenance) = &version.provenance {
            validate_optional_artifact_pair(
                &metadata.package.name,
                &version.version,
                "provenance attestation",
                provenance.attestation_path.as_deref(),
                provenance.attestation_integrity.as_deref(),
            )?;
            validate_optional_artifact_pair(
                &metadata.package.name,
                &version.version,
                "signature",
                provenance.signature_path.as_deref(),
                provenance.signature_integrity.as_deref(),
            )?;
            if let Some(signature_kind) = provenance.signature_kind.as_deref() {
                validate_signature_kind(signature_kind)?;
                if provenance.signature_path.is_none() || provenance.signature_integrity.is_none() {
                    bail!(
                        "hosted registry version {} {} has signature_kind without signature metadata",
                        metadata.package.name,
                        version.version
                    );
                }
            }
        }
    }
    Ok(metadata)
}

fn write_mirror_index(root: &Path, packages: &BTreeMap<String, String>) -> Result<()> {
    let mut doc = DocumentMut::new();
    let mut registry_table = Table::new();
    registry_table["format"] = value("ricochet-static-registry-v1");
    doc.as_table_mut()
        .insert("registry", Item::Table(registry_table));
    let mut packages_table = Table::new();
    for (package, metadata_path) in packages {
        packages_table[package] = value(metadata_path.clone());
    }
    doc.as_table_mut()
        .insert("packages", Item::Table(packages_table));
    fs::write(root.join("index.toml"), doc.to_string())
        .with_context(|| format!("failed to write {}", root.join("index.toml").display()))
}

fn write_mirror_package_metadata(
    discovery: &HostedRegistryDiscovery,
    root: &Path,
    metadata: &HostedPackageMetadata,
    metadata_relative: &Path,
) -> Result<()> {
    let metadata_path = root.join(metadata_relative);
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut doc = DocumentMut::new();
    let mut package_table = Table::new();
    package_table["name"] = value(metadata.package.name.clone());
    doc.as_table_mut()
        .insert("package", Item::Table(package_table));

    let versions_array = doc["versions"].or_insert(Item::ArrayOfTables(Default::default()));
    let versions_array = versions_array
        .as_array_of_tables_mut()
        .expect("versions should be an array of tables");
    for version in &metadata.versions {
        mirror_artifact(
            discovery,
            root,
            &version.archive.path,
            &version.archive.integrity,
            ARCHIVE_MEDIA_TYPE,
            "hosted registry package archive",
        )?;
        let mut table = Table::new();
        table["version"] = value(version.version.clone());
        table["archive"] = value(version.archive.path.clone());
        table["archive_integrity"] = value(version.archive.integrity.clone());
        table["package_integrity"] = value(version.package_integrity.clone());
        table["yanked"] = value(version.yanked);
        if let Some(provenance) = &version.provenance {
            if let (Some(path), Some(integrity)) = (
                provenance.attestation_path.as_deref(),
                provenance.attestation_integrity.as_deref(),
            ) {
                mirror_artifact(
                    discovery,
                    root,
                    path,
                    integrity,
                    "application/octet-stream",
                    "hosted registry provenance artifact",
                )?;
                table["provenance"] = value(integrity.to_string());
            }
            if let (Some(path), Some(integrity)) = (
                provenance.signature_path.as_deref(),
                provenance.signature_integrity.as_deref(),
            ) {
                mirror_artifact(
                    discovery,
                    root,
                    path,
                    integrity,
                    "application/octet-stream",
                    "hosted registry signature artifact",
                )?;
                table["signature"] = value(integrity.to_string());
            }
            if let Some(signature_kind) = provenance.signature_kind.as_deref() {
                table["signature_kind"] = value(signature_kind.to_string());
            }
        }
        versions_array.push(table);
    }

    fs::write(&metadata_path, doc.to_string())
        .with_context(|| format!("failed to write {}", metadata_path.display()))
}

fn mirror_artifact(
    discovery: &HostedRegistryDiscovery,
    root: &Path,
    relative_path: &str,
    expected_integrity: &str,
    expected_media_type: &'static str,
    label: &'static str,
) -> Result<()> {
    validate_registry_relative_path(relative_path, label)?;
    validate_package_integrity(expected_integrity)?;
    let destination = root.join(relative_path);
    if destination.exists() {
        let bytes = fs::read(&destination)
            .with_context(|| format!("failed to read {}", destination.display()))?;
        let actual = bytes_integrity(&bytes);
        if actual != expected_integrity {
            bail!(
                "static mirror artifact {} already exists with integrity {}, expected {}",
                destination.display(),
                actual,
                expected_integrity
            );
        }
        return Ok(());
    }
    let url = resolve_archive_url(&discovery.base_url, relative_path)?;
    let bytes = if expected_media_type == ARCHIVE_MEDIA_TYPE {
        read_bytes(url, MAX_ARCHIVE_BYTES, ARCHIVE_MEDIA_TYPE, label)?
    } else {
        read_bytes_any_media(url, MAX_ARCHIVE_BYTES, label)?
    };
    let actual = bytes_integrity(&bytes);
    if actual != expected_integrity {
        bail!(
            "{label} {relative_path:?} has integrity {}, expected {}",
            actual,
            expected_integrity
        );
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&destination, bytes)
        .with_context(|| format!("failed to write {}", destination.display()))
}

fn mirror_metadata_relative_path(package: &str) -> PathBuf {
    PathBuf::from("packages")
        .join(registry_package_relative_path(package))
        .with_extension("toml")
}

fn registry_package_relative_path(package: &str) -> PathBuf {
    package.split('/').collect()
}

fn validate_optional_artifact_pair(
    package: &str,
    version: &str,
    label: &str,
    path: Option<&str>,
    integrity: Option<&str>,
) -> Result<()> {
    match (path, integrity) {
        (None, None) => Ok(()),
        (Some(path), Some(integrity)) => {
            validate_registry_relative_path(path, label)?;
            validate_package_integrity(integrity)
        }
        (Some(_), None) => {
            bail!("hosted registry version {package} {version} has {label} path without integrity")
        }
        (None, Some(_)) => {
            bail!("hosted registry version {package} {version} has {label} integrity without path")
        }
    }
}

fn latest_version<'a>(
    versions: &'a [HostedVersion],
    requirement: Option<&str>,
) -> Option<&'a HostedVersion> {
    let requirement = requirement.and_then(|req| VersionReq::parse(req).ok());
    let mut candidates = versions
        .iter()
        .filter(|version| !version.yanked)
        .filter_map(|version| {
            let parsed = Version::parse(&version.version).ok()?;
            if requirement
                .as_ref()
                .is_some_and(|requirement| !requirement.matches(&parsed))
            {
                return None;
            }
            Some((parsed, version))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|(_, version)| version)
}

fn select_version<'a>(
    metadata: &'a HostedPackageMetadata,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<&'a HostedVersion> {
    if let Some(locked_version) = locked.and_then(|lock| lock.package_version.as_deref()) {
        if package_version_satisfies(spec.version_req.as_deref(), locked_version)? {
            let version = metadata
                .versions
                .iter()
                .find(|version| version.version == locked_version)
                .with_context(|| {
                    format!(
                        "hosted registry package {} locked version {} is not present in the current registry metadata",
                        metadata.package.name, locked_version
                    )
                })?;
            return Ok(version);
        }
    }
    latest_version(&metadata.versions, spec.version_req.as_deref()).with_context(|| {
        let requirement = spec.version_req.as_deref().unwrap_or("*");
        format!(
            "hosted registry package {} has no non-yanked version satisfying {}",
            metadata.package.name, requirement
        )
    })
}

fn validate_locked_version(
    metadata: &HostedPackageMetadata,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
    version: &HostedVersion,
) -> Result<()> {
    let Some(locked) = locked else {
        return Ok(());
    };
    if locked.package_version.as_deref() != Some(version.version.as_str()) {
        return Ok(());
    }
    let provenance = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.attestation_integrity.as_deref());
    let signature = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.signature_integrity.as_deref());
    let signature_kind = version
        .provenance
        .as_ref()
        .and_then(|provenance| provenance.signature_kind.as_deref());

    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "source",
        Some(locked.source.as_str()),
        Some(spec.source.as_str()),
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "registry",
        locked.registry.as_deref(),
        spec.registry.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "package",
        locked.package.as_deref(),
        spec.package.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "archive_integrity",
        locked.archive_integrity.as_deref(),
        Some(version.archive.integrity.as_str()),
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "integrity",
        locked.integrity.as_deref(),
        Some(version.package_integrity.as_str()),
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "provenance",
        locked.provenance.as_deref(),
        provenance,
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "signature",
        locked.signature.as_deref(),
        signature,
    )?;
    ensure_locked_field(
        &metadata.package.name,
        &version.version,
        "signature_kind",
        locked.signature_kind.as_deref(),
        signature_kind,
    )
}

fn ensure_locked_field(
    package_name: &str,
    version: &str,
    field: &str,
    locked: Option<&str>,
    current: Option<&str>,
) -> Result<()> {
    if locked == current {
        return Ok(());
    }
    let locked = locked.unwrap_or("<missing>");
    let current = current.unwrap_or("<missing>");
    bail!(
        "hosted registry package {package_name} {version} {field} changed: lockfile has {locked}, registry has {current}; refusing ordinary install"
    );
}

fn refresh_spec_registry(spec: &mut DependencySpec, registry: &str) {
    if spec.registry.as_deref() == Some(registry) {
        return;
    }
    let package = spec.registry_package_name().to_string();
    spec.registry = Some(registry.to_string());
    spec.source = format!("registry+{registry}#{package}");
    spec.display_source = if package == spec.name {
        format!("registry:{package} from {registry}")
    } else {
        format!("registry:{package} as {} from {registry}", spec.name)
    };
}

fn validate_token_env_name(name: &str) -> Result<&str> {
    if name.is_empty() {
        bail!("environment variable name must not be empty");
    }
    if name.contains('=') {
        bail!("environment variable name must not contain =");
    }
    if name.contains('\0') {
        bail!("environment variable name must not contain NUL");
    }
    Ok(name)
}

fn resolve_bearer_token(name: &str) -> Result<String> {
    validate_token_env_name(name)?;
    let token = env::var(name)
        .with_context(|| format!("hosted registry token env var {name} is not set"))?;
    if token.is_empty() {
        bail!("hosted registry token env var {name} is empty");
    }
    Ok(token)
}

fn read_publish_artifact_bytes(artifact: &PublishArtifact) -> Result<Vec<u8>> {
    let bytes = fs::read(&artifact.source)
        .with_context(|| format!("failed to read {}", artifact.source.display()))?;
    let integrity = bytes_integrity(&bytes);
    if integrity != artifact.integrity {
        bail!(
            "publish artifact {} changed while reading: expected {}, got {}",
            artifact.target,
            artifact.integrity,
            integrity
        );
    }
    Ok(bytes)
}

fn generate_idempotency_key() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("failed to generate idempotency key: {error}"))?;
    Ok(hex_digest(&bytes))
}

fn push_multipart_part(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: Option<&str>,
    content_type: &str,
    bytes: &[u8],
) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"");
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(b"\"");
    if let Some(filename) = filename {
        body.extend_from_slice(b"; filename=\"");
        body.extend_from_slice(filename.as_bytes());
        body.extend_from_slice(b"\"");
    }
    body.extend_from_slice(b"\r\nContent-Type: ");
    body.extend_from_slice(content_type.as_bytes());
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

fn send_mutation(request: HostedMutationRequest) -> Result<()> {
    let url_text = request.url.to_string();
    let url_text_for_thread = url_text.clone();
    let result = thread::spawn(move || -> Result<()> {
        let client = http_client()?;
        let mut http_request = client
            .request(request.method, request.url)
            .header(reqwest::header::ACCEPT, request.accept)
            .header("Idempotency-Key", request.idempotency_key)
            .bearer_auth(&request.token)
            .body(request.body);
        if let Some(content_type) = request.content_type {
            http_request = http_request.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        let response = http_request
            .send()
            .with_context(|| format!("failed to send {} {url_text_for_thread}", request.label))?;
        let status = response.status();
        if !status.is_success() {
            let error_suffix =
                registry_error_suffix(response, request.label, &url_text_for_thread, &request.token)
                    .unwrap_or_default();
            if request.duplicate_conflict_hint && status == reqwest::StatusCode::CONFLICT {
                bail!(
                    "hosted registry duplicate version/version exists: HTTP status {status}{error_suffix}"
                );
            }
            bail!(
                "{} {url_text_for_thread} returned non-success HTTP status {status}{error_suffix}",
                request.label
            );
        }
        validate_response_content_type_one_of(
            &response,
            &[PACKAGE_MEDIA_TYPE, DISCOVERY_MEDIA_TYPE],
            request.label,
            &url_text_for_thread,
        )?;
        let bytes =
            read_limited_response(response, MAX_METADATA_BYTES, request.label, &url_text_for_thread)?;
        validate_success_body_protocol(&bytes, request.label)
    })
    .join();

    match result {
        Ok(result) => result,
        Err(_) => bail!("hosted registry mutation worker panicked for {url_text}"),
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(FETCH_TIMEOUT)
        .build()
        .context("failed to build hosted registry HTTP client")
}

fn registry_error_suffix(
    response: Response,
    label: &str,
    url: &str,
    bearer_token: &str,
) -> Result<String> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or("").trim().to_string());
    let bytes = read_limited_response(response, MAX_METADATA_BYTES, label, url)?;
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let looks_like_registry_error = content_type
        .as_deref()
        .is_some_and(|media_type| media_type.eq_ignore_ascii_case(ERROR_MEDIA_TYPE));
    if !looks_like_registry_error {
        return Ok(String::new());
    }
    let envelope: HostedErrorEnvelope =
        serde_json::from_slice(&bytes).context("failed to parse hosted registry error JSON")?;
    let Some(error) = envelope.error else {
        return Ok(String::new());
    };
    let mut parts = Vec::new();
    if let Some(code) = error.code {
        parts.push(redact_bearer_token_echo(&code, bearer_token));
    }
    if let Some(message) = error.message {
        parts.push(redact_bearer_token_echo(&message, bearer_token));
    }
    if let Some(details) = error.details.and_then(|details| {
        registry_error_details_suffix_part(redact_bearer_token_echo_in_json(details, bearer_token))
    }) {
        parts.push(details);
    }
    if parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(" ({})", parts.join(": ")))
    }
}

fn redact_bearer_token_echo(value: &str, bearer_token: &str) -> String {
    if bearer_token.is_empty() {
        return value.to_string();
    }
    value
        .replace(&format!("Bearer {bearer_token}"), "Bearer [redacted token]")
        .replace(bearer_token, "[redacted token]")
}

fn redact_bearer_token_echo_in_json(
    value: serde_json::Value,
    bearer_token: &str,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(value) => {
            serde_json::Value::String(redact_bearer_token_echo(&value, bearer_token))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| redact_bearer_token_echo_in_json(value, bearer_token))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    (
                        redact_bearer_token_echo(&key, bearer_token),
                        redact_bearer_token_echo_in_json(value, bearer_token),
                    )
                })
                .collect(),
        ),
        value => value,
    }
}

fn registry_error_details_suffix_part(details: serde_json::Value) -> Option<String> {
    match details {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) if value.is_empty() => None,
        serde_json::Value::String(value) => Some(format!("details {value}")),
        serde_json::Value::Array(values) if values.is_empty() => None,
        serde_json::Value::Object(values) if values.is_empty() => None,
        details => serde_json::to_string(&details)
            .ok()
            .map(|details| format!("details {details}")),
    }
}

fn validate_success_body_protocol(bytes: &[u8], label: &str) -> Result<()> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    let body: serde_json::Value =
        serde_json::from_slice(bytes).with_context(|| format!("failed to parse {label} JSON"))?;
    let protocol = body
        .get("protocol")
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("{label} must include protocol"))?;
    ensure_protocol(protocol, label)
}

fn read_json<T: DeserializeOwned>(
    url: Url,
    limit: usize,
    accept: &'static str,
    label: &'static str,
) -> Result<T> {
    let bytes = read_bytes(url, limit, accept, label)?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {label} JSON"))
}

fn read_bytes(
    url: Url,
    limit: usize,
    accept: &'static str,
    label: &'static str,
) -> Result<Vec<u8>> {
    let url_text = url.to_string();
    let url_text_for_thread = url_text.clone();
    let result = thread::spawn(move || -> Result<Vec<u8>> {
        let client = http_client()?;
        let response = client
            .get(url)
            .header(reqwest::header::ACCEPT, accept)
            .send()
            .with_context(|| format!("failed to fetch {label} {url_text_for_thread}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("{label} {url_text_for_thread} returned non-success HTTP status {status}");
        }
        validate_response_content_type(&response, accept, label, &url_text_for_thread)?;
        read_limited_response(response, limit, label, &url_text_for_thread)
    })
    .join();

    match result {
        Ok(result) => result,
        Err(_) => bail!("hosted registry fetch worker panicked for {url_text}"),
    }
}

fn read_bytes_any_media(url: Url, limit: usize, label: &'static str) -> Result<Vec<u8>> {
    let url_text = url.to_string();
    let url_text_for_thread = url_text.clone();
    let result = thread::spawn(move || -> Result<Vec<u8>> {
        let client = http_client()?;
        let response = client
            .get(url)
            .send()
            .with_context(|| format!("failed to fetch {label} {url_text_for_thread}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("{label} {url_text_for_thread} returned non-success HTTP status {status}");
        }
        read_limited_response(response, limit, label, &url_text_for_thread)
    })
    .join();

    match result {
        Ok(result) => result,
        Err(_) => bail!("hosted registry fetch worker panicked for {url_text}"),
    }
}

fn validate_response_content_type(
    response: &Response,
    expected: &str,
    label: &str,
    url: &str,
) -> Result<()> {
    validate_response_content_type_one_of(response, &[expected], label, url)
}

fn validate_response_content_type_one_of(
    response: &Response,
    expected: &[&str],
    label: &str,
    url: &str,
) -> Result<()> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .with_context(|| {
            format!(
                "{label} {url} must include Content-Type {}",
                expected.join(", ")
            )
        })?;
    let content_type = content_type
        .to_str()
        .with_context(|| format!("{label} {url} has invalid Content-Type header"))?;
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if expected
        .iter()
        .any(|expected| media_type.eq_ignore_ascii_case(expected))
    {
        return Ok(());
    }
    bail!(
        "{label} {url} returned Content-Type {content_type:?}, expected {}",
        expected.join(", ")
    )
}

fn read_limited_response(
    response: Response,
    limit: usize,
    label: &str,
    url: &str,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!("{label} {url} is too large");
    }
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {label} {url}"))?;
    if bytes.len() > limit {
        bail!("{label} {url} is too large: {} bytes", bytes.len());
    }
    Ok(bytes)
}

fn endpoint_url(base_url: &str, segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse(base_url)
        .with_context(|| format!("invalid hosted registry base URL {base_url:?}"))?;
    {
        let mut path_segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("hosted registry URL {base_url:?} cannot be a base"))?;
        for segment in segments {
            path_segments.push(segment);
        }
    }
    Ok(url)
}

fn resolve_archive_url(base_url: &str, path: &str) -> Result<Url> {
    validate_registry_relative_path(path, "archive")?;
    let base = Url::parse(base_url)
        .with_context(|| format!("invalid hosted registry base URL {base_url:?}"))?;
    let mut resolved = base.clone();
    {
        let mut path_segments = resolved
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("hosted registry URL {base_url:?} cannot be a base"))?;
        for segment in path.split('/') {
            path_segments.push(segment);
        }
    }
    if !same_origin(&base, &resolved) {
        bail!("archive path {path:?} resolves outside hosted registry origin");
    }
    Ok(resolved)
}

fn validate_registry_relative_path(path: &str, label: &str) -> Result<()> {
    if path.is_empty() {
        bail!("{label} must be a registry-relative path");
    }
    if has_url_scheme(path) {
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

fn has_url_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn ensure_protocol(protocol: &str, label: &str) -> Result<()> {
    if protocol != PROTOCOL {
        bail!("{label} uses unsupported protocol {protocol:?}");
    }
    Ok(())
}

fn is_allowed_loopback_http(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_base_url_must_stay_on_requested_origin() {
        ensure_discovered_base_same_origin(
            "https://registry.example.test/api",
            "https://registry.example.test/api/v2",
        )
        .expect("same-origin discovery base should be accepted");

        let error = ensure_discovered_base_same_origin(
            "https://registry.example.test/api",
            "https://attacker.example.test/api",
        )
        .expect_err("cross-origin discovery base should be rejected");

        assert!(
            error
                .to_string()
                .contains("must stay on requested registry origin"),
            "unexpected error: {error:#}"
        );
    }
}
