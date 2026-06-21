use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use semver::{Version, VersionReq};
use tar::{Archive, Builder, EntryType};
use toml_edit::{value, DocumentMut, Item, Table};

use super::{
    absolute_path_from_current, bytes_integrity, default_dependency_alias, file_integrity,
    package_tree_integrity, package_version_satisfies, path_to_slash, project_dependency_path,
    read_package_metadata, registry_package_at, registry_package_relative_path,
    validate_package_integrity, validate_package_version, validate_project_relative_path,
    validate_registry_package_name, validate_signature_kind, DependencySpec, LockedPackage,
};

const FORMAT: &str = "ricochet-static-registry-v1";
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_ARCHIVE_ENTRY_BYTES: usize = 64 * 1024 * 1024;
const MAX_UNPACKED_BYTES: usize = 128 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct StaticRegistryIndex {
    source: String,
    packages: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct StaticRegistryPackageMetadata {
    name: String,
    versions: Vec<StaticRegistryVersion>,
}

#[derive(Debug, Clone)]
struct StaticRegistryVersion {
    version: String,
    archive: String,
    archive_integrity: String,
    package_integrity: String,
    yanked: bool,
    provenance: Option<String>,
    signature: Option<String>,
    signature_kind: Option<String>,
}

pub(super) fn rebuild(path: &Path) -> Result<()> {
    let registry_root = absolute_path_from_current(path)?;
    if !registry_root.is_dir() {
        bail!(
            "static registry rebuild expected an existing registry directory: {}",
            registry_root.display()
        );
    }

    let mut packages: BTreeMap<String, Vec<StaticRegistryVersion>> = BTreeMap::new();
    for package_root in local_registry_package_roots(&registry_root)? {
        for version_entry in fs::read_dir(&package_root)
            .with_context(|| format!("failed to read {}", package_root.display()))?
        {
            let version_entry = version_entry
                .with_context(|| format!("failed to read entry in {}", package_root.display()))?;
            let version_root = version_entry.path();
            if !version_entry
                .file_type()
                .with_context(|| format!("failed to inspect {}", version_root.display()))?
                .is_dir()
            {
                continue;
            }
            let version = version_entry.file_name().to_string_lossy().to_string();
            validate_package_version(&version)?;
            let package_dir = version_root.join("package");
            if !package_dir.is_dir() {
                continue;
            }
            let metadata = read_package_metadata(&package_dir)?;
            let package_name = metadata.name.with_context(|| {
                format!(
                    "registry package {} is missing [package] name",
                    package_dir.display()
                )
            })?;
            validate_registry_package_name(&package_name)?;
            let registry_package = registry_package_at(&package_root, &package_name, &version)
                .with_context(|| {
                    format!("failed to validate registry package {package_name} {version}")
                })?;

            let archive_relative = archive_relative_path(&package_name, &version);
            let archive_path = registry_root.join(&archive_relative);
            create_package_archive(&registry_package.package_dir, &archive_path)?;
            let archive_integrity = file_integrity(&archive_path)?;

            packages
                .entry(package_name)
                .or_default()
                .push(StaticRegistryVersion {
                    version,
                    archive: path_to_slash(&archive_relative),
                    archive_integrity,
                    package_integrity: registry_package.integrity,
                    yanked: false,
                    provenance: registry_package.provenance,
                    signature: registry_package.signature,
                    signature_kind: registry_package.signature_kind,
                });
        }
    }

    if packages.is_empty() {
        bail!(
            "registry {} does not contain any publishable packages",
            registry_root.display()
        );
    }

    let mut index_doc = DocumentMut::new();
    let mut registry_table = Table::new();
    registry_table["format"] = value(FORMAT);
    index_doc
        .as_table_mut()
        .insert("registry", Item::Table(registry_table));
    let mut packages_table = Table::new();
    for (package, versions) in packages.iter_mut() {
        versions.sort_by(|left, right| {
            Version::parse(&left.version)
                .expect("validated package version should parse")
                .cmp(
                    &Version::parse(&right.version)
                        .expect("validated package version should parse"),
                )
        });
        let metadata_relative = metadata_relative_path(package);
        write_package_metadata(&registry_root, package, versions, &metadata_relative)?;
        packages_table[package] = value(path_to_slash(&metadata_relative));
    }
    index_doc
        .as_table_mut()
        .insert("packages", Item::Table(packages_table));
    fs::write(registry_root.join("index.toml"), index_doc.to_string()).with_context(|| {
        format!(
            "failed to write {}",
            registry_root.join("index.toml").display()
        )
    })?;

    println!(
        "rebuilt static registry {} with {} packages",
        registry_root.display(),
        packages.len()
    );
    Ok(())
}

pub(super) fn check(path: &Path) -> Result<()> {
    let registry_root = absolute_path_from_current(path)?;
    let index_source = file_url_from_path(&registry_root.join("index.toml"));
    let index = load_index(&index_source)?;
    let mut checked = 0usize;
    for (package, metadata_path) in &index.packages {
        let metadata = load_package(&index.source, package, metadata_path)?;
        for version in metadata.versions {
            validate_package_integrity(&version.archive_integrity)?;
            validate_package_integrity(&version.package_integrity)?;
            let archive_source = resolve_resource(&index.source, &version.archive)?;
            let archive_path = file_url_to_path(&archive_source).with_context(|| {
                format!(
                    "rco registry check requires local file archives, got {}",
                    archive_source
                )
            })?;
            let actual = file_integrity(&archive_path)?;
            if actual != version.archive_integrity {
                bail!(
                    "static registry archive for {} {} has integrity {}, expected {}",
                    metadata.name,
                    version.version,
                    actual,
                    version.archive_integrity
                );
            }
            checked += 1;
        }
    }
    println!("checked {checked} static registry versions");
    Ok(())
}

pub(super) fn search(
    query: &str,
    registry: Option<&Path>,
    registry_url: Option<&str>,
) -> Result<()> {
    if registry.is_some() && registry_url.is_some() {
        bail!("use either --registry or --registry-url, not both");
    }
    let index_source = if let Some(registry_url) = registry_url {
        validate_url(registry_url)?.to_string()
    } else {
        let registry = registry
            .map(absolute_path_from_current)
            .transpose()?
            .unwrap_or_else(|| PathBuf::from("."));
        file_url_from_path(&registry.join("index.toml"))
    };
    let index = load_index(&index_source)?;
    let query = query.to_ascii_lowercase();
    let mut found = 0usize;
    for (package, metadata_path) in &index.packages {
        if !package.to_ascii_lowercase().contains(&query) {
            continue;
        }
        let metadata = load_package(&index.source, package, metadata_path)?;
        let Some(version) = latest_version(&metadata.versions, None) else {
            continue;
        };
        println!("{} {}", metadata.name, version.version);
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
        .expect("install_static_registry_dependency only handles registry dependencies");
    let index = load_index(registry)?;
    let package_name = spec.registry_package_name();
    let metadata_path = index
        .packages
        .get(package_name)
        .with_context(|| format!("static registry does not contain package {package_name}"))?;
    let metadata = load_package(&index.source, package_name, metadata_path)?;
    let version = select_version(&metadata, spec, locked)?;
    validate_locked_version(&metadata, spec, locked, version)?;
    let package_cache =
        project_dependency_path(project_root, &spec.path, "static registry package cache")?;

    if package_cache.exists() {
        let cached_integrity = package_tree_integrity(&package_cache)?;
        if cached_integrity != version.package_integrity {
            bail!(
                "static registry package cache for {} already exists with integrity {cached_integrity}, expected {}; remove {} or choose a different dependency name",
                spec.name,
                version.package_integrity,
                package_cache.display()
            );
        }
    } else {
        if let Some(parent) = package_cache.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
            super::ensure_existing_project_dir(
                project_root,
                parent,
                "static registry package cache parent",
            )?;
        }
        let archive_source = resolve_resource(&index.source, &version.archive)?;
        let archive_bytes = read_bytes(&archive_source, MAX_ARCHIVE_BYTES)?;
        let actual_archive_integrity = bytes_integrity(&archive_bytes);
        if actual_archive_integrity != version.archive_integrity {
            bail!(
                "static registry archive for {} {} has integrity {}, expected {}",
                metadata.name,
                version.version,
                actual_archive_integrity,
                version.archive_integrity
            );
        }
        extract_package_archive(&archive_bytes, &package_cache)?;
        let extracted_metadata = read_package_metadata(&package_cache)?;
        if extracted_metadata.name.as_deref() != Some(&metadata.name) {
            bail!(
                "static registry archive for {} {} has manifest package name {:?}",
                metadata.name,
                version.version,
                extracted_metadata.name
            );
        }
        if extracted_metadata.version.as_deref() != Some(&version.version) {
            bail!(
                "static registry archive for {} {} has manifest version {:?}",
                metadata.name,
                version.version,
                extracted_metadata.version
            );
        }
        let extracted_integrity = package_tree_integrity(&package_cache)?;
        if extracted_integrity != version.package_integrity {
            bail!(
                "static registry archive for {} {} unpacked to integrity {}, expected {}",
                metadata.name,
                version.version,
                extracted_integrity,
                version.package_integrity
            );
        }
    }

    spec.package_version = Some(version.version.clone());
    spec.integrity = Some(version.package_integrity.clone());
    spec.provenance = version.provenance.clone();
    spec.signature = version.signature.clone();
    spec.signature_kind = version.signature_kind.clone();
    Ok(())
}

pub(super) fn is_static_source(registry: &str) -> bool {
    if registry.starts_with("file://") {
        return true;
    }
    if !(registry.starts_with("http://") || registry.starts_with("https://")) {
        return false;
    }
    reqwest::Url::parse(registry)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .is_some_and(|leaf| leaf == "index.toml")
}

pub(super) fn validate_url(registry_url: &str) -> Result<&str> {
    if registry_url.starts_with("https://") || registry_url.starts_with("file://") {
        Ok(registry_url)
    } else {
        bail!("static registry URL {registry_url:?} must start with https:// or file://");
    }
}

fn local_registry_package_roots(registry_root: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for entry in fs::read_dir(registry_root)
        .with_context(|| format!("failed to read {}", registry_root.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", registry_root.display()))?;
        if !entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "packages" || name == "artifacts" {
            continue;
        }
        if name.starts_with('@') {
            for scoped_entry in fs::read_dir(entry.path())
                .with_context(|| format!("failed to read {}", entry.path().display()))?
            {
                let scoped_entry = scoped_entry.with_context(|| {
                    format!("failed to read entry in {}", entry.path().display())
                })?;
                if scoped_entry
                    .file_type()
                    .with_context(|| {
                        format!("failed to inspect {}", scoped_entry.path().display())
                    })?
                    .is_dir()
                {
                    roots.push(scoped_entry.path());
                }
            }
        } else {
            roots.push(entry.path());
        }
    }
    roots.sort();
    Ok(roots)
}

fn write_package_metadata(
    registry_root: &Path,
    package: &str,
    versions: &[StaticRegistryVersion],
    metadata_relative: &Path,
) -> Result<()> {
    let metadata_path = registry_root.join(metadata_relative);
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut doc = DocumentMut::new();
    let mut package_table = Table::new();
    package_table["name"] = value(package);
    doc.as_table_mut()
        .insert("package", Item::Table(package_table));

    let versions_array = doc["versions"].or_insert(Item::ArrayOfTables(Default::default()));
    let versions_array = versions_array
        .as_array_of_tables_mut()
        .expect("versions should be an array of tables");
    for version in versions {
        let mut table = Table::new();
        table["version"] = value(version.version.clone());
        table["archive"] = value(version.archive.clone());
        table["archive_integrity"] = value(version.archive_integrity.clone());
        table["package_integrity"] = value(version.package_integrity.clone());
        table["yanked"] = value(version.yanked);
        if let Some(provenance) = &version.provenance {
            table["provenance"] = value(provenance.clone());
        }
        if let Some(signature) = &version.signature {
            table["signature"] = value(signature.clone());
        }
        if let Some(signature_kind) = &version.signature_kind {
            table["signature_kind"] = value(signature_kind.clone());
        }
        versions_array.push(table);
    }

    fs::write(&metadata_path, doc.to_string())
        .with_context(|| format!("failed to write {}", metadata_path.display()))
}

fn create_package_archive(package_dir: &Path, archive_path: &Path) -> Result<()> {
    package_tree_integrity(package_dir)?;
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let file = fs::File::create(archive_path)
        .with_context(|| format!("failed to create {}", archive_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    append_package_archive_entries(package_dir, package_dir, &mut builder)?;
    builder
        .finish()
        .with_context(|| format!("failed to finish {}", archive_path.display()))?;
    let encoder = builder
        .into_inner()
        .with_context(|| format!("failed to finish {}", archive_path.display()))?;
    encoder
        .finish()
        .with_context(|| format!("failed to finish {}", archive_path.display()))?;
    Ok(())
}

fn append_package_archive_entries(
    root: &Path,
    current: &Path,
    builder: &mut Builder<GzEncoder<fs::File>>,
) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", current.display()))?;
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "package archive cannot include symlink {}; copy the target file into the package",
                path.display()
            );
        }
        if metadata.is_dir() {
            if file_name == ".git" {
                continue;
            }
            append_package_archive_entries(root, &path, builder)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to make {} package-relative", path.display()))?;
            builder
                .append_path_with_name(&path, relative)
                .with_context(|| format!("failed to archive {}", path.display()))?;
        }
    }
    Ok(())
}

