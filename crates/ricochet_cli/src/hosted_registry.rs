use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Response;
use reqwest::Url;
use semver::{Version, VersionReq};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::{
    bytes_integrity, package_tree_integrity, package_version_satisfies, project_dependency_path,
    read_package_metadata, static_registry, validate_package_integrity, validate_package_version,
    validate_registry_package_name, validate_signature_kind, DependencySpec, LockedPackage,
};

const PROTOCOL: &str = "ricochet-hosted-registry-v1";
const DISCOVERY_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.v1+json";
const SEARCH_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.search.v1+json";
const PACKAGE_MEDIA_TYPE: &str = "application/vnd.ricochet.registry.package.v1+json";
const ARCHIVE_MEDIA_TYPE: &str = "application/vnd.ricochet.package.archive.v1+gzip";
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

#[derive(Debug, Deserialize)]
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

pub(super) fn search(query: &str, registry_url: &str) -> Result<()> {
    let discovery = discover(registry_url)?;
    let mut search_url = endpoint_url(&discovery.base_url, &["v1", "search"])?;
    search_url
        .query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", "50")
        .append_pair("offset", "0");
    let response: HostedSearchResponse = read_json(
        search_url,
        MAX_METADATA_BYTES,
        SEARCH_MEDIA_TYPE,
        "hosted registry search response",
    )?;
    ensure_protocol(&response.protocol, "hosted registry search response")?;
    let packages = response.packages.or(response.results).with_context(|| {
        "hosted registry search response must include packages or results array"
    })?;

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
        .unwrap_or(requested_base);
    Ok(HostedRegistryDiscovery { base_url })
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
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(FETCH_TIMEOUT)
            .build()
            .context("failed to build hosted registry HTTP client")?;
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

fn validate_response_content_type(
    response: &Response,
    expected: &str,
    label: &str,
    url: &str,
) -> Result<()> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .with_context(|| format!("{label} {url} must include Content-Type {expected}"))?;
    let content_type = content_type
        .to_str()
        .with_context(|| format!("{label} {url} has invalid Content-Type header"))?;
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if media_type.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    bail!("{label} {url} returned Content-Type {content_type:?}, expected {expected}")
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
