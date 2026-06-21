use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use ricochet_bytecode::{Chunk, Op};
use sha2::{Digest, Sha256};

use crate::{
    compile_source_with_imported_macros_and_module_id, expand_source_with_imported_macros,
    exported_macro_table_from_source_with_imports, format_compile_error, ImportedMacroTable,
    MacroExpansion, MacroPackageMetadata, MacroSourceKind,
};

pub fn compile_file_with_imports(source_path: impl AsRef<Path>) -> Result<Chunk> {
    SourceResolver::default()
        .compile_file(source_path.as_ref())
        .map(|compiled| compiled.chunk)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedImportKind {
    Local,
    Package { package: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    pub path: PathBuf,
    pub kind: ResolvedImportKind,
}

pub fn resolve_import_with_metadata(
    parent: impl AsRef<Path>,
    import: &str,
) -> Result<ResolvedImport> {
    resolve_import_metadata(parent.as_ref(), import)
}

pub fn verify_runtime_import_locks_for_parent(parent: impl AsRef<Path>) -> Result<()> {
    let Some(manifest_path) = find_nearest_manifest(parent.as_ref()) else {
        return Ok(());
    };
    verify_manifest_dependency_locks(&manifest_path)
}

pub struct FileMacroExpansion {
    pub source: String,
    pub expansion: MacroExpansion,
}

pub fn expand_file_with_imports(source_path: impl AsRef<Path>) -> Result<FileMacroExpansion> {
    SourceResolver::default()
        .expand_file(source_path.as_ref())
        .map(|resolved| FileMacroExpansion {
            source: resolved.original_source,
            expansion: resolved.expansion,
        })
}

#[derive(Default)]
struct SourceResolver {
    loaded: BTreeSet<PathBuf>,
    visiting: BTreeSet<PathBuf>,
    exported_macros: HashMap<PathBuf, ImportedMacroTable>,
    root: Option<PathBuf>,
}

struct CompiledFile {
    chunk: Chunk,
    exported_macros: ImportedMacroTable,
}

struct ResolvedExpansion {
    original_source: String,
    expansion: MacroExpansion,
    exported_macros: ImportedMacroTable,
}

struct SourceIdentity {
    module_id: String,
    source_kind: MacroSourceKind,
    package: Option<MacroPackageMetadata>,
}

#[derive(Default)]
struct PackageLockMetadata {
    package: Option<String>,
    version: Option<String>,
    integrity: Option<String>,
    source_kind: Option<String>,
    commit: Option<String>,
}

#[derive(Default)]
struct PackageManifestMetadata {
    package: Option<String>,
    version: Option<String>,
}

impl SourceResolver {
    fn compile_file(&mut self, source_path: &Path) -> Result<CompiledFile> {
        let canonical = fs::canonicalize(source_path)
            .with_context(|| format!("failed to resolve {}", source_path.display()))?;
        let root = self.root_for(&canonical)?;
        let diagnostic_file = source_path.to_string_lossy().into_owned();
        if self.loaded.contains(&canonical) {
            let exported_macros =
                self.exported_macros
                    .get(&canonical)
                    .cloned()
                    .with_context(|| {
                        format!(
                            "missing cached macro table for already loaded import {}",
                            source_path.display()
                        )
                    })?;
            return Ok(CompiledFile {
                chunk: Chunk::new(diagnostic_file),
                exported_macros,
            });
        }
        if !self.visiting.insert(canonical.clone()) {
            bail!("cyclic Ricochet import involving {}", source_path.display());
        }

        let source = read_source_path(source_path)?;
        let module_id = logical_module_id(&canonical, &root);
        let imports = static_imports(&source)
            .with_context(|| format!("failed to scan imports in {}", source_path.display()))?;
        let source_without_imports = strip_static_imports(&source)?;
        let mut combined = Chunk::new(diagnostic_file.clone());
        let mut imported_macro_tables = Vec::new();
        let parent = canonical
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        for import in unique_imports(&imports) {
            let resolved_import = resolve_import_metadata(parent, import)?;
            let compiled_import = self.compile_file(&resolved_import.path).with_context(|| {
                format!("failed to import {import:?} from {}", source_path.display())
            })?;
            imported_macro_tables.push(bind_imported_macro_table(
                parent,
                import,
                &resolved_import,
                &compiled_import.exported_macros,
            )?);
            append_chunk(&mut combined, compiled_import.chunk);
        }

        let exported_macros = exported_macro_table_from_source_with_imports(
            &module_id,
            &source_without_imports,
            &imported_macro_tables,
        )
        .map_err(|error| {
            anyhow::anyhow!(format_compile_error(
                &diagnostic_file,
                &source_without_imports,
                &error
            ))
        })?
        .with_source_identity(
            module_id.clone(),
            sha256_text(&source),
            MacroSourceKind::Local,
            None,
        );

        let own_chunk = compile_source_with_imported_macros_and_module_id(
            &diagnostic_file,
            &module_id,
            &source_without_imports,
            &imported_macro_tables,
        )
        .map_err(|error| {
            anyhow::anyhow!(format_compile_error(
                &diagnostic_file,
                &source_without_imports,
                &error
            ))
        })?;
        append_chunk(&mut combined, own_chunk);
        self.visiting.remove(&canonical);
        self.loaded.insert(canonical.clone());
        self.exported_macros
            .insert(canonical, exported_macros.clone());
        Ok(CompiledFile {
            chunk: combined,
            exported_macros,
        })
    }

    fn expand_file(&mut self, source_path: &Path) -> Result<ResolvedExpansion> {
        let canonical = fs::canonicalize(source_path)
            .with_context(|| format!("failed to resolve {}", source_path.display()))?;
        let root = self.root_for(&canonical)?;
        if !self.visiting.insert(canonical.clone()) {
            bail!("cyclic Ricochet import involving {}", source_path.display());
        }

        let source = read_source_path(source_path)?;
        let file = logical_module_id(&canonical, &root);
        let imports = static_imports(&source)
            .with_context(|| format!("failed to scan imports in {}", source_path.display()))?;
        let source_without_imports = strip_static_imports(&source)?;
        let mut imported_macro_tables = Vec::new();
        let parent = canonical
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        for import in unique_imports(&imports) {
            let resolved_import = resolve_import_metadata(parent, import)?;
            let imported = self.expand_file(&resolved_import.path).with_context(|| {
                format!("failed to import {import:?} from {}", source_path.display())
            })?;
            imported_macro_tables.push(bind_imported_macro_table(
                parent,
                import,
                &resolved_import,
                &imported.exported_macros,
            )?);
        }

        let exported_macros = exported_macro_table_from_source_with_imports(
            &file,
            &source_without_imports,
            &imported_macro_tables,
        )
        .map_err(|error| {
            anyhow::anyhow!(format_compile_error(&file, &source_without_imports, &error))
        })?
        .with_source_identity(
            file.clone(),
            sha256_text(&source),
            MacroSourceKind::Local,
            None,
        );

        let mut expansion = expand_source_with_imported_macros(
            &file,
            &source_without_imports,
            &imported_macro_tables,
        )
        .map_err(|error| {
            anyhow::anyhow!(format_compile_error(&file, &source_without_imports, &error))
        })?;
        if let Some(root_table) = expansion
            .macro_tables
            .iter_mut()
            .find(|table| table.module_id == file && table.import_specifier.is_none())
        {
            root_table.source_hash = sha256_text(&source);
            root_table.source_kind = MacroSourceKind::Local;
            root_table.package = None;
        }
        self.visiting.remove(&canonical);
        Ok(ResolvedExpansion {
            original_source: source,
            expansion,
            exported_macros,
        })
    }

    fn root_for(&mut self, canonical_source_path: &Path) -> Result<PathBuf> {
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        let parent = canonical_source_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let root = import_root_for_parent(parent)?;
        self.root = Some(root.clone());
        Ok(root)
    }
}

fn read_source_path(source_path: &Path) -> Result<String> {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;

    Ok(source)
}

fn logical_module_id(canonical_source_path: &Path, root: &Path) -> String {
    let path = canonical_source_path
        .strip_prefix(root)
        .unwrap_or(canonical_source_path);
    let module_id = path.to_string_lossy().replace('\\', "/");
    if module_id.is_empty() {
        ".".to_string()
    } else {
        module_id
    }
}

fn bind_imported_macro_table(
    parent: &Path,
    import: &str,
    resolved_import: &ResolvedImport,
    table: &ImportedMacroTable,
) -> Result<ImportedMacroTable> {
    let source_hash = sha256_text(&read_source_path(&resolved_import.path)?);
    let bound = match source_identity_for_import(parent, import, resolved_import)? {
        Some(identity) => table.with_source_identity(
            identity.module_id,
            source_hash,
            identity.source_kind,
            identity.package,
        ),
        None => table.with_source_identity(
            table.module_id().to_string(),
            source_hash,
            MacroSourceKind::Local,
            None,
        ),
    };
    Ok(bound.with_import_specifier(import))
}

fn source_identity_for_import(
    parent: &Path,
    import: &str,
    resolved_import: &ResolvedImport,
) -> Result<Option<SourceIdentity>> {
    match &resolved_import.kind {
        ResolvedImportKind::Local => Ok(None),
        ResolvedImportKind::Package { package } => {
            let Some(package_import) = parse_package_import(import) else {
                bail!("failed to parse package import metadata for {import:?}");
            };
            let lock = package_lock_metadata(parent, package)?;
            let manifest = package_manifest_metadata_for_source(&resolved_import.path)?;
            let module_path = package_import.module;
            let module_id = canonical_package_module_id(package, &module_path, lock.as_ref());
            Ok(Some(SourceIdentity {
                module_id,
                source_kind: MacroSourceKind::Package,
                package: Some(MacroPackageMetadata {
                    name: package.to_string(),
                    package: lock
                        .as_ref()
                        .and_then(|entry| entry.package.clone())
                        .or(manifest.package),
                    module_path,
                    version: lock
                        .as_ref()
                        .and_then(|entry| entry.version.clone())
                        .or(manifest.version),
                    integrity: lock.as_ref().and_then(|entry| entry.integrity.clone()),
                    source_kind: lock.as_ref().and_then(|entry| entry.source_kind.clone()),
                    commit: lock.as_ref().and_then(|entry| entry.commit.clone()),
                }),
            }))
        }
    }
}

fn package_lock_metadata(parent: &Path, package: &str) -> Result<Option<PackageLockMetadata>> {
    let Some(manifest_path) = find_nearest_manifest(parent) else {
        return Ok(None);
    };
    let Some(lock_path) = manifest_path
        .parent()
        .map(|manifest_dir| manifest_dir.join("ricochet.lock"))
    else {
        return Ok(None);
    };
    if !lock_path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&lock_path)
        .with_context(|| format!("failed to read {}", lock_path.display()))?;
    let lock: toml::Value = toml::from_str(&source)
        .with_context(|| format!("failed to parse {}", lock_path.display()))?;
    let Some(entry) = lock
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|packages| packages.get(package))
        .and_then(toml::Value::as_table)
    else {
        return Ok(None);
    };

    Ok(Some(PackageLockMetadata {
        package: entry
            .get("package")
            .and_then(toml::Value::as_str)
            .map(str::to_string),
        version: entry
            .get("version")
            .and_then(toml::Value::as_str)
            .map(str::to_string),
        integrity: entry
            .get("integrity")
            .and_then(toml::Value::as_str)
            .map(str::to_string),
        source_kind: package_lock_source_kind(entry),
        commit: entry
            .get("commit")
            .and_then(toml::Value::as_str)
            .map(str::to_string),
    }))
}