fn load_index(source: &str) -> Result<StaticRegistryIndex> {
    validate_url(source)?;
    let bytes = read_bytes(source, MAX_METADATA_BYTES)?;
    let text = String::from_utf8(bytes).context("static registry index must be UTF-8")?;
    let doc = text
        .parse::<DocumentMut>()
        .context("failed to parse static registry index")?;
    let format = doc
        .get("registry")
        .and_then(Item::as_table)
        .and_then(|registry| registry.get("format"))
        .and_then(Item::as_str)
        .context("static registry index must include [registry] format")?;
    if format != FORMAT {
        bail!("unsupported static registry format {format:?}");
    }
    let packages_table = doc
        .get("packages")
        .and_then(Item::as_table)
        .context("static registry index must include [packages]")?;
    let mut packages = BTreeMap::new();
    for (package, item) in packages_table.iter() {
        validate_registry_package_name(package)?;
        let metadata = item
            .as_str()
            .with_context(|| format!("static registry package {package} must map to a string"))?;
        validate_relative_path(metadata, "package metadata")?;
        packages.insert(package.to_string(), metadata.to_string());
    }
    Ok(StaticRegistryIndex {
        source: source.to_string(),
        packages,
    })
}

fn load_package(
    index_source: &str,
    expected_package: &str,
    metadata_path: &str,
) -> Result<StaticRegistryPackageMetadata> {
    let metadata_source = resolve_resource(index_source, metadata_path)?;
    let bytes = read_bytes(&metadata_source, MAX_METADATA_BYTES)?;
    let text =
        String::from_utf8(bytes).context("static registry package metadata must be UTF-8")?;
    let doc = text
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse static registry package {expected_package}"))?;
    let package = doc
        .get("package")
        .and_then(Item::as_table)
        .and_then(|package| package.get("name"))
        .and_then(Item::as_str)
        .context("static registry package metadata must include [package] name")?;
    validate_registry_package_name(package)?;
    if package != expected_package {
        bail!(
            "static registry package metadata name {:?} does not match index package {:?}",
            package,
            expected_package
        );
    }
    let versions_array = doc
        .get("versions")
        .and_then(Item::as_array_of_tables)
        .context("static registry package metadata must include [[versions]]")?;
    let mut versions = Vec::new();
    let mut seen_versions = BTreeSet::new();
    for table in versions_array {
        let version = table
            .get("version")
            .and_then(Item::as_str)
            .context("static registry version must include version")?
            .to_string();
        validate_package_version(&version)?;
        if !seen_versions.insert(version.clone()) {
            bail!("static registry package {package} lists duplicate version {version}");
        }
        let archive = table
            .get("archive")
            .and_then(Item::as_str)
            .context("static registry version must include archive")?
            .to_string();
        validate_relative_path(&archive, "archive")?;
        let archive_integrity = table
            .get("archive_integrity")
            .and_then(Item::as_str)
            .context("static registry version must include archive_integrity")?
            .to_string();
        validate_package_integrity(&archive_integrity)?;
        let package_integrity = table
            .get("package_integrity")
            .and_then(Item::as_str)
            .context("static registry version must include package_integrity")?
            .to_string();
        validate_package_integrity(&package_integrity)?;
        let yanked = table.get("yanked").and_then(Item::as_bool).unwrap_or(false);
        let provenance = table
            .get("provenance")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(provenance) = provenance.as_deref() {
            validate_package_integrity(provenance)?;
        }
        let signature = table
            .get("signature")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(signature) = signature.as_deref() {
            validate_package_integrity(signature)?;
        }
        let signature_kind = table
            .get("signature_kind")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(signature_kind) = signature_kind.as_deref() {
            validate_signature_kind(signature_kind)?;
        }
        if signature_kind.is_some() && signature.is_none() {
            bail!(
                "static registry version {package} {version} has signature_kind without signature"
            );
        }
        versions.push(StaticRegistryVersion {
            version,
            archive,
            archive_integrity,
            package_integrity,
            yanked,
            provenance,
            signature,
            signature_kind,
        });
    }
    Ok(StaticRegistryPackageMetadata {
        name: package.to_string(),
        versions,
    })
}

