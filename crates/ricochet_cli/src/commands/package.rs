use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub(crate) enum LinuxPackageFormat {
    Tar,
    Deb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmbeddedAppKind {
    Console,
    Tui,
    Gui,
    MvcGui,
}

impl EmbeddedAppKind {
    fn marker(self) -> &'static [u8] {
        match self {
            EmbeddedAppKind::Console => EMBEDDED_APP_MARKER,
            EmbeddedAppKind::Tui => EMBEDDED_TUI_APP_MARKER,
            EmbeddedAppKind::Gui => EMBEDDED_GUI_APP_MARKER,
            EmbeddedAppKind::MvcGui => EMBEDDED_MVC_GUI_APP_MARKER,
        }
    }
}

#[derive(Debug)]
pub(crate) struct EmbeddedApp {
    pub(crate) kind: EmbeddedAppKind,
    pub(crate) payload: EmbeddedAppPayload,
}

#[derive(Debug)]
pub(crate) enum EmbeddedAppPayload {
    Chunk(Chunk),
    MvcBundle(MvcBundle),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MvcBundle {
    files: Vec<MvcBundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MvcBundleFile {
    relative_path: PathBuf,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MvcBundlePolicy {
    sqlite_state_path: Option<PathBuf>,
    manifest_path: PathBuf,
    routes_path: PathBuf,
    lock_path: Option<PathBuf>,
    dependency_roots: Vec<PathBuf>,
    dependency_files: Vec<MvcBundleDependencyFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MvcBundleDependencyFile {
    dependency_name: String,
    relative_path: PathBuf,
}

impl MvcBundlePolicy {
    fn is_dependency_root(&self, relative_path: &Path) -> bool {
        self.dependency_roots
            .iter()
            .any(|root| root == relative_path)
    }

    fn is_dependency_file(&self, relative_path: &Path) -> bool {
        self.dependency_files
            .iter()
            .any(|file| file.relative_path == relative_path)
    }
}

const MVC_DATA_HOME_ENV: &str = "RICOCHET_MVC_DATA_HOME";

pub(crate) fn package(path: &str, output: &Path, options: PackageOptions<'_>) -> Result<()> {
    if output.is_dir() {
        bail!("package output is a directory: {}", output.display());
    }
    if options.tui && options.gui {
        bail!("--tui cannot be used with --gui");
    }
    if options.tui && options.mvc {
        bail!("--mvc requires --gui and cannot be used with --tui");
    }
    if options.mvc && !options.gui {
        bail!("--mvc requires --gui");
    }
    if options.gui_launcher.is_some() && !options.gui {
        bail!("--gui-launcher requires --gui");
    }
    if options.gui && !native_gui_packaging_supported() {
        bail!("rco package --gui is currently available from Windows, Linux, and macOS builds");
    }
    if options.package_license.is_some() && (options.linux_packages.is_empty() || !options.gui) {
        bail!("--package-license requires --gui with --linux-package");
    }
    let linux_project_license = linux_package_project_license(
        options.gui && !options.linux_packages.is_empty(),
        options.package_license,
    )?;
    if !options.linux_packages.is_empty() {
        ensure_linux_package_host()?;
    }

    let package_kind = if options.mvc {
        EmbeddedAppKind::MvcGui
    } else if options.gui {
        EmbeddedAppKind::Gui
    } else if options.tui {
        EmbeddedAppKind::Tui
    } else {
        EmbeddedAppKind::Console
    };
    let bytes = if options.mvc {
        build_mvc_bundle(Path::new(path), output)?.to_bytes()?
    } else {
        compile_source_file(Path::new(path))?.to_bytes()?
    };
    let launcher = package_launcher(options.gui, options.gui_launcher)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(&launcher, output).with_context(|| {
        format!(
            "failed to copy launcher {} to {}",
            launcher.display(),
            output.display()
        )
    })?;
    append_embedded_payload(output, &bytes, package_kind)?;

    println!("packaged {}", output.display());

    if !options.linux_packages.is_empty() {
        create_linux_package_artifacts(
            output,
            options.linux_packages,
            options.package_name,
            options.package_version,
            linux_project_license,
            options.package_description,
            options.gui,
        )?;
    }

    Ok(())
}

pub(crate) struct PackageOptions<'a> {
    pub(crate) tui: bool,
    pub(crate) gui: bool,
    pub(crate) mvc: bool,
    pub(crate) gui_launcher: Option<&'a Path>,
    pub(crate) linux_packages: &'a [LinuxPackageFormat],
    pub(crate) package_name: Option<&'a str>,
    pub(crate) package_version: &'a str,
    pub(crate) package_license: Option<&'a str>,
    pub(crate) package_description: &'a str,
}

impl MvcBundle {
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        output.extend_from_slice(MVC_BUNDLE_MAGIC);
        write_u64(&mut output, self.files.len() as u64);
        for file in &self.files {
            validate_bundle_relative_path(&file.relative_path)?;
            let path = path_to_bundle_string(&file.relative_path)?;
            let path_bytes = path.as_bytes();
            write_u32(&mut output, path_bytes.len() as u32);
            write_u64(&mut output, file.bytes.len() as u64);
            output.extend_from_slice(path_bytes);
            output.extend_from_slice(&file.bytes);
        }
        Ok(output)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = ByteCursor::new(bytes);
        cursor.expect_bytes(MVC_BUNDLE_MAGIC)?;
        let file_count = cursor.read_u64()? as usize;
        let mut files = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let path_len = cursor.read_u32()? as usize;
            let file_len = cursor.read_u64()? as usize;
            let path = cursor.read_bytes(path_len)?;
            let path = std::str::from_utf8(path).context("MVC bundle path is not UTF-8")?;
            let relative_path = bundle_string_to_path(path)?;
            validate_bundle_relative_path(&relative_path)?;
            let bytes = cursor.read_bytes(file_len)?.to_vec();
            files.push(MvcBundleFile {
                relative_path,
                bytes,
            });
        }
        cursor.finish()?;
        Ok(Self { files })
    }

    fn extract_to(&self, root: &Path) -> Result<()> {
        for file in &self.files {
            validate_bundle_relative_path(&file.relative_path)?;
            let destination = root.join(&file.relative_path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&destination, &file.bytes)
                .with_context(|| format!("failed to extract {}", destination.display()))?;
        }
        Ok(())
    }
}

struct ByteCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> Result<()> {
        let actual = self.read_bytes(expected.len())?;
        if actual != expected {
            bail!("embedded MVC bundle has an unsupported format");
        }
        Ok(())
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("u32 byte count"),
        ))
    }

    fn read_u64(&mut self) -> Result<u64> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("u64 byte count"),
        ))
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .context("embedded MVC bundle length overflow")?;
        if end > self.bytes.len() {
            bail!("embedded MVC bundle ended unexpectedly");
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn finish(&self) -> Result<()> {
        if self.offset != self.bytes.len() {
            bail!("embedded MVC bundle has trailing bytes");
        }
        Ok(())
    }
}

