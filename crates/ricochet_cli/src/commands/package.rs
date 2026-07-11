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
    let manifest_path = project_root.join("ricochet.toml");
    if !manifest_path.is_file() {
        bail!(
            "rco package --mvc expects {} to contain ricochet.toml",
            project_root.display()
        );
    }

    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", project_root.display()))?;
    validate_mvc_bundle_manifest(&project_root, &manifest_path)?;
    let output_path = absolute_package_output_path(output)?;
    let mut files = Vec::new();
    collect_mvc_bundle_files(&project_root, &project_root, &output_path, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(MvcBundle { files })
}

fn validate_mvc_bundle_manifest(project_root: &Path, manifest_path: &Path) -> Result<()> {
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
        return Ok(());
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

    Ok(())
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

fn collect_mvc_bundle_files(
    project_root: &Path,
    current: &Path,
    output_path: &Path,
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
            collect_mvc_bundle_files(project_root, &path, output_path, files)?;
        } else if file_type.is_file() {
            if same_package_output_file(&path, output_path) {
                continue;
            }
            validate_bundle_relative_path(relative_path)?;
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            files.push(MvcBundleFile {
                relative_path: relative_path.to_path_buf(),
                bytes,
            });
        }
    }
    Ok(())
}

fn should_skip_mvc_bundle_directory(relative_path: &Path) -> bool {
    relative_path.components().next().is_some_and(|component| {
        matches!(
            component,
            Component::Normal(name) if name == ".git" || name == "target"
        )
    })
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
            "{description}\n\nCommands:\n  ./{name} --help\n  ./{name}\n\nInstall locally:\n  ./install.sh\n{}",
            if gui {
                "\nLinux GUI apps open embedded WebView windows and require GTK 3 plus WebKitGTK 4.1 runtime libraries.\n"
            } else {
                ""
            }
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
            "Package: {name}\nVersion: {debian_version}\nSection: devel\nPriority: optional\nArchitecture: amd64\n{}Maintainer: Ricochet Packager <noreply@ricochet.today>\nDescription: {description}\n",
            if gui {
                "Depends: libgtk-3-0, libwebkit2gtk-4.1-0\n"
            } else {
                ""
            }
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