fn package_manifest_metadata_for_source(source_path: &Path) -> Result<PackageManifestMetadata> {
    let parent = source_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let Some(manifest_path) = find_nearest_manifest(parent) else {
        return Ok(PackageManifestMetadata::default());
    };
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let version = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    Ok(PackageManifestMetadata { package, version })
}

fn package_lock_source_kind(entry: &toml::map::Map<String, toml::Value>) -> Option<String> {
    if entry.get("git").is_some() || entry.get("commit").is_some() {
        Some("git".to_string())
    } else if entry.get("registry").is_some() {
        Some("registry".to_string())
    } else if entry.get("path").is_some() {
        Some("path".to_string())
    } else {
        None
    }
}

fn canonical_package_module_id(
    package: &str,
    module_path: &str,
    lock: Option<&PackageLockMetadata>,
) -> String {
    let module_path = module_path.replace('\\', "/");
    let revision = lock
        .and_then(|entry| entry.integrity.clone())
        .or_else(|| {
            lock.and_then(|entry| entry.commit.clone().map(|commit| format!("git:{commit}")))
        })
        .or_else(|| {
            lock.and_then(|entry| {
                entry
                    .version
                    .clone()
                    .map(|version| format!("version:{version}"))
            })
        });
    match revision {
        Some(revision) => format!("{package}@{revision}/{module_path}"),
        None => format!("{package}/{module_path}"),
    }
}