fn build_mvc_bundle(project_root: &Path, output: &Path) -> Result<MvcBundle> {
    if !project_root.is_dir() {
        bail!(
            "rco package --mvc expects a Ricochet MVC project directory, got {}",
            project_root.display()
        );
    }
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", project_root.display()))?;
    let manifest_path = project_root.join("ricochet.toml");
    if !manifest_path.is_file() {
        bail!(
            "rco package --mvc expects {} to contain ricochet.toml",
            project_root.display()
        );
    }
    let policy = validate_mvc_bundle_manifest(&project_root, &manifest_path)?;
    let sqlite_migrations = if policy.sqlite_state_path.is_some() {
        let migrations = discover_migrations(&project_root)?;
        if migrations.is_empty() {
            bail!(
                "packaged file-backed SQLite requires at least one db/migrations migration; development database files are private runtime state and are never embedded"
            );
        }
        Some(migrations)
    } else {
        None
    };
    let output_path = absolute_package_output_path(output)?;
    let mut files = Vec::new();
    if let Some(relative_paths) = git_mvc_bundle_file_paths(&project_root)? {
        for relative_path in relative_paths {
            if policy.is_dependency_root(&relative_path)
                && project_root.join(&relative_path).is_dir()
            {
                continue;
            }
            add_mvc_bundle_file(
                &project_root,
                &relative_path,
                &output_path,
                &policy,
                &mut files,
            )?;
        }
    } else {
        collect_non_git_mvc_bundle_files(
            &project_root,
            &project_root,
            &output_path,
            &policy,
            &mut files,
        )?;
    }
    for dependency_file in &policy.dependency_files {
        if !mvc_bundle_contains(&files, &dependency_file.relative_path) {
            add_mvc_bundle_file(
                &project_root,
                &dependency_file.relative_path,
                &output_path,
                &policy,
                &mut files,
            )?;
        }
    }
    if !mvc_bundle_contains(&files, &policy.manifest_path) {
        bail!(
            "required MVC manifest {} was excluded from the bundle",
            path_to_bundle_string(&policy.manifest_path)?
        );
    }
    if !mvc_bundle_contains(&files, &policy.routes_path) {
        bail!(
            "configured MVC routes file {} was excluded from the bundle",
            path_to_bundle_string(&policy.routes_path)?
        );
    }
    if let Some(lock_path) = &policy.lock_path {
        if !mvc_bundle_contains(&files, lock_path) {
            bail!(
                "dependency lockfile {} was excluded from the bundle",
                path_to_bundle_string(lock_path)?
            );
        }
    }
    for dependency_file in &policy.dependency_files {
        if !mvc_bundle_contains(&files, &dependency_file.relative_path) {
            bail!(
                "declared dependency {} file {} was excluded from the bundle",
                dependency_file.dependency_name,
                path_to_bundle_string(&dependency_file.relative_path)?
            );
        }
    }
    if let Some(migrations) = sqlite_migrations {
        for migration in migrations {
            let relative_source = canonical_mvc_bundle_file_path(
                &project_root,
                &migration.source.path,
                "packaged file-backed SQLite migration",
            )?;
            if !files
                .iter()
                .any(|file| file.relative_path == relative_source)
            {
                bail!(
                    "packaged file-backed SQLite migration {} was excluded from the bundle; keep every apply migration eligible under the Git ignore boundary",
                    relative_source.display()
                );
            }
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(MvcBundle { files })
}

fn mvc_bundle_contains(files: &[MvcBundleFile], relative_path: &Path) -> bool {
    files.iter().any(|file| file.relative_path == relative_path)
}

fn validate_mvc_bundle_manifest(
    project_root: &Path,
    manifest_path: &Path,
) -> Result<MvcBundlePolicy> {
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    verify_dependency_manifest(project_root, manifest_path, &manifest, false)?;

    let Some(capabilities) = manifest
        .get("web")
        .and_then(Item::as_table)
        .and_then(|web| web.get("capabilities"))
        .and_then(Item::as_table)
    else {
        return mvc_bundle_policy_from_manifest(project_root, manifest_path, &manifest);
    };

    for key in ["fs_root", "process_root"] {
        let Some(value) = capabilities.get(key) else {
            continue;
        };
        let path = value
            .as_str()
            .with_context(|| format!("web.capabilities.{key} must be a string path"))?;
        validate_project_relative_path(path, &format!("web.capabilities.{key}"))?;
        let candidate = project_root.join(path);
        ensure_contained_candidate(project_root, &candidate, &format!("web.capabilities.{key}"))?;
    }

    mvc_bundle_policy_from_manifest(project_root, manifest_path, &manifest)
}

fn mvc_bundle_policy_from_manifest(
    project_root: &Path,
    manifest_path: &Path,
    manifest: &DocumentMut,
) -> Result<MvcBundlePolicy> {
    let manifest_path =
        canonical_mvc_bundle_file_path(project_root, manifest_path, "required MVC manifest")?;
    let routes = manifest
        .get("web")
        .and_then(Item::as_table)
        .and_then(|web| web.get("routes"))
        .and_then(Item::as_str)
        .context("web.routes must be a string path")?;
    validate_project_relative_path(routes, "web.routes")?;
    let routes_path = canonical_mvc_bundle_file_path(
        project_root,
        &project_root.join(routes),
        "configured MVC routes file",
    )?;

    let (dependency_roots, dependency_files) =
        mvc_bundle_dependency_files(project_root, &manifest_path, manifest)?;
    let lock_path = if dependency_roots.is_empty() {
        None
    } else {
        Some(canonical_mvc_bundle_file_path(
            project_root,
            &project_root.join("ricochet.lock"),
            "dependency lockfile",
        )?)
    };
    let sqlite_state_path = mvc_bundle_sqlite_state_path(manifest)?;
    Ok(MvcBundlePolicy {
        sqlite_state_path,
        manifest_path,
        routes_path,
        lock_path,
        dependency_roots,
        dependency_files,
    })
}

fn mvc_bundle_sqlite_state_path(manifest: &DocumentMut) -> Result<Option<PathBuf>> {
    let Some(database) = manifest
        .get("database")
        .and_then(Item::as_table)
        .and_then(|database| database.get("default"))
        .and_then(Item::as_table)
    else {
        return Ok(None);
    };
    let adapter = database
        .get("adapter")
        .and_then(Item::as_str)
        .context("database.default.adapter must be a string")?;
    if !adapter.eq_ignore_ascii_case("sqlite") {
        return Ok(None);
    }
    let url = database
        .get("url")
        .and_then(Item::as_str)
        .context("database.default.url must be a string")?
        .trim();
    if url == ":memory:" || url == "sqlite::memory:" {
        return Ok(None);
    }
    if url.contains("${") {
        bail!(
            "packaged MVC SQLite database.default.url must be a literal project-relative path or :memory: so its persistent data location is deterministic"
        );
    }
    let path = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    if path == ":memory:" {
        return Ok(None);
    }
    validate_project_relative_path(path, "database.default.url")?;
    let path = normalized_mvc_bundle_relative_path(Path::new(path))?;
    Ok(Some(path))
}

fn mvc_bundle_dependency_files(
    project_root: &Path,
    manifest_path: &Path,
    manifest: &DocumentMut,
) -> Result<(Vec<PathBuf>, Vec<MvcBundleDependencyFile>)> {
    let Some(dependencies) = manifest.get("dependencies").and_then(Item::as_table) else {
        return Ok((Vec::new(), Vec::new()));
    };

    let mut roots = Vec::new();
    let mut required = Vec::new();
    for (name, item) in dependencies.iter() {
        let spec = dependency_spec_from_manifest_table(name, item, manifest_path)?;
        validate_project_relative_path(&spec.path, &format!("packaged MVC dependency {name}"))?;
        let dependency_root = if spec.git.is_some() || spec.registry.is_some() {
            project_dependency_path(project_root, &spec.path, "package cache")?
        } else {
            resolve_local_dependency_dir(project_root, &spec.path)?
        };
        ensure_mvc_bundle_path_has_no_links(
            project_root,
            &dependency_root,
            &format!("packaged MVC dependency {name} root"),
        )?;
        let dependency_root = dependency_root.canonicalize().with_context(|| {
            format!(
                "failed to resolve packaged MVC dependency {name} root {}",
                dependency_root.display()
            )
        })?;
        let dependency_root_relative =
            canonical_mvc_bundle_relative_path(project_root, &dependency_root, "dependency root")?;
        if has_repository_metadata_component(&dependency_root_relative) {
            bail!(
                "dependency {name} root contains forbidden repository metadata: {}",
                dependency_root.display()
            );
        }
        roots.push(dependency_root_relative);
        validate_mvc_dependency_tree(name, &dependency_root)?;
        let mut integrity_files = Vec::new();
        collect_package_integrity_files(&dependency_root, &dependency_root, &mut integrity_files)?;
        for (_, path) in integrity_files {
            let relative_path =
                canonical_mvc_bundle_relative_path(project_root, &path, "dependency file")?;
            required.push(MvcBundleDependencyFile {
                dependency_name: name.to_string(),
                relative_path,
            });
        }
    }
    roots.sort();
    roots.dedup();
    required.sort_by(|left, right| {
        left.dependency_name
            .cmp(&right.dependency_name)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok((roots, required))
}

fn canonical_mvc_bundle_file_path(
    project_root: &Path,
    path: &Path,
    description: &str,
) -> Result<PathBuf> {
    ensure_mvc_bundle_path_has_no_links(project_root, path, description)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {description} {}", path.display()))?;
    if !canonical.is_file() {
        bail!("{description} is not a regular file: {}", path.display());
    }
    canonical_mvc_bundle_relative_path(project_root, &canonical, description)
}

fn ensure_mvc_bundle_path_has_no_links(
    project_root: &Path,
    path: &Path,
    description: &str,
) -> Result<()> {
    let relative = path.strip_prefix(project_root).with_context(|| {
        format!(
            "{description} path is outside the MVC project: {}",
            path.display()
        )
    })?;
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(value) => current.push(value),
            Component::CurDir => continue,
            _ => bail!(
                "{description} path must stay project-relative: {}",
                path.display()
            ),
        }
        let metadata = fs::symlink_metadata(&current).with_context(|| {
            format!(
                "failed to inspect {description} path component {}",
                current.display()
            )
        })?;
        if mvc_metadata_is_link_or_reparse(&metadata) {
            bail!(
                "{description} path contains a symbolic link or reparse point: {}",
                current.display()
            );
        }
    }
    Ok(())
}

fn mvc_metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn has_repository_metadata_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(value) if is_repository_metadata_name(value.to_string_lossy().as_ref()))
    })
}

fn is_repository_metadata_name(name: &str) -> bool {
    [".git", ".hg", ".svn"]
        .iter()
        .any(|metadata| name.eq_ignore_ascii_case(metadata))
}

fn validate_mvc_dependency_tree(dependency_name: &str, root: &Path) -> Result<()> {
    validate_mvc_dependency_tree_entries(dependency_name, root)
}

fn validate_mvc_dependency_tree_entries(dependency_name: &str, current: &Path) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", current.display()))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect dependency file {}", path.display()))?;
        if mvc_metadata_is_link_or_reparse(&metadata) {
            bail!(
                "packaged MVC dependency {dependency_name} contains a symbolic link or reparse point: {}",
                path.display()
            );
        }
        let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if is_repository_metadata_name(file_name) {
            if metadata.is_dir() && file_name == ".git" {
                continue;
            }
            bail!(
                "dependency {dependency_name} contains forbidden repository metadata: {}",
                path.display()
            );
        }
        if metadata.is_dir() {
            validate_mvc_dependency_tree_entries(dependency_name, &path)?;
        }
    }
    Ok(())
}

fn canonical_mvc_bundle_relative_path(
    project_root: &Path,
    canonical_path: &Path,
    description: &str,
) -> Result<PathBuf> {
    let relative_path = canonical_path.strip_prefix(project_root).with_context(|| {
        format!(
            "{description} resolves outside the MVC project: {}",
            canonical_path.display()
        )
    })?;
    normalized_mvc_bundle_relative_path(relative_path)
}

fn normalized_mvc_bundle_relative_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            _ => bail!(
                "MVC bundle path must stay project-relative: {}",
                path.display()
            ),
        }
    }
    validate_bundle_relative_path(&normalized)?;
    Ok(normalized)
}

fn absolute_package_output_path(output: &Path) -> Result<PathBuf> {
    if output.is_absolute() {
        Ok(output.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("failed to read current directory")?
            .join(output))
    }
}

fn collect_non_git_mvc_bundle_files(
    project_root: &Path,
    current: &Path,
    output_path: &Path,
    policy: &MvcBundlePolicy,
    files: &mut Vec<MvcBundleFile>,
) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", current.display()))?;
        let path = entry.path();
        let relative_path = path
            .strip_prefix(project_root)
            .with_context(|| format!("failed to make {} project-relative", path.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            if should_skip_mvc_bundle_directory(relative_path) {
                continue;
            }
            collect_non_git_mvc_bundle_files(project_root, &path, output_path, policy, files)?;
        } else if file_type.is_file() {
            add_mvc_bundle_file(project_root, relative_path, output_path, policy, files)?;
        } else if file_type.is_symlink() {
            bail!(
                "refusing to package symbolic link {}; MVC bundles contain regular project files only",
                relative_path.display()
            );
        }
    }
    Ok(())
}