fn latest_version<'a>(
    versions: &'a [StaticRegistryVersion],
    requirement: Option<&str>,
) -> Option<&'a StaticRegistryVersion> {
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
    metadata: &'a StaticRegistryPackageMetadata,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<&'a StaticRegistryVersion> {
    if let Some(locked_version) = locked.and_then(|lock| lock.package_version.as_deref()) {
        if package_version_satisfies(spec.version_req.as_deref(), locked_version)? {
            let version = metadata
                .versions
                .iter()
                .find(|version| version.version == locked_version)
                .with_context(|| {
                    format!(
                        "static registry package {} locked version {} is not present in the current registry metadata",
                        metadata.name, locked_version
                    )
                })?;
            return Ok(version);
        }
    }
    latest_version(&metadata.versions, spec.version_req.as_deref()).with_context(|| {
        let requirement = spec.version_req.as_deref().unwrap_or("*");
        format!(
            "static registry package {} has no version satisfying {}",
            metadata.name, requirement
        )
    })
}

fn validate_locked_version(
    metadata: &StaticRegistryPackageMetadata,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
    version: &StaticRegistryVersion,
) -> Result<()> {
    let Some(locked) = locked else {
        return Ok(());
    };
    if locked.package_version.as_deref() != Some(version.version.as_str()) {
        return Ok(());
    }

    ensure_locked_field(
        &metadata.name,
        &version.version,
        "source",
        Some(locked.source.as_str()),
        Some(spec.source.as_str()),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "registry",
        locked.registry.as_deref(),
        spec.registry.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "package",
        locked.package.as_deref(),
        spec.package.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "integrity",
        locked.integrity.as_deref(),
        Some(version.package_integrity.as_str()),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "provenance",
        locked.provenance.as_deref(),
        version.provenance.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "signature",
        locked.signature.as_deref(),
        version.signature.as_deref(),
    )?;
    ensure_locked_field(
        &metadata.name,
        &version.version,
        "signature_kind",
        locked.signature_kind.as_deref(),
        version.signature_kind.as_deref(),
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
        "static registry package {package_name} {version} {field} changed: lockfile has {locked}, registry has {current}; refusing ordinary install"
    );
}