fn static_imports(source: &str) -> Result<Vec<String>> {
    source
        .lines()
        .filter_map(|line| parse_static_import_line(line.trim()))
        .collect()
}

fn unique_imports(imports: &[String]) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    imports
        .iter()
        .filter_map(|import| {
            if seen.insert(import.as_str()) {
                Some(import.as_str())
            } else {
                None
            }
        })
        .collect()
}

fn strip_static_imports(source: &str) -> Result<String> {
    let mut stripped = String::new();
    for segment in source.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map(|line| (line, "\n"))
            .unwrap_or((segment, ""));
        let (content, carriage_return) = line
            .strip_suffix('\r')
            .map(|line| (line, "\r"))
            .unwrap_or((line, ""));
        if parse_static_import_line(content.trim())
            .transpose()?
            .is_some()
        {
            stripped.push_str(&" ".repeat(content.len()));
            stripped.push_str(carriage_return);
            stripped.push_str(newline);
        } else {
            stripped.push_str(segment);
        }
    }
    Ok(stripped)
}

fn parse_static_import_line(line: &str) -> Option<Result<String>> {
    let rest = line.strip_prefix('"')?;
    let (value, rest) = match parse_string_prefix(rest) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => return Some(Err(anyhow::anyhow!("unterminated import string"))),
        Err(error) => return Some(Err(error)),
    };
    if rest.trim() == "import" {
        Some(Ok(value))
    } else {
        None
    }
}