fn git_mvc_bundle_file_paths(project_root: &Path) -> Result<Option<Vec<PathBuf>>> {
    let has_git_marker = project_root
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists());
    let discovery = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    let discovery = match discovery {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !has_git_marker => {
            return Ok(None);
        }
        Err(error) => {
            return Err(error).context(
                "failed to inspect Git state for MVC packaging; refusing to guess which ignored files are private",
            );
        }
    };
    if !discovery.status.success() {
        if !has_git_marker {
            return Ok(None);
        }
        bail!(
            "failed to inspect Git worktree for MVC packaging (status {}): {}",
            discovery.status,
            String::from_utf8_lossy(&discovery.stderr).trim()
        );
    }
    if String::from_utf8_lossy(&discovery.stdout).trim() != "true" {
        bail!(
            "MVC project is associated with Git but is not inside a worktree; refusing to guess which ignored files are private"
        );
    }

    let worktree_root = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to locate the MVC project's Git worktree root")?;
    if !worktree_root.status.success() {
        bail!(
            "failed to locate the MVC project's Git worktree root (status {}): {}",
            worktree_root.status,
            String::from_utf8_lossy(&worktree_root.stderr).trim()
        );
    }
    let worktree_root = PathBuf::from(
        String::from_utf8(worktree_root.stdout)
            .context("Git returned a non-UTF-8 worktree root")?
            .trim(),
    )
    .canonicalize()
    .context("failed to resolve the MVC project's Git worktree root")?;

    if worktree_root != project_root {
        let ignored_root = std::process::Command::new("git")
            .arg("-C")
            .arg(project_root)
            .args(["check-ignore", "--quiet", "--no-index", "--"])
            .arg(project_root)
            .output()
            .context("failed to determine whether the MVC project root is Git-ignored")?;
        match ignored_root.status.code() {
            Some(0) => {
                let tracked = std::process::Command::new("git")
                    .arg("-C")
                    .arg(project_root)
                    .args(["ls-files", "--cached", "-z", "--", "."])
                    .output()
                    .context(
                        "failed to inspect tracked files under the ignored MVC project root",
                    )?;
                if !tracked.status.success() {
                    bail!(
                        "failed to inspect tracked files under the ignored MVC project root (status {}): {}",
                        tracked.status,
                        String::from_utf8_lossy(&tracked.stderr).trim()
                    );
                }
                if tracked.stdout.is_empty() {
                    return Ok(None);
                }
            }
            Some(1) => {}
            _ => {
                bail!(
                    "failed to determine whether the MVC project root is Git-ignored (status {}): {}",
                    ignored_root.status,
                    String::from_utf8_lossy(&ignored_root.stderr).trim()
                );
            }
        }
    }

    let listing = std::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--deduplicate",
            "-z",
            "--",
            ".",
        ])
        .output()
        .context("failed to list Git-selected MVC package files")?;
    if !listing.status.success() {
        bail!(
            "failed to list Git-selected MVC package files (status {}): {}",
            listing.status,
            String::from_utf8_lossy(&listing.stderr).trim()
        );
    }
    let listing = String::from_utf8(listing.stdout)
        .context("Git returned a non-UTF-8 MVC package path; bundle paths must be UTF-8")?;
    let mut paths = Vec::new();
    for path in listing.split('\0').filter(|path| !path.is_empty()) {
        let path = PathBuf::from(path);
        validate_bundle_relative_path(&path)?;
        paths.push(path);
    }
    Ok(Some(paths))
}

fn add_mvc_bundle_file(
    project_root: &Path,
    relative_path: &Path,
    output_path: &Path,
    policy: &MvcBundlePolicy,
    files: &mut Vec<MvcBundleFile>,
) -> Result<()> {
    validate_bundle_relative_path(relative_path)?;
    let path = project_root.join(relative_path);
    if same_package_output_file(&path, output_path) || omit_mvc_bundle_file(relative_path, policy) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to inspect MVC package file {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to package symbolic link {}; MVC bundles contain regular project files only",
            relative_path.display()
        );
    }
    if !metadata.is_file() {
        bail!(
            "Git selected non-file MVC package path {}; initialize submodules and package regular files only",
            relative_path.display()
        );
    }
    if private_credential_path(relative_path) {
        bail!(
            "refusing to package private key or credential file {}; remove it from the MVC project or add it to Git ignore rules",
            relative_path.display()
        );
    }
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    if contains_private_key_pem(&bytes) {
        bail!(
            "refusing to package private key or credential file {}; PEM private keys must never be embedded",
            relative_path.display()
        );
    }
    files.push(MvcBundleFile {
        relative_path: relative_path.to_path_buf(),
        bytes,
    });
    Ok(())
}

fn omit_mvc_bundle_file(relative_path: &Path, policy: &MvcBundlePolicy) -> bool {
    if has_repository_metadata_component(relative_path) {
        return true;
    }
    let Some(file_name) = relative_path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let file_name = file_name.to_ascii_lowercase();
    if matches!(
        file_name.as_str(),
        ".gitignore" | ".gitattributes" | ".gitmodules"
    ) && !policy.is_dependency_file(relative_path)
    {
        return true;
    }
    if file_name == ".env.example" || file_name == ".env.sample" || file_name == ".env.template" {
        return false;
    }
    if file_name == ".env" || file_name.starts_with(".env.") {
        return true;
    }

    let sqlite_base_name = file_name
        .strip_suffix("-wal")
        .or_else(|| file_name.strip_suffix("-shm"))
        .or_else(|| file_name.strip_suffix("-journal"))
        .unwrap_or(&file_name);
    if sqlite_base_name.ends_with(".sqlite")
        || sqlite_base_name.ends_with(".sqlite3")
        || sqlite_base_name.ends_with(".db")
    {
        return true;
    }
    if let Some(configured_path) = &policy.sqlite_state_path {
        let candidate = path_to_bundle_string(relative_path)
            .unwrap_or_else(|_| relative_path.to_string_lossy().replace('\\', "/"))
            .to_ascii_lowercase();
        let configured = path_to_bundle_string(configured_path)
            .unwrap_or_else(|_| configured_path.to_string_lossy().replace('\\', "/"))
            .to_ascii_lowercase();
        if candidate == configured
            || candidate == format!("{configured}-wal")
            || candidate == format!("{configured}-shm")
            || candidate == format!("{configured}-journal")
        {
            return true;
        }
    }
    false
}

fn private_credential_path(relative_path: &Path) -> bool {
    let Some(file_name) = relative_path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let file_name = file_name.to_ascii_lowercase();
    matches!(
        file_name.as_str(),
        "id_rsa"
            | "id_dsa"
            | "id_ecdsa"
            | "id_ed25519"
            | "credentials.json"
            | "secrets.json"
            | "secrets.toml"
            | "secrets.yaml"
            | "secrets.yml"
    ) || [".key", ".p12", ".pfx", ".jks", ".keystore"]
        .iter()
        .any(|extension| file_name.ends_with(extension))
}

fn contains_private_key_pem(bytes: &[u8]) -> bool {
    bytes.split(|byte| *byte == b'\n').any(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        line.len() <= 160
            && line.starts_with(b"-----BEGIN ")
            && line.ends_with(b"-----")
            && line
                .windows(b"PRIVATE KEY".len())
                .any(|window| window == b"PRIVATE KEY")
    })
}

fn should_skip_mvc_bundle_directory(relative_path: &Path) -> bool {
    has_repository_metadata_component(relative_path)
        || relative_path.components().next().is_some_and(
            |component| matches!(component, Component::Normal(name) if name == "target"),
        )
}

fn same_package_output_file(path: &Path, output_path: &Path) -> bool {
    if path == output_path {
        return true;
    }
    match (path.canonicalize(), output_path.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn validate_bundle_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("MVC bundle path must not be empty");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => bail!(
                "MVC bundle path must stay project-relative: {}",
                path.display()
            ),
        }
    }
    Ok(())
}

fn path_to_bundle_string(path: &Path) -> Result<String> {
    validate_bundle_relative_path(path)?;
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .with_context(|| format!("MVC bundle path is not UTF-8: {}", path.display()))?;
                parts.push(value.to_string());
            }
            _ => bail!(
                "MVC bundle path must stay project-relative: {}",
                path.display()
            ),
        }
    }
    Ok(parts.join("/"))
}

fn bundle_string_to_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() || path.split('/').any(|part| part.is_empty()) {
        bail!("MVC bundle path must not be empty");
    }
    let mut result = PathBuf::new();
    for part in path.split('/') {
        if part == "." || part == ".." || part.contains('\\') {
            bail!("MVC bundle path must stay project-relative: {path}");
        }
        result.push(part);
    }
    Ok(result)
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn extract_embedded_mvc_bundle(bundle: &MvcBundle) -> Result<PathBuf> {
    let root = unique_mvc_extract_dir()?;
    bundle.extract_to(&root)?;
    Ok(root)
}

pub(crate) fn packaged_mvc_data_root(project_root: &Path) -> Result<PathBuf> {
    let manifest_path = project_root.join("ricochet.toml");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let package_name = manifest
        .get("package")
        .and_then(Item::as_table)
        .and_then(|package| package.get("name"))
        .and_then(Item::as_str)
        .context("packaged MVC manifest package.name must be a string")?;
    let executable = std::env::current_exe()
        .context("failed to locate packaged MVC executable for its user data identity")?
        .canonicalize()
        .context("failed to resolve packaged MVC executable for its user data identity")?;
    let storage_key = mvc_app_storage_key(package_name, &executable)?;
    let data_root = mvc_data_home_base()?.join(storage_key);
    fs::create_dir_all(&data_root).with_context(|| {
        format!(
            "failed to create MVC user data directory {}",
            data_root.display()
        )
    })?;
    restrict_mvc_data_directory_permissions(&data_root)?;
    data_root.canonicalize().with_context(|| {
        format!(
            "failed to resolve MVC user data directory {}",
            data_root.display()
        )
    })
}

#[cfg(unix)]
fn restrict_mvc_data_directory_permissions(data_root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(data_root, fs::Permissions::from_mode(0o700)).with_context(|| {
        format!(
            "failed to restrict packaged MVC user data permissions on {}",
            data_root.display()
        )
    })
}

#[cfg(not(unix))]
fn restrict_mvc_data_directory_permissions(_data_root: &Path) -> Result<()> {
    Ok(())
}

fn mvc_data_home_base() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(MVC_DATA_HOME_ENV) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            bail!("{MVC_DATA_HOME_ENV} must be an absolute path");
        }
        return Ok(path);
    }
    default_mvc_data_home()
}

#[cfg(windows)]
fn default_mvc_data_home() -> Result<PathBuf> {
    let local_app_data = absolute_mvc_data_env_path("LOCALAPPDATA")?;
    Ok(local_app_data.join("Ricochet/MvcApps"))
}

#[cfg(target_os = "macos")]
fn default_mvc_data_home() -> Result<PathBuf> {
    let home = absolute_mvc_data_env_path("HOME")?;
    Ok(home.join("Library/Application Support/Ricochet/MvcApps"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_mvc_data_home() -> Result<PathBuf> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        let data_home = PathBuf::from(data_home);
        if !data_home.is_absolute() {
            bail!("XDG_DATA_HOME must be an absolute path for packaged MVC user data");
        }
        return Ok(data_home.join("ricochet/mvc-apps"));
    }
    let home = absolute_mvc_data_env_path("HOME")?;
    Ok(home.join(".local/share/ricochet/mvc-apps"))
}

#[cfg(not(any(windows, unix)))]
fn default_mvc_data_home() -> Result<PathBuf> {
    bail!("packaged MVC user data is not supported on this platform")
}

fn absolute_mvc_data_env_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("{name} is not set; cannot locate packaged MVC user data"))?;
    if !path.is_absolute() {
        bail!("{name} must be an absolute path for packaged MVC user data");
    }
    Ok(path)
}