fn validate_relative_path(path: &str, label: &str) -> Result<()> {
    if has_url_scheme(path) {
        bail!("{label} must be a registry-relative path, got {path:?}");
    }
    validate_project_relative_path(path, label)
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

fn resolve_resource(index_source: &str, resource: &str) -> Result<String> {
    validate_relative_path(resource, "static registry resource")?;
    if index_source.starts_with("file://") {
        let index_path = file_url_to_path(index_source)
            .with_context(|| format!("invalid file registry URL {index_source:?}"))?;
        let base = index_path
            .parent()
            .with_context(|| format!("file registry URL {index_source:?} has no parent"))?;
        return Ok(file_url_from_path(&base.join(resource)));
    }
    let slash = index_source
        .rfind('/')
        .with_context(|| format!("static registry index URL {index_source:?} has no base path"))?;
    Ok(format!("{}/{}", &index_source[..slash], resource))
}

fn read_bytes(source: &str, limit: usize) -> Result<Vec<u8>> {
    validate_url(source)?;
    if let Some(path) = file_url_to_path(source) {
        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.len() > limit as u64 {
            bail!(
                "static registry file {} is too large: {} bytes",
                path.display(),
                metadata.len()
            );
        }
        return fs::read(&path).with_context(|| format!("failed to read {}", path.display()));
    }

    let source_for_thread = source.to_string();
    let result = thread::spawn(move || -> Result<Vec<u8>> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(FETCH_TIMEOUT)
            .build()
            .context("failed to build static registry HTTP client")?;
        let response = client
            .get(&source_for_thread)
            .send()
            .with_context(|| {
                format!("failed to fetch static registry resource {source_for_thread}")
            })?
            .error_for_status()
            .with_context(|| {
                format!("static registry resource {source_for_thread} returned an error")
            })?;
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            bail!("static registry resource {source_for_thread} is too large");
        }
        let mut bytes = Vec::new();
        let read_limit = limit as u64 + 1;
        response
            .take(read_limit)
            .read_to_end(&mut bytes)
            .with_context(|| {
                format!("failed to read static registry resource {source_for_thread}")
            })?;
        if bytes.len() > limit {
            bail!(
                "static registry resource {source_for_thread} is too large: {} bytes",
                bytes.len()
            );
        }
        Ok(bytes)
    })
    .join();

    match result {
        Ok(result) => result,
        Err(_) => bail!("static registry fetch worker panicked for {source}"),
    }
}