fn parse_string_prefix(source: &str) -> Result<Option<(String, &str)>> {
    let mut value = String::new();
    let mut escape = false;
    for (index, ch) in source.char_indices() {
        if escape {
            let decoded = match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => bail!("invalid import string escape \\{other}"),
            };
            value.push(decoded);
            escape = false;
            continue;
        }

        match ch {
            '\\' => escape = true,
            '"' => return Ok(Some((value, &source[index + 1..]))),
            ch => value.push(ch),
        }
    }

    if escape {
        bail!("unterminated import string escape");
    }
    Ok(None)
}

fn resolve_import_metadata(parent: &Path, import: &str) -> Result<ResolvedImport> {
    validate_import_path(import)?;
    let import_root = import_root_for_parent(parent)?;
    let relative_path = relative_import_path(parent, import);
    if relative_path.is_file() {
        return contained_file(&import_root, &relative_path, import).map(|path| ResolvedImport {
            path,
            kind: ResolvedImportKind::Local,
        });
    }

    if let Some(package_import) = parse_package_import(import) {
        let package = package_import.package.to_string();
        if let Some(package_path) = resolve_package_import(parent, package_import)? {
            return Ok(ResolvedImport {
                path: package_path,
                kind: ResolvedImportKind::Package { package },
            });
        }
    }

    Ok(ResolvedImport {
        path: relative_path,
        kind: ResolvedImportKind::Local,
    })
}

fn import_root_for_parent(parent: &Path) -> Result<PathBuf> {
    if let Some(manifest_path) = find_nearest_manifest(parent) {
        let manifest_dir = manifest_path
            .parent()
            .expect("manifest path should have a parent");
        return fs::canonicalize(manifest_dir)
            .with_context(|| format!("failed to resolve project root {}", manifest_dir.display()));
    }

    fs::canonicalize(parent)
        .with_context(|| format!("failed to resolve import root {}", parent.display()))
}