fn mvc_app_storage_key(package_name: &str, executable: &Path) -> Result<String> {
    if package_name.is_empty() {
        bail!("packaged MVC manifest package.name must not be empty");
    }
    if package_name.trim() != package_name {
        bail!("packaged MVC manifest package.name must not have leading or trailing whitespace");
    }
    if package_name.len() > 256 {
        bail!("packaged MVC manifest package.name must not exceed 256 bytes");
    }
    let mut slug = String::new();
    let mut previous_separator = false;
    for character in package_name.chars() {
        let normalized = character.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() || matches!(normalized, '-' | '_') {
            slug.push(normalized);
            previous_separator = false;
        } else if !previous_separator && !slug.is_empty() {
            slug.push('-');
            previous_separator = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "app" } else { slug };
    let executable = executable
        .to_str()
        .context("packaged MVC executable path must be UTF-8 for stable user data identity")?;
    #[cfg(windows)]
    let executable = executable.to_ascii_lowercase();
    let mut identity = Sha256::new();
    identity.update(package_name.as_bytes());
    identity.update([0]);
    identity.update(executable.as_bytes());
    let digest = identity.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{slug}-{hash}"))
}

fn unique_mvc_extract_dir() -> Result<PathBuf> {
    let temp_dir = tempfile::Builder::new()
        .prefix("ricochet-mvc-")
        .tempdir()
        .context("failed to create MVC extraction directory")?;
    Ok(temp_dir.keep())
}

fn native_gui_packaging_supported() -> bool {
    cfg!(any(windows, target_os = "linux", target_os = "macos"))
}

fn package_launcher(gui: bool, gui_launcher: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = gui_launcher {
        if !path.is_file() {
            bail!("GUI launcher does not exist: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    let current_exe =
        std::env::current_exe().context("failed to locate current Ricochet executable")?;
    if !gui {
        return Ok(current_exe);
    }

    if current_exe
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "rco-gui")
    {
        return Ok(current_exe);
    }

    let gui_launcher =
        current_exe.with_file_name(format!("rco-gui{}", std::env::consts::EXE_SUFFIX));
    if gui_launcher.is_file() {
        return Ok(gui_launcher);
    }

    bail!(
        "rco package --gui requires the rco-gui launcher next to rco; build it with `cargo build -p ricochet_cli --bin rco-gui` or pass --gui-launcher PATH"
    )
}

fn append_embedded_payload(path: &Path, payload: &[u8], kind: EmbeddedAppKind) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {} for packaging", path.display()))?;
    file.write_all(payload)
        .with_context(|| format!("failed to append embedded app to {}", path.display()))?;
    file.write_all(kind.marker())
        .with_context(|| format!("failed to append package marker to {}", path.display()))?;
    file.write_all(&(payload.len() as u64).to_le_bytes())
        .with_context(|| format!("failed to append package length to {}", path.display()))?;
    Ok(())
}

fn ensure_linux_package_host() -> Result<()> {
    if std::env::consts::OS != "linux" {
        bail!(
            "Linux package artifacts can only be built on Linux; run this command on a Linux host or in the release workflow"
        );
    }
    Ok(())
}

fn create_linux_package_artifacts(
    executable: &Path,
    formats: &[LinuxPackageFormat],
    package_name: Option<&str>,
    package_version: &str,
    project_license: Option<&str>,
    package_description: &str,
    gui: bool,
) -> Result<()> {
    let artifact_dir = artifact_directory_for(executable);
    fs::create_dir_all(&artifact_dir)
        .with_context(|| format!("failed to create {}", artifact_dir.display()))?;

    let name = match package_name {
        Some(name) => name.to_string(),
        None => default_linux_package_name(executable),
    };
    validate_linux_package_name(&name)?;
    validate_linux_package_version(package_version)?;
    let description = linux_package_description(package_description);
    let staging_root = linux_package_staging_root(&name, package_version)?;
    let unique_formats: BTreeSet<_> = formats.iter().copied().collect();
    let metadata = LinuxPackageMetadata {
        name: &name,
        version: package_version,
        project_license,
        description: &description,
        gui,
    };

    for format in unique_formats {
        match format {
            LinuxPackageFormat::Tar => {
                create_linux_tarball(executable, &artifact_dir, &staging_root, &metadata)?
            }
            LinuxPackageFormat::Deb => {
                create_linux_deb(executable, &artifact_dir, &staging_root, &metadata)?
            }
        }
    }

    Ok(())
}

fn artifact_directory_for(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn default_linux_package_name(executable: &Path) -> String {
    let stem = executable
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("ricochet-app");
    sanitize_linux_package_name(stem)
}

fn sanitize_linux_package_name(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.') {
            output.push(ch);
        } else if ch == '_' || ch.is_ascii_whitespace() {
            output.push('-');
        }
    }

    while output
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit())
    {
        output.remove(0);
    }
    while output
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit())
    {
        output.pop();
    }

    if output.len() < 2 {
        "ricochet-app".to_string()
    } else {
        output
    }
}

fn validate_linux_package_name(name: &str) -> Result<()> {
    if name.len() < 2 {
        bail!("Linux package name must contain at least two characters");
    }
    let mut chars = name.chars();
    let first = chars
        .next()
        .expect("name length was checked before reading first char");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        bail!("Linux package name must start with a lowercase letter or digit");
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.'))
    {
        bail!("Linux package name may only contain lowercase letters, digits, '+', '-', or '.'");
    }
    Ok(())
}

fn validate_linux_package_version(version: &str) -> Result<()> {
    if version.trim().is_empty() {
        bail!("Linux package version must not be empty");
    }
    if version
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '/' | '\\'))
    {
        bail!("Linux package version must not contain whitespace or path separators");
    }
    Version::parse(version)
        .with_context(|| format!("Linux package version must be valid SemVer: {version:?}"))?;
    Ok(())
}

fn linux_package_project_license(
    writes_appstream_metadata: bool,
    project_license: Option<&str>,
) -> Result<Option<&str>> {
    if !writes_appstream_metadata {
        return Ok(None);
    }

    let project_license = project_license
        .map(str::trim)
        .filter(|license| !license.is_empty())
        .context(
            "--package-license SPDX is required with --gui when creating Linux package artifacts",
        )?;
    if let Err(error) = spdx::Expression::parse(project_license) {
        bail!("--package-license must be a valid SPDX expression: {error}");
    }

    Ok(Some(project_license))
}

fn linux_package_description(description: &str) -> String {
    let description = description
        .lines()
        .next()
        .unwrap_or("Packaged Ricochet application")
        .trim();
    if description.is_empty() {
        "Packaged Ricochet application".to_string()
    } else {
        description.to_string()
    }
}

fn linux_package_staging_root(name: &str, version: &str) -> Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_nanos();
    let root = std::env::temp_dir()
        .join("ricochet-package")
        .join(format!("{name}-{version}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

#[derive(Clone, Copy)]
struct LinuxPackageMetadata<'a> {
    name: &'a str,
    version: &'a str,
    project_license: Option<&'a str>,
    description: &'a str,
    gui: bool,
}

fn create_linux_tarball(
    executable: &Path,
    artifact_dir: &Path,
    staging_root: &Path,
    metadata: &LinuxPackageMetadata<'_>,
) -> Result<()> {
    let LinuxPackageMetadata {
        name,
        version,
        project_license,
        description,
        gui,
    } = *metadata;
    let package_dir_name = format!("{name}-v{version}-linux-x64");
    let package_dir = staging_root.join(&package_dir_name);
    let archive = artifact_dir.join(format!("{package_dir_name}.tar.gz"));
    assert_new_artifact(&archive)?;

    fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;
    copy_executable(executable, &package_dir.join(name))?;
    fs::write(
        package_dir.join("README.txt"),
        format!(
            "{description}\n\nCommands:\n  ./{name} --help\n  ./{name}\n\nInstall locally:\n  ./install.sh\n\nCurrent Linux launchers require GTK 3, WebKitGTK 4.1, and libxdo 3 runtime libraries.\n"
        ),
    )
    .with_context(|| format!("failed to write {}", package_dir.join("README.txt").display()))?;
    fs::write(
        package_dir.join("CHANGELOG.txt"),
        format!("{name} ({version})\n\n  * Packaged Ricochet application release.\n"),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_dir.join("CHANGELOG.txt").display()
        )
    })?;
    if gui {
        write_linux_gui_metadata(
            &package_dir.join("share"),
            name,
            version,
            description,
            project_license.expect("GUI package license was validated before staging"),
        )?;
    }
    write_linux_install_script(&package_dir.join("install.sh"), name, gui)?;

    let output = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(staging_root)
        .arg(&package_dir_name)
        .output()
        .context("failed to launch tar for Linux package")?;
    ensure_command_success("tar", &output)?;

    println!("packaged {}", archive.display());
    Ok(())
}

fn create_linux_deb(
    executable: &Path,
    artifact_dir: &Path,
    staging_root: &Path,
    metadata: &LinuxPackageMetadata<'_>,
) -> Result<()> {
    let LinuxPackageMetadata {
        name,
        version,
        project_license,
        description,
        gui,
    } = *metadata;
    let debian_version = debian_package_version(version)?;
    let deb_path = artifact_dir.join(format!("{name}_{debian_version}_amd64.deb"));
    assert_new_artifact(&deb_path)?;

    let deb_root = staging_root.join(format!("{name}-deb-root"));
    let control_dir = deb_root.join("DEBIAN");
    let bin_dir = deb_root.join("usr/bin");
    let doc_dir = deb_root.join("usr/share/doc").join(name);
    let share_dir = deb_root.join("usr/share");

    fs::create_dir_all(&control_dir)
        .with_context(|| format!("failed to create {}", control_dir.display()))?;
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("failed to create {}", bin_dir.display()))?;
    fs::create_dir_all(&doc_dir)
        .with_context(|| format!("failed to create {}", doc_dir.display()))?;

    copy_executable(executable, &bin_dir.join(name))?;
    fs::write(
        doc_dir.join("README.txt"),
        format!("{description}\n\nThis package was generated from a Ricochet .rco file.\n"),
    )
    .with_context(|| format!("failed to write {}", doc_dir.join("README.txt").display()))?;
    fs::write(
        doc_dir.join("changelog"),
        format!("{name} ({debian_version})\n\n  * Packaged Ricochet application release.\n"),
    )
    .with_context(|| format!("failed to write {}", doc_dir.join("changelog").display()))?;
    if gui {
        write_linux_gui_metadata(
            &share_dir,
            name,
            version,
            description,
            project_license.expect("GUI package license was validated before staging"),
        )?;
    }
    fs::write(
        control_dir.join("control"),
        format!(
            "Package: {name}\nVersion: {debian_version}\nSection: devel\nPriority: optional\nArchitecture: amd64\nDepends: libgtk-3-0, libwebkit2gtk-4.1-0, libxdo3\nMaintainer: Ricochet Packager <noreply@ricochet.today>\nDescription: {description}\n"
        ),
    )
    .with_context(|| format!("failed to write {}", control_dir.join("control").display()))?;

    let output = std::process::Command::new("dpkg-deb")
        .arg("--root-owner-group")
        .arg("--build")
        .arg(&deb_root)
        .arg(&deb_path)
        .output()
        .context("failed to launch dpkg-deb for Linux package")?;
    ensure_command_success("dpkg-deb", &output)?;

    println!("packaged {}", deb_path.display());
    Ok(())
}