fn file_url_from_path(path: &Path) -> String {
    let path = path_to_slash(path);
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

fn file_url_to_path(source: &str) -> Option<PathBuf> {
    let mut path = source.strip_prefix("file://")?.to_string();
    if path.len() >= 4
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[2] == b':'
        && path.as_bytes()[1].is_ascii_alphabetic()
    {
        path.remove(0);
    }
    Some(PathBuf::from(path))
}

pub(super) fn extract_package_archive(bytes: &[u8], destination: &Path) -> Result<()> {
    if destination.exists() {
        bail!(
            "package archive destination already exists: {}",
            destination.display()
        );
    }
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    let mut entry_count = 0_usize;
    let mut total_unpacked = 0_usize;
    for entry in archive
        .entries()
        .context("failed to read static registry package archive")?
    {
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            bail!(
                "static registry package archive has too many entries: more than {MAX_ARCHIVE_ENTRIES}"
            );
        }
        let mut entry = entry.context("failed to read static registry archive entry")?;
        let entry_type = entry.header().entry_type();
        if entry_type == EntryType::Symlink || entry_type == EntryType::Link {
            bail!("static registry package archives must not contain links");
        }
        if !(entry_type == EntryType::Regular || entry_type == EntryType::Directory) {
            bail!("static registry package archives may only contain files and directories");
        }
        let entry_path = entry
            .path()
            .context("failed to read static registry archive path")?
            .into_owned();
        validate_archive_relative_path(&entry_path)?;
        let destination_path = destination.join(&entry_path);
        if entry_type == EntryType::Directory {
            fs::create_dir_all(&destination_path)
                .with_context(|| format!("failed to create {}", destination_path.display()))?;
        } else {
            let entry_size = entry
                .header()
                .size()
                .context("failed to read static registry archive entry size")?;
            if entry_size > MAX_ARCHIVE_ENTRY_BYTES as u64 {
                bail!(
                    "static registry archive entry {} is too large: {} bytes",
                    entry_path.display(),
                    entry_size
                );
            }
            if total_unpacked as u64 + entry_size > MAX_UNPACKED_BYTES as u64 {
                bail!(
                    "static registry package archive unpacks to more than {MAX_UNPACKED_BYTES} bytes"
                );
            }
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            let mut output = fs::File::create(&destination_path)
                .with_context(|| format!("failed to create {}", destination_path.display()))?;
            let copied = io::copy(&mut entry, &mut output)
                .with_context(|| format!("failed to unpack {}", destination_path.display()))?;
            total_unpacked += copied as usize;
        }
    }
    Ok(())
}

fn validate_archive_relative_path(path: &Path) -> Result<()> {
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("static registry archive path must not contain .."),
            Component::RootDir | Component::Prefix(_) => {
                bail!("static registry archive path must be relative")
            }
        }
    }
    Ok(())
}

fn metadata_relative_path(package: &str) -> PathBuf {
    PathBuf::from("packages")
        .join(registry_package_relative_path(package))
        .with_extension("toml")
}

fn archive_relative_path(package: &str, version: &str) -> PathBuf {
    let leaf = default_dependency_alias(package);
    PathBuf::from("artifacts")
        .join(registry_package_relative_path(package))
        .join(version)
        .join(format!("{leaf}-{version}.tar.gz"))
}