fn relative_import_path(parent: &Path, import: &str) -> PathBuf {
    let import_path = Path::new(import);
    let mut path = parent.join(import_path);
    if path.extension().is_none() {
        path.set_extension("rco");
    }
    path
}

#[derive(Debug)]
struct PackageImport<'a> {
    package: &'a str,
    module: String,
}

fn parse_package_import(import: &str) -> Option<PackageImport<'_>> {
    let import_path = Path::new(import);
    if import_path.is_absolute() || import.starts_with('.') || import.contains('\\') {
        return None;
    }

    if let Some((package, module)) = import.split_once('/') {
        if !package.is_empty() && !module.is_empty() && package_module_is_safe(module) {
            return Some(PackageImport {
                package,
                module: module.to_string(),
            });
        }
    }

    let (package, module) = import.split_once('.')?;
    if package.is_empty() || module.is_empty() {
        return None;
    }
    let module = module.replace('.', "/");
    if !package_module_is_safe(&module) {
        return None;
    }

    Some(PackageImport { package, module })
}

fn resolve_package_import(
    parent: &Path,
    package_import: PackageImport<'_>,
) -> Result<Option<PathBuf>> {
    let Some(manifest_path) = find_nearest_manifest(parent) else {
        return Ok(None);
    };
    let Some(base_path) = dependency_base_path(&manifest_path, package_import.package)? else {
        return Ok(None);
    };

    let base_path = contained_dependency_base(&manifest_path, &base_path, package_import.package)?;
    let candidates = package_import_candidates(&base_path, &package_import.module);
    if let Some(candidate) = candidates.iter().find(|candidate| candidate.is_file()) {
        return contained_file(&base_path, candidate, &package_import.module).map(Some);
    }

    bail!(
        "package dependency {:?} does not contain import {:?}; tried {}",
        package_import.package,
        package_import.module,
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn find_nearest_manifest(parent: &Path) -> Option<PathBuf> {
    let start = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    start
        .ancestors()
        .map(|ancestor| ancestor.join("ricochet.toml"))
        .find(|manifest_path| manifest_path.is_file())
}

fn dependency_base_path(manifest_path: &Path, package: &str) -> Result<Option<PathBuf>> {
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let Some(dependency) = manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get(package))
    else {
        return Ok(None);
    };

    let manifest_dir = manifest_path
        .parent()
        .expect("manifest path should have a parent");
    if let Some(path) = dependency.get("path").and_then(|path| path.as_str()) {
        validate_dependency_path(path, package)?;
        let path = Path::new(path);
        return Ok(Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            manifest_dir.join(path)
        }));
    }

    if dependency.get("git").is_some() {
        return Ok(Some(
            manifest_dir
                .join(".ricochet")
                .join("packages")
                .join(package),
        ));
    }

    Ok(None)
}

fn verify_manifest_dependency_locks(manifest_path: &Path) -> Result<()> {
    let manifest_dir = manifest_path
        .parent()
        .expect("manifest path should have a parent");
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&source)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let lock_path = manifest_dir.join("ricochet.lock");
    let lock = if lock_path.is_file() {
        let source = fs::read_to_string(&lock_path)
            .with_context(|| format!("failed to read {}", lock_path.display()))?;
        Some(
            toml::from_str::<toml::Value>(&source)
                .with_context(|| format!("failed to parse {}", lock_path.display()))?,
        )
    } else {
        None
    };

    let Some(dependencies) = manifest.get("dependencies").and_then(toml::Value::as_table) else {
        verify_no_stale_runtime_locks(&lock_path, lock.as_ref(), &BTreeSet::new())?;
        return Ok(());
    };

    let mut declared = BTreeSet::new();
    for (name, dependency) in dependencies {
        declared.insert(name.clone());
        verify_runtime_dependency_lock(manifest_path, &lock_path, lock.as_ref(), name, dependency)?;
    }
    verify_no_stale_runtime_locks(&lock_path, lock.as_ref(), &declared)
}