fn debian_package_version(version: &str) -> Result<String> {
    let parsed = Version::parse(version).with_context(|| {
        format!("cannot convert invalid SemVer {version:?} to a Debian version")
    })?;
    let mut debian = format!("{}.{}.{}", parsed.major, parsed.minor, parsed.patch);
    if !parsed.pre.is_empty() {
        debian.push('~');
        debian.push_str(parsed.pre.as_str());
    }
    if !parsed.build.is_empty() {
        debian.push('+');
        debian.push_str(parsed.build.as_str());
    }
    Ok(debian)
}

fn assert_new_artifact(path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            "package artifact already exists: {}; choose a different --output, --package-name, or --package-version",
            path.display()
        );
    }
    Ok(())
}

fn copy_executable(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy executable {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    set_executable_permissions(destination)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set executable permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn write_linux_gui_metadata(
    share_dir: &Path,
    name: &str,
    version: &str,
    description: &str,
    project_license: &str,
) -> Result<()> {
    let applications_dir = share_dir.join("applications");
    let icons_dir = share_dir.join("icons/hicolor/scalable/apps");
    let metainfo_dir = share_dir.join("metainfo");
    fs::create_dir_all(&applications_dir)
        .with_context(|| format!("failed to create {}", applications_dir.display()))?;
    fs::create_dir_all(&icons_dir)
        .with_context(|| format!("failed to create {}", icons_dir.display()))?;
    fs::create_dir_all(&metainfo_dir)
        .with_context(|| format!("failed to create {}", metainfo_dir.display()))?;

    fs::write(
        applications_dir.join(format!("{name}.desktop")),
        linux_desktop_entry(name, description),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            applications_dir.join(format!("{name}.desktop")).display()
        )
    })?;
    fs::write(
        icons_dir.join(format!("{name}.svg")),
        linux_app_icon_svg(name),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            icons_dir.join(format!("{name}.svg")).display()
        )
    })?;
    fs::write(
        metainfo_dir.join(format!("{name}.metainfo.xml")),
        linux_app_metainfo(name, version, description, project_license),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            metainfo_dir.join(format!("{name}.metainfo.xml")).display()
        )
    })?;
    Ok(())
}

fn linux_desktop_entry(name: &str, description: &str) -> String {
    let display_name = linux_app_display_name(name);
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\nIcon={}\nTerminal=false\nCategories=Development;Utility;\nStartupNotify=true\n",
        desktop_entry_escape(&display_name),
        desktop_entry_escape(description),
        desktop_entry_escape(name),
        desktop_entry_escape(name)
    )
}

fn linux_app_metainfo(
    name: &str,
    version: &str,
    description: &str,
    project_license: &str,
) -> String {
    let component_id = appstream_component_id(name);
    let display_name = linux_app_display_name(name);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<component type=\"desktop-application\">\n  <id>{}</id>\n  <metadata_license>CC0-1.0</metadata_license>\n  <project_license>{}</project_license>\n  <name>{}</name>\n  <summary>{}</summary>\n  <description>\n    <p>{}</p>\n  </description>\n  <launchable type=\"desktop-id\">{}.desktop</launchable>\n  <provides>\n    <binary>{}</binary>\n  </provides>\n  <releases>\n    <release version=\"{}\" />\n  </releases>\n</component>\n",
        xml_escape(&component_id),
        xml_escape(project_license),
        xml_escape(&display_name),
        xml_escape(description),
        xml_escape(description),
        xml_escape(name),
        xml_escape(name),
        xml_escape(version)
    )
}

fn linux_app_icon_svg(name: &str) -> String {
    let letters = linux_app_display_name(name)
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>();
    let letters = if letters.is_empty() {
        "Rc".to_string()
    } else {
        letters
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 128 128\">\n  <rect width=\"128\" height=\"128\" rx=\"24\" fill=\"#1f2937\"/>\n  <path d=\"M28 36h72v16H48v18h42v16H48v30H28z\" fill=\"#f8fafc\"/>\n  <text x=\"64\" y=\"112\" text-anchor=\"middle\" font-family=\"Arial, sans-serif\" font-size=\"22\" font-weight=\"700\" fill=\"#38bdf8\">{}</text>\n</svg>\n",
        xml_escape(&letters)
    )
}

fn linux_app_display_name(name: &str) -> String {
    let mut output = String::new();
    for part in name
        .split(|ch: char| !(ch.is_ascii_alphanumeric()))
        .filter(|part| !part.is_empty())
    {
        if !output.is_empty() {
            output.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            for ch in chars {
                output.push(ch.to_ascii_lowercase());
            }
        }
    }
    if output.is_empty() {
        "Ricochet App".to_string()
    } else {
        output
    }
}

fn appstream_component_id(name: &str) -> String {
    let mut suffix = String::new();
    for ch in name.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '.') {
            suffix.push(ch);
        } else if ch == '+' {
            suffix.push('-');
        }
    }
    if suffix.is_empty() {
        "today.ricochet.ricochet-app".to_string()
    } else {
        format!("today.ricochet.{suffix}")
    }
}

fn desktop_entry_escape(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' => ' ',
            _ => ch,
        })
        .collect()
}

fn xml_escape(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(ch),
        }
    }
    output
}

fn write_linux_install_script(path: &Path, binary_name: &str, gui: bool) -> Result<()> {
    let metadata_install = if gui {
        r#"
share_dir="$prefix/share"
if [ -d "$script_dir/share/applications" ]; then
  mkdir -p "$share_dir/applications"
  cp "$script_dir/share/applications/"*.desktop "$share_dir/applications/"
fi
if [ -d "$script_dir/share/metainfo" ]; then
  mkdir -p "$share_dir/metainfo"
  cp "$script_dir/share/metainfo/"*.metainfo.xml "$share_dir/metainfo/"
fi
if [ -d "$script_dir/share/icons/hicolor/scalable/apps" ]; then
  mkdir -p "$share_dir/icons/hicolor/scalable/apps"
  cp "$script_dir/share/icons/hicolor/scalable/apps/"*.svg "$share_dir/icons/hicolor/scalable/apps/"
fi
"#
    } else {
        ""
    };
    fs::write(
        path,
        format!(
            r#"#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="${{PREFIX:-$HOME/.local}}"
bin_dir="$prefix/bin"

mkdir -p "$bin_dir"
cp "$script_dir/{binary_name}" "$bin_dir/{binary_name}"
chmod 755 "$bin_dir/{binary_name}"
{metadata_install}

printf 'Installed {binary_name} to %s\n' "$bin_dir"
printf 'Make sure %s is on your PATH.\n' "$bin_dir"
"#
        ),
    )
    .with_context(|| format!("failed to write {}", path.display()))?;
    set_executable_permissions(path)?;
    Ok(())
}

fn ensure_command_success(command: &str, output: &std::process::Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "{command} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn embedded_app_from_current_exe() -> Result<Option<EmbeddedApp>> {
    let current_exe =
        std::env::current_exe().context("failed to locate current Ricochet executable")?;
    let bytes = fs::read(&current_exe)
        .with_context(|| format!("failed to read {}", current_exe.display()))?;
    embedded_app_from_bytes(&bytes)
        .with_context(|| format!("failed to load embedded app from {}", current_exe.display()))
}

fn embedded_app_from_bytes(bytes: &[u8]) -> Result<Option<EmbeddedApp>> {
    for kind in [
        EmbeddedAppKind::MvcGui,
        EmbeddedAppKind::Gui,
        EmbeddedAppKind::Tui,
        EmbeddedAppKind::Console,
    ] {
        if let Some(app) = embedded_app_from_bytes_with_marker(bytes, kind)? {
            return Ok(Some(app));
        }
    }
    Ok(None)
}

