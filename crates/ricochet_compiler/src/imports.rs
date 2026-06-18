use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use ricochet_bytecode::{Chunk, Op};

use crate::{compile_source, format_compile_error};

pub fn compile_file_with_imports(source_path: impl AsRef<Path>) -> Result<Chunk> {
    SourceResolver::default().compile_file(source_path.as_ref())
}

#[derive(Default)]
struct SourceResolver {
    loaded: BTreeSet<PathBuf>,
    visiting: BTreeSet<PathBuf>,
}

impl SourceResolver {
    fn compile_file(&mut self, source_path: &Path) -> Result<Chunk> {
        let canonical = fs::canonicalize(source_path)
            .with_context(|| format!("failed to resolve {}", source_path.display()))?;
        if self.loaded.contains(&canonical) {
            return Ok(Chunk::new(source_path.to_string_lossy()));
        }
        if !self.visiting.insert(canonical.clone()) {
            bail!("cyclic Ricochet import involving {}", source_path.display());
        }

        let (file, source) = read_source_path(source_path)?;
        let imports = static_imports(&source)
            .with_context(|| format!("failed to scan imports in {}", source_path.display()))?;
        let source_without_imports = strip_static_imports(&source)?;
        let mut combined = Chunk::new(file.clone());
        let parent = source_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        for import in imports {
            let import_path = resolve_import(parent, &import)?;
            let chunk = self.compile_file(&import_path).with_context(|| {
                format!("failed to import {import:?} from {}", source_path.display())
            })?;
            append_chunk(&mut combined, chunk);
        }

        let own_chunk = compile_source(&file, &source_without_imports).map_err(|error| {
            anyhow::anyhow!(format_compile_error(&file, &source_without_imports, &error))
        })?;
        append_chunk(&mut combined, own_chunk);
        self.visiting.remove(&canonical);
        self.loaded.insert(canonical);
        Ok(combined)
    }
}

fn read_source_path(source_path: &Path) -> Result<(String, String)> {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let file = source_path.to_string_lossy();

    Ok((file.into_owned(), source))
}

fn static_imports(source: &str) -> Result<Vec<String>> {
    source
        .lines()
        .filter_map(|line| parse_static_import_line(line.trim()))
        .collect()
}

fn strip_static_imports(source: &str) -> Result<String> {
    let mut stripped = String::new();
    for line in source.lines() {
        if parse_static_import_line(line.trim()).transpose()?.is_some() {
            stripped.push('\n');
        } else {
            stripped.push_str(line);
            stripped.push('\n');
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

fn resolve_import(parent: &Path, import: &str) -> Result<PathBuf> {
    validate_import_path(import)?;
    let import_root = import_root_for_parent(parent)?;
    let relative_path = relative_import_path(parent, import);
    if relative_path.is_file() {
        return contained_file(&import_root, &relative_path, import);
    }

    if let Some(package_import) = parse_package_import(import) {
        if let Some(package_path) = resolve_package_import(parent, package_import)? {
            return Ok(package_path);
        }
    }

    Ok(relative_path)
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