fn verify_runtime_dependency_lock(
    manifest_path: &Path,
    lock_path: &Path,
    lock: Option<&toml::Value>,
    name: &str,
    dependency: &toml::Value,
) -> Result<()> {
    let dependency = dependency.as_table().with_context(|| {
        format!(
            "dependency {name} in {} must be a table",
            manifest_path.display()
        )
    })?;
    let Some(base_path) = dependency_base_path(manifest_path, name)? else {
        return Ok(());
    };
    let package_dir = contained_dependency_base(manifest_path, &base_path, name)?;
    let lock_entry = lock
        .and_then(|lock| lock.get("package"))
        .and_then(toml::Value::as_table)
        .and_then(|packages| packages.get(name))
        .and_then(toml::Value::as_table)
        .with_context(|| {
            format!(
                "dependency {name} is missing from {}; run rco install",
                lock_path.display()
            )
        })?;

    let expected_path = expected_dependency_lock_path(name, dependency);
    let lock_path_value = lock_entry
        .get("path")
        .and_then(toml::Value::as_str)
        .with_context(|| format!("lock entry for {name} must include a string path"))?;
    if lock_path_value != expected_path {
        bail!("lock entry for {name} has path {lock_path_value:?}, expected {expected_path:?}");
    }

    if dependency.get("git").is_some()
        && lock_entry
            .get("commit")
            .and_then(toml::Value::as_str)
            .is_none()
    {
        bail!("git dependency {name} is not pinned; run rco install");
    }

    if let Some(registry) = dependency.get("registry").and_then(toml::Value::as_str) {
        let locked_registry = lock_entry.get("registry").and_then(toml::Value::as_str);
        if locked_registry != Some(registry) {
            bail!(
                "lock entry for {name} has registry {:?}, expected {:?}",
                locked_registry,
                registry
            );
        }
    }

    let expected_integrity = lock_entry
        .get("integrity")
        .and_then(toml::Value::as_str)
        .with_context(|| format!("lock entry for {name} is missing package integrity"))?;
    validate_package_integrity(expected_integrity)?;
    let actual_integrity = package_tree_integrity(&package_dir)?;
    if actual_integrity != expected_integrity {
        bail!(
            "package integrity for {name} changed: expected {expected_integrity}, got {actual_integrity}; run rco install if this update is intentional"
        );
    }

    Ok(())
}

fn expected_dependency_lock_path(name: &str, dependency: &toml::Table) -> String {
    dependency
        .get("path")
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!(".ricochet/packages/{name}"))
}

fn verify_no_stale_runtime_locks(
    lock_path: &Path,
    lock: Option<&toml::Value>,
    declared: &BTreeSet<String>,
) -> Result<()> {
    let Some(packages) = lock
        .and_then(|lock| lock.get("package"))
        .and_then(toml::Value::as_table)
    else {
        return Ok(());
    };

    for name in packages.keys() {
        if !declared.contains(name) {
            bail!(
                "{} contains package {name:?}, but ricochet.toml does not declare it",
                lock_path.display()
            );
        }
    }
    Ok(())
}

fn package_tree_integrity(package_dir: &Path) -> Result<String> {
    if !package_dir.is_dir() {
        bail!(
            "cannot compute package integrity for non-directory {}",
            package_dir.display()
        );
    }

    let mut files = Vec::new();
    collect_package_integrity_files(package_dir, package_dir, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    hasher.update(b"ricochet-package-integrity-v1\0");
    for (relative, path) in files {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read package file {}", path.display()))?;
        hasher.update(relative.as_bytes());
        hasher.update(b"\0");
        hasher.update(bytes.len().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(&bytes);
        hasher.update(b"\0");
    }

    let digest = hasher.finalize();
    Ok(format!("sha256:{}", hex_digest(&digest)))
}

fn collect_package_integrity_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
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
                "package integrity cannot include symlink {}; copy the target file into the package",
                path.display()
            );
        }
        if metadata.is_dir() {
            if file_name == ".git" {
                continue;
            }
            collect_package_integrity_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .with_context(|| format!("failed to make {} package-relative", path.display()))?;
            files.push((path_to_slash(relative), path));
        }
    }
    Ok(())
}