fn embedded_app_from_bytes_with_marker(
    bytes: &[u8],
    kind: EmbeddedAppKind,
) -> Result<Option<EmbeddedApp>> {
    let marker = kind.marker();
    let trailer_len = marker.len() + 8;
    if bytes.len() < trailer_len {
        return Ok(None);
    }

    let length_start = bytes.len() - 8;
    let marker_start = length_start - marker.len();
    if &bytes[marker_start..length_start] != marker {
        return Ok(None);
    }

    let mut length_bytes = [0_u8; 8];
    length_bytes.copy_from_slice(&bytes[length_start..]);
    let chunk_len = u64::from_le_bytes(length_bytes) as usize;
    if marker_start < chunk_len {
        bail!("embedded Ricochet app length exceeds executable size");
    }
    let payload_start = marker_start - chunk_len;
    let payload_bytes = &bytes[payload_start..marker_start];
    let payload = match kind {
        EmbeddedAppKind::Console | EmbeddedAppKind::Tui | EmbeddedAppKind::Gui => {
            EmbeddedAppPayload::Chunk(Chunk::from_bytes(payload_bytes)?)
        }
        EmbeddedAppKind::MvcGui => {
            EmbeddedAppPayload::MvcBundle(MvcBundle::from_bytes(payload_bytes)?)
        }
    };
    Ok(Some(EmbeddedApp { kind, payload }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_mvc_manifest(root: &Path, package_name: &str) {
        fs::write(
            root.join("ricochet.toml"),
            format!(
                r#"[package]
name = "{package_name}"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#
            ),
        )
        .expect("test MVC manifest should be written");
    }

    fn write_test_mvc_path_dependency(root: &Path, dependency_name: &str) {
        let dependency_path = root.join("packages").join(dependency_name);
        fs::create_dir_all(&dependency_path).expect("test dependency directory");
        fs::write(
            dependency_path.join("ricochet.toml"),
            format!(
                r#"[package]
name = "{dependency_name}"
version = "0.1.0"
"#
            ),
        )
        .expect("test dependency manifest");
        fs::write(dependency_path.join("main.rco"), b"42").expect("test dependency source");

        let manifest_path = root.join("ricochet.toml");
        let mut manifest = fs::read_to_string(&manifest_path).expect("MVC manifest source");
        manifest.push_str(&format!(
            r#"
[dependencies.{dependency_name}]
path = "./packages/{dependency_name}"
version = "^0.1.0"
"#
        ));
        fs::write(&manifest_path, manifest).expect("MVC dependency manifest");

        let integrity = package_tree_integrity(&dependency_path).expect("dependency integrity");
        fs::write(
            root.join("ricochet.lock"),
            format!(
                r#"[package]

[package.{dependency_name}]
source = "path+./packages/{dependency_name}"
path = "./packages/{dependency_name}"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("test dependency lockfile");
    }

    fn write_test_mvc_git_dependency(root: &Path, dependency_name: &str) {
        let dependency_path = root
            .join(".ricochet")
            .join("packages")
            .join(dependency_name);
        fs::create_dir_all(&dependency_path).expect("test Git dependency directory");
        fs::write(
            dependency_path.join("ricochet.toml"),
            format!(
                r#"[package]
name = "{dependency_name}"
version = "0.1.0"
"#
            ),
        )
        .expect("test Git dependency manifest");
        fs::write(dependency_path.join("main.rco"), b"42").expect("test Git dependency source");
        fs::write(dependency_path.join(".gitignore"), b"target/\n")
            .expect("test Git dependency ignore metadata");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(&dependency_path)
            .output()
            .expect("git should initialize the test dependency");
        assert!(git.status.success());
        let add = std::process::Command::new("git")
            .arg("-C")
            .arg(&dependency_path)
            .args(["add", "--", "."])
            .output()
            .expect("git should stage the test dependency");
        assert!(add.status.success());
        let commit = std::process::Command::new("git")
            .arg("-C")
            .arg(&dependency_path)
            .args([
                "-c",
                "user.name=Ricochet Tests",
                "-c",
                "user.email=tests@ricochet.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ])
            .output()
            .expect("git should commit the test dependency");
        assert!(
            commit.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
        let revision = std::process::Command::new("git")
            .arg("-C")
            .arg(&dependency_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("git should resolve the test dependency commit");
        assert!(revision.status.success());
        let revision = String::from_utf8(revision.stdout)
            .expect("test dependency commit should be UTF-8")
            .trim()
            .to_string();

        let manifest_path = root.join("ricochet.toml");
        let mut manifest = fs::read_to_string(&manifest_path).expect("MVC manifest source");
        manifest.push_str(&format!(
            r#"
[dependencies.{dependency_name}]
git = "https://example.invalid/{dependency_name}.git"
path = ".ricochet/packages/{dependency_name}"
version = "^0.1.0"
"#
        ));
        fs::write(&manifest_path, manifest).expect("MVC Git dependency manifest");

        let integrity = package_tree_integrity(&dependency_path).expect("Git dependency integrity");
        fs::write(
            root.join("ricochet.lock"),
            format!(
                r#"[package]

[package.{dependency_name}]
source = "git+https://example.invalid/{dependency_name}.git"
path = ".ricochet/packages/{dependency_name}"
git = "https://example.invalid/{dependency_name}.git"
commit = "{revision}"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("test Git dependency lockfile");
    }

    fn bundle_paths(bundle: &MvcBundle) -> BTreeSet<String> {
        bundle
            .files
            .iter()
            .map(|file| path_to_bundle_string(&file.relative_path).expect("valid bundle path"))
            .collect()
    }

    #[cfg(unix)]
    fn create_test_directory_link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).expect("test directory symlink");
    }

    #[cfg(windows)]
    fn create_test_directory_link(target: &Path, link: &Path) {
        std::os::windows::fs::symlink_dir(target, link).expect("test directory symlink");
    }

    #[test]
    fn mvc_bundle_honors_git_ignores_and_never_embeds_local_secrets_or_database_state() {
        let repository = tempfile::tempdir().expect("temporary Git repository");
        let root = repository.path().join("apps/bundle-policy-app");
        fs::create_dir_all(&root).expect("nested MVC project");
        let root = root.as_path();
        write_test_mvc_manifest(root, "bundle_policy_app");
        let manifest_path = root.join("ricochet.toml");
        let mut manifest = fs::read_to_string(&manifest_path).expect("manifest source");
        manifest.push_str(
            r#"
[database.default]
adapter = "sqlite"
url = "./data/state.db"
"#,
        );
        fs::write(&manifest_path, manifest).expect("SQLite manifest");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("app/Controllers")).expect("controller directory");
        fs::create_dir_all(root.join("custom-assets")).expect("custom asset directory");
        fs::create_dir_all(root.join("db/migrations")).expect("migration directory");
        fs::create_dir_all(root.join("data")).expect("database directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("app/Controllers/HomeController.rco"), b"source")
            .expect("controller source");
        fs::write(root.join("custom-assets/theme.bin"), b"asset").expect("custom asset");
        fs::write(root.join("ignored-local.txt"), b"ignored").expect("ignored file");
        fs::write(root.join(".env"), b"TOKEN=secret").expect("environment file");
        fs::write(root.join(".env.production"), b"TOKEN=production-secret")
            .expect("production environment file");
        fs::write(root.join(".env.example"), b"TOKEN=replace-me").expect("environment template");
        fs::write(root.join("db/development.sqlite3"), b"private database")
            .expect("development database");
        fs::write(root.join("db/development.sqlite3-wal"), b"private WAL").expect("database WAL");
        fs::write(root.join("db/development.sqlite3-shm"), b"private SHM").expect("database SHM");
        fs::write(root.join("db/unconfigured.db"), b"private database").expect("private DB");
        fs::write(root.join("db/unconfigured.db-journal"), b"private journal")
            .expect("private DB journal");
        fs::write(
            root.join("db/migrations/0001_schema.sql"),
            b"create table entries (id integer primary key);",
        )
        .expect("migration");
        fs::write(root.join("data/state.db"), b"private configured database")
            .expect("configured database");
        fs::write(root.join("data/state.db-wal"), b"private configured WAL")
            .expect("configured WAL");
        fs::write(root.join("data/state.db-shm"), b"private configured SHM")
            .expect("configured SHM");
        fs::write(
            root.join(".gitignore"),
            b"ignored-local.txt\n.env\n.env.*\n!.env.example\n",
        )
        .expect("gitignore");
        fs::write(
            repository.path().join(".gitignore"),
            b"apps/bundle-policy-app/ignored-from-parent.txt\n",
        )
        .expect("repository gitignore");
        fs::write(root.join("ignored-from-parent.txt"), b"ignored").expect("parent-ignored file");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(repository.path())
            .output()
            .expect("git should be available for the Git packaging policy test");
        assert!(
            git.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&git.stderr)
        );

        let output = root.join(format!("bundle-policy{}", std::env::consts::EXE_SUFFIX));
        let bundle = build_mvc_bundle(root, &output).expect("MVC bundle should build");
        let paths = bundle_paths(&bundle);

        for expected in [
            "ricochet.toml",
            "config/routes.rco",
            "app/Controllers/HomeController.rco",
            "custom-assets/theme.bin",
            "db/migrations/0001_schema.sql",
            ".env.example",
        ] {
            assert!(paths.contains(expected), "bundle should contain {expected}");
        }
        for forbidden in [
            ".gitignore",
            "ignored-local.txt",
            "ignored-from-parent.txt",
            ".env",
            ".env.production",
            "db/development.sqlite3",
            "db/development.sqlite3-wal",
            "db/development.sqlite3-shm",
            "db/unconfigured.db",
            "db/unconfigured.db-journal",
            "data/state.db",
            "data/state.db-wal",
            "data/state.db-shm",
        ] {
            assert!(
                !paths.contains(forbidden),
                "bundle must not contain {forbidden}; got {paths:#?}"
            );
        }
    }

    #[test]
    fn mvc_bundle_uses_git_ignores_when_the_project_is_the_worktree_root() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "worktree_root_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("included.asset"), b"included").expect("included asset");
        fs::write(root.join("ignored.asset"), b"ignored").expect("ignored asset");
        fs::write(root.join(".gitignore"), b"ignored.asset\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the worktree-root packaging test");
        assert!(git.status.success());

        let output = root.join(format!("worktree-root{}", std::env::consts::EXE_SUFFIX));
        let bundle = build_mvc_bundle(root, &output).expect("worktree-root MVC bundle");
        let paths = bundle_paths(&bundle);

        assert!(paths.contains("included.asset"));
        assert!(!paths.contains("ignored.asset"));
    }

    #[test]
    fn mvc_bundle_omits_unignored_repository_metadata_from_git_selected_apps() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "git_metadata_boundary_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("assets/.HG")).expect("Mercurial metadata directory");
        fs::create_dir_all(root.join("assets/.svn")).expect("Subversion metadata directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("assets/public.txt"), b"public").expect("public asset");
        fs::write(root.join("assets/.HG/hgrc"), b"secret = fixture\n")
            .expect("Mercurial metadata fixture");
        fs::write(root.join("assets/.svn/entries"), b"private history")
            .expect("Subversion metadata fixture");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the repository-metadata boundary test");
        assert!(git.status.success());

        let output = root.join(format!(
            "git-metadata-boundary-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let paths = bundle_paths(
            &build_mvc_bundle(root, &output).expect("Git-selected MVC bundle should build"),
        );
        assert!(paths.contains("assets/public.txt"));
        assert!(!paths.contains("assets/.HG/hgrc"));
        assert!(!paths.contains("assets/.svn/entries"));
    }

    #[test]
    fn mvc_bundle_omits_case_variant_repository_metadata_from_standalone_apps() {
        let project = tempfile::tempdir().expect("temporary standalone MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "standalone_metadata_boundary_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("assets/.Hg")).expect("Mercurial metadata directory");
        fs::create_dir_all(root.join("assets/.SVN")).expect("Subversion metadata directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("assets/public.txt"), b"public").expect("public asset");
        fs::write(root.join("assets/.Hg/hgrc"), b"secret = fixture\n")
            .expect("Mercurial metadata fixture");
        fs::write(root.join("assets/.SVN/entries"), b"private history")
            .expect("Subversion metadata fixture");

        let output = root.join(format!(
            "standalone-metadata-boundary-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let paths = bundle_paths(
            &build_mvc_bundle(root, &output).expect("standalone MVC bundle should build"),
        );
        assert!(paths.contains("assets/public.txt"));
        assert!(!paths.contains("assets/.Hg/hgrc"));
        assert!(!paths.contains("assets/.SVN/entries"));
    }

    #[test]
    fn mvc_bundle_rejects_ignored_required_manifest() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "ignored_manifest_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join(".gitignore"), b"/ricochet.toml\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the ignored-manifest test");
        assert!(git.status.success());

        let output = root.join(format!(
            "ignored-manifest-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("Git-ignored MVC manifest must stop packaging");

        assert!(
            error
                .to_string()
                .contains("required MVC manifest ricochet.toml was excluded from the bundle"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_ignored_required_routes() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "ignored_routes_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join(".gitignore"), b"/config/routes.rco\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the ignored-routes test");
        assert!(git.status.success());

        let output = root.join(format!(
            "ignored-routes-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("Git-ignored configured routes must stop packaging");

        assert!(
            error.to_string().contains(
                "configured MVC routes file config/routes.rco was excluded from the bundle"
            ),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_ignored_required_dependency_lockfile() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "ignored_lock_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        fs::write(root.join(".gitignore"), b"/ricochet.lock\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the ignored-lockfile test");
        assert!(git.status.success());

        let output = root.join(format!("ignored-lock-app{}", std::env::consts::EXE_SUFFIX));
        let error = build_mvc_bundle(root, &output)
            .expect_err("Git-ignored dependency lockfile must stop packaging");

        assert!(
            error
                .to_string()
                .contains("dependency lockfile ricochet.lock was excluded from the bundle"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_includes_ignored_locked_dependency_tree() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "ignored_dependency_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        fs::write(root.join(".gitignore"), b"/packages/greeter/\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the ignored-dependency test");
        assert!(git.status.success());

        let output = root.join(format!(
            "ignored-dependency-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let bundle = build_mvc_bundle(root, &output)
            .expect("locked dependency files should be included explicitly");
        let paths = bundle_paths(&bundle);
        assert!(paths.contains("packages/greeter/main.rco"));
        assert!(paths.contains("packages/greeter/ricochet.toml"));
    }

    #[test]
    fn mvc_bundle_preserves_the_complete_locked_dependency_tree() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "dependency_bundle_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the dependency-bundle test");
        assert!(git.status.success());

        let output = root.join(format!(
            "dependency-bundle-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let bundle = build_mvc_bundle(root, &output).expect("dependency bundle should build");
        let paths = bundle_paths(&bundle);
        for expected in [
            "ricochet.toml",
            "ricochet.lock",
            "config/routes.rco",
            "packages/greeter/main.rco",
            "packages/greeter/ricochet.toml",
        ] {
            assert!(paths.contains(expected), "bundle should contain {expected}");
        }

        let extracted = tempfile::tempdir().expect("temporary extracted bundle");
        bundle
            .extract_to(extracted.path())
            .expect("bundle extraction");
        let manifest_path = extracted.path().join("ricochet.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("extracted manifest")
            .parse::<DocumentMut>()
            .expect("parsed extracted manifest");
        assert_eq!(
            verify_dependency_manifest(extracted.path(), &manifest_path, &manifest, false)
                .expect("extracted dependency graph should retain locked integrity"),
            1
        );
    }

    #[test]
    fn mvc_bundle_expands_a_fetched_git_dependency_instead_of_its_nested_repo_placeholder() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "git_dependency_bundle_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_git_dependency(root, "greeter");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the nested-repository test");
        assert!(git.status.success());

        let output = root.join(format!(
            "git-dependency-bundle-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let bundle = build_mvc_bundle(root, &output)
            .expect("fetched Git dependency should expand into regular locked files");
        let paths = bundle_paths(&bundle);
        assert!(paths.contains(".ricochet/packages/greeter/main.rco"));
        assert!(paths.contains(".ricochet/packages/greeter/ricochet.toml"));
        assert!(paths.contains(".ricochet/packages/greeter/.gitignore"));
        assert!(
            paths
                .iter()
                .all(|path| !path.split('/').any(|part| part == ".git")),
            "bundle must not contain nested Git metadata: {paths:#?}"
        );

        let extracted = tempfile::tempdir().expect("temporary extracted bundle");
        bundle
            .extract_to(extracted.path())
            .expect("bundle extraction");
        verify_runtime_import_locks_for_parent(extracted.path())
            .expect("runtime dependency integrity should survive without Git metadata");
    }

    #[test]
    fn mvc_bundle_rejects_dependency_git_metadata_files() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "linked_dependency_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        let dependency_path = root.join("packages/greeter");
        fs::write(
            dependency_path.join(".git"),
            b"gitdir: C:/outside/worktrees/greeter\n",
        )
        .expect("linked-worktree metadata fixture");
        let integrity = package_tree_integrity(&dependency_path).expect("dependency integrity");
        fs::write(
            root.join("ricochet.lock"),
            format!(
                r#"[package]

[package.greeter]
source = "path+./packages/greeter"
path = "./packages/greeter"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("updated dependency lockfile");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the Git-metadata test");
        assert!(git.status.success());

        let output = root.join(format!(
            "linked-dependency-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("dependency .git metadata files must never be embedded");
        assert!(
            error
                .to_string()
                .contains("dependency greeter contains forbidden repository metadata"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_case_variant_dependency_git_metadata_directories() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "case_variant_git_metadata_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        let dependency_path = root.join("packages/greeter");
        fs::create_dir_all(dependency_path.join(".GIT")).expect("case-variant Git metadata");
        fs::write(dependency_path.join(".GIT/config"), b"external metadata")
            .expect("case-variant Git metadata fixture");
        let integrity = package_tree_integrity(&dependency_path).expect("dependency integrity");
        fs::write(
            root.join("ricochet.lock"),
            format!(
                r#"[package]

[package.greeter]
source = "path+./packages/greeter"
path = "./packages/greeter"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("updated dependency lockfile");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the case-variant Git-metadata test");
        assert!(git.status.success());

        let output = root.join(format!(
            "case-variant-git-metadata-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("case-variant dependency Git metadata must never be embedded");
        assert!(
            error
                .to_string()
                .contains("dependency greeter contains forbidden repository metadata"),
            "unexpected error: {error:#}"
        );
    }

    fn assert_dependency_repository_metadata_is_rejected(
        metadata_directory: &str,
        metadata_file: &str,
    ) {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "repository_metadata_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        write_test_mvc_path_dependency(root, "greeter");
        let dependency_path = root.join("packages/greeter");
        let metadata_path = dependency_path.join(metadata_directory);
        fs::create_dir_all(&metadata_path).expect("dependency repository metadata directory");
        fs::write(metadata_path.join(metadata_file), b"secret = fixture\n")
            .expect("dependency repository metadata fixture");
        let integrity = package_tree_integrity(&dependency_path).expect("dependency integrity");
        fs::write(
            root.join("ricochet.lock"),
            format!(
                r#"[package]

[package.greeter]
source = "path+./packages/greeter"
path = "./packages/greeter"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("updated dependency lockfile");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the repository-metadata test");
        assert!(git.status.success());

        let output = root.join(format!(
            "repository-metadata-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("dependency repository metadata must never be embedded");
        assert!(
            error
                .to_string()
                .contains("dependency greeter contains forbidden repository metadata"),
            "unexpected error for {metadata_directory}: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_case_variant_dependency_mercurial_metadata() {
        assert_dependency_repository_metadata_is_rejected(".HG", "hgrc");
    }

    #[test]
    fn mvc_bundle_rejects_dependency_subversion_metadata() {
        assert_dependency_repository_metadata_is_rejected(".svn", "entries");
    }

    #[test]
    fn mvc_bundle_rejects_a_required_route_with_a_symlink_ancestor() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "symlinked_routes_app");
        fs::create_dir_all(root.join("actual-config")).expect("actual config directory");
        fs::write(root.join("actual-config/routes.rco"), b"").expect("actual routes");
        create_test_directory_link(&root.join("actual-config"), &root.join("config"));
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the symlink-ancestor test");
        assert!(git.status.success());

        let output = root.join(format!(
            "symlinked-routes-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("required logical paths must not traverse symlink ancestors");
        assert!(
            error.to_string().contains(
                "configured MVC routes file path contains a symbolic link or reparse point"
            ),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_required_paths_follow_host_filesystem_case_semantics() {
        let project = tempfile::tempdir().expect("temporary Git MVC project");
        let root = project.path();
        fs::create_dir_all(root.join("Config")).expect("config directory");
        fs::create_dir_all(root.join("Packages/Greeter")).expect("dependency directory");
        fs::create_dir_all(root.join("DB/Migrations")).expect("migration directory");
        fs::write(
            root.join("Ricochet.toml"),
            r#"[package]
name = "case_semantics_app"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[dependencies.greeter]
path = "./packages/greeter"
version = "^0.1.0"

[database.default]
adapter = "sqlite"
url = "data/state.db"
"#,
        )
        .expect("case-variant MVC manifest");
        fs::write(root.join("Config/Routes.rco"), b"").expect("case-variant routes");
        fs::write(
            root.join("Packages/Greeter/Ricochet.toml"),
            b"[package]\nname = \"greeter\"\nversion = \"0.1.0\"\n",
        )
        .expect("case-variant dependency manifest");
        fs::write(root.join("Packages/Greeter/Main.rco"), b"42")
            .expect("case-variant dependency source");
        fs::write(
            root.join("DB/Migrations/0001_schema.sql"),
            b"create table entries (id integer primary key);",
        )
        .expect("case-variant migration");
        let integrity = package_tree_integrity(&root.join("Packages/Greeter"))
            .expect("case-variant dependency integrity");
        fs::write(
            root.join("Ricochet.lock"),
            format!(
                r#"[package]

[package.greeter]
source = "path+./packages/greeter"
path = "./packages/greeter"
version_req = "^0.1.0"
version = "0.1.0"
integrity = "{integrity}"
"#
            ),
        )
        .expect("case-variant dependency lockfile");
        let case_insensitive = root.join("ricochet.toml").is_file();
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(root)
            .output()
            .expect("git should be available for the path-case test");
        assert!(git.status.success());

        let output = root.join(format!(
            "case-semantics-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let result = build_mvc_bundle(root, &output);
        if case_insensitive {
            let paths = bundle_paths(&result.expect("host-valid path casing should package"));
            for expected in [
                "Ricochet.toml",
                "Ricochet.lock",
                "Config/Routes.rco",
                "Packages/Greeter/Main.rco",
                "Packages/Greeter/Ricochet.toml",
                "DB/Migrations/0001_schema.sql",
            ] {
                assert!(paths.contains(expected), "bundle should contain {expected}");
            }
        } else {
            assert!(
                result.is_err(),
                "case-sensitive hosts must reject mismatched manifest casing"
            );
        }
    }

    #[test]
    fn mvc_bundle_preserves_arbitrary_non_git_sources_and_assets_inside_the_project() {
        let project = tempfile::tempdir().expect("temporary MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "non_git_bundle_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("unconventional/runtime/templates"))
            .expect("custom source directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(
            root.join("unconventional/runtime/feature.rco"),
            b"custom source",
        )
        .expect("custom source");
        fs::write(
            root.join("unconventional/runtime/templates/splash.dat"),
            b"custom asset",
        )
        .expect("custom asset");

        let output = root.join(format!("non-git-bundle{}", std::env::consts::EXE_SUFFIX));
        let bundle = build_mvc_bundle(root, &output).expect("MVC bundle should build");
        let paths = bundle_paths(&bundle);

        assert!(paths.contains("unconventional/runtime/feature.rco"));
        assert!(paths.contains("unconventional/runtime/templates/splash.dat"));
    }

    #[test]
    fn mvc_bundle_treats_an_enclosing_worktree_ignored_project_as_standalone() {
        let repository = tempfile::tempdir().expect("temporary Git repository");
        let root = repository.path().join("apps/ignored-app");
        fs::create_dir_all(root.join("config")).expect("nested ignored MVC project");
        write_test_mvc_manifest(&root, "ignored_root_app");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("custom.asset"), b"standalone asset").expect("custom asset");
        fs::write(root.join(".env"), b"TOKEN=private").expect("private environment");
        fs::write(repository.path().join(".gitignore"), b"apps/ignored-app/\n")
            .expect("repository gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(repository.path())
            .output()
            .expect("git should be available for the ignored-root packaging test");
        assert!(git.status.success());

        let output = root.join(format!("ignored-root{}", std::env::consts::EXE_SUFFIX));
        let bundle =
            build_mvc_bundle(&root, &output).expect("ignored root should package standalone");
        let paths = bundle_paths(&bundle);

        assert!(paths.contains("custom.asset"));
        assert!(!paths.contains(".env"));
    }

    #[test]
    fn mvc_bundle_keeps_git_selection_for_tracked_files_under_an_ignored_parent() {
        let repository = tempfile::tempdir().expect("temporary Git repository");
        let root = repository.path().join("apps/tracked-ignored-app");
        fs::create_dir_all(root.join("config")).expect("nested ignored MVC project");
        write_test_mvc_manifest(&root, "tracked_ignored_root_app");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("included.asset"), b"tracked").expect("tracked asset");
        fs::write(root.join("untracked-private.txt"), b"private")
            .expect("untracked private fixture");
        fs::write(
            repository.path().join(".gitignore"),
            b"apps/tracked-ignored-app/\n",
        )
        .expect("repository gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(repository.path())
            .output()
            .expect("git should be available for the tracked ignored-root test");
        assert!(git.status.success());
        let add = std::process::Command::new("git")
            .arg("-C")
            .arg(repository.path())
            .args([
                "add",
                "--force",
                "--",
                "apps/tracked-ignored-app/ricochet.toml",
                "apps/tracked-ignored-app/config/routes.rco",
                "apps/tracked-ignored-app/included.asset",
            ])
            .output()
            .expect("git add should launch");
        assert!(
            add.status.success(),
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        );

        let output = root.join(format!(
            "tracked-ignored-root{}",
            std::env::consts::EXE_SUFFIX
        ));
        let bundle =
            build_mvc_bundle(&root, &output).expect("tracked ignored-root MVC bundle should build");
        let paths = bundle_paths(&bundle);

        assert!(paths.contains("included.asset"));
        assert!(!paths.contains("untracked-private.txt"));
    }

    #[test]
    fn mvc_bundle_fails_loudly_for_private_key_material() {
        let project = tempfile::tempdir().expect("temporary MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "private_key_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(
            root.join("production.key"),
            b"-----BEGIN PRIVATE KEY-----\nprivate\n-----END PRIVATE KEY-----\n",
        )
        .expect("private key fixture");

        let output = root.join(format!("private-key-app{}", std::env::consts::EXE_SUFFIX));
        let error = build_mvc_bundle(root, &output)
            .expect_err("private key material must stop MVC packaging");

        assert!(
            error
                .to_string()
                .contains("refusing to package private key or credential file production.key"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_detects_legacy_private_key_headers_in_arbitrary_files() {
        let project = tempfile::tempdir().expect("temporary MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "legacy_private_key_app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(
            root.join("certificate.pem"),
            b"-----BEGIN DSA PRIVATE KEY-----\nprivate\n-----END DSA PRIVATE KEY-----\n",
        )
        .expect("legacy private key fixture");

        let output = root.join(format!(
            "legacy-private-key-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("legacy private key material must stop MVC packaging");

        assert!(
            error
                .to_string()
                .contains("PEM private keys must never be embedded"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_file_backed_sqlite_without_migrations() {
        let project = tempfile::tempdir().expect("temporary MVC project");
        let root = project.path();
        write_test_mvc_manifest(root, "missing_migrations_app");
        let manifest_path = root.join("ricochet.toml");
        let mut manifest = fs::read_to_string(&manifest_path).expect("manifest source");
        manifest.push_str(
            r#"
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
"#,
        );
        fs::write(&manifest_path, manifest).expect("SQLite manifest");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("db")).expect("database directory");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(root.join("db/development.sqlite3"), b"development state")
            .expect("development database");

        let output = root.join(format!(
            "missing-migrations-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(root, &output)
            .expect_err("file-backed packaged SQLite requires migrations");

        assert!(
            error
                .to_string()
                .contains("requires at least one db/migrations migration"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_bundle_rejects_gitignored_sqlite_migrations() {
        let repository = tempfile::tempdir().expect("temporary Git repository");
        let root = repository.path().join("apps/ignored-migration-app");
        fs::create_dir_all(root.join("config")).expect("config directory");
        fs::create_dir_all(root.join("db/migrations")).expect("migrations directory");
        write_test_mvc_manifest(&root, "ignored_migration_app");
        let manifest_path = root.join("ricochet.toml");
        let mut manifest = fs::read_to_string(&manifest_path).expect("manifest source");
        manifest.push_str(
            r#"
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
"#,
        );
        fs::write(&manifest_path, manifest).expect("SQLite manifest");
        fs::write(root.join("config/routes.rco"), b"").expect("routes");
        fs::write(
            root.join("db/migrations/0001_schema.sql"),
            b"create table entries (id integer primary key);",
        )
        .expect("migration");
        fs::write(root.join(".gitignore"), b"/db/migrations/\n").expect("gitignore");
        let git = std::process::Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(repository.path())
            .output()
            .expect("git should be available for the ignored-migration test");
        assert!(git.status.success());

        let output = root.join(format!(
            "ignored-migration-app{}",
            std::env::consts::EXE_SUFFIX
        ));
        let error = build_mvc_bundle(&root, &output)
            .expect_err("Git-ignored migrations cannot satisfy packaged SQLite");

        assert!(
            error.to_string().contains("was excluded from the bundle"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn mvc_user_data_identity_is_stable_and_distinguishes_package_names() {
        let first_path = Path::new("/installed/acme-notes");
        let second_path = Path::new("/copied/acme-notes");
        let first = mvc_app_storage_key("Acme Notes", first_path).expect("first package identity");
        let same = mvc_app_storage_key("Acme Notes", first_path).expect("stable package identity");
        let renamed =
            mvc_app_storage_key("Acme/Notes", first_path).expect("renamed package identity");
        let copied =
            mvc_app_storage_key("Acme Notes", second_path).expect("copied package identity");

        assert_eq!(first, same);
        assert_ne!(first, renamed);
        assert_ne!(first, copied);
        assert!(first.starts_with("acme-notes-"));
        assert_eq!(first.len() - "acme-notes-".len(), 64);
    }

    #[test]
    fn linux_gui_packages_require_an_explicit_project_license() {
        let error = linux_package_project_license(true, None)
            .expect_err("GUI package metadata must not invent a project license");
        assert_eq!(
            error.to_string(),
            "--package-license SPDX is required with --gui when creating Linux package artifacts"
        );

        assert_eq!(
            linux_package_project_license(false, None)
                .expect("non-GUI Linux packages do not write AppStream project metadata"),
            None
        );
        assert_eq!(
            linux_package_project_license(true, Some("Apache-2.0"))
                .expect("valid explicit SPDX identifier"),
            Some("Apache-2.0")
        );
        assert_eq!(
            linux_package_project_license(true, Some("Apache-2.0 OR BSD-3-Clause"))
                .expect("valid compound SPDX expression"),
            Some("Apache-2.0 OR BSD-3-Clause")
        );
    }

    #[test]
    fn linux_package_project_license_rejects_invalid_spdx_syntax() {
        for invalid in [
            "OR",
            "Apache-2.0 OR",
            "()",
            "Apache-2.0 MIT",
            "MIT\n</project_license>",
        ] {
            let error = linux_package_project_license(true, Some(invalid))
                .expect_err("invalid SPDX syntax must be rejected");
            assert!(
                error
                    .to_string()
                    .starts_with("--package-license must be a valid SPDX expression:"),
                "unexpected error for {invalid:?}: {error}"
            );
        }
    }

    #[test]
    fn linux_app_metainfo_uses_the_callers_project_license() {
        let metainfo =
            linux_app_metainfo("example-app", "1.2.3", "Example application", "Apache-2.0");

        assert!(metainfo.contains("<project_license>Apache-2.0</project_license>"));
        let stale_license = format!("<project_license>{}</project_license>", "MIT");
        assert!(!metainfo.contains(&stale_license));
    }

    #[test]
    fn debian_package_versions_sort_semver_prereleases_before_stable() {
        assert_eq!(
            debian_package_version("0.1.19").expect("stable SemVer"),
            "0.1.19"
        );
        assert_eq!(
            debian_package_version("0.1.19-rc.5").expect("prerelease SemVer"),
            "0.1.19~rc.5"
        );
        assert_eq!(
            debian_package_version("0.1.19-rc.5+build.7").expect("SemVer with build metadata"),
            "0.1.19~rc.5+build.7"
        );
    }

    #[test]
    fn linux_package_versions_must_be_semver() {
        validate_linux_package_version("1.2.3-rc.4+build.7")
            .expect("valid semantic versions are accepted");
        for invalid in ["dev", "1.2", "01.2.3"] {
            assert!(
                validate_linux_package_version(invalid).is_err(),
                "invalid semantic version {invalid:?} must be rejected"
            );
        }
    }
}