fn validate_package_integrity(integrity: &str) -> Result<()> {
    let Some(hex) = integrity.strip_prefix("sha256:") else {
        bail!("invalid package integrity {integrity:?}; expected sha256:<64 hex chars>");
    };
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("invalid package integrity {integrity:?}; expected sha256:<64 hex chars>");
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

fn sha256_text(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    format!("sha256:{}", hex_digest(&digest))
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn package_import_candidates(base_path: &Path, module: &str) -> Vec<PathBuf> {
    [base_path.join(module), base_path.join("src").join(module)]
        .into_iter()
        .map(|mut candidate| {
            if candidate.extension().is_none() {
                candidate.set_extension("rco");
            }
            candidate
        })
        .collect()
}

fn validate_import_path(import: &str) -> Result<()> {
    if import.is_empty() {
        bail!("import path must not be empty");
    }
    if import.contains('#') {
        bail!("import path must not contain #: {import:?}");
    }
    if import.contains('\\') {
        bail!("import path must use forward slashes: {import:?}");
    }
    let path = Path::new(import);
    if path.is_absolute() {
        bail!("absolute imports are not allowed: {import:?}");
    }
    let explicit_relative = import.starts_with("./") || import.starts_with("../");
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir if explicit_relative => {}
            Component::CurDir | Component::ParentDir => {
                bail!("import path must not contain . or .. components: {import:?}");
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("absolute imports are not allowed: {import:?}");
            }
        }
    }
    Ok(())
}

fn validate_dependency_path(path: &str, package: &str) -> Result<()> {
    if path.contains('\\') {
        bail!("dependency {package:?} path must use forward slashes");
    }
    let path = Path::new(path);
    if path.is_absolute() {
        bail!("dependency {package:?} path must be inside the project");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                bail!("dependency {package:?} path must not contain .. components");
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("dependency {package:?} path must be inside the project");
            }
        }
    }
    Ok(())
}

fn package_module_is_safe(module: &str) -> bool {
    !module.is_empty()
        && !module.contains('\\')
        && Path::new(module)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn contained_dependency_base(
    manifest_path: &Path,
    base_path: &Path,
    package: &str,
) -> Result<PathBuf> {
    let manifest_dir = manifest_path
        .parent()
        .expect("manifest path should have a parent");
    let manifest_dir = fs::canonicalize(manifest_dir)
        .with_context(|| format!("failed to resolve {}", manifest_dir.display()))?;
    let canonical_base = fs::canonicalize(base_path).with_context(|| {
        format!(
            "failed to resolve dependency {package:?} at {}",
            base_path.display()
        )
    })?;
    if !canonical_base.starts_with(&manifest_dir) {
        bail!(
            "dependency {package:?} resolves outside the project root: {}",
            base_path.display()
        );
    }
    Ok(canonical_base)
}

fn contained_file(root: &Path, candidate: &Path, import: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(candidate).with_context(|| {
        format!(
            "failed to resolve import {import:?} at {}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(root) {
        bail!(
            "import {import:?} resolves outside allowed root {}",
            root.display()
        );
    }
    Ok(canonical)
}

fn append_chunk(target: &mut Chunk, chunk: Chunk) {
    let instruction_offset = target.instructions.len();
    let block_offset = target.blocks.len();

    target.blocks.extend(chunk.blocks);
    target
        .instructions
        .extend(chunk.instructions.into_iter().map(|mut instruction| {
            instruction.op = rebase_op(instruction.op, instruction_offset, block_offset);
            instruction
        }));
}

fn rebase_op(op: Op, instruction_offset: usize, block_offset: usize) -> Op {
    match op {
        Op::PushBlock(index) => Op::PushBlock(index + block_offset),
        Op::AddMethod { name, block, args } => Op::AddMethod {
            name,
            block: block + block_offset,
            args,
        },
        Op::AddFunction { name, block, args } => Op::AddFunction {
            name,
            block: block + block_offset,
            args,
        },
        Op::JumpIfFalse(target) => Op::JumpIfFalse(target + instruction_offset),
        Op::Jump(target) => Op::Jump(target + instruction_offset),
        op => op,
    }
}
