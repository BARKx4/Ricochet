use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::io::{self, BufRead, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use axum::{body::Body, http::Request};
use clap::{Args, Parser, Subcommand, ValueEnum};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use ricochet_bytecode::{Chunk, Op, SourceSpan};
use ricochet_compiler::{compile_file_with_imports, compile_source, CompileError};
use ricochet_syntax::{
    format_source, parse_module, utf16_range_for_span, ArgsDecl, Expr, Item as SyntaxItem,
    LexError, Module, ParseError, SourceDiagnostic, Span, SpannedExpr, TokenKind,
};
use ricochet_vm::{
    DebugAction, DebugEvent, DebugPause, DebugPauseReason, DebugTask, MapValue, RicochetResult,
    Value, Vm,
};
use ricochet_web::{MysqlDatabase, PostgresDatabase};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, EntryType};
use toml_edit::{value, DocumentMut, Item, Table};
use tower::ServiceExt;

mod lsp;

const DEFAULT_BUILD_SOURCE: &str = "main.rco";
const BUILD_OUTPUT: &str = "build/app.rcob";
const EMBEDDED_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_APP_V1\0";
const EMBEDDED_TUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_TUI_APP_V1\0";
const EMBEDDED_GUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_GUI_APP_V1\0";
const EMBEDDED_MVC_GUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_MVC_GUI_APP_V1\0";
const MVC_BUNDLE_MAGIC: &[u8] = b"RICOCHET_MVC_BUNDLE_V1\0";
const GUI_EXPORT_HTML_ENV: &str = "RICOCHET_GUI_EXPORT_HTML";
const GUI_EXPORT_PATH_ENV: &str = "RICOCHET_GUI_EXPORT_PATH";
const GUI_EVENT_ENV: &str = "RICOCHET_GUI_EVENT";
const DEFAULT_MVC_GUI_TITLE: &str = "Ricochet MVC App";
const DEFAULT_MVC_GUI_WIDTH: u32 = 1100;
const DEFAULT_MVC_GUI_HEIGHT: u32 = 760;
const STATIC_REGISTRY_FORMAT: &str = "ricochet-static-registry-v1";
const MAX_STATIC_REGISTRY_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_STATIC_REGISTRY_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(name = "rco")]
#[command(about = "Ricochet language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New {
        #[arg(
            long,
            help = "Seed a local SQLite database and configure Active Record"
        )]
        with_sqlite: bool,
        path: String,
    },
    Check {
        path: Option<String>,
    },
    Repl {
        #[arg(long)]
        debug: bool,
        #[command(flatten)]
        capabilities: CapabilityOptions,
    },
    Run {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
        #[arg(
            long = "trace-file",
            value_name = "PATH",
            help = "Write recorded debug events to a JSON trace file"
        )]
        trace_file: Option<PathBuf>,
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Debug {
        #[arg(long, help = "Emit debugger events as JSON Lines")]
        json: bool,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
        #[arg(
            long = "trace-file",
            value_name = "PATH",
            help = "Also write recorded debug events to a JSON trace file"
        )]
        trace_file: Option<PathBuf>,
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    DebugAdapter,
    Bench {
        #[arg(long, default_value_t = 5)]
        iterations: usize,
        #[arg(long, help = "Run a small CI-friendly benchmark smoke")]
        smoke: bool,
        #[arg(long, help = "Emit benchmark results as JSON")]
        json: bool,
    },
    RunBytecode {
        #[arg(long)]
        debug: bool,
        #[arg(
            long = "trace-file",
            value_name = "PATH",
            help = "Write recorded debug events to a JSON trace file"
        )]
        trace_file: Option<PathBuf>,
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Build {
        path: Option<String>,
    },
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    Gui {
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Tui {
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Package {
        path: Option<String>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(
            long,
            help = "Package as a terminal UI app using the console launcher without final stack output"
        )]
        tui: bool,
        #[arg(
            long,
            help = "Package as a native desktop GUI app using the rco-gui launcher"
        )]
        gui: bool,
        #[arg(
            long,
            help = "Package an MVC app directory as a local-server desktop GUI app; requires --gui"
        )]
        mvc: bool,
        #[arg(
            long = "gui-launcher",
            value_name = "PATH",
            help = "Use a specific rco-gui launcher executable for --gui packages"
        )]
        gui_launcher: Option<PathBuf>,
        #[arg(
            long = "linux-package",
            value_enum,
            value_name = "FORMAT",
            help = "Also create a Linux package artifact: tar or deb. Repeat for both."
        )]
        linux_packages: Vec<LinuxPackageFormat>,
        #[arg(long = "package-name", value_name = "NAME")]
        package_name: Option<String>,
        #[arg(long = "package-version", default_value = "0.1.0")]
        package_version: String,
        #[arg(
            long = "package-description",
            default_value = "Packaged Ricochet application"
        )]
        package_description: String,
    },
    Add {
        source: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(
            long = "as",
            value_name = "NAME",
            help = "Use NAME as the local import/dependency alias"
        )]
        alias: Option<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Use a local file-backed package registry for registry:name dependencies"
        )]
        registry: Option<PathBuf>,
        #[arg(
            long = "registry-url",
            value_name = "URL",
            help = "Use a static package registry index URL for registry:name dependencies"
        )]
        registry_url: Option<String>,
        #[arg(
            long = "version",
            value_name = "REQ",
            help = "Require the dependency package version to satisfy REQ, for example ^0.1.0"
        )]
        version: Option<String>,
        #[arg(long)]
        no_fetch: bool,
    },
    Publish {
        path: Option<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Publish to a local file-backed package registry"
        )]
        registry: PathBuf,
        #[arg(
            long = "provenance-file",
            value_name = "PATH",
            help = "Attach a provenance attestation file to the registry package metadata"
        )]
        provenance_file: Option<PathBuf>,
        #[arg(
            long = "signature-file",
            value_name = "PATH",
            help = "Attach a detached package signature file to the registry package metadata"
        )]
        signature_file: Option<PathBuf>,
        #[arg(
            long = "signature-kind",
            value_name = "KIND",
            help = "Describe the detached signature format, for example minisign or sigstore"
        )]
        signature_kind: Option<String>,
        #[arg(long, help = "Validate and describe the publish without writing files")]
        dry_run: bool,
    },
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
    Search {
        query: String,
        #[arg(long, value_name = "PATH", help = "Search a local static registry")]
        registry: Option<PathBuf>,
        #[arg(
            long = "registry-url",
            value_name = "URL",
            help = "Search a static package registry index URL"
        )]
        registry_url: Option<String>,
    },
    Install,
    Verify {
        path: Option<String>,
    },
    Audit {
        path: Option<String>,
        #[arg(long, help = "Emit the dependency audit report as JSON")]
        json: bool,
    },
    Doctor {
        path: Option<String>,
        #[arg(
            long,
            help = "Print effective manifest capability declarations for MVC apps"
        )]
        capabilities: bool,
    },
    Words {
        #[arg(long, help = "Emit the built-in editor word inventory as JSON")]
        json: bool,
        #[arg(
            long,
            help = "Check docs/reference/app.js and the TextMate grammar against the built-in inventory"
        )]
        check: bool,
        #[arg(
            long = "docs-app",
            value_name = "PATH",
            default_value = "docs/reference/app.js",
            help = "Reference docs app.js path used by --check"
        )]
        docs_app: PathBuf,
        #[arg(
            long = "grammar",
            value_name = "PATH",
            default_value = "editors/vscode/syntaxes/ricochet.tmLanguage.json",
            help = "TextMate grammar path used by --check"
        )]
        grammar: PathBuf,
    },
    LspDiagnostics {
        path: String,
        #[arg(long, help = "Pretty-print the JSON response")]
        pretty: bool,
    },
    Lsp {
        #[arg(long, help = "Trace JSON-RPC messages to stderr")]
        trace: bool,
    },
    Doc {
        path: Option<String>,
    },
    Fmt {
        #[arg(long)]
        check: bool,
        path: Option<String>,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 3000)]
        port: u16,
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        watch: bool,
        #[arg(
            long = "allow-env",
            help = "Enable MVC process environment access for trusted local apps"
        )]
        allow_env: bool,
        #[arg(
            long = "no-env",
            help = "Keep MVC process environment access disabled; conflicts with --allow-env"
        )]
        no_env: bool,
        #[arg(
            long = "env-allow",
            value_name = "NAME",
            help = "Allow MVC controllers to read or write only NAME in the process environment; repeat for multiple variables"
        )]
        env_allow: Vec<String>,
        #[arg(
            long = "allow-process",
            help = "Enable MVC process execution for trusted local apps"
        )]
        allow_process: bool,
        #[arg(
            long = "process-root",
            value_name = "PATH",
            help = "Restrict MVC process and PTY cwd values to PATH; defaults to --fs-root when omitted"
        )]
        process_root: Option<PathBuf>,
        #[arg(
            long = "allow-pty",
            help = "Enable MVC PTY sessions for trusted local apps"
        )]
        allow_pty: bool,
        #[arg(
            long,
            value_name = "PATH",
            help = "Enable MVC filesystem access bounded to PATH"
        )]
        fs_root: Option<PathBuf>,
        #[arg(
            long,
            help = "Allow MVC filesystem reads while denying writes and directory creation"
        )]
        fs_readonly: bool,
        #[arg(
            long = "http-allow-host",
            value_name = "HOST",
            help = "Enable MVC HTTP access only to HOST; repeat for multiple hosts"
        )]
        http_allow_hosts: Vec<String>,
    },
    Routes {
        path: Option<String>,
    },
    Test {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        filter: Option<String>,
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum MigrateCommand {
    Status { path: Option<String> },
    Apply { path: Option<String> },
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    Rebuild { path: PathBuf },
    Check { path: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum LinuxPackageFormat {
    Tar,
    Deb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbeddedAppKind {
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
struct EmbeddedApp {
    kind: EmbeddedAppKind,
    payload: EmbeddedAppPayload,
}

#[derive(Debug)]
enum EmbeddedAppPayload {
    Chunk(Chunk),
    MvcBundle(MvcBundle),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MvcBundle {
    files: Vec<MvcBundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MvcBundleFile {
    relative_path: PathBuf,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
struct WebviewDocument {
    title: String,
    html: String,
    width: u32,
    height: u32,
    state: Value,
    actions: Vec<WebviewAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebviewAction {
    action: String,
    callback: String,
}

#[derive(Clone, Debug, Default, Args)]
struct CapabilityOptions {
    #[arg(
        long = "capability-profile",
        value_enum,
        default_value = "trusted",
        help = "Select host capability defaults: trusted enables filesystem/HTTP/TUI/webview, sandboxed disables them unless bounded by flags; process and PTY execution are always opt-in"
    )]
    capability_profile: CapabilityProfile,
    #[arg(long, help = "Disable the filesystem host capability for this run")]
    no_fs: bool,
    #[arg(long, value_name = "PATH", help = "Restrict filesystem access to PATH")]
    fs_root: Option<PathBuf>,
    #[arg(
        long,
        help = "Allow filesystem reads but deny writes and directory creation"
    )]
    fs_readonly: bool,
    #[arg(long, help = "Disable the HTTP host capability for this run")]
    no_http: bool,
    #[arg(
        long,
        help = "Enable the process execution host capability for this run"
    )]
    allow_process: bool,
    #[arg(
        long = "process-root",
        value_name = "PATH",
        help = "Restrict process and PTY cwd values to PATH; defaults to --fs-root when omitted"
    )]
    process_root: Option<PathBuf>,
    #[arg(long, help = "Enable the PTY host capability for this run")]
    allow_pty: bool,
    #[arg(long, help = "Disable the terminal UI host capability for this run")]
    no_tui: bool,
    #[arg(
        long,
        help = "Enable the terminal UI host capability under the sandboxed profile"
    )]
    allow_tui: bool,
    #[arg(long, help = "Disable the webview UI host capability for this run")]
    no_webview: bool,
    #[arg(
        long,
        help = "Enable the webview UI host capability under the sandboxed profile"
    )]
    allow_webview: bool,
    #[arg(long, help = "Disable process environment access for this run")]
    no_env: bool,
    #[arg(
        long = "env-allow",
        value_name = "NAME",
        help = "Allow reading or writing only NAME in the process environment; repeat for multiple variables"
    )]
    env_allow: Vec<String>,
    #[arg(long, help = "Disable blocking sleep for this run")]
    no_sleep: bool,
    #[arg(
        long = "http-allow-host",
        value_name = "HOST",
        help = "Allow HTTP requests only to HOST; repeat for multiple hosts"
    )]
    http_allow_hosts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum CapabilityProfile {
    #[default]
    Trusted,
    Sandboxed,
}

impl CapabilityOptions {
    fn apply_to(&self, vm: &mut Vm) -> Result<()> {
        if self.no_fs {
            if self.fs_root.is_some() {
                bail!("--fs-root cannot be used with --no-fs");
            }
            if self.fs_readonly {
                bail!("--fs-readonly cannot be used with --no-fs");
            }
        }
        if self.no_http && !self.http_allow_hosts.is_empty() {
            bail!("--http-allow-host cannot be used with --no-http");
        }
        if self.no_webview && self.allow_webview {
            bail!("--allow-webview cannot be used with --no-webview");
        }
        if self.no_tui && self.allow_tui {
            bail!("--allow-tui cannot be used with --no-tui");
        }
        if self.no_env && !self.env_allow.is_empty() {
            bail!("--env-allow cannot be used with --no-env");
        }
        if self.capability_profile == CapabilityProfile::Sandboxed
            && self.fs_readonly
            && self.fs_root.is_none()
        {
            bail!("--capability-profile sandboxed requires --fs-root when --fs-readonly is used");
        }

        let filesystem_enabled = !self.no_fs
            && (self.capability_profile == CapabilityProfile::Trusted || self.fs_root.is_some());
        let http_enabled = !self.no_http
            && (self.capability_profile == CapabilityProfile::Trusted
                || !self.http_allow_hosts.is_empty());
        let process_enabled = self.allow_process;
        let pty_enabled = self.allow_pty;
        let terminal_enabled = !self.no_tui
            && (self.capability_profile == CapabilityProfile::Trusted || self.allow_tui);
        let webview_enabled = !self.no_webview
            && (self.capability_profile == CapabilityProfile::Trusted || self.allow_webview);
        let environment_enabled = !self.no_env
            && (self.capability_profile == CapabilityProfile::Trusted
                || !self.env_allow.is_empty());
        let sleep_enabled = !self.no_sleep && self.capability_profile == CapabilityProfile::Trusted;

        vm.set_host_capabilities(filesystem_enabled, http_enabled);
        vm.set_process_enabled(process_enabled);
        vm.set_pty_enabled(pty_enabled);
        vm.set_terminal_enabled(terminal_enabled);
        vm.set_webview_enabled(webview_enabled);
        vm.set_environment_enabled(environment_enabled);
        if self.env_allow.is_empty() {
            vm.clear_environment_allowed_names();
        } else {
            vm.set_environment_allowed_names(self.env_allow.clone());
        }
        vm.set_sleep_enabled(sleep_enabled);
        if let Some(root) = &self.fs_root {
            let root = fs::canonicalize(root)
                .with_context(|| format!("failed to resolve --fs-root {}", root.display()))?;
            if !root.is_dir() {
                bail!("--fs-root must be a directory: {}", root.display());
            }
            vm.set_filesystem_root(root);
        }
        if let Some(root) = &self.process_root {
            let root = fs::canonicalize(root)
                .with_context(|| format!("failed to resolve --process-root {}", root.display()))?;
            if !root.is_dir() {
                bail!("--process-root must be a directory: {}", root.display());
            }
            vm.set_process_root(root);
        }
        if self.fs_readonly {
            vm.set_filesystem_writes_enabled(false);
        }
        if !self.http_allow_hosts.is_empty() {
            vm.set_http_allowed_hosts(self.http_allow_hosts.clone());
        }
        Ok(())
    }
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn run_cli() -> Result<()> {
    if let Some(app) = embedded_app_from_current_exe()? {
        match app.payload {
            EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Console => {
                run_chunk_cli(
                    &chunk,
                    RunChunkCliOptions {
                        debug: false,
                        step: false,
                        breakpoints: &[],
                        breakpoint_file: None,
                        trace_file: None,
                        args: std::env::args().skip(1).collect(),
                        capabilities: CapabilityOptions::default(),
                        debug_output: DebugOutput::Text,
                        print_final_stack: true,
                    },
                )?
            }
            EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Tui => {
                run_embedded_tui_app(&chunk, std::env::args().skip(1).collect())?
            }
            EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Gui => {
                run_embedded_gui_app(&chunk, std::env::args().skip(1).collect())?
            }
            EmbeddedAppPayload::MvcBundle(bundle) if app.kind == EmbeddedAppKind::MvcGui => {
                run_embedded_mvc_gui_app(bundle, std::env::args().skip(1).collect()).await?
            }
            _ => bail!("embedded Ricochet app payload does not match its marker"),
        }
        return Ok(());
    }

    let cli = Cli::parse();
    match cli.command {
        Command::New { path, with_sqlite } => {
            new_project(Path::new(&path), NewProjectOptions { with_sqlite })?
        }
        Command::Check { path } => check(path.as_deref().unwrap_or("."))?,
        Command::Repl {
            debug,
            capabilities,
        } => {
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            let stdout = io::stdout();
            run_repl(
                stdin.lock(),
                stdout.lock(),
                debug,
                interactive,
                capabilities,
            )?;
        }
        Command::Run {
            debug,
            step,
            breakpoints,
            trace_file,
            capabilities,
            path,
            args,
        } => run_file(
            &path,
            debug,
            step,
            &breakpoints,
            trace_file.as_deref(),
            args,
            capabilities,
        )?,
        Command::Debug {
            json,
            step,
            breakpoints,
            trace_file,
            capabilities,
            path,
            args,
        } => debug_file(
            &path,
            json,
            step,
            &breakpoints,
            trace_file.as_deref(),
            args,
            capabilities,
        )?,
        Command::DebugAdapter => run_debug_adapter()?,
        Command::Bench {
            iterations,
            smoke,
            json,
        } => {
            run_benchmarks(BenchmarkOptions {
                iterations,
                smoke,
                json,
            })
            .await?
        }
        Command::RunBytecode {
            debug,
            trace_file,
            capabilities,
            path,
            args,
        } => run_bytecode(&path, debug, trace_file.as_deref(), args, capabilities)?,
        Command::Build { path } => build(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE))?,
        Command::Migrate { command } => migrate(command).await?,
        Command::Gui {
            capabilities,
            path,
            args,
        } => run_gui_file(&path, args, capabilities)?,
        Command::Tui {
            capabilities,
            path,
            args,
        } => run_tui_file(&path, args, capabilities)?,
        Command::Package {
            path,
            output,
            tui,
            gui,
            mvc,
            gui_launcher,
            linux_packages,
            package_name,
            package_version,
            package_description,
        } => package(
            path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE),
            &output,
            PackageOptions {
                tui,
                gui,
                mvc,
                gui_launcher: gui_launcher.as_deref(),
                linux_packages: &linux_packages,
                package_name: package_name.as_deref(),
                package_version: &package_version,
                package_description: &package_description,
            },
        )?,
        Command::Add {
            source,
            name,
            alias,
            registry,
            registry_url,
            version,
            no_fetch,
        } => {
            if name.is_some() && alias.is_some() {
                bail!("use either --name or --as, not both");
            }
            add_dependency(
                &source,
                alias.as_deref().or(name.as_deref()),
                registry.as_deref(),
                registry_url.as_deref(),
                version.as_deref(),
                no_fetch,
            )?
        }
        Command::Publish {
            path,
            registry,
            provenance_file,
            signature_file,
            signature_kind,
            dry_run,
        } => publish_package(
            path.as_deref(),
            &registry,
            PublishRegistryOptions {
                dry_run,
                provenance_file: provenance_file.as_deref(),
                signature_file: signature_file.as_deref(),
                signature_kind: signature_kind.as_deref(),
            },
        )?,
        Command::Registry { command } => match command {
            RegistryCommand::Rebuild { path } => rebuild_static_registry(&path)?,
            RegistryCommand::Check { path } => check_static_registry(&path)?,
        },
        Command::Search {
            query,
            registry,
            registry_url,
        } => search_registry(&query, registry.as_deref(), registry_url.as_deref())?,
        Command::Install => install_dependencies()?,
        Command::Verify { path } => verify_dependencies(path.as_deref())?,
        Command::Audit { path, json } => audit_dependencies(path.as_deref(), json)?,
        Command::Doctor { path, capabilities } => {
            doctor(path.as_deref().unwrap_or("."), capabilities)?
        }
        Command::Words {
            json,
            check,
            docs_app,
            grammar,
        } => words(json, check, &docs_app, &grammar)?,
        Command::LspDiagnostics { path, pretty } => lsp_diagnostics(&path, pretty)?,
        Command::Lsp { trace } => lsp::run_lsp_server(trace)?,
        Command::Doc { path } => doc_path(path.as_deref().unwrap_or("."))?,
        Command::Fmt { check, path } => format_path(path.as_deref().unwrap_or("."), check)?,
        Command::Serve {
            host,
            port,
            debug,
            watch,
            allow_env,
            no_env,
            env_allow,
            allow_process,
            process_root,
            allow_pty,
            fs_root,
            fs_readonly,
            http_allow_hosts,
        } => {
            if allow_env && no_env {
                bail!("--allow-env cannot be used with --no-env");
            }
            if allow_env && !env_allow.is_empty() {
                bail!("--allow-env cannot be used with --env-allow");
            }
            if no_env && !env_allow.is_empty() {
                bail!("--env-allow cannot be used with --no-env");
            }
            ricochet_web::serve_current_dir(ricochet_web::ServeOptions {
                host,
                port,
                debug,
                watch,
                allow_env,
                env_allow,
                allow_process,
                process_root,
                allow_pty,
                fs_root,
                fs_readonly,
                http_allow_hosts,
            })
            .await?
        }
        Command::Routes { path } => routes(path.as_deref().unwrap_or("."))?,
        Command::Test {
            debug,
            filter,
            capabilities,
            path,
        } => {
            run_tests(
                path.as_deref().unwrap_or("tests"),
                debug,
                filter.as_deref(),
                capabilities,
            )?;
        }
    }
    Ok(())
}

pub async fn run_gui_launcher() -> Result<()> {
    let app = embedded_app_from_current_exe()?
        .context("rco-gui must be packaged with `rco package --gui` before it can launch an app")?;
    match app.payload {
        EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Gui => {
            run_embedded_gui_app(&chunk, std::env::args().skip(1).collect())
        }
        EmbeddedAppPayload::MvcBundle(bundle) if app.kind == EmbeddedAppKind::MvcGui => {
            run_embedded_mvc_gui_app(bundle, std::env::args().skip(1).collect()).await
        }
        _ => {
            bail!("rco-gui can only launch apps packaged with `rco package --gui`");
        }
    }
}

fn run_repl<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    debug: bool,
    interactive: bool,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let mut vm = Vm::default();
    capabilities.apply_to(&mut vm)?;
    if debug {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
    }

    let mut source = String::new();
    let mut line = String::new();
    let mut output_cursor = 0usize;

    loop {
        if interactive {
            let prompt = if source.is_empty() { "rco> " } else { "...> " };
            write!(output, "{prompt}")?;
            output.flush()?;
        }

        line.clear();
        let bytes_read = input.read_line(&mut line)?;
        if bytes_read == 0 {
            if source.trim().is_empty() {
                return Ok(());
            }

            return match compile_source("<repl>", &source) {
                Err(error) if is_incomplete_compile_error(&error) => {
                    bail!("incomplete Ricochet input: {error}")
                }
                Err(error) => Err(error.into()),
                Ok(chunk) => {
                    vm.run_chunk(&chunk)?;
                    write_repl_result(&vm, &mut output, &mut output_cursor)?;
                    Ok(())
                }
            };
        }

        source.push_str(&line);
        if source.trim().is_empty() {
            source.clear();
            continue;
        }

        match compile_source("<repl>", &source) {
            Ok(chunk) => {
                match vm.run_chunk(&chunk) {
                    Ok(()) => write_repl_result(&vm, &mut output, &mut output_cursor)?,
                    Err(error) => writeln!(output, "ERROR {error}")?,
                }
                source.clear();
            }
            Err(error) if is_incomplete_compile_error(&error) => {}
            Err(error) => {
                writeln!(output, "ERROR {error}")?;
                source.clear();
            }
        }
    }
}

fn write_repl_result<W: Write>(vm: &Vm, output: &mut W, output_cursor: &mut usize) -> Result<()> {
    write!(output, "{}", &vm.stdout()[*output_cursor..])?;
    *output_cursor = vm.stdout().len();
    writeln!(output, "{:?}", vm.stack())?;
    output.flush()?;
    Ok(())
}

fn is_incomplete_compile_error(error: &CompileError) -> bool {
    matches!(
        error,
        CompileError::Parse(ParseError::Expected {
            found: TokenKind::Eof,
            ..
        }) | CompileError::Parse(ParseError::Unexpected {
            found: TokenKind::Eof,
            ..
        }) | CompileError::Parse(ParseError::Lex(
            LexError::UnterminatedString(_) | LexError::UnterminatedComment(_),
        ))
    )
}

fn check(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_file() {
        check_source_file(path)?;
        println!("checked {}", path.display());
        return Ok(());
    }

    if !path.is_dir() {
        bail!("check path does not exist: {}", path.display());
    }

    if path.join("ricochet.toml").is_file() {
        let _app = ricochet_web::server::build_app_from_dir(path)
            .with_context(|| format!("failed to check MVC app {}", path.display()))?;
        println!("checked {}", path.display());
        return Ok(());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();
    for file in &files {
        check_source_file(file)?;
    }

    println!(
        "checked {} Ricochet files in {}",
        files.len(),
        path.display()
    );
    Ok(())
}

fn check_source_file(path: &Path) -> Result<()> {
    compile_source_file(path).with_context(|| format!("failed to compile {}", path.display()))?;
    Ok(())
}

fn doctor(path: &str, show_capabilities: bool) -> Result<()> {
    let path = Path::new(path);
    let mut report = DoctorReport::default();

    println!("Ricochet doctor");
    doctor_step(&mut report, "path", || {
        if path.exists() {
            Ok(path
                .canonicalize()
                .unwrap_or_else(|_| path.to_path_buf())
                .display()
                .to_string())
        } else {
            bail!("path does not exist: {}", path.display())
        }
    });

    if path.is_file() {
        doctor_step(&mut report, "source compile", || {
            check_source_file(path)?;
            Ok("single source file compiles".to_string())
        });
        report.finish()?;
        return Ok(());
    }

    if !path.is_dir() {
        doctor_step(&mut report, "path kind", || -> Result<String> {
            bail!("path is neither a file nor a directory: {}", path.display())
        });
        report.finish()?;
        return Ok(());
    }

    let manifest_path = path.join("ricochet.toml");
    if manifest_path.is_file() {
        doctor_mvc_project(path, &manifest_path, show_capabilities, &mut report)?;
    } else {
        doctor_source_tree(path, &mut report)?;
    }

    report.finish()
}

fn doctor_mvc_project(
    project_root: &Path,
    manifest_path: &Path,
    show_capabilities: bool,
    report: &mut DoctorReport,
) -> Result<()> {
    let manifest_source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = match manifest_source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))
    {
        Ok(manifest) => {
            let name = manifest
                .get("package")
                .and_then(Item::as_table)
                .and_then(|package| package.get("name"))
                .and_then(Item::as_str)
                .unwrap_or("<unnamed>");
            println!("OK manifest: package {name}");
            Some(manifest)
        }
        Err(error) => {
            report.failures += 1;
            eprintln!("FAIL manifest: {error:#}");
            None
        }
    };

    if let Some(manifest) = manifest.as_ref() {
        doctor_step(report, "dependencies", || {
            let verified =
                verify_dependency_manifest(project_root, manifest_path, manifest, false)?;
            Ok(format!("{verified} package dependency lock(s) verified"))
        });
    }

    let has_web = manifest
        .as_ref()
        .is_some_and(|manifest| manifest.get("web").and_then(Item::as_table).is_some());
    doctor_step(report, "project kind", || {
        Ok(if has_web {
            "MVC app".to_string()
        } else {
            "package/source project".to_string()
        })
    });
    if has_web {
        doctor_step(report, "routes", || {
            let routes = ricochet_web::routes_from_dir(project_root)?;
            Ok(format!("{} route(s)", routes.len()))
        });
        doctor_step(report, "MVC app build", || {
            let _app = ricochet_web::server::build_app_from_dir(project_root)?;
            Ok("controllers, models, routes, and views compile".to_string())
        });
    }
    doctor_step(report, "source files", || {
        let mut files = Vec::new();
        collect_rco_files(project_root, &mut files)?;
        files.sort();
        if has_web {
            Ok(format!("{} .rco file(s) discovered", files.len()))
        } else {
            for file in &files {
                check_source_file(file)?;
            }
            Ok(format!("{} .rco file(s) compile", files.len()))
        }
    });

    if show_capabilities {
        if let Some(manifest) = manifest {
            print_doctor_capabilities(&manifest);
        }
    }

    Ok(())
}

fn doctor_source_tree(path: &Path, report: &mut DoctorReport) -> Result<()> {
    doctor_step(report, "source files", || {
        let mut files = Vec::new();
        collect_rco_files(path, &mut files)?;
        files.sort();
        for file in &files {
            check_source_file(file)?;
        }
        Ok(format!("{} .rco file(s) compile", files.len()))
    });
    Ok(())
}

fn print_doctor_capabilities(manifest: &DocumentMut) {
    println!("Capabilities:");
    let Some(capabilities) = manifest
        .get("web")
        .and_then(Item::as_table)
        .and_then(|web| web.get("capabilities"))
        .and_then(Item::as_table)
    else {
        println!("  web.capabilities: <none>");
        return;
    };

    for key in [
        "fs_root",
        "fs_readonly",
        "allow_env",
        "env_allow",
        "allow_process",
        "process_root",
        "allow_pty",
        "http_allow_hosts",
    ] {
        if let Some(value) = capabilities.get(key) {
            println!("  {key}: {}", value.to_string().trim());
        }
    }
}

#[derive(Default)]
struct DoctorReport {
    failures: usize,
}

impl DoctorReport {
    fn finish(self) -> Result<()> {
        if self.failures > 0 {
            bail!("doctor found {} issue(s)", self.failures);
        }
        println!("Doctor found no issues.");
        Ok(())
    }
}

fn doctor_step<T>(
    report: &mut DoctorReport,
    name: &str,
    check: impl FnOnce() -> Result<T>,
) -> Option<T>
where
    T: std::fmt::Display,
{
    match check() {
        Ok(detail) => {
            println!("OK {name}: {detail}");
            Some(detail)
        }
        Err(error) => {
            report.failures += 1;
            eprintln!("FAIL {name}: {error:#}");
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReferenceWord {
    word: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    group: String,
    #[serde(default)]
    stack: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    example: String,
}

#[derive(Debug, Serialize)]
struct WordInventoryEntry {
    word: String,
    detail: String,
    documentation: String,
}

fn words(json_output: bool, check: bool, docs_app: &Path, grammar: &Path) -> Result<()> {
    let entries = lsp::word_docs()
        .iter()
        .map(|entry| WordInventoryEntry {
            word: entry.label.to_string(),
            detail: entry.detail.to_string(),
            documentation: entry.documentation.to_string(),
        })
        .collect::<Vec<_>>();

    if json_output {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if !check {
        print_word_inventory(&entries);
    }

    if check {
        let summary = check_word_inventory(docs_app, grammar)?;
        let message = format!(
            "word inventory check passed: {} documented words, {} TextMate token literals, {} built-in LSP entries ({} documented token words missing from the embedded LSP inventory, {} duplicate reference entries)",
            summary.documented_words,
            summary.grammar_token_words,
            summary.lsp_words,
            summary.documented_only_words,
            summary.duplicate_reference_entries
        );
        if json_output {
            eprintln!("{message}");
        } else {
            println!("{message}");
        }
    }

    Ok(())
}

fn print_word_inventory(entries: &[WordInventoryEntry]) {
    let mut groups: BTreeMap<&str, Vec<&WordInventoryEntry>> = BTreeMap::new();
    for entry in entries {
        groups.entry(entry.detail.as_str()).or_default().push(entry);
    }

    println!(
        "Ricochet built-in word inventory ({} entries)",
        entries.len()
    );
    for (detail, group_entries) in groups {
        println!();
        println!("{detail}");
        for entry in group_entries {
            println!("  {:<24} {}", entry.word, entry.documentation);
        }
    }
}

#[derive(Debug)]
struct WordInventoryCheckSummary {
    documented_words: usize,
    grammar_token_words: usize,
    lsp_words: usize,
    documented_only_words: usize,
    duplicate_reference_entries: usize,
}

const REFERENCE_WORD_GROUPS: &[&str] = &[
    "stack",
    "math",
    "data",
    "collection",
    "string",
    "oop",
    "control",
    "web",
    "result",
    "system",
    "inspect",
];

fn check_word_inventory(docs_app: &Path, grammar: &Path) -> Result<WordInventoryCheckSummary> {
    let docs_source = fs::read_to_string(docs_app)
        .with_context(|| format!("failed to read docs reference app {}", docs_app.display()))?;
    let docs_json = extract_reference_words_json(&docs_source)?;
    let reference_words: Vec<ReferenceWord> = serde_json::from_str(docs_json)
        .with_context(|| format!("failed to parse WORDS catalog in {}", docs_app.display()))?;

    let grammar_source = fs::read_to_string(grammar)
        .with_context(|| format!("failed to read TextMate grammar {}", grammar.display()))?;
    let grammar_json: serde_json::Value = serde_json::from_str(&grammar_source)
        .with_context(|| format!("failed to parse TextMate grammar {}", grammar.display()))?;
    let mut grammar_patterns = Vec::new();
    collect_textmate_patterns(&grammar_json, &mut grammar_patterns);
    let grammar_regexes = grammar_patterns.join("\n");
    let grammar_builtin_words = textmate_builtin_words(&grammar_json)?;

    let mut documented_primary = BTreeSet::new();
    let mut documented_all_names = BTreeSet::new();
    let mut duplicate_words = Vec::new();
    let mut invalid_reference_entries = Vec::new();
    let mut token_words = BTreeSet::new();

    for entry in &reference_words {
        validate_reference_word_entry(entry, &mut invalid_reference_entries);
        if !documented_primary.insert(entry.word.clone()) {
            duplicate_words.push(entry.word.clone());
        }
        documented_all_names.insert(entry.word.clone());
        if is_ricochet_token_literal(&entry.word) {
            token_words.insert(entry.word.clone());
        }
        for alias in &entry.aliases {
            documented_all_names.insert(alias.clone());
        }
    }

    let mut missing_from_grammar = Vec::new();
    for word in &token_words {
        let escaped = regex_escape_literal(word);
        if !grammar_regexes.contains(&escaped) {
            missing_from_grammar.push(word.clone());
        }
    }

    let lsp_words = lsp::word_docs()
        .iter()
        .map(|entry| entry.label.to_string())
        .collect::<BTreeSet<_>>();
    let stale_lsp_words = lsp_words
        .iter()
        .filter(|word| !documented_all_names.contains(*word))
        .cloned()
        .collect::<Vec<_>>();
    let documented_only_words = token_words
        .iter()
        .filter(|word| !lsp_words.contains(*word))
        .count();
    let stale_grammar_builtin_words = grammar_builtin_words
        .iter()
        .filter(|word| !documented_all_names.contains(*word))
        .cloned()
        .collect::<Vec<_>>();

    let mut failures = Vec::new();
    if !duplicate_words.is_empty() {
        failures.push(format!(
            "docs reference contains duplicate words: {}",
            duplicate_words.join(", ")
        ));
    }
    if !invalid_reference_entries.is_empty() {
        failures.push(format!(
            "docs reference contains malformed entries:\n{}",
            invalid_reference_entries.join("\n")
        ));
    }
    if !missing_from_grammar.is_empty() {
        failures.push(format!(
            "TextMate grammar is missing documented words: {}",
            missing_from_grammar.join(", ")
        ));
    }
    if !stale_lsp_words.is_empty() {
        failures.push(format!(
            "LSP inventory contains words absent from docs/reference/app.js: {}",
            stale_lsp_words.join(", ")
        ));
    }
    if !stale_grammar_builtin_words.is_empty() {
        failures.push(format!(
            "TextMate builtin regex contains undocumented words: {}",
            stale_grammar_builtin_words.join(", ")
        ));
    }
    if failures.is_empty() {
        Ok(WordInventoryCheckSummary {
            documented_words: documented_primary.len(),
            grammar_token_words: token_words.len(),
            lsp_words: lsp_words.len(),
            documented_only_words,
            duplicate_reference_entries: duplicate_words.len(),
        })
    } else {
        bail!("word inventory check failed:\n{}", failures.join("\n"));
    }
}

fn validate_reference_word_entry(entry: &ReferenceWord, failures: &mut Vec<String>) {
    let label = if entry.word.trim().is_empty() {
        "<blank>"
    } else {
        entry.word.trim()
    };

    if entry.word.trim().is_empty() {
        failures.push("entry has blank word".to_string());
    }
    if entry.group.trim().is_empty() {
        failures.push(format!("{label}: missing group"));
    } else if !REFERENCE_WORD_GROUPS.contains(&entry.group.as_str()) {
        failures.push(format!("{label}: unknown group '{}'", entry.group));
    }
    if entry.stack.trim().is_empty() {
        failures.push(format!("{label}: missing stack"));
    }
    if entry.body.trim().is_empty() {
        failures.push(format!("{label}: missing body"));
    }
    if entry.example.trim().is_empty() {
        failures.push(format!("{label}: missing example"));
    }
}

fn textmate_builtin_words(grammar: &serde_json::Value) -> Result<BTreeSet<String>> {
    let pattern = grammar
        .get("repository")
        .and_then(|repository| repository.get("builtins"))
        .and_then(|builtins| builtins.get("patterns"))
        .and_then(serde_json::Value::as_array)
        .and_then(|patterns| patterns.first())
        .and_then(|entry| entry.get("match"))
        .and_then(serde_json::Value::as_str)
        .context("TextMate grammar is missing repository.builtins.patterns[0].match")?;
    let body = pattern
        .strip_prefix(r"(?<!\S)(?:")
        .and_then(|body| body.strip_suffix(r")(?!\S)"))
        .context("TextMate builtin regex does not use the expected Ricochet word alternation")?;

    let mut words = BTreeSet::new();
    for part in body.split('|') {
        if part.is_empty() {
            bail!("TextMate builtin regex contains an empty alternation branch");
        }
        words.insert(unescape_textmate_builtin_literal(part)?);
    }
    Ok(words)
}

fn unescape_textmate_builtin_literal(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let Some(escaped) = chars.next() else {
                bail!("TextMate builtin regex contains a trailing escape");
            };
            output.push(escaped);
        } else {
            output.push(ch);
        }
    }
    Ok(output)
}

fn extract_reference_words_json(source: &str) -> Result<&str> {
    let marker_start = source
        .find("const WORDS")
        .context("could not find const WORDS in docs reference app")?;
    let after_marker = &source[marker_start..];
    let array_offset = after_marker
        .find('[')
        .context("could not find WORDS array start in docs reference app")?;
    let array_start = marker_start + array_offset;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in source[array_start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = array_start + offset + ch.len_utf8();
                    return Ok(&source[array_start..end]);
                }
            }
            _ => {}
        }
    }

    bail!("could not find WORDS array end in docs reference app")
}

fn collect_textmate_patterns(node: &serde_json::Value, patterns: &mut Vec<String>) {
    match node {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_textmate_patterns(item, patterns);
            }
        }
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "match" | "begin" | "end" | "firstLineMatch") {
                    if let Some(pattern) = value.as_str() {
                        patterns.push(pattern.to_string());
                    }
                } else {
                    collect_textmate_patterns(value, patterns);
                }
            }
        }
        _ => {}
    }
}

fn is_ricochet_token_literal(word: &str) -> bool {
    if word.is_empty()
        || word
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '/' || !ch.is_ascii())
    {
        return false;
    }

    if word
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        let allowed = [
            "Object",
            "Model",
            "Controller",
            "Result",
            "Array",
            "List",
            "Map",
            "Set",
            "Subclass",
            "Field",
            "Accessor",
            "Table",
            "Method",
            "GET",
            "POST",
            "PUT",
            "PATCH",
            "DELETE",
        ];
        return allowed.contains(&word)
            && word
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '!' | '?' | '-'));
    }

    let mut chars = word.chars();
    let starts_like_word = chars
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '!' | '?' | '-'));
    let is_operator = word
        .chars()
        .all(|ch| matches!(ch, '+' | '*' | '%' | '<' | '>' | '=' | '!' | '-'));
    starts_like_word || is_operator
}

fn regex_escape_literal(word: &str) -> String {
    let mut escaped = String::new();
    for ch in word.chars() {
        if matches!(
            ch,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn lsp_diagnostics(path: &str, pretty: bool) -> Result<()> {
    let path = Path::new(path);
    if !path.is_file() {
        bail!(
            "lsp-diagnostics path must be a source file: {}",
            path.display()
        );
    }

    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let file = path.to_string_lossy().into_owned();
    let diagnostics = source_lsp_diagnostics(&file, &source);
    let payload = json!({
        "uri": file_uri(path),
        "diagnostics": diagnostics,
    });

    if pretty {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", serde_json::to_string(&payload)?);
    }
    Ok(())
}

pub(crate) fn source_lsp_diagnostics(file: &str, source: &str) -> Vec<serde_json::Value> {
    match compile_source(file, source) {
        Ok(_) => syntax_lsp_diagnostics(file, source),
        Err(error) => vec![compile_error_lsp_diagnostic(file, source, &error)],
    }
}

fn compile_error_lsp_diagnostic(
    file: &str,
    source: &str,
    error: &CompileError,
) -> serde_json::Value {
    let (span, message, help) = match error {
        CompileError::Parse(error) => {
            let diagnostic = ricochet_syntax::parse_error_diagnostic(file, source, error);
            (diagnostic.span, diagnostic.message, diagnostic.help)
        }
        CompileError::Unsupported {
            feature,
            span,
            help,
        } => (
            *span,
            format!("unsupported compiler feature: {feature}"),
            help.clone(),
        ),
        CompileError::LoopControlOutsideLoop { word, span } => (
            *span,
            format!("{word} can only be used inside a loop"),
            None,
        ),
    };
    let range = utf16_range_for_span(source, span);
    let mut diagnostic = json!({
        "range": {
            "start": {
                "line": range.start.line,
                "character": range.start.character,
            },
            "end": {
                "line": range.end.line,
                "character": range.end.character,
            },
        },
        "severity": 1,
        "source": "ricochet",
        "message": message,
    });
    if let Some(help) = help {
        diagnostic["codeDescription"] = json!({ "href": "https://github.com/BARKx4/Ricochet" });
        diagnostic["data"] = json!({ "help": help });
    }
    diagnostic
}

struct SyntaxLint {
    span: Span,
    message: String,
    help: String,
    code: &'static str,
}

fn syntax_lsp_diagnostics(file: &str, source: &str) -> Vec<serde_json::Value> {
    let Ok(module) = parse_module(source) else {
        return Vec::new();
    };
    let mut lints = Vec::new();
    collect_module_lints(&module, &mut lints);
    lints
        .into_iter()
        .map(|lint| syntax_lint_lsp_diagnostic(file, source, lint))
        .collect()
}

fn collect_module_lints(module: &Module, lints: &mut Vec<SyntaxLint>) {
    for item in &module.items {
        collect_item_lints(item, lints);
    }
}

fn collect_item_lints(item: &SyntaxItem, lints: &mut Vec<SyntaxLint>) {
    match item {
        SyntaxItem::Class(class) => {
            for item in &class.body {
                collect_item_lints(item, lints);
            }
        }
        SyntaxItem::Method(method) => collect_expr_list_lints(&method.body, lints),
        SyntaxItem::Function(function) => collect_expr_list_lints(&function.body, lints),
        SyntaxItem::Expr { expr, .. } => collect_expr_lints(expr, lints),
    }
}

fn collect_expr_list_lints(exprs: &[SpannedExpr], lints: &mut Vec<SyntaxLint>) {
    for expr in exprs {
        collect_expr_lints(&expr.expr, lints);
    }
}

fn collect_expr_lints(expr: &Expr, lints: &mut Vec<SyntaxLint>) {
    match expr {
        Expr::Sequence(exprs) => {
            for pair in exprs.windows(2) {
                if let [name_expr, get_expr] = pair {
                    if let (Expr::Symbol(name), Expr::Symbol(word)) =
                        (&name_expr.expr, &get_expr.expr)
                    {
                        if word == "get" && is_plain_reference_name(name) {
                            lints.push(prefer_reference_lint(
                                name,
                                Span {
                                    start: name_expr.span.start,
                                    end: get_expr.span.end,
                                },
                            ));
                        }
                    }
                }
            }
            collect_expr_list_lints(exprs, lints);
        }
        Expr::Block(exprs) => collect_expr_list_lints(exprs, lints),
        Expr::If {
            then_body,
            else_body,
        } => {
            collect_expr_list_lints(then_body, lints);
            collect_expr_list_lints(else_body, lints);
        }
        Expr::While { condition, body } => {
            collect_expr_list_lints(condition, lints);
            collect_expr_list_lints(body, lints);
        }
        Expr::Symbol(_)
        | Expr::BangWord(_)
        | Expr::DotWord(_)
        | Expr::Reference(_)
        | Expr::String(_)
        | Expr::Number(_)
        | Expr::Args(_) => {}
    }
}

fn prefer_reference_lint(name: &str, span: Span) -> SyntaxLint {
    SyntaxLint {
        span,
        message: format!("prefer ${name} for variable reads"),
        help: format!(
            "Use ${name} for ordinary variable reads. Keep \"{name}\" get only when the variable name is data on the stack."
        ),
        code: "prefer-dollar-reference",
    }
}

fn is_plain_reference_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn syntax_lint_lsp_diagnostic(file: &str, source: &str, lint: SyntaxLint) -> serde_json::Value {
    let range = utf16_range_for_span(source, lint.span);
    json!({
        "range": {
            "start": {
                "line": range.start.line,
                "character": range.start.character,
            },
            "end": {
                "line": range.end.line,
                "character": range.end.character,
            },
        },
        "severity": 2,
        "source": "ricochet",
        "code": lint.code,
        "codeDescription": { "href": "https://github.com/BARKx4/Ricochet" },
        "message": lint.message,
        "data": { "help": lint.help, "file": file },
    })
}

fn file_uri(path: &Path) -> String {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut path = path.to_string_lossy().replace('\\', "/");
    if !path.starts_with('/') {
        path = format!("/{path}");
    }
    format!("file://{}", percent_encode_uri_path(&path))
}

fn percent_encode_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

async fn migrate(command: MigrateCommand) -> Result<()> {
    match command {
        MigrateCommand::Status { path } => {
            migrate_status(Path::new(path.as_deref().unwrap_or("."))).await
        }
        MigrateCommand::Apply { path } => {
            migrate_apply(Path::new(path.as_deref().unwrap_or("."))).await
        }
    }
}

async fn migrate_status(path: &Path) -> Result<()> {
    let project_root = migration_project_root(path)?;
    let Some(database) = project_database_config(&project_root)? else {
        println!("No [database.default] configured.");
        return Ok(());
    };
    let migrations = discover_migrations(&project_root)?;
    let target = migration_target(&project_root, &database);

    println!("Migrations for {target}");
    if migrations.is_empty() {
        println!("No migration files found in db/migrations.");
        return Ok(());
    }
    let applied = migration_applied_versions(&project_root, &database).await?;
    for migration in migrations {
        let marker = if applied.contains(&migration.version) {
            "x"
        } else {
            " "
        };
        println!("[{marker}] {}", migration.version);
    }
    Ok(())
}

async fn migrate_apply(path: &Path) -> Result<()> {
    let project_root = migration_project_root(path)?;
    let Some(database) = project_database_config(&project_root)? else {
        bail!("No [database.default] configured.");
    };
    let migrations = discover_migrations(&project_root)?;
    if migrations.is_empty() {
        println!("No migration files found in db/migrations.");
        return Ok(());
    }

    match database.adapter.as_str() {
        "sqlite" => migrate_apply_sqlite(&project_root, &database, migrations),
        "postgres" | "postgresql" => migrate_apply_postgres(&database, migrations).await,
        "mysql" | "mariadb" => migrate_apply_mysql(&database, migrations).await,
        adapter => bail!(
            "rco migrate supports sqlite, postgres, and mysql projects; found adapter {:?}",
            adapter
        ),
    }
}

async fn migration_applied_versions(
    project_root: &Path,
    database: &MigrationDatabase,
) -> Result<BTreeSet<String>> {
    match database.adapter.as_str() {
        "sqlite" => {
            let database_path = sqlite_database_path(project_root, &database.url);
            sqlite_applied_migrations_if_present(&database_path)
        }
        "postgres" | "postgresql" => {
            let database = PostgresDatabase::connect(&database.url)
                .await
                .context("failed to connect to PostgreSQL for migrations")?;
            let versions = database
                .migration_versions()
                .await
                .context("failed to read PostgreSQL schema_migrations")?;
            Ok(versions
                .unwrap_or_default()
                .into_iter()
                .collect::<BTreeSet<_>>())
        }
        "mysql" | "mariadb" => {
            let database = MysqlDatabase::connect(&database.url)
                .await
                .context("failed to connect to MySQL for migrations")?;
            let versions = database
                .migration_versions()
                .await
                .context("failed to read MySQL schema_migrations")?;
            Ok(versions
                .unwrap_or_default()
                .into_iter()
                .collect::<BTreeSet<_>>())
        }
        adapter => bail!(
            "rco migrate supports sqlite, postgres, and mysql projects; found adapter {:?}",
            adapter
        ),
    }
}

fn migrate_apply_sqlite(
    project_root: &Path,
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
) -> Result<()> {
    let database_path = sqlite_database_path(project_root, &database.url);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut connection = rusqlite::Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    ensure_schema_migrations_table(&connection)?;
    let mut applied = sqlite_applied_migrations(&connection)?;
    let mut applied_count = 0usize;
    for migration in migrations {
        if applied.contains(&migration.version) {
            continue;
        }
        let sql = fs::read_to_string(&migration.path)
            .with_context(|| format!("failed to read {}", migration.path.display()))?;
        let tx = connection
            .transaction()
            .with_context(|| format!("failed to start migration {}", migration.version))?;
        tx.execute_batch(&sql)
            .with_context(|| format!("failed to apply migration {}", migration.version))?;
        tx.execute(
            "insert into schema_migrations (version, applied_at) values (?1, ?2)",
            (&migration.version, migration_timestamp()),
        )
        .with_context(|| format!("failed to record migration {}", migration.version))?;
        tx.commit()
            .with_context(|| format!("failed to commit migration {}", migration.version))?;
        applied.insert(migration.version.clone());
        applied_count += 1;
        println!("applied {}", migration.version);
    }

    print_migration_apply_summary(applied_count);
    Ok(())
}

async fn migrate_apply_postgres(
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
) -> Result<()> {
    let database = PostgresDatabase::connect(&database.url)
        .await
        .context("failed to connect to PostgreSQL for migrations")?;
    database
        .ensure_schema_migrations_table()
        .await
        .context("failed to create PostgreSQL schema_migrations")?;
    let mut applied = database
        .migration_versions()
        .await
        .context("failed to read PostgreSQL schema_migrations")?
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut applied_count = 0usize;
    for migration in migrations {
        if applied.contains(&migration.version) {
            continue;
        }
        let sql = fs::read_to_string(&migration.path)
            .with_context(|| format!("failed to read {}", migration.path.display()))?;
        let applied_at = migration_timestamp();
        database
            .apply_migration(&migration.version, &applied_at, &sql)
            .await
            .with_context(|| format!("failed to apply migration {}", migration.version))?;
        applied.insert(migration.version.clone());
        applied_count += 1;
        println!("applied {}", migration.version);
    }
    print_migration_apply_summary(applied_count);
    Ok(())
}

async fn migrate_apply_mysql(
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
) -> Result<()> {
    let database = MysqlDatabase::connect(&database.url)
        .await
        .context("failed to connect to MySQL for migrations")?;
    database
        .ensure_schema_migrations_table()
        .await
        .context("failed to create MySQL schema_migrations")?;
    let mut applied = database
        .migration_versions()
        .await
        .context("failed to read MySQL schema_migrations")?
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut applied_count = 0usize;
    for migration in migrations {
        if applied.contains(&migration.version) {
            continue;
        }
        let sql = fs::read_to_string(&migration.path)
            .with_context(|| format!("failed to read {}", migration.path.display()))?;
        let applied_at = migration_timestamp();
        database
            .apply_migration(&migration.version, &applied_at, &sql)
            .await
            .with_context(|| format!("failed to apply migration {}", migration.version))?;
        applied.insert(migration.version.clone());
        applied_count += 1;
        println!("applied {}", migration.version);
    }
    print_migration_apply_summary(applied_count);
    Ok(())
}

fn print_migration_apply_summary(applied_count: usize) {
    if applied_count == 0 {
        println!("No pending migrations.");
    } else {
        println!("Applied {applied_count} migration(s).");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationDatabase {
    adapter: String,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationFile {
    version: String,
    path: PathBuf,
}

fn migration_project_root(path: &Path) -> Result<PathBuf> {
    let path = if path.is_file() {
        path.parent().unwrap_or_else(|| Path::new("."))
    } else {
        path
    };
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve migration path {}", path.display()))?;
    if canonical.join("ricochet.toml").is_file() {
        return Ok(canonical);
    }
    for ancestor in canonical.ancestors() {
        if ancestor.join("ricochet.toml").is_file() {
            return Ok(ancestor.to_path_buf());
        }
    }
    bail!(
        "migrate must be run inside a Ricochet project with ricochet.toml: {}",
        path.display()
    )
}

fn project_database_config(project_root: &Path) -> Result<Option<MigrationDatabase>> {
    let manifest_path = project_root.join("ricochet.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = manifest_source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let Some(default) = manifest
        .get("database")
        .and_then(Item::as_table)
        .and_then(|database| database.get("default"))
        .and_then(Item::as_table)
    else {
        return Ok(None);
    };
    let adapter = default
        .get("adapter")
        .and_then(Item::as_str)
        .context("database.default.adapter must be a string")?
        .to_string();
    let url = default
        .get("url")
        .and_then(Item::as_str)
        .context("database.default.url must be a string")?
        .to_string();
    Ok(Some(MigrationDatabase { adapter, url }))
}

fn migration_target(project_root: &Path, database: &MigrationDatabase) -> String {
    match database.adapter.as_str() {
        "sqlite" => sqlite_database_path(project_root, &database.url)
            .display()
            .to_string(),
        "postgres" | "postgresql" => "PostgreSQL database".to_string(),
        "mysql" | "mariadb" => "MySQL database".to_string(),
        adapter => format!("{adapter} database"),
    }
}

fn sqlite_database_path(project_root: &Path, url: &str) -> PathBuf {
    let path = Path::new(url);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn discover_migrations(project_root: &Path) -> Result<Vec<MigrationFile>> {
    let migrations_dir = project_root.join("db").join("migrations");
    if !migrations_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut migrations = Vec::new();
    for entry in fs::read_dir(&migrations_dir)
        .with_context(|| format!("failed to read {}", migrations_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", migrations_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("sql") {
            continue;
        }
        let version = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("migration file name must be UTF-8")?
            .to_string();
        validate_migration_version(&version, &path)?;
        migrations.push(MigrationFile { version, path });
    }
    migrations.sort_by(|left, right| left.version.cmp(&right.version));
    Ok(migrations)
}

fn validate_migration_version(version: &str, path: &Path) -> Result<()> {
    if version.is_empty()
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        bail!(
            "migration file name must contain only letters, digits, '_' or '-': {}",
            path.display()
        );
    }
    Ok(())
}

fn sqlite_applied_migrations_if_present(database_path: &Path) -> Result<BTreeSet<String>> {
    if !database_path.is_file() {
        return Ok(BTreeSet::new());
    }
    let connection = rusqlite::Connection::open(database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    if !schema_migrations_table_exists(&connection)? {
        return Ok(BTreeSet::new());
    }
    sqlite_applied_migrations(&connection)
}

fn ensure_schema_migrations_table(connection: &rusqlite::Connection) -> Result<()> {
    connection
        .execute_batch(
            r#"
create table if not exists schema_migrations (
  version text primary key,
  applied_at text not null
);
"#,
        )
        .context("failed to create schema_migrations table")?;
    Ok(())
}

fn schema_migrations_table_exists(connection: &rusqlite::Connection) -> Result<bool> {
    let count: i64 = connection.query_row(
        "select count(*) from sqlite_master where type = 'table' and name = 'schema_migrations'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn sqlite_applied_migrations(connection: &rusqlite::Connection) -> Result<BTreeSet<String>> {
    let mut statement = connection
        .prepare("select version from schema_migrations order by version")
        .context("failed to read schema_migrations")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut applied = BTreeSet::new();
    for row in rows {
        applied.insert(row?);
    }
    Ok(applied)
}

fn migration_timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    millis.to_string()
}

fn routes(path: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.is_dir() {
        bail!(
            "routes path does not exist or is not a directory: {}",
            path.display()
        );
    }

    for route in ricochet_web::routes_from_dir(path)? {
        println!(
            "{} {} {}#{}",
            route.method, route.path, route.controller, route.action
        );
    }

    Ok(())
}

fn doc_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.exists() {
        bail!("doc path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.to_path_buf());
    } else {
        collect_rco_files(path, &mut files)?;
        files.sort();
    }

    let mut output = String::new();
    writeln!(&mut output, "# Ricochet Documentation")?;
    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let module =
            parse_module(&source).with_context(|| format!("failed to parse {}", file.display()))?;
        write_module_docs(&mut output, &file, &module)?;
    }
    print!("{output}");
    Ok(())
}

fn write_module_docs(output: &mut String, file: &Path, module: &Module) -> Result<()> {
    writeln!(output)?;
    writeln!(output, "## File `{}`", file.display())?;
    for item in &module.items {
        write_item_docs(output, item, 0)?;
    }
    Ok(())
}

fn write_item_docs(output: &mut String, item: &SyntaxItem, indent: usize) -> Result<()> {
    match item {
        SyntaxItem::Class(class) => {
            writeln!(output)?;
            writeln!(output, "{}## Class `{}`", doc_indent(indent), class.name)?;
            writeln!(
                output,
                "{}Extends `{}`.",
                doc_indent(indent),
                class.superclass
            )?;
            write_docs(output, &class.docs, indent)?;
            for item in &class.body {
                write_item_docs(output, item, indent + 1)?;
            }
        }
        SyntaxItem::Function(function) => {
            writeln!(output)?;
            writeln!(
                output,
                "{}## Function `{}`{}",
                doc_indent(indent),
                function.name,
                doc_args(function.args.as_ref())
            )?;
            write_docs(output, &function.docs, indent)?;
        }
        SyntaxItem::Method(method) => {
            writeln!(
                output,
                "{}- Method: `{}`{}",
                doc_indent(indent),
                method.name,
                doc_args(method.args.as_ref())
            )?;
            write_docs(output, &method.docs, indent + 1)?;
        }
        SyntaxItem::Expr { expr, docs, .. } => {
            if let Some((kind, name)) = documented_expr_declaration(expr) {
                writeln!(output, "{}- {kind}: `{name}`", doc_indent(indent))?;
                write_docs(output, docs, indent + 1)?;
            }
        }
    }
    Ok(())
}

fn documented_expr_declaration(expr: &Expr) -> Option<(&'static str, &str)> {
    let Expr::Sequence(exprs) = expr else {
        return None;
    };

    match exprs.as_slice() {
        [name, declaration] => {
            let name = declaration_name(name)?;
            match &declaration.expr {
                Expr::Symbol(word) if word == "Field" => Some(("Field", name)),
                Expr::Symbol(word) if word == "Accessor" => Some(("Accessor", name)),
                Expr::Symbol(word) if word == "Table" => Some(("Table", name)),
                _ => None,
            }
        }
        [body, name, declaration]
            if matches!(&body.expr, Expr::Block(_))
                && matches!(&declaration.expr, Expr::Symbol(word) if word == "Method") =>
        {
            declaration_name(name).map(|name| ("Method", name))
        }
        [args, body, name, declaration]
            if matches!(&args.expr, Expr::Args(_))
                && matches!(&body.expr, Expr::Block(_))
                && matches!(&declaration.expr, Expr::Symbol(word) if word == "Method") =>
        {
            declaration_name(name).map(|name| ("Method", name))
        }
        _ => None,
    }
}

fn declaration_name(expression: &SpannedExpr) -> Option<&str> {
    match &expression.expr {
        Expr::Symbol(name) | Expr::String(name) => Some(name),
        _ => None,
    }
}

fn write_docs(output: &mut String, docs: &[String], indent: usize) -> Result<()> {
    for doc in docs {
        writeln!(output, "{}{}", doc_indent(indent), doc)?;
    }
    Ok(())
}

fn doc_args(args: Option<&ArgsDecl>) -> String {
    let Some(args) = args else {
        return String::new();
    };

    let inputs = if args.inputs.is_empty() {
        String::new()
    } else {
        args.inputs.join(" ")
    };
    let outputs = if args.outputs.is_empty() {
        String::new()
    } else {
        format!(" -> {}", args.outputs.join(" "))
    };
    format!(" `({inputs}{outputs})`")
}

fn doc_indent(indent: usize) -> String {
    "  ".repeat(indent)
}

#[derive(Debug, Clone, Copy, Default)]
struct NewProjectOptions {
    with_sqlite: bool,
}

fn new_project(path: &Path, options: NewProjectOptions) -> Result<()> {
    create_new_project(path, options, true)
}

fn create_new_project(path: &Path, options: NewProjectOptions, announce: bool) -> Result<()> {
    ensure_project_path_is_ready(path)?;

    fs::create_dir_all(path.join("app").join("Controllers"))
        .with_context(|| format!("failed to create app/Controllers in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Models"))
        .with_context(|| format!("failed to create app/Models in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("home"))
        .with_context(|| format!("failed to create app/Views/home in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("users"))
        .with_context(|| format!("failed to create app/Views/users in {}", path.display()))?;
    if options.with_sqlite {
        fs::create_dir_all(path.join("app").join("Views").join("auth"))
            .with_context(|| format!("failed to create app/Views/auth in {}", path.display()))?;
    }
    fs::create_dir_all(path.join("config"))
        .with_context(|| format!("failed to create config in {}", path.display()))?;
    fs::create_dir_all(path.join("tests"))
        .with_context(|| format!("failed to create tests in {}", path.display()))?;
    fs::create_dir_all(path.join("public"))
        .with_context(|| format!("failed to create public in {}", path.display()))?;
    if options.with_sqlite {
        fs::create_dir_all(path.join("db").join("migrations"))
            .with_context(|| format!("failed to create db/migrations in {}", path.display()))?;
    }

    write_project_file(
        path.join("ricochet.toml"),
        &manifest_source(&project_name(path), options),
    )?;
    write_project_file(
        path.join("config").join("routes.rco"),
        routes_source(options),
    )?;
    write_project_file(
        path.join("app")
            .join("Controllers")
            .join("HomeController.rco"),
        r#"HomeController Controller Subclass
  [
    "Hello Ricochet" title var
    $ctx
    "home/index" swap view
  ] "index" Method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Models").join("User.rco"),
        user_model_source(options),
    )?;
    write_project_file(
        path.join("app")
            .join("Controllers")
            .join("UserController.rco"),
        user_controller_source(options),
    )?;
    if options.with_sqlite {
        write_project_file(
            path.join("app")
                .join("Controllers")
                .join("AuthController.rco"),
            auth_controller_source(),
        )?;
    }
    write_project_file(
        path.join("app")
            .join("Views")
            .join("home")
            .join("index.html"),
        "<link rel=\"stylesheet\" href=\"/assets/app.css\">\n<h1>{ $title }</h1>\n",
    )?;
    write_project_file(
        path.join("app")
            .join("Views")
            .join("users")
            .join("index.html"),
        users_index_view_source(options),
    )?;
    if options.with_sqlite {
        write_project_file(
            path.join("app")
                .join("Views")
                .join("auth")
                .join("login.html"),
            auth_login_view_source(),
        )?;
        write_project_file(
            path.join("app")
                .join("Views")
                .join("auth")
                .join("show.html"),
            auth_show_view_source(),
        )?;
    }
    write_project_file(
        path.join("tests").join("ApplicationSmokeTest.rco"),
        r#"ApplicationSmokeTest TestCase Subclass
  [
    User new
    "ada@example.com" swap email.set
    displayName
    "ada@example.com" assert-equals
  ] "testUserDisplayNameFallsBackToEmail" Method

  [
    users array
    User new
    "grace@example.com" swap email.set
    $users swap push! drop
    $users count
    1 assert-equals
  ] "testCollectionsCanHoldModels" Method
end
"#,
    )?;
    write_project_file(
        path.join("public").join("app.css"),
        "body {\n  font-family: system-ui, sans-serif;\n  margin: 2rem;\n}\n",
    )?;

    if options.with_sqlite {
        write_project_file(
            path.join("db")
                .join("migrations")
                .join("0001_create_users.sql"),
            initial_sqlite_migration_source(),
        )?;
        create_sqlite_development_database(path)?;
        if announce {
            println!(
                "created {} with SQLite database at {}",
                path.display(),
                path.join("db").join("development.sqlite3").display()
            );
        }
    } else if announce {
        println!("created {}", path.display());
    }
    Ok(())
}

fn routes_source(options: NewProjectOptions) -> &'static str {
    if options.with_sqlite {
        r#"GET "/" HomeController "index" route
GET "/users" UserController "index" route
GET "/login" AuthController "login" route
POST "/login" AuthController "create" route
GET "/me" AuthController "show" route
POST "/logout" AuthController "destroy" route
"#
    } else {
        r#"GET "/" HomeController "index" route
GET "/users" UserController "index" route
"#
    }
}

fn user_model_source(options: NewProjectOptions) -> &'static str {
    if options.with_sqlite {
        r#"User Model Subclass
  "users" Table
  "id" Accessor
  "email" Accessor
  "name" Accessor

  [
    self name.get nil? if
      self email.get
    else
      self name.get
    end
  ] "displayName" Method
end
"#
    } else {
        r#"User Model Subclass
  "email" Accessor
  "name" Accessor

  [
    self name.get nil? if
      self email.get
    else
      self name.get
    end
  ] "displayName" Method
end
"#
    }
}

fn user_controller_source(options: NewProjectOptions) -> &'static str {
    if options.with_sqlite {
        r#"UserController Controller Subclass
  ( session ctx ) [
    ctx var
    session var
    $session "last_page" "users" put! drop
    User default-page
    dup ok? if
      value users var
      $users count userCount var
      $users first firstUser var
      $firstUser "email" at firstEmail var
      "Users" title var
      $ctx
      "users/index" swap view
    else
      error "message" at text
    end
  ] "index" Method
end
"#
    } else {
        r#"UserController Controller Subclass
  [
    users array
    User new
    "ada@example.com" swap email.set
    "Ada Lovelace" swap name.set
    $users swap push! drop
    $users count userCount var
    "Users" title var
    $ctx
    "users/index" swap view
  ] "index" Method
end
"#
    }
}

fn auth_controller_source() -> &'static str {
    r#"AuthController Controller Subclass
  ( ctx ) [
    ctx var
    "Sign in" title var
    $ctx
    "auth/login" swap view
  ] "login" Method

  ( email session ) [
    session var
    email var
    $email nil? if
      "Email is required" text 400 status
    else
      $email blank? if
        "Email is required" text 400 status
      else
        $session "user_email" $email put! drop
        "/me" redirect
      end
    end
  ] "create" Method

  ( session ctx ) [
    ctx var
    session var
    $session "user_email" at nil? if
      "Not signed in" text
    else
      $session "user_email" at userEmail var
      "Signed in" title var
      $ctx
      "auth/show" swap view
    end
  ] "show" Method

  ( session ) [
    session var
    $session "user_email" remove! drop
    "/login" redirect
  ] "destroy" Method
end
"#
}

fn users_index_view_source(options: NewProjectOptions) -> &'static str {
    if options.with_sqlite {
        "<h1>{ $title }</h1>\n<p>{ $userCount } users ready.</p>\n<p>First user: { $firstEmail }</p>\n"
    } else {
        "<h1>{ $title }</h1>\n<p>{ $userCount } users ready.</p>\n"
    }
}

fn auth_login_view_source() -> &'static str {
    "<h1>{ $title }</h1>\n<form method=\"post\" action=\"/login\">\n  <label>Email <input name=\"email\" type=\"email\" value=\"ada@example.com\"></label>\n  <button type=\"submit\">Sign in</button>\n</form>\n"
}

fn auth_show_view_source() -> &'static str {
    "<h1>{ $title }</h1>\n<p>Signed in as { $userEmail }</p>\n<form method=\"post\" action=\"/logout\">\n  <button type=\"submit\">Sign out</button>\n</form>\n"
}

fn initial_sqlite_migration_source() -> &'static str {
    r#"create table users (
  id integer primary key,
  email text not null,
  name text not null
);

insert into users (email, name) values
  ('ada@example.com', 'Ada Lovelace'),
  ('grace@example.com', 'Grace Hopper');
"#
}

fn create_sqlite_development_database(path: &Path) -> Result<()> {
    let database_path = path.join("db").join("development.sqlite3");
    let connection = rusqlite::Connection::open(&database_path)
        .with_context(|| format!("failed to create {}", database_path.display()))?;
    connection
        .execute_batch(&format!(
            r#"
create table if not exists schema_migrations (
  version text primary key,
  applied_at text not null
);

{}

insert into schema_migrations (version, applied_at)
values ('0001_create_users', 'scaffold');
"#,
            initial_sqlite_migration_source()
        ))
        .with_context(|| format!("failed to seed {}", database_path.display()))?;
    Ok(())
}

fn ensure_project_path_is_ready(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "project path already exists and is not a directory: {}",
                path.display()
            );
        }

        if fs::read_dir(path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .next()
            .transpose()
            .with_context(|| format!("failed to read entry in {}", path.display()))?
            .is_some()
        {
            bail!(
                "project path already exists and is not empty: {}",
                path.display()
            );
        }
    }

    Ok(())
}

fn write_project_file(path: PathBuf, contents: &str) -> Result<()> {
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn manifest_source(name: &str, options: NewProjectOptions) -> String {
    let mut manifest = format!(
        r#"[package]
name = "{name}"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.static]
dir = "public"
mount = "/assets"
"#
    );

    if options.with_sqlite {
        manifest.push_str(
            r#"
[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
"#,
        );
    }

    manifest
}

fn project_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ricochet_app");
    let sanitized = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "ricochet_app".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug)]
struct DependencySpec {
    name: String,
    package: Option<String>,
    path: String,
    source: String,
    git: Option<String>,
    rev: Option<String>,
    commit: Option<String>,
    registry: Option<String>,
    version_req: Option<String>,
    package_version: Option<String>,
    integrity: Option<String>,
    provenance: Option<String>,
    signature: Option<String>,
    signature_kind: Option<String>,
    display_source: String,
}

impl DependencySpec {
    fn registry_package_name(&self) -> &str {
        self.package.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug)]
struct LockedPackage {
    source: String,
    package: Option<String>,
    path: String,
    git: Option<String>,
    rev: Option<String>,
    commit: Option<String>,
    registry: Option<String>,
    version_req: Option<String>,
    package_version: Option<String>,
    integrity: Option<String>,
    provenance: Option<String>,
    signature: Option<String>,
    signature_kind: Option<String>,
}

#[derive(Debug)]
enum DependencySource {
    Local {
        path: PathBuf,
    },
    GitHub {
        owner: String,
        repo: String,
        rev: Option<String>,
    },
    Registry {
        package: String,
        version: Option<String>,
        registry: String,
    },
}

#[derive(Debug)]
struct RegistryPackage {
    package_dir: PathBuf,
    version: String,
    integrity: String,
    provenance: Option<String>,
    signature: Option<String>,
    signature_kind: Option<String>,
}

#[derive(Debug, Default)]
struct PublishRegistryOptions<'a> {
    dry_run: bool,
    provenance_file: Option<&'a Path>,
    signature_file: Option<&'a Path>,
    signature_kind: Option<&'a str>,
}

#[derive(Debug)]
struct PublishArtifact {
    source: PathBuf,
    target: &'static str,
    integrity: String,
}

fn add_dependency(
    source: &str,
    name: Option<&str>,
    registry: Option<&Path>,
    registry_url: Option<&str>,
    version_req: Option<&str>,
    no_fetch: bool,
) -> Result<()> {
    let manifest_path = find_project_manifest_for_current_dir("add")?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let dependency_source = parse_dependency_source(source, registry, registry_url)?;
    let mut spec = dependency_spec(project_root, source, dependency_source, name, version_req)?;
    let skip_remote_fetch = no_fetch && (spec.git.is_some() || spec.registry.is_some());

    if spec.git.is_some() && !no_fetch {
        spec.commit = Some(fetch_git_dependency(project_root, &spec)?);
    }
    if spec.registry.is_some() && !no_fetch {
        install_registry_dependency(project_root, &mut spec, None)?;
    }
    if !skip_remote_fetch {
        spec.package_version = package_version_for_spec(project_root, &spec)?;
        spec.integrity = Some(package_integrity(project_root, &spec)?);
    }

    write_dependency_manifest(&manifest_path, &spec)?;
    if !skip_remote_fetch {
        write_lockfile(&project_root.join("ricochet.lock"), &spec)?;
    }

    if skip_remote_fetch {
        println!(
            "added {} from {} (fetch skipped)",
            spec.name, spec.display_source
        );
    } else {
        println!("added {} from {}", spec.name, spec.display_source);
    }
    Ok(())
}

fn publish_package(
    path: Option<&str>,
    registry: &Path,
    options: PublishRegistryOptions<'_>,
) -> Result<()> {
    let manifest_path = project_manifest_path_for_command("publish", path)?;
    let package_root = manifest_path
        .parent()
        .expect("package manifest should have a parent");
    let metadata = read_package_metadata(package_root)?;
    let name = metadata
        .name
        .as_deref()
        .with_context(|| format!("{} must include [package] name", manifest_path.display()))?;
    validate_registry_package_name(name)?;
    let version = metadata
        .version
        .as_deref()
        .with_context(|| format!("{} must include [package] version", manifest_path.display()))?;
    validate_package_version(version)?;

    let registry_root = absolute_path_from_current(registry)?;
    let package_root_canonical = fs::canonicalize(package_root)
        .with_context(|| format!("failed to resolve {}", package_root.display()))?;
    if registry_root.starts_with(&package_root_canonical) {
        bail!("publish registry must not be inside the package being published");
    }

    if options.signature_kind.is_some() && options.signature_file.is_none() {
        bail!("--signature-kind requires --signature-file");
    }
    let signature_kind = if options.signature_file.is_some() {
        Some(options.signature_kind.unwrap_or("detached"))
    } else {
        None
    };
    if let Some(signature_kind) = signature_kind {
        validate_signature_kind(signature_kind)?;
    }
    let provenance = prepare_publish_artifact(
        "provenance attestation",
        options.provenance_file,
        "provenance.attestation",
    )?;
    let signature = prepare_publish_artifact(
        "detached signature",
        options.signature_file,
        "signature.sig",
    )?;

    let package_integrity = package_tree_integrity(package_root)?;
    let version_root = registry_root
        .join(registry_package_relative_path(name))
        .join(version);
    let destination = version_root.join("package");
    if version_root.exists() {
        bail!(
            "registry already contains {name} {version}: {}",
            version_root.display()
        );
    }

    if options.dry_run {
        println!(
            "would publish {name} {version} to {} with integrity {package_integrity}",
            version_root.display()
        );
        if let Some(provenance) = &provenance {
            println!("would attach provenance {}", provenance.integrity);
        }
        if let Some(signature) = &signature {
            println!(
                "would attach {} signature {}",
                signature_kind.unwrap_or("detached"),
                signature.integrity
            );
        }
        return Ok(());
    }

    fs::create_dir_all(&registry_root)
        .with_context(|| format!("failed to create {}", registry_root.display()))?;
    fs::create_dir_all(&version_root)
        .with_context(|| format!("failed to create {}", version_root.display()))?;
    copy_package_tree(package_root, &destination)?;
    let copied_integrity = package_tree_integrity(&destination)?;
    if copied_integrity != package_integrity {
        bail!(
            "published package integrity changed while copying: expected {package_integrity}, got {copied_integrity}"
        );
    }
    if let Some(provenance) = &provenance {
        copy_publish_artifact(provenance, &version_root)?;
    }
    if let Some(signature) = &signature {
        copy_publish_artifact(signature, &version_root)?;
    }

    let mut metadata_doc = DocumentMut::new();
    let mut package_table = Table::new();
    package_table["name"] = value(name);
    package_table["version"] = value(version);
    package_table["integrity"] = value(package_integrity.clone());
    metadata_doc
        .as_table_mut()
        .insert("package", Item::Table(package_table));
    let mut provenance_table = Table::new();
    let mut has_provenance = false;
    if let Some(provenance) = &provenance {
        provenance_table["attestation"] = value(provenance.target);
        provenance_table["attestation_integrity"] = value(provenance.integrity.clone());
        has_provenance = true;
    }
    if let Some(signature) = &signature {
        provenance_table["signature"] = value(signature.target);
        provenance_table["signature_integrity"] = value(signature.integrity.clone());
        provenance_table["signature_kind"] = value(signature_kind.unwrap_or("detached"));
        has_provenance = true;
    }
    if has_provenance {
        metadata_doc
            .as_table_mut()
            .insert("provenance", Item::Table(provenance_table));
    }
    fs::write(version_root.join("metadata.toml"), metadata_doc.to_string()).with_context(|| {
        format!(
            "failed to write {}",
            version_root.join("metadata.toml").display()
        )
    })?;

    println!(
        "published {name} {version} to {} with integrity {package_integrity}",
        version_root.display()
    );
    if let Some(provenance) = &provenance {
        println!("attached provenance {}", provenance.integrity);
    }
    if let Some(signature) = &signature {
        println!(
            "attached {} signature {}",
            signature_kind.unwrap_or("detached"),
            signature.integrity
        );
    }
    Ok(())
}

fn prepare_publish_artifact(
    label: &str,
    source: Option<&Path>,
    target: &'static str,
) -> Result<Option<PublishArtifact>> {
    let Some(source) = source else {
        return Ok(None);
    };
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect {label} {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "{label} must be a regular file, not a symlink: {}",
            source.display()
        );
    }
    if !metadata.is_file() {
        bail!("{label} is not a file: {}", source.display());
    }
    Ok(Some(PublishArtifact {
        source: source.to_path_buf(),
        target,
        integrity: file_integrity(source)?,
    }))
}

fn copy_publish_artifact(artifact: &PublishArtifact, version_root: &Path) -> Result<()> {
    let destination = version_root.join(artifact.target);
    fs::copy(&artifact.source, &destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            artifact.source.display(),
            destination.display()
        )
    })?;
    let copied_integrity = file_integrity(&destination)?;
    if copied_integrity != artifact.integrity {
        bail!(
            "published artifact {} changed while copying: expected {}, got {}",
            artifact.target,
            artifact.integrity,
            copied_integrity
        );
    }
    Ok(())
}

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

fn rebuild_static_registry(path: &Path) -> Result<()> {
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

            let archive_relative = registry_package_archive_relative_path(&package_name, &version);
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
    registry_table["format"] = value(STATIC_REGISTRY_FORMAT);
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
        let metadata_relative = registry_package_metadata_relative_path(package);
        write_static_package_metadata(&registry_root, package, versions, &metadata_relative)?;
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

fn check_static_registry(path: &Path) -> Result<()> {
    let registry_root = absolute_path_from_current(path)?;
    let index_source = file_url_from_path(&registry_root.join("index.toml"));
    let index = load_static_registry_index(&index_source)?;
    let mut checked = 0usize;
    for (package, metadata_path) in &index.packages {
        let metadata = load_static_registry_package(&index.source, package, metadata_path)?;
        for version in metadata.versions {
            validate_package_integrity(&version.archive_integrity)?;
            validate_package_integrity(&version.package_integrity)?;
            let archive_source = resolve_static_registry_resource(&index.source, &version.archive)?;
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

fn search_registry(query: &str, registry: Option<&Path>, registry_url: Option<&str>) -> Result<()> {
    if registry.is_some() && registry_url.is_some() {
        bail!("use either --registry or --registry-url, not both");
    }
    let index_source = if let Some(registry_url) = registry_url {
        validate_static_registry_url(registry_url)?.to_string()
    } else {
        let registry = registry
            .map(absolute_path_from_current)
            .transpose()?
            .unwrap_or_else(|| PathBuf::from("."));
        file_url_from_path(&registry.join("index.toml"))
    };
    let index = load_static_registry_index(&index_source)?;
    let query = query.to_ascii_lowercase();
    let mut found = 0usize;
    for (package, metadata_path) in &index.packages {
        if !package.to_ascii_lowercase().contains(&query) {
            continue;
        }
        let metadata = load_static_registry_package(&index.source, package, metadata_path)?;
        let Some(version) = latest_static_registry_version(&metadata.versions, None) else {
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

fn write_static_package_metadata(
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

fn load_static_registry_index(source: &str) -> Result<StaticRegistryIndex> {
    validate_static_registry_url(source)?;
    let bytes = read_static_registry_bytes(source, MAX_STATIC_REGISTRY_METADATA_BYTES)?;
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
    if format != STATIC_REGISTRY_FORMAT {
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
        validate_static_registry_relative_path(metadata, "package metadata")?;
        packages.insert(package.to_string(), metadata.to_string());
    }
    Ok(StaticRegistryIndex {
        source: source.to_string(),
        packages,
    })
}

fn load_static_registry_package(
    index_source: &str,
    expected_package: &str,
    metadata_path: &str,
) -> Result<StaticRegistryPackageMetadata> {
    let metadata_source = resolve_static_registry_resource(index_source, metadata_path)?;
    let bytes = read_static_registry_bytes(&metadata_source, MAX_STATIC_REGISTRY_METADATA_BYTES)?;
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
        validate_static_registry_relative_or_absolute_url(&archive, "archive")?;
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

fn latest_static_registry_version<'a>(
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

fn static_registry_version<'a>(
    metadata: &'a StaticRegistryPackageMetadata,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<&'a StaticRegistryVersion> {
    if let Some(locked_version) = locked.and_then(|lock| lock.package_version.as_deref()) {
        if package_version_satisfies(spec.version_req.as_deref(), locked_version)? {
            if let Some(version) = metadata
                .versions
                .iter()
                .find(|version| version.version == locked_version)
            {
                return Ok(version);
            }
        }
    }
    latest_static_registry_version(&metadata.versions, spec.version_req.as_deref()).with_context(
        || {
            let requirement = spec.version_req.as_deref().unwrap_or("*");
            format!(
                "static registry package {} has no version satisfying {}",
                metadata.name, requirement
            )
        },
    )
}

fn validate_static_registry_relative_path(path: &str, label: &str) -> Result<()> {
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("file://") {
        bail!("{label} must be a registry-relative path, got {path:?}");
    }
    validate_project_relative_path(path, label)
}

fn validate_static_registry_relative_or_absolute_url(value: &str, label: &str) -> Result<()> {
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("file://")
    {
        validate_static_registry_url(value)?;
        return Ok(());
    }
    validate_static_registry_relative_path(value, label)
}

fn resolve_static_registry_resource(index_source: &str, resource: &str) -> Result<String> {
    validate_static_registry_relative_or_absolute_url(resource, "static registry resource")?;
    if resource.starts_with("http://")
        || resource.starts_with("https://")
        || resource.starts_with("file://")
    {
        return Ok(resource.to_string());
    }
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

fn read_static_registry_bytes(source: &str, limit: usize) -> Result<Vec<u8>> {
    validate_static_registry_url(source)?;
    if let Some(path) = file_url_to_path(source) {
        let metadata =
            fs::metadata(&path).with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.len() as usize > limit {
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
        let response = reqwest::blocking::get(&source_for_thread)
            .with_context(|| {
                format!("failed to fetch static registry resource {source_for_thread}")
            })?
            .error_for_status()
            .with_context(|| {
                format!("static registry resource {source_for_thread} returned an error")
            })?;
        let bytes = response.bytes().with_context(|| {
            format!("failed to read static registry resource {source_for_thread}")
        })?;
        if bytes.len() > limit {
            bail!(
                "static registry resource {source_for_thread} is too large: {} bytes",
                bytes.len()
            );
        }
        Ok(bytes.to_vec())
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

fn file_integrity(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(bytes_integrity(&bytes))
}

fn bytes_integrity(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", hex_digest(&digest))
}

fn extract_package_archive(bytes: &[u8], destination: &Path) -> Result<()> {
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
    for entry in archive
        .entries()
        .context("failed to read static registry package archive")?
    {
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
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            let mut output = fs::File::create(&destination_path)
                .with_context(|| format!("failed to create {}", destination_path.display()))?;
            io::copy(&mut entry, &mut output)
                .with_context(|| format!("failed to unpack {}", destination_path.display()))?;
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

fn validate_signature_kind(kind: &str) -> Result<&str> {
    if kind.is_empty()
        || !kind
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        bail!("invalid signature kind {kind:?}; use letters, numbers, _, - or .");
    }
    Ok(kind)
}

fn install_dependencies() -> Result<()> {
    let manifest_path = find_project_manifest_for_current_dir("install")?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let Some(dependencies) = doc.get("dependencies").and_then(Item::as_table) else {
        println!("no dependencies to install");
        return Ok(());
    };

    let lock_path = project_root.join("ricochet.lock");
    let mut installed = 0usize;
    for (name, item) in dependencies.iter() {
        validate_package_name(name)?;
        let table = item.as_table().with_context(|| {
            format!(
                "dependency {name} in {} must be a table",
                manifest_path.display()
            )
        })?;
        let git = table.get("git").and_then(Item::as_str).map(str::to_string);
        let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
        let registry = table
            .get("registry")
            .and_then(Item::as_str)
            .map(str::to_string);
        let package = table
            .get("package")
            .and_then(Item::as_str)
            .map(str::to_string);
        if git.is_some() && registry.is_some() {
            bail!("dependency {name} cannot include both git and registry");
        }
        if package.is_some() && registry.is_none() {
            bail!("dependency {name} can only include package with a registry");
        }
        if let Some(package) = package.as_deref() {
            validate_registry_package_name(package)?;
        }
        let path = dependency_manifest_path(name, table, registry.is_some(), &manifest_path)?;
        let version_req = table
            .get("version")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(version_req) = version_req.as_deref() {
            validate_version_req(version_req)?;
        }
        let lock_doc = read_optional_toml_document(&lock_path)?;
        let locked = locked_package(lock_doc.as_ref(), name)?;
        let commit = git
            .as_ref()
            .and_then(|_| locked.as_ref().and_then(|lock| lock.commit.clone()));
        let display_source = git.clone().unwrap_or_else(|| path.clone());
        let mut spec = DependencySpec {
            name: name.to_string(),
            package: package.clone().filter(|package| package != name),
            source: git
                .as_ref()
                .map(|git| format!("git+{git}"))
                .or_else(|| {
                    registry.as_ref().map(|registry| {
                        format!("registry+{registry}#{}", package.as_deref().unwrap_or(name))
                    })
                })
                .unwrap_or_else(|| format!("path+{path}")),
            path: path.clone(),
            git,
            rev,
            commit,
            registry: registry.clone(),
            version_req,
            package_version: None,
            integrity: None,
            provenance: None,
            signature: None,
            signature_kind: None,
            display_source: registry
                .as_ref()
                .map(|registry| {
                    let package = package.as_deref().unwrap_or(name);
                    if package == name {
                        format!("registry:{package} from {registry}")
                    } else {
                        format!("registry:{package} as {name} from {registry}")
                    }
                })
                .unwrap_or(display_source),
        };

        if spec.registry.is_some() {
            install_registry_dependency(project_root, &mut spec, locked.as_ref())?;
        } else if spec.git.is_some() {
            let package_dir =
                project_dependency_path(project_root, &spec.path, "git package cache")?;
            if !package_dir.is_dir() {
                spec.commit = Some(fetch_git_dependency(project_root, &spec)?);
            } else if let Some(commit) = spec.commit.as_deref() {
                let actual = current_git_commit(&package_dir)?;
                if actual != commit {
                    bail!(
                        "package cache for {} is at {actual}, expected locked commit {commit}",
                        spec.name
                    );
                }
            } else {
                spec.commit = Some(current_git_commit(&package_dir)?);
            }
            spec.package_version = package_version_for_spec(project_root, &spec)?;
            spec.integrity = Some(package_integrity(project_root, &spec)?);
        } else {
            let dependency_dir = PathBuf::from(&spec.path);
            let dependency_dir = if dependency_dir.is_absolute() {
                dependency_dir
            } else {
                project_root.join(dependency_dir)
            };
            ensure_existing_project_dir(project_root, &dependency_dir, "local dependency")?;
            if !dependency_dir.is_dir() {
                bail!(
                    "local Ricochet dependency {name} is not a directory: {}",
                    dependency_dir.display()
                );
            }
            spec.package_version = package_version_for_spec(project_root, &spec)?;
            spec.integrity = Some(package_integrity(project_root, &spec)?);
        }

        write_lockfile(&lock_path, &spec)?;
        println!("installed {} from {}", spec.name, spec.display_source);
        installed += 1;
    }

    if installed == 0 {
        println!("no dependencies to install");
    }
    Ok(())
}

fn verify_dependencies(path: Option<&str>) -> Result<()> {
    let manifest_path = project_manifest_path_for_command("verify", path)?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let verified = verify_dependency_manifest(project_root, &manifest_path, &doc, true)?;
    println!("verified {verified} dependencies");
    Ok(())
}

#[derive(Debug, Serialize)]
struct DependencyAuditReport {
    manifest: String,
    ok: bool,
    dependencies: Vec<DependencyAuditEntry>,
    stale_locks: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DependencyAuditEntry {
    name: String,
    kind: String,
    source: String,
    path: String,
    version_req: Option<String>,
    locked_version: Option<String>,
    locked_integrity: Option<String>,
    locked_provenance: Option<String>,
    locked_signature: Option<String>,
    locked_signature_kind: Option<String>,
    status: String,
    issues: Vec<String>,
}

fn audit_dependencies(path: Option<&str>, json_output: bool) -> Result<()> {
    let manifest_path = project_manifest_path_for_command("audit", path)?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let report = dependency_audit_report(project_root, &manifest_path, &doc)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_dependency_audit_report(&report);
    }
    if !report.ok {
        bail!("dependency audit found issues");
    }
    Ok(())
}

fn dependency_audit_report(
    project_root: &Path,
    manifest_path: &Path,
    doc: &DocumentMut,
) -> Result<DependencyAuditReport> {
    let lock_path = project_root.join("ricochet.lock");
    let lock_doc = read_optional_toml_document(&lock_path)?;
    let mut entries = Vec::new();
    let mut declared = BTreeSet::new();

    if let Some(dependencies) = doc.get("dependencies").and_then(Item::as_table) {
        for (name, item) in dependencies.iter() {
            declared.insert(name.to_string());
            let spec = dependency_spec_from_manifest_table(name, item, manifest_path)?;
            let locked = locked_package(lock_doc.as_ref(), name)?;
            let mut issues = Vec::new();
            if let Err(error) =
                verify_dependency(project_root, &lock_path, lock_doc.as_ref(), &spec)
            {
                issues.push(error.to_string());
            }
            let (
                locked_version,
                locked_integrity,
                locked_provenance,
                locked_signature,
                locked_signature_kind,
            ) = locked
                .as_ref()
                .map(|lock| {
                    (
                        lock.package_version.clone(),
                        lock.integrity.clone(),
                        lock.provenance.clone(),
                        lock.signature.clone(),
                        lock.signature_kind.clone(),
                    )
                })
                .unwrap_or((None, None, None, None, None));
            entries.push(DependencyAuditEntry {
                name: name.to_string(),
                kind: dependency_kind(&spec).to_string(),
                source: spec.source,
                path: spec.path,
                version_req: spec.version_req,
                locked_version,
                locked_integrity,
                locked_provenance,
                locked_signature,
                locked_signature_kind,
                status: if issues.is_empty() {
                    "ok".to_string()
                } else {
                    "problem".to_string()
                },
                issues,
            });
        }
    }

    let stale_locks = stale_lock_packages(lock_doc.as_ref(), &declared);
    let ok = entries.iter().all(|entry| entry.issues.is_empty()) && stale_locks.is_empty();
    Ok(DependencyAuditReport {
        manifest: path_to_slash(manifest_path),
        ok,
        dependencies: entries,
        stale_locks,
    })
}

fn print_dependency_audit_report(report: &DependencyAuditReport) {
    println!("Dependency audit for {}", report.manifest);
    if report.dependencies.is_empty() {
        println!("no dependencies declared");
    }
    for entry in &report.dependencies {
        println!("- {} [{}] {}", entry.name, entry.kind, entry.status);
        println!("  source: {}", entry.source);
        println!("  path: {}", entry.path);
        if let Some(version_req) = &entry.version_req {
            let locked = entry.locked_version.as_deref().unwrap_or("<unlocked>");
            println!("  version: {version_req} -> {locked}");
        } else if let Some(locked) = &entry.locked_version {
            println!("  version: {locked}");
        }
        if let Some(integrity) = &entry.locked_integrity {
            println!("  integrity: {integrity}");
        }
        if let Some(provenance) = &entry.locked_provenance {
            println!("  provenance: {provenance}");
        }
        if let Some(signature) = &entry.locked_signature {
            let kind = entry.locked_signature_kind.as_deref().unwrap_or("detached");
            println!("  signature: {kind} {signature}");
        }
        for issue in &entry.issues {
            println!("  issue: {issue}");
        }
    }
    if !report.stale_locks.is_empty() {
        println!("stale lock entries:");
        for name in &report.stale_locks {
            println!("- {name}");
        }
    }
    if report.ok {
        println!("dependency audit passed");
    }
}

fn verify_dependency_manifest(
    project_root: &Path,
    manifest_path: &Path,
    doc: &DocumentMut,
    verbose: bool,
) -> Result<usize> {
    let lock_path = project_root.join("ricochet.lock");
    let lock_doc = read_optional_toml_document(&lock_path)?;
    let Some(dependencies) = doc.get("dependencies").and_then(Item::as_table) else {
        verify_no_stale_lock_packages(&lock_path, lock_doc.as_ref(), &BTreeSet::new())?;
        return Ok(0);
    };

    let mut declared = BTreeSet::new();
    let mut verified = 0usize;
    for (name, item) in dependencies.iter() {
        validate_package_name(name)?;
        declared.insert(name.to_string());
        let spec = dependency_spec_from_manifest_table(name, item, manifest_path)?;
        verify_dependency(project_root, &lock_path, lock_doc.as_ref(), &spec)?;
        if verbose {
            println!("verified {}", spec.name);
        }
        verified += 1;
    }

    verify_no_stale_lock_packages(&lock_path, lock_doc.as_ref(), &declared)?;
    Ok(verified)
}

fn dependency_spec_from_manifest_table(
    name: &str,
    item: &Item,
    manifest_path: &Path,
) -> Result<DependencySpec> {
    validate_package_name(name)?;
    let table = item.as_table().with_context(|| {
        format!(
            "dependency {name} in {} must be a table",
            manifest_path.display()
        )
    })?;
    let git = table.get("git").and_then(Item::as_str).map(str::to_string);
    let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
    let registry = table
        .get("registry")
        .and_then(Item::as_str)
        .map(str::to_string);
    let package = table
        .get("package")
        .and_then(Item::as_str)
        .map(str::to_string);
    if git.is_some() && registry.is_some() {
        bail!("dependency {name} cannot include both git and registry");
    }
    if package.is_some() && registry.is_none() {
        bail!("dependency {name} can only include package with a registry");
    }
    if let Some(package) = package.as_deref() {
        validate_registry_package_name(package)?;
    }
    let path = dependency_manifest_path(name, table, registry.is_some(), manifest_path)?;
    let version_req = table
        .get("version")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(version_req) = version_req.as_deref() {
        validate_version_req(version_req)?;
    }
    Ok(DependencySpec {
        name: name.to_string(),
        package: package.clone().filter(|package| package != name),
        source: git
            .as_ref()
            .map(|git| format!("git+{git}"))
            .or_else(|| {
                registry.as_ref().map(|registry| {
                    format!("registry+{registry}#{}", package.as_deref().unwrap_or(name))
                })
            })
            .unwrap_or_else(|| format!("path+{path}")),
        path,
        git,
        rev,
        commit: None,
        registry,
        version_req,
        package_version: None,
        integrity: None,
        provenance: None,
        signature: None,
        signature_kind: None,
        display_source: String::new(),
    })
}

fn dependency_kind(spec: &DependencySpec) -> &'static str {
    if spec.registry.is_some() {
        "registry"
    } else if spec.git.is_some() {
        "git"
    } else {
        "path"
    }
}

fn verify_dependency(
    project_root: &Path,
    lock_path: &Path,
    lock_doc: Option<&DocumentMut>,
    spec: &DependencySpec,
) -> Result<()> {
    let Some(lock) = locked_package(lock_doc, &spec.name)? else {
        bail!(
            "dependency {} is missing from {}; run rco install",
            spec.name,
            lock_path.display()
        );
    };
    if lock.path != spec.path {
        bail!(
            "lock entry for {} has path {:?}, expected {:?}",
            spec.name,
            lock.path,
            spec.path
        );
    }
    if lock.source != spec.source {
        bail!(
            "lock entry for {} has source {:?}, expected {:?}",
            spec.name,
            lock.source,
            spec.source
        );
    }
    if lock.package != spec.package {
        bail!(
            "lock entry for {} has package {:?}, expected {:?}",
            spec.name,
            lock.package,
            spec.package
        );
    }
    if lock.version_req != spec.version_req {
        bail!(
            "lock entry for {} has version requirement {:?}, expected {:?}",
            spec.name,
            lock.version_req,
            spec.version_req
        );
    }

    if let Some(registry) = spec.registry.as_deref() {
        if lock.registry.as_deref() != Some(registry) {
            bail!(
                "lock entry for {} has registry {:?}, expected {:?}",
                spec.name,
                lock.registry,
                registry
            );
        }
        if lock.git.is_some() || lock.rev.is_some() || lock.commit.is_some() {
            bail!(
                "lock entry for {} is git-shaped, but manifest uses a registry",
                spec.name
            );
        }
        verify_package_version(project_root, spec, &lock)?;
        verify_package_integrity(project_root, spec, &lock)?;
    } else if let Some(git) = spec.git.as_deref() {
        if lock.git.as_deref() != Some(git) {
            bail!(
                "lock entry for {} has git {:?}, expected {:?}",
                spec.name,
                lock.git,
                git
            );
        }
        if lock.rev.as_deref() != spec.rev.as_deref() {
            bail!(
                "lock entry for {} has rev {:?}, expected {:?}",
                spec.name,
                lock.rev,
                spec.rev
            );
        }
        let commit = lock.commit.as_deref().with_context(|| {
            format!(
                "git dependency {} is not pinned; run rco install",
                spec.name
            )
        })?;
        validate_git_commit(commit)?;
        let package_dir = project_dependency_path(project_root, &spec.path, "git package cache")?;
        if !package_dir.is_dir() {
            bail!(
                "package cache for {} is missing: {}; run rco install",
                spec.name,
                package_dir.display()
            );
        }
        let actual = current_git_commit(&package_dir)?;
        if actual != commit {
            bail!(
                "package cache for {} is at {actual}, expected locked commit {commit}",
                spec.name
            );
        }
        verify_package_version(project_root, spec, &lock)?;
        verify_package_integrity(project_root, spec, &lock)?;
    } else {
        if lock.git.is_some() || lock.commit.is_some() || lock.registry.is_some() {
            bail!(
                "lock entry for {} is remote-shaped, but manifest uses a local path",
                spec.name
            );
        }
        let dependency_dir = resolve_local_dependency_dir(project_root, &spec.path)?;
        if !dependency_dir.is_dir() {
            bail!(
                "local Ricochet dependency {} is not a directory: {}",
                spec.name,
                dependency_dir.display()
            );
        }
        verify_package_version(project_root, spec, &lock)?;
        verify_package_integrity(project_root, spec, &lock)?;
    }

    Ok(())
}

fn verify_no_stale_lock_packages(
    lock_path: &Path,
    lock_doc: Option<&DocumentMut>,
    declared: &BTreeSet<String>,
) -> Result<()> {
    let stale = stale_lock_packages(lock_doc, declared);
    if let Some(name) = stale.first() {
        bail!(
            "{} contains package {name:?}, but ricochet.toml does not declare it",
            lock_path.display()
        );
    }

    Ok(())
}

fn stale_lock_packages(lock_doc: Option<&DocumentMut>, declared: &BTreeSet<String>) -> Vec<String> {
    let Some(packages) = lock_doc
        .and_then(|doc| doc.get("package"))
        .and_then(Item::as_table)
    else {
        return Vec::new();
    };

    let mut stale = Vec::new();
    for (name, _) in packages.iter() {
        if !declared.contains(name) {
            stale.push(name.to_string());
        }
    }
    stale
}

fn read_optional_toml_document(path: &Path) -> Result<Option<DocumentMut>> {
    if !path.is_file() {
        return Ok(None);
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    source
        .parse::<DocumentMut>()
        .map(Some)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn locked_package(lock_doc: Option<&DocumentMut>, name: &str) -> Result<Option<LockedPackage>> {
    let Some(table) = lock_doc
        .and_then(|doc| doc.get("package"))
        .and_then(Item::as_table)
        .and_then(|packages| packages.get(name))
        .and_then(Item::as_table)
    else {
        return Ok(None);
    };

    let source = table
        .get("source")
        .and_then(Item::as_str)
        .with_context(|| format!("lock entry for {name} must include a string source"))?
        .to_string();
    let package = table
        .get("package")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(package) = package.as_deref() {
        validate_registry_package_name(package)?;
    }
    let path = table
        .get("path")
        .and_then(Item::as_str)
        .with_context(|| format!("lock entry for {name} must include a string path"))?
        .to_string();
    let git = table.get("git").and_then(Item::as_str).map(str::to_string);
    let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
    let commit = table
        .get("commit")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(commit) = commit.as_deref() {
        validate_git_commit(commit)?;
    }
    let registry = table
        .get("registry")
        .and_then(Item::as_str)
        .map(str::to_string);
    let version_req = table
        .get("version_req")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(version_req) = version_req.as_deref() {
        validate_version_req(version_req)?;
    }
    let package_version = table
        .get("version")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(package_version) = package_version.as_deref() {
        validate_package_version(package_version)?;
    }
    let integrity = table
        .get("integrity")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(integrity) = integrity.as_deref() {
        validate_package_integrity(integrity)?;
    }
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
        bail!("lock entry for {name} has signature_kind without signature");
    }

    Ok(Some(LockedPackage {
        source,
        package,
        path,
        git,
        rev,
        commit,
        registry,
        version_req,
        package_version,
        integrity,
        provenance,
        signature,
        signature_kind,
    }))
}

fn project_manifest_path_for_command(command: &str, path: Option<&str>) -> Result<PathBuf> {
    let Some(path) = path else {
        return find_project_manifest_for_current_dir(command);
    };
    let path = Path::new(path);
    let manifest_path = if path.file_name().is_some_and(|name| name == "ricochet.toml") {
        path.to_path_buf()
    } else {
        path.join("ricochet.toml")
    };
    if manifest_path.is_file() {
        Ok(manifest_path)
    } else {
        bail!(
            "rco {command} expected a Ricochet project path with ricochet.toml: {}",
            path.display()
        );
    }
}

fn find_project_manifest_for_current_dir(command: &str) -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    for ancestor in current_dir.ancestors() {
        let manifest_path = ancestor.join("ricochet.toml");
        if manifest_path.is_file() {
            return Ok(manifest_path);
        }
    }

    bail!("rco {command} must be run inside a Ricochet project with ricochet.toml");
}

fn parse_dependency_source(
    source: &str,
    registry: Option<&Path>,
    registry_url: Option<&str>,
) -> Result<DependencySource> {
    if let Some(rest) = source.strip_prefix("registry:") {
        if registry.is_some() && registry_url.is_some() {
            bail!("use either --registry or --registry-url, not both");
        }
        let (package, version) = split_registry_package_version(rest)?;
        validate_registry_package_name(package)?;
        if let Some(version) = version.as_deref() {
            validate_package_version(version)?;
        }
        let registry = match registry_url {
            Some(registry_url) => validate_static_registry_url(registry_url)?.to_string(),
            None => registry_path_value(registry_source_path(registry)?.as_path(), false)?,
        };
        return Ok(DependencySource::Registry {
            package: package.to_string(),
            version,
            registry,
        });
    }

    if registry.is_some() {
        bail!("--registry can only be used with registry:name dependencies");
    }
    if registry_url.is_some() {
        bail!("--registry-url can only be used with registry:name dependencies");
    }

    if let Some(rest) = source.strip_prefix("github:") {
        let (repository, rev) = rest
            .split_once('@')
            .map(|(repository, rev)| (repository, Some(rev.to_string())))
            .unwrap_or((rest, None));
        let (owner, repo) = repository.split_once('/').with_context(|| {
            format!("invalid GitHub dependency {source:?}; expected github:owner/repo@ref")
        })?;
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            bail!("invalid GitHub dependency {source:?}; expected github:owner/repo@ref");
        }
        return Ok(DependencySource::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            rev,
        });
    }

    Ok(DependencySource::Local {
        path: PathBuf::from(source),
    })
}

fn registry_source_path(registry: Option<&Path>) -> Result<PathBuf> {
    if let Some(registry) = registry {
        return Ok(registry.to_path_buf());
    }
    let value = std::env::var("RICOCHET_REGISTRY")
        .context("registry dependencies require --registry PATH or RICOCHET_REGISTRY")?;
    Ok(PathBuf::from(value))
}

fn split_registry_package_version(source: &str) -> Result<(&str, Option<String>)> {
    if source.is_empty() {
        bail!("registry dependency package name must not be empty");
    }
    let version_separator = if let Some(rest) = source.strip_prefix('@') {
        rest.rfind('@').map(|index| index + 1)
    } else {
        source.rfind('@')
    };
    if let Some(index) = version_separator {
        let package = &source[..index];
        let version = &source[index + 1..];
        if version.is_empty() {
            bail!("registry dependency version must not be empty");
        }
        Ok((package, Some(version.to_string())))
    } else {
        Ok((source, None))
    }
}

fn registry_path_value(registry: &Path, create: bool) -> Result<String> {
    let registry = absolute_path_from_current(registry)?;
    if create {
        fs::create_dir_all(&registry)
            .with_context(|| format!("failed to create {}", registry.display()))?;
    }
    if !registry.is_dir() {
        bail!(
            "package registry is not a directory: {}",
            registry.display()
        );
    }
    let registry = fs::canonicalize(&registry)
        .with_context(|| format!("failed to resolve {}", registry.display()))?;
    Ok(path_to_slash(&registry))
}

fn absolute_path_from_current(path: &Path) -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    })
}

fn dependency_manifest_path(
    name: &str,
    table: &Table,
    is_registry: bool,
    manifest_path: &Path,
) -> Result<String> {
    if let Some(path) = table.get("path").and_then(Item::as_str) {
        return Ok(path.to_string());
    }
    if is_registry {
        return Ok(format!(".ricochet/packages/{name}"));
    }
    bail!(
        "dependency {name} in {} must include a string path",
        manifest_path.display()
    )
}

fn dependency_spec(
    project_root: &Path,
    original_source: &str,
    source: DependencySource,
    name_override: Option<&str>,
    version_req: Option<&str>,
) -> Result<DependencySpec> {
    let version_req = version_req
        .map(validate_version_req)
        .transpose()?
        .map(str::to_string);
    match source {
        DependencySource::Local { path } => {
            let current_dir =
                std::env::current_dir().context("failed to determine current directory")?;
            let absolute_path = if path.is_absolute() {
                path
            } else {
                current_dir.join(path)
            };
            if !absolute_path.is_dir() {
                bail!(
                    "local Ricochet dependency is not a directory: {}",
                    absolute_path.display()
                );
            }

            let metadata = read_package_metadata(&absolute_path)?;
            let name = match name_override {
                Some(name) => name.to_string(),
                None => metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| directory_package_name(&absolute_path)),
            };
            validate_package_name(&name)?;
            validate_package_version_requirement(
                &name,
                version_req.as_deref(),
                metadata.version.as_deref(),
            )?;
            let path = dependency_path_value(project_root, &absolute_path, original_source)?;

            Ok(DependencySpec {
                name,
                package: None,
                path: path.clone(),
                source: format!("path+{path}"),
                git: None,
                rev: None,
                commit: None,
                registry: None,
                version_req,
                package_version: metadata.version,
                integrity: Some(package_tree_integrity(&absolute_path)?),
                provenance: None,
                signature: None,
                signature_kind: None,
                display_source: path,
            })
        }
        DependencySource::GitHub { owner, repo, rev } => {
            let name = name_override.unwrap_or(&repo).to_string();
            validate_package_name(&name)?;
            let git = format!("https://github.com/{owner}/{repo}.git");
            let path = format!(".ricochet/packages/{name}");

            Ok(DependencySpec {
                name,
                package: None,
                path,
                source: format!("git+{git}"),
                git: Some(git),
                rev,
                commit: None,
                registry: None,
                version_req,
                package_version: None,
                integrity: None,
                provenance: None,
                signature: None,
                signature_kind: None,
                display_source: original_source.to_string(),
            })
        }
        DependencySource::Registry {
            package,
            version,
            registry,
        } => {
            validate_registry_package_name(&package)?;
            let name = match name_override {
                Some(name_override) => name_override.to_string(),
                None => default_dependency_alias(&package).to_string(),
            };
            validate_package_name(&name)?;
            let version_req = match (version_req, version) {
                (Some(_), Some(_)) => {
                    bail!("use either registry:name@version or --version REQ, not both")
                }
                (Some(version_req), None) => Some(version_req),
                (None, Some(version)) => Some(format!("={version}")),
                (None, None) => None,
            };
            let path = format!(".ricochet/packages/{name}");
            let package_field = if package == name {
                None
            } else {
                Some(package.clone())
            };
            Ok(DependencySpec {
                name: name.clone(),
                package: package_field,
                path,
                source: format!("registry+{registry}#{package}"),
                git: None,
                rev: None,
                commit: None,
                registry: Some(registry.clone()),
                version_req,
                package_version: None,
                integrity: None,
                provenance: None,
                signature: None,
                signature_kind: None,
                display_source: if package == name {
                    format!("registry:{package} from {registry}")
                } else {
                    format!("registry:{package} as {name} from {registry}")
                },
            })
        }
    }
}

#[derive(Debug, Default)]
struct PackageMetadata {
    name: Option<String>,
    version: Option<String>,
}

fn read_package_metadata(path: &Path) -> Result<PackageMetadata> {
    let manifest_path = path.join("ricochet.toml");
    if !manifest_path.is_file() {
        return Ok(PackageMetadata::default());
    }

    let source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let package = doc.get("package").and_then(Item::as_table);
    let version = package
        .and_then(|package| package.get("version"))
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(version) = version.as_deref() {
        validate_package_version(version)?;
    }
    Ok(PackageMetadata {
        name: package
            .and_then(|package| package.get("name"))
            .and_then(Item::as_str)
            .map(str::to_string),
        version,
    })
}

fn directory_package_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package")
        .to_string()
}

fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!("invalid Ricochet package name {name:?}; use letters, numbers, _ or -");
    }

    Ok(())
}

fn validate_registry_package_name(name: &str) -> Result<()> {
    if let Some(rest) = name.strip_prefix('@') {
        let (scope, package) = rest.split_once('/').with_context(|| {
            format!("invalid scoped Ricochet package name {name:?}; expected @scope/name")
        })?;
        if package.contains('/') {
            bail!("invalid scoped Ricochet package name {name:?}; expected @scope/name");
        }
        validate_registry_package_segment(scope, "scope", name)?;
        validate_registry_package_segment(package, "package", name)?;
        return Ok(());
    }

    validate_package_name(name)
}

fn validate_registry_package_segment(segment: &str, label: &str, package: &str) -> Result<()> {
    if segment.is_empty()
        || !segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        bail!("invalid {label} in Ricochet package name {package:?}; use letters, numbers, _ or -");
    }
    Ok(())
}

fn default_dependency_alias(package: &str) -> &str {
    package.rsplit('/').next().unwrap_or(package)
}

fn registry_package_relative_path(package: &str) -> PathBuf {
    package.split('/').collect()
}

fn registry_package_metadata_relative_path(package: &str) -> PathBuf {
    PathBuf::from("packages")
        .join(registry_package_relative_path(package))
        .with_extension("toml")
}

fn registry_package_archive_relative_path(package: &str, version: &str) -> PathBuf {
    let leaf = default_dependency_alias(package);
    PathBuf::from("artifacts")
        .join(registry_package_relative_path(package))
        .join(version)
        .join(format!("{leaf}-{version}.tar.gz"))
}

fn validate_static_registry_url(registry_url: &str) -> Result<&str> {
    if registry_url.starts_with("https://")
        || registry_url.starts_with("http://")
        || registry_url.starts_with("file://")
    {
        Ok(registry_url)
    } else {
        bail!("static registry URL {registry_url:?} must start with https://, http://, or file://");
    }
}

fn dependency_path_value(
    project_root: &Path,
    absolute_path: &Path,
    fallback: &str,
) -> Result<String> {
    let canonical_root = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve {}", project_root.display()))?;
    let canonical_path = fs::canonicalize(absolute_path)
        .with_context(|| format!("failed to resolve {}", absolute_path.display()))?;
    if let Ok(relative_path) = canonical_path.strip_prefix(&canonical_root) {
        let relative = path_to_slash(relative_path);
        if relative.is_empty() {
            bail!("local dependency cannot point at the project root");
        }
        return Ok(format!("./{relative}"));
    }

    let _ = fallback;
    bail!(
        "local dependency must be inside the project root: {}",
        absolute_path.display()
    )
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn fetch_git_dependency(project_root: &Path, spec: &DependencySpec) -> Result<String> {
    let git = spec
        .git
        .as_deref()
        .expect("fetch_git_dependency only handles git dependencies");
    let package_dir = project_dependency_path(project_root, &spec.path, "git package cache")?;
    if package_dir.exists() {
        bail!(
            "package cache already exists: {}; remove it or choose a different --name",
            package_dir.display()
        );
    }

    if let Some(parent) = package_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        ensure_existing_project_dir(project_root, parent, "package cache parent")?;
    }

    let mut command = std::process::Command::new("git");
    command.arg("clone");
    if spec.commit.is_none() {
        command.arg("--depth").arg("1");
        if let Some(rev) = spec.rev.as_deref() {
            command.arg("--branch").arg(rev);
        }
    }
    command.arg(git).arg(&package_dir);

    let output = command
        .output()
        .with_context(|| format!("failed to launch git to fetch {}", spec.display_source))?;
    if !output.status.success() {
        bail!(
            "failed to fetch Ricochet package {}\nstdout:\n{}\nstderr:\n{}",
            spec.display_source,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    ensure_existing_project_dir(project_root, &package_dir, "git package cache")?;
    if let Some(commit) = spec.commit.as_deref() {
        checkout_git_commit(&package_dir, commit)?;
    }

    current_git_commit(&package_dir)
}

fn project_dependency_path(project_root: &Path, path: &str, description: &str) -> Result<PathBuf> {
    validate_project_relative_path(path, description)?;
    let candidate = project_root.join(Path::new(path));
    ensure_contained_candidate(project_root, &candidate, description)?;
    Ok(candidate)
}

fn resolve_local_dependency_dir(project_root: &Path, path: &str) -> Result<PathBuf> {
    let dependency_dir = PathBuf::from(path);
    let dependency_dir = if dependency_dir.is_absolute() {
        dependency_dir
    } else {
        project_root.join(dependency_dir)
    };
    ensure_existing_project_dir(project_root, &dependency_dir, "local dependency")?;
    Ok(dependency_dir)
}

fn install_registry_dependency(
    project_root: &Path,
    spec: &mut DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<()> {
    let registry = spec
        .registry
        .as_deref()
        .expect("install_registry_dependency only handles registry dependencies");
    if is_static_registry_source(registry) {
        return install_static_registry_dependency(project_root, spec, locked);
    }

    let package = resolve_registry_package(project_root, spec, locked)?;
    let package_cache =
        project_dependency_path(project_root, &spec.path, "registry package cache")?;
    if package_cache.exists() {
        let cached_integrity = package_tree_integrity(&package_cache)?;
        if cached_integrity != package.integrity {
            bail!(
                "registry package cache for {} already exists with integrity {cached_integrity}, expected {}; remove {} or choose a different dependency name",
                spec.name,
                package.integrity,
                package_cache.display()
            );
        }
    } else {
        if let Some(parent) = package_cache.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
            ensure_existing_project_dir(project_root, parent, "registry package cache parent")?;
        }
        copy_package_tree(&package.package_dir, &package_cache)?;
    }
    spec.package_version = Some(package.version);
    spec.integrity = Some(package.integrity);
    spec.provenance = package.provenance;
    spec.signature = package.signature;
    spec.signature_kind = package.signature_kind;
    Ok(())
}

fn install_static_registry_dependency(
    project_root: &Path,
    spec: &mut DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<()> {
    let registry = spec
        .registry
        .as_deref()
        .expect("install_static_registry_dependency only handles registry dependencies");
    let index = load_static_registry_index(registry)?;
    let package_name = spec.registry_package_name();
    let metadata_path = index
        .packages
        .get(package_name)
        .with_context(|| format!("static registry does not contain package {package_name}"))?;
    let metadata = load_static_registry_package(&index.source, package_name, metadata_path)?;
    let version = static_registry_version(&metadata, spec, locked)?;
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
            ensure_existing_project_dir(
                project_root,
                parent,
                "static registry package cache parent",
            )?;
        }
        let archive_source = resolve_static_registry_resource(&index.source, &version.archive)?;
        let archive_bytes =
            read_static_registry_bytes(&archive_source, MAX_STATIC_REGISTRY_ARCHIVE_BYTES)?;
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

fn is_static_registry_source(registry: &str) -> bool {
    registry.starts_with("http://")
        || registry.starts_with("https://")
        || registry.starts_with("file://")
}

fn resolve_registry_package(
    project_root: &Path,
    spec: &DependencySpec,
    locked: Option<&LockedPackage>,
) -> Result<RegistryPackage> {
    let registry = spec
        .registry
        .as_deref()
        .expect("resolve_registry_package only handles registry dependencies");
    let registry_root = resolve_registry_root(project_root, registry)?;
    let package_name = spec.registry_package_name();
    let package_root = registry_root.join(registry_package_relative_path(package_name));
    if !package_root.is_dir() {
        bail!(
            "registry {} does not contain package {}",
            registry_root.display(),
            package_name
        );
    }

    if let Some(locked_version) = locked.and_then(|lock| lock.package_version.as_deref()) {
        if package_version_satisfies(spec.version_req.as_deref(), locked_version)? {
            return registry_package_at(&package_root, package_name, locked_version);
        }
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(&package_root)
        .with_context(|| format!("failed to read {}", package_root.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", package_root.display()))?;
        if !entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let version_text = entry.file_name().to_string_lossy().to_string();
        let version = Version::parse(&version_text).with_context(|| {
            format!(
                "registry package {} has invalid version directory {:?}",
                package_name, version_text
            )
        })?;
        if package_version_satisfies(spec.version_req.as_deref(), &version_text)? {
            candidates.push((version, version_text));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    let Some((_, version)) = candidates.pop() else {
        let requirement = spec.version_req.as_deref().unwrap_or("*");
        bail!(
            "registry package {} has no version satisfying {}",
            package_name,
            requirement
        );
    };

    registry_package_at(&package_root, package_name, &version)
}

fn registry_package_at(
    package_root: &Path,
    package_name: &str,
    version: &str,
) -> Result<RegistryPackage> {
    validate_package_version(version)?;
    let version_root = package_root.join(version);
    let package_dir = version_root.join("package");
    if !package_dir.is_dir() {
        bail!(
            "registry package {} {} is missing package directory: {}",
            package_name,
            version,
            package_dir.display()
        );
    }
    let metadata = read_package_metadata(&package_dir)?;
    if metadata.name.as_deref() != Some(package_name) {
        bail!(
            "registry package {} {} has manifest package name {:?}",
            package_name,
            version,
            metadata.name
        );
    }
    if metadata.version.as_deref() != Some(version) {
        bail!(
            "registry package {} {} has manifest version {:?}",
            package_name,
            version,
            metadata.version
        );
    }
    let integrity = package_tree_integrity(&package_dir)?;
    let metadata_path = version_root.join("metadata.toml");
    let mut provenance = None;
    let mut signature = None;
    let mut signature_kind = None;
    if metadata_path.is_file() {
        let registry_metadata = fs::read_to_string(&metadata_path)
            .with_context(|| format!("failed to read {}", metadata_path.display()))?;
        let registry_metadata = registry_metadata
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", metadata_path.display()))?;
        if let Some(recorded_integrity) = registry_metadata["package"]["integrity"].as_str() {
            validate_package_integrity(recorded_integrity)?;
            if recorded_integrity != integrity {
                bail!(
                    "registry package {} {} integrity metadata is {}, but package hashes to {}",
                    package_name,
                    version,
                    recorded_integrity,
                    integrity
                );
            }
        }
        if let Some(provenance_table) = registry_metadata.get("provenance").and_then(Item::as_table)
        {
            provenance = registry_artifact_integrity(
                &version_root,
                package_name,
                version,
                provenance_table,
                "attestation",
                "attestation_integrity",
            )?;
            signature = registry_artifact_integrity(
                &version_root,
                package_name,
                version,
                provenance_table,
                "signature",
                "signature_integrity",
            )?;
            signature_kind = provenance_table
                .get("signature_kind")
                .and_then(Item::as_str)
                .map(str::to_string);
            if let Some(signature_kind) = signature_kind.as_deref() {
                validate_signature_kind(signature_kind)?;
            }
            if signature_kind.is_some() && signature.is_none() {
                bail!(
                    "registry package {} {} has signature_kind without signature metadata",
                    package_name,
                    version
                );
            }
        }
    }
    Ok(RegistryPackage {
        package_dir,
        version: version.to_string(),
        integrity,
        provenance,
        signature,
        signature_kind,
    })
}

fn registry_artifact_integrity(
    version_root: &Path,
    package_name: &str,
    version: &str,
    table: &Table,
    path_key: &str,
    integrity_key: &str,
) -> Result<Option<String>> {
    let path = table.get(path_key).and_then(Item::as_str);
    let integrity = table.get(integrity_key).and_then(Item::as_str);
    match (path, integrity) {
        (None, None) => Ok(None),
        (Some(_), None) => bail!(
            "registry package {} {} has provenance {} without {}",
            package_name,
            version,
            path_key,
            integrity_key
        ),
        (None, Some(_)) => bail!(
            "registry package {} {} has provenance {} without {}",
            package_name,
            version,
            integrity_key,
            path_key
        ),
        (Some(path), Some(expected)) => {
            validate_project_relative_path(path, "registry provenance artifact")?;
            validate_package_integrity(expected)?;
            let artifact_path = version_root.join(path);
            let metadata = fs::symlink_metadata(&artifact_path)
                .with_context(|| format!("failed to inspect {}", artifact_path.display()))?;
            if metadata.file_type().is_symlink() {
                bail!(
                    "registry package {} {} provenance artifact is a symlink: {}",
                    package_name,
                    version,
                    artifact_path.display()
                );
            }
            if !metadata.is_file() {
                bail!(
                    "registry package {} {} provenance artifact is missing: {}",
                    package_name,
                    version,
                    artifact_path.display()
                );
            }
            let actual = file_integrity(&artifact_path)?;
            if actual != expected {
                bail!(
                    "registry package {} {} provenance artifact {} integrity is {}, but file hashes to {}",
                    package_name,
                    version,
                    path,
                    expected,
                    actual
                );
            }
            Ok(Some(expected.to_string()))
        }
    }
}

fn resolve_registry_root(project_root: &Path, registry: &str) -> Result<PathBuf> {
    let registry = PathBuf::from(registry);
    let registry = if registry.is_absolute() {
        registry
    } else {
        project_root.join(registry)
    };
    if !registry.is_dir() {
        bail!(
            "package registry is not a directory: {}",
            registry.display()
        );
    }
    Ok(registry)
}

fn package_version_satisfies(version_req: Option<&str>, package_version: &str) -> Result<bool> {
    validate_package_version(package_version)?;
    let Some(version_req) = version_req else {
        return Ok(true);
    };
    let requirement = VersionReq::parse(version_req).with_context(|| {
        format!("invalid dependency version requirement {version_req:?}; expected semver syntax")
    })?;
    let version = Version::parse(package_version)
        .with_context(|| format!("invalid package version {package_version:?}"))?;
    Ok(requirement.matches(&version))
}

fn package_integrity(project_root: &Path, spec: &DependencySpec) -> Result<String> {
    let package_dir = if spec.git.is_some() || spec.registry.is_some() {
        project_dependency_path(project_root, &spec.path, "package cache")?
    } else {
        resolve_local_dependency_dir(project_root, &spec.path)?
    };
    package_tree_integrity(&package_dir)
}

fn package_version_for_spec(project_root: &Path, spec: &DependencySpec) -> Result<Option<String>> {
    let package_dir = if spec.git.is_some() || spec.registry.is_some() {
        project_dependency_path(project_root, &spec.path, "package cache")?
    } else {
        resolve_local_dependency_dir(project_root, &spec.path)?
    };
    let metadata = read_package_metadata(&package_dir)?;
    validate_package_version_requirement(
        &spec.name,
        spec.version_req.as_deref(),
        metadata.version.as_deref(),
    )?;
    Ok(metadata.version)
}

fn verify_package_version(
    project_root: &Path,
    spec: &DependencySpec,
    lock: &LockedPackage,
) -> Result<()> {
    let actual = package_version_for_spec(project_root, spec)?;
    if lock.package_version != actual {
        bail!(
            "package version for {} changed: lock has {:?}, package has {:?}; run rco install if this update is intentional",
            spec.name,
            lock.package_version,
            actual
        );
    }
    validate_package_version_requirement(&spec.name, spec.version_req.as_deref(), actual.as_deref())
}

fn verify_package_integrity(
    project_root: &Path,
    spec: &DependencySpec,
    lock: &LockedPackage,
) -> Result<()> {
    let expected = lock.integrity.as_deref().with_context(|| {
        format!(
            "lock entry for {} is missing package integrity; run rco install",
            spec.name
        )
    })?;
    validate_package_integrity(expected)?;
    let actual = package_integrity(project_root, spec)?;
    if actual != expected {
        bail!(
            "package integrity for {} changed: expected {expected}, got {actual}; run rco install if this update is intentional",
            spec.name
        );
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

fn copy_package_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() {
        bail!("package source is not a directory: {}", source.display());
    }
    if destination.exists() {
        bail!(
            "package destination already exists: {}",
            destination.display()
        );
    }
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    copy_package_tree_entries(source, source, destination)
}

fn copy_package_tree_entries(root: &Path, current: &Path, destination_root: &Path) -> Result<()> {
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
                "package copy cannot include symlink {}; copy the target file into the package",
                path.display()
            );
        }
        if file_name == ".git" && metadata.is_dir() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to make {} package-relative", path.display()))?;
        let destination = destination_root.join(relative);
        if metadata.is_dir() {
            fs::create_dir_all(&destination)
                .with_context(|| format!("failed to create {}", destination.display()))?;
            copy_package_tree_entries(root, &path, destination_root)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&path, &destination).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    path.display(),
                    destination.display()
                )
            })?;
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

fn validate_package_version(version: &str) -> Result<&str> {
    Version::parse(version).with_context(|| {
        format!("invalid package version {version:?}; expected semantic version")
    })?;
    Ok(version)
}

fn validate_version_req(version_req: &str) -> Result<&str> {
    VersionReq::parse(version_req).with_context(|| {
        format!("invalid dependency version requirement {version_req:?}; expected semver syntax")
    })?;
    Ok(version_req)
}

fn validate_package_version_requirement(
    package: &str,
    version_req: Option<&str>,
    package_version: Option<&str>,
) -> Result<()> {
    let Some(version_req) = version_req else {
        if let Some(package_version) = package_version {
            validate_package_version(package_version)?;
        }
        return Ok(());
    };
    let requirement = VersionReq::parse(version_req).with_context(|| {
        format!("invalid dependency version requirement {version_req:?} for {package}")
    })?;
    let package_version = package_version.with_context(|| {
        format!("dependency {package} declares version requirement {version_req:?}, but the package has no [package] version")
    })?;
    let version = Version::parse(package_version)
        .with_context(|| format!("invalid package version {package_version:?} for {package}"))?;
    if !requirement.matches(&version) {
        bail!(
            "dependency {package} version {package_version} does not satisfy requirement {version_req}"
        );
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

fn validate_project_relative_path(path: &str, description: &str) -> Result<()> {
    if path.contains('\\') {
        bail!("{description} path must use forward slashes");
    }
    let path = Path::new(path);
    if path.is_absolute() {
        bail!("{description} path must be relative to the project root");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => bail!("{description} path must not contain .. components"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("{description} path must be relative to the project root")
            }
        }
    }
    Ok(())
}

fn ensure_contained_candidate(
    project_root: &Path,
    candidate: &Path,
    description: &str,
) -> Result<()> {
    let canonical_root = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve {}", project_root.display()))?;
    let existing = nearest_existing_ancestor(candidate);
    let canonical_existing = fs::canonicalize(existing)
        .with_context(|| format!("failed to resolve {}", existing.display()))?;
    if !canonical_existing.starts_with(&canonical_root) {
        bail!(
            "{description} resolves outside the project root: {}",
            candidate.display()
        );
    }
    Ok(())
}

fn ensure_existing_project_dir(project_root: &Path, path: &Path, description: &str) -> Result<()> {
    ensure_contained_candidate(project_root, path, description)?;
    let canonical_root = fs::canonicalize(project_root)
        .with_context(|| format!("failed to resolve {}", project_root.display()))?;
    let canonical_path =
        fs::canonicalize(path).with_context(|| format!("failed to resolve {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!(
            "{description} resolves outside the project root: {}",
            path.display()
        );
    }
    Ok(())
}

fn nearest_existing_ancestor(path: &Path) -> &Path {
    path.ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or_else(|| Path::new("."))
}

fn current_git_commit(package_dir: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(package_dir)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD^{commit}")
        .output()
        .with_context(|| format!("failed to resolve git commit in {}", package_dir.display()))?;
    if !output.status.success() {
        bail!(
            "failed to resolve git commit in {}\nstdout:\n{}\nstderr:\n{}",
            package_dir.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_git_commit(&commit)?;
    Ok(commit)
}

fn checkout_git_commit(package_dir: &Path, commit: &str) -> Result<()> {
    validate_git_commit(commit)?;
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(package_dir)
        .arg("checkout")
        .arg("--detach")
        .arg(commit)
        .output()
        .with_context(|| format!("failed to checkout {commit} in {}", package_dir.display()))?;
    if !output.status.success() {
        bail!(
            "failed to checkout locked commit {commit} in {}\nstdout:\n{}\nstderr:\n{}",
            package_dir.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn validate_git_commit(commit: &str) -> Result<()> {
    if commit.len() != 40 || !commit.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("invalid Git commit object id {commit:?}");
    }
    Ok(())
}

fn write_dependency_manifest(manifest_path: &Path, spec: &DependencySpec) -> Result<()> {
    let source = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let dependencies = ensure_table(doc.as_table_mut(), "dependencies", manifest_path)?;
    let mut dependency = Table::new();
    if let Some(package) = &spec.package {
        dependency["package"] = value(package.clone());
    }
    dependency["path"] = value(spec.path.clone());
    if let Some(git) = &spec.git {
        dependency["git"] = value(git.clone());
    }
    if let Some(rev) = &spec.rev {
        dependency["rev"] = value(rev.clone());
    }
    if let Some(registry) = &spec.registry {
        dependency["registry"] = value(registry.clone());
    }
    if let Some(version_req) = &spec.version_req {
        dependency["version"] = value(version_req.clone());
    }
    dependencies.insert(&spec.name, Item::Table(dependency));

    fs::write(manifest_path, doc.to_string())
        .with_context(|| format!("failed to write {}", manifest_path.display()))
}

fn write_lockfile(lock_path: &Path, spec: &DependencySpec) -> Result<()> {
    let source = if lock_path.is_file() {
        fs::read_to_string(lock_path)
            .with_context(|| format!("failed to read {}", lock_path.display()))?
    } else {
        String::new()
    };
    let mut doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", lock_path.display()))?;

    let packages = ensure_table(doc.as_table_mut(), "package", lock_path)?;
    let mut package = Table::new();
    package["source"] = value(spec.source.clone());
    if let Some(package_name) = &spec.package {
        package["package"] = value(package_name.clone());
    }
    package["path"] = value(spec.path.clone());
    if let Some(git) = &spec.git {
        package["git"] = value(git.clone());
    }
    if let Some(rev) = &spec.rev {
        package["rev"] = value(rev.clone());
    }
    if let Some(commit) = &spec.commit {
        package["commit"] = value(commit.clone());
    }
    if let Some(registry) = &spec.registry {
        package["registry"] = value(registry.clone());
    }
    if let Some(version_req) = &spec.version_req {
        validate_version_req(version_req)?;
        package["version_req"] = value(version_req.clone());
    }
    if let Some(package_version) = &spec.package_version {
        validate_package_version(package_version)?;
        package["version"] = value(package_version.clone());
    }
    if let Some(integrity) = &spec.integrity {
        validate_package_integrity(integrity)?;
        package["integrity"] = value(integrity.clone());
    }
    if let Some(provenance) = &spec.provenance {
        validate_package_integrity(provenance)?;
        package["provenance"] = value(provenance.clone());
    }
    if let Some(signature) = &spec.signature {
        validate_package_integrity(signature)?;
        package["signature"] = value(signature.clone());
    }
    if let Some(signature_kind) = &spec.signature_kind {
        validate_signature_kind(signature_kind)?;
        if spec.signature.is_none() {
            bail!(
                "signature_kind cannot be written without signature for {}",
                spec.name
            );
        }
        package["signature_kind"] = value(signature_kind.clone());
    }
    packages.insert(&spec.name, Item::Table(package));

    fs::write(lock_path, doc.to_string())
        .with_context(|| format!("failed to write {}", lock_path.display()))
}

fn ensure_table<'a>(root: &'a mut Table, key: &str, path: &Path) -> Result<&'a mut Table> {
    if !root.contains_key(key) {
        root.insert(key, Item::Table(Table::new()));
    }

    root.get_mut(key)
        .and_then(Item::as_table_mut)
        .with_context(|| format!("{key} in {} must be a table", path.display()))
}

fn run_file(
    path: &str,
    debug: bool,
    step: bool,
    breakpoints: &[usize],
    trace_file: Option<&Path>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    run_chunk_cli(
        &chunk,
        RunChunkCliOptions {
            debug,
            step,
            breakpoints,
            breakpoint_file: Some(&chunk.file),
            trace_file,
            args,
            capabilities,
            debug_output: DebugOutput::Text,
            print_final_stack: true,
        },
    )
}

fn debug_file(
    path: &str,
    json_output: bool,
    step: bool,
    breakpoints: &[usize],
    trace_file: Option<&Path>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    run_chunk_cli(
        &chunk,
        RunChunkCliOptions {
            debug: true,
            step,
            breakpoints,
            breakpoint_file: Some(&chunk.file),
            trace_file,
            args,
            capabilities,
            debug_output: if json_output {
                DebugOutput::JsonLines
            } else {
                DebugOutput::Text
            },
            print_final_stack: false,
        },
    )
}

fn run_bytecode(
    path: &str,
    debug: bool,
    trace_file: Option<&Path>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {path}"))?;
    let chunk = Chunk::from_bytes(&bytes).with_context(|| format!("failed to decode {path}"))?;
    run_chunk_cli(
        &chunk,
        RunChunkCliOptions {
            debug,
            step: false,
            breakpoints: &[],
            breakpoint_file: None,
            trace_file,
            args,
            capabilities,
            debug_output: DebugOutput::Text,
            print_final_stack: true,
        },
    )
}

fn run_gui_file(path: &str, args: Vec<String>, capabilities: CapabilityOptions) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    run_gui_chunk(&chunk, args, capabilities)
}

fn run_tui_file(path: &str, args: Vec<String>, capabilities: CapabilityOptions) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    run_chunk_cli(
        &chunk,
        RunChunkCliOptions {
            debug: false,
            step: false,
            breakpoints: &[],
            breakpoint_file: None,
            trace_file: None,
            args,
            capabilities,
            debug_output: DebugOutput::Text,
            print_final_stack: false,
        },
    )
}

fn run_embedded_gui_app(chunk: &Chunk, args: Vec<String>) -> Result<()> {
    run_gui_chunk(chunk, args, CapabilityOptions::default())
}

fn run_embedded_tui_app(chunk: &Chunk, args: Vec<String>) -> Result<()> {
    run_chunk_cli(
        chunk,
        RunChunkCliOptions {
            debug: false,
            step: false,
            breakpoints: &[],
            breakpoint_file: None,
            trace_file: None,
            args,
            capabilities: CapabilityOptions::default(),
            debug_output: DebugOutput::Text,
            print_final_stack: false,
        },
    )
}

async fn run_embedded_mvc_gui_app(bundle: MvcBundle, _args: Vec<String>) -> Result<()> {
    let project_root = extract_embedded_mvc_bundle(&bundle)?;
    std::env::set_current_dir(&project_root).with_context(|| {
        format!(
            "failed to use embedded MVC project directory {}",
            project_root.display()
        )
    })?;

    let serve_options = ricochet_web::ServeOptions {
        fs_root: Some(project_root.clone()),
        ..Default::default()
    };
    let app = ricochet_web::build_served_app_from_dir(&project_root, false, false, &serve_options)
        .await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local MVC GUI server")?;
    let address = listener
        .local_addr()
        .context("failed to read local MVC GUI server address")?;
    let server = tokio::spawn(async move {
        if let Err(error) = ricochet_web::serve_app_on_listener(listener, app).await {
            eprintln!("Ricochet MVC GUI server stopped: {error:?}");
        }
    });
    let url = format!("http://{address}/");

    if let Ok(path) = std::env::var(GUI_EXPORT_HTML_ENV) {
        let export_request_path =
            std::env::var(GUI_EXPORT_PATH_ENV).unwrap_or_else(|_| "/".to_string());
        if !export_request_path.starts_with('/') {
            bail!("{GUI_EXPORT_PATH_ENV} must start with /");
        }
        let html = fetch_http_body(address, &export_request_path).await?;
        fs::write(&path, html).with_context(|| {
            format!("failed to write GUI HTML export requested by {GUI_EXPORT_HTML_ENV}={path}")
        })?;
        server.abort();
        return Ok(());
    }

    let result = open_native_webview_url(
        DEFAULT_MVC_GUI_TITLE,
        &url,
        DEFAULT_MVC_GUI_WIDTH,
        DEFAULT_MVC_GUI_HEIGHT,
    );
    server.abort();
    result
}

async fn fetch_http_body(address: SocketAddr, path: &str) -> Result<String> {
    let mut last_error = None;
    for _ in 0..50 {
        match try_fetch_http_body(address, path).await {
            Ok(body) => return Ok(body),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("local MVC GUI server did not respond"))
        .context(format!("failed to fetch http://{address}{path}")))
}

async fn try_fetch_http_body(address: SocketAddr, path: &str) -> Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .context("failed to connect to local MVC GUI server")?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .context("failed to send MVC GUI export request")?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .context("failed to read MVC GUI export response")?;
    let response = String::from_utf8_lossy(&response);
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .context("MVC GUI export response was not valid HTTP")?;
    let status_line = headers.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!("MVC GUI export request returned {status_line}");
    }
    Ok(body.to_string())
}

fn run_gui_chunk(chunk: &Chunk, args: Vec<String>, capabilities: CapabilityOptions) -> Result<()> {
    let document = render_webview_document(chunk, args, capabilities)?;
    if let Ok(path) = std::env::var(GUI_EXPORT_HTML_ENV) {
        fs::write(&path, &document.html).with_context(|| {
            format!("failed to write GUI HTML export requested by {GUI_EXPORT_HTML_ENV}={path}")
        })?;
        return Ok(());
    }
    open_native_webview(document)
}

fn render_webview_document(
    chunk: &Chunk,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<WebviewDocument> {
    let mut vm = cli_vm(args, &capabilities)?;
    let result = vm.run_chunk(chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
    if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
        std::process::exit(code);
    }
    if let Err(error) = result {
        bail!("{}", runtime_error_message(&vm, &error));
    }
    let document = webview_document_from_vm(&vm)?;
    dispatch_webview_event_if_requested(&mut vm, document)
}

fn webview_document_from_vm(vm: &Vm) -> Result<WebviewDocument> {
    for value in vm.stack().iter().rev() {
        if let Some(document) = webview_document_from_value(value)? {
            return Ok(document);
        }
    }

    if let Some(value) = vm.variable("document") {
        if let Some(document) = webview_document_from_value(value)? {
            return Ok(document);
        }
    }

    bail!(
        "GUI apps must leave a `webview_window` result on the stack or store it in a variable named `document`"
    )
}

fn webview_document_from_value(value: &Value) -> Result<Option<WebviewDocument>> {
    match value {
        Value::Result(RicochetResult::Ok(inner)) => webview_document_from_value(inner),
        Value::Result(RicochetResult::Err(error)) => {
            bail!(
                "GUI app returned an error result: {}: {}",
                error.kind,
                error.message
            )
        }
        Value::Map(map) => webview_document_from_map(map),
        _ => Ok(None),
    }
}

fn webview_document_from_map(map: &MapValue) -> Result<Option<WebviewDocument>> {
    if map.get("type") != Some(Value::String("webview".to_string())) {
        return Ok(None);
    }

    Ok(Some(WebviewDocument {
        title: required_document_string(map, "title")?,
        html: required_document_string(map, "html")?,
        width: required_document_dimension(map, "width")?,
        height: required_document_dimension(map, "height")?,
        state: optional_document_value(map, "state")
            .unwrap_or_else(|| Value::Map(BTreeMap::new().into())),
        actions: optional_document_value(map, "actions")
            .map(|value| webview_actions_from_value(&value))
            .transpose()?
            .unwrap_or_default(),
    }))
}

fn dispatch_webview_event_if_requested(
    vm: &mut Vm,
    document: WebviewDocument,
) -> Result<WebviewDocument> {
    let Ok(event_source) = std::env::var(GUI_EVENT_ENV) else {
        return Ok(document);
    };
    let event_json: serde_json::Value = serde_json::from_str(&event_source)
        .with_context(|| format!("{GUI_EVENT_ENV} must be a JSON object"))?;
    let action_name = event_json
        .get("action")
        .and_then(|value| value.as_str())
        .context("GUI action event is missing string field `action`")?;
    let action = document
        .actions
        .iter()
        .find(|action| action.action == action_name)
        .with_context(|| format!("GUI document has no action named {action_name:?}"))?;

    vm.push_value(document.state.clone());
    vm.push_value(json_to_ricochet_value(event_json));
    let mut chunk = Chunk::new("<gui-event>");
    chunk.push(Op::CallWord(action.callback.clone()), gui_event_span());
    let result = vm.run_chunk(&chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
    if let Err(error) = result {
        bail!("{}", runtime_error_message(vm, &error));
    }
    webview_document_from_vm(vm).with_context(|| {
        format!(
            "GUI action callback {:?} must return a webview document",
            action.callback
        )
    })
}

fn webview_actions_from_value(value: &Value) -> Result<Vec<WebviewAction>> {
    let values = match value {
        Value::Array(values) => values.snapshot(),
        Value::List(values) => values.snapshot(),
        value => bail!("webview document `actions` must be an array or list, got {value:?}"),
    };

    values
        .iter()
        .map(webview_action_from_value)
        .collect::<Result<Vec<_>>>()
}

fn webview_action_from_value(value: &Value) -> Result<WebviewAction> {
    let Value::Map(map) = value else {
        bail!("webview action entries must be maps, got {value:?}");
    };
    if let Some(Value::String(kind)) = map.get("type") {
        if kind != "action" {
            bail!("webview action `type` must be \"action\", got {kind:?}");
        }
    }
    Ok(WebviewAction {
        action: required_document_string(map, "action")?,
        callback: required_document_string(map, "callback")?,
    })
}

fn optional_document_value(map: &MapValue, key: &str) -> Option<Value> {
    map.get(key)
}

fn required_document_string(map: &MapValue, key: &str) -> Result<String> {
    match map.get(key) {
        Some(Value::String(value)) => Ok(value),
        Some(value) => bail!("webview document `{key}` must be a string, got {value:?}"),
        None => bail!("webview document is missing `{key}`"),
    }
}

fn required_document_dimension(map: &MapValue, key: &str) -> Result<u32> {
    match map.get(key) {
        Some(Value::Number(value)) if value > 0 => u32::try_from(value)
            .with_context(|| format!("webview document `{key}` is too large: {value}")),
        Some(Value::Number(value)) => {
            bail!("webview document `{key}` must be positive, got {value}")
        }
        Some(value) => bail!("webview document `{key}` must be a number, got {value:?}"),
        None => bail!("webview document is missing `{key}`"),
    }
}

fn gui_event_span() -> SourceSpan {
    SourceSpan {
        file: "<gui-event>".to_string(),
        start: 0,
        end: 0,
        line: 1,
        column: 1,
    }
}

fn json_to_ricochet_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => Value::Number(
            value
                .as_i64()
                .unwrap_or_else(|| value.as_u64().unwrap_or(i64::MAX as u64) as i64),
        ),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(json_to_ricochet_value)
                .collect::<Vec<_>>()
                .into(),
        ),
        serde_json::Value::Object(values) => Value::Map(
            values
                .into_iter()
                .map(|(key, value)| (key, json_to_ricochet_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into(),
        ),
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn open_native_webview(document: WebviewDocument) -> Result<()> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(document.title.clone())
        .with_inner_size(LogicalSize::new(
            f64::from(document.width),
            f64::from(document.height),
        ))
        .build(&event_loop)
        .context("failed to create native GUI window")?;
    let _webview = WebViewBuilder::new()
        .with_html(document.html)
        .build(&window)
        .context("failed to create native WebView")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(any(windows, target_os = "macos"))]
fn open_native_webview_url(title: &str, url: &str, width: u32, height: u32) -> Result<()> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title.to_string())
        .with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
        .build(&event_loop)
        .context("failed to create native GUI window")?;
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)
        .context("failed to create native WebView")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(target_os = "linux")]
fn open_native_webview(document: WebviewDocument) -> Result<()> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::platform::unix::WindowExtUnix;
    use tao::window::WindowBuilder;
    use wry::{WebViewBuilder, WebViewBuilderExtUnix};

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(document.title.clone())
        .with_inner_size(LogicalSize::new(
            f64::from(document.width),
            f64::from(document.height),
        ))
        .build(&event_loop)
        .context("failed to create native GUI window")?;
    let _webview = WebViewBuilder::new()
        .with_html(document.html)
        .build_gtk(window.gtk_window())
        .context("failed to create native WebView")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(target_os = "linux")]
fn open_native_webview_url(title: &str, url: &str, width: u32, height: u32) -> Result<()> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::platform::unix::WindowExtUnix;
    use tao::window::WindowBuilder;
    use wry::{WebViewBuilder, WebViewBuilderExtUnix};

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title.to_string())
        .with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
        .build(&event_loop)
        .context("failed to create native GUI window")?;
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build_gtk(window.gtk_window())
        .context("failed to create native WebView")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn open_native_webview(_document: WebviewDocument) -> Result<()> {
    bail!("native GUI hosting is currently implemented for Windows, Linux, and macOS builds")
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn open_native_webview_url(_title: &str, _url: &str, _width: u32, _height: u32) -> Result<()> {
    bail!("native GUI hosting is currently implemented for Windows, Linux, and macOS builds")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebugOutput {
    Text,
    JsonLines,
}

struct RunChunkCliOptions<'a> {
    debug: bool,
    step: bool,
    breakpoints: &'a [usize],
    breakpoint_file: Option<&'a str>,
    trace_file: Option<&'a Path>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    debug_output: DebugOutput,
    print_final_stack: bool,
}

fn run_chunk_cli(chunk: &Chunk, options: RunChunkCliOptions<'_>) -> Result<()> {
    let mut vm = cli_vm(options.args, &options.capabilities)?;
    let debugger_enabled = options.debug
        || options.step
        || !options.breakpoints.is_empty()
        || options.trace_file.is_some()
        || options.debug_output == DebugOutput::JsonLines;
    if debugger_enabled {
        vm.enable_debug();
        match options.debug_output {
            DebugOutput::Text => vm.set_debug_sink(print_debug_event),
            DebugOutput::JsonLines => vm.set_debug_sink(print_debug_event_json_line),
        }
    }
    if options.step {
        vm.enable_step_debugging();
    }
    for &line in options.breakpoints {
        if line == 0 {
            bail!("breakpoint lines are 1-based");
        }
        let file = options.breakpoint_file.unwrap_or(&chunk.file);
        vm.add_line_breakpoint(file.to_string(), line);
    }
    if options.debug_output == DebugOutput::Text
        && (options.step || !options.breakpoints.is_empty())
    {
        vm.set_debug_controller(read_terminal_debug_action);
    }

    let result = vm.run_chunk(chunk);
    match options.debug_output {
        DebugOutput::Text => {
            print!("{}", vm.stdout());
            eprint!("{}", vm.stderr());
        }
        DebugOutput::JsonLines => {
            if !vm.stdout().is_empty() {
                emit_json_line(json!({
                    "event": "output",
                    "stream": "stdout",
                    "text": vm.stdout(),
                }))?;
            }
            if !vm.stderr().is_empty() {
                emit_json_line(json!({
                    "event": "output",
                    "stream": "stderr",
                    "text": vm.stderr(),
                }))?;
            }
        }
    }
    if let Some(trace_file) = options.trace_file {
        write_debug_trace(trace_file, vm.debug_events())?;
    }
    if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
        std::process::exit(code);
    }
    if let Err(error) = result {
        bail!("{}", runtime_error_message(&vm, &error));
    }

    if options.print_final_stack {
        println!("{:?}", vm.stack());
    }

    Ok(())
}

fn runtime_error_message(vm: &Vm, error: &ricochet_vm::VmError) -> String {
    let Some(site) = vm.last_error_site() else {
        return error.to_string();
    };
    let source = match fs::read_to_string(&site.span.file) {
        Ok(source) => source,
        Err(_) => {
            return format!(
                "{error}\n --> {}:{}:{}\nhelp: while executing {} in {}",
                site.span.file, site.span.line, site.span.column, site.opcode, site.frame
            );
        }
    };
    SourceDiagnostic::new(
        site.span.file.clone(),
        Span {
            start: site.span.start,
            end: site.span.end,
        },
        error.to_string(),
    )
    .with_help(format!("while executing {} in {}", site.opcode, site.frame))
    .render(&source)
}

fn write_debug_trace(path: &Path, events: &[DebugEvent]) -> Result<()> {
    let trace: Vec<_> = events.iter().map(debug_event_json).collect();
    let json = serde_json::to_string_pretty(&trace)?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

fn print_debug_event_json_line(event: &DebugEvent) {
    let value = debug_event_json(event);
    println!(
        "{}",
        serde_json::to_string(&value).expect("debug event JSON should serialize")
    );
}

fn emit_json_line(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn debug_event_json(event: &DebugEvent) -> serde_json::Value {
    match event {
        DebugEvent::Paused(pause) => json!({
            "event": "paused",
            "reason": match pause.reason {
                DebugPauseReason::Step => "step",
                DebugPauseReason::Breakpoint => "breakpoint",
            },
            "frame": pause.frame,
            "source": pause.source,
            "opcode": pause.opcode,
            "stack": debug_stack_json(&pause.stack),
            "locals": debug_bindings_json(&pause.locals),
            "globals": debug_bindings_json(&pause.globals),
            "self": pause.current_self.as_ref().map(debug_value_json),
            "tasks": debug_tasks_json(&pause.tasks),
        }),
        DebugEvent::Instruction {
            frame,
            source,
            opcode,
            stack_before,
            stack_after,
        } => json!({
            "event": "instruction",
            "frame": frame,
            "source": source,
            "opcode": opcode,
            "stack_before": debug_stack_json(stack_before),
            "stack_after": debug_stack_json(stack_after),
        }),
        DebugEvent::Fault {
            frame,
            message,
            stack,
        } => json!({
            "event": "fault",
            "frame": frame,
            "message": message,
            "stack": debug_stack_json(stack),
        }),
    }
}

fn debug_stack_json(stack: &[Value]) -> Vec<serde_json::Value> {
    stack.iter().map(debug_value_json).collect()
}

fn debug_bindings_json(bindings: &[(String, Value)]) -> Vec<serde_json::Value> {
    bindings
        .iter()
        .map(|(name, value)| {
            json!({
                "name": name,
                "value": debug_value_json(value),
            })
        })
        .collect()
}

fn debug_tasks_json(tasks: &[DebugTask]) -> Vec<serde_json::Value> {
    tasks
        .iter()
        .map(|task| {
            json!({
                "id": task.id,
                "status": task.status,
                "pending": task.pending,
                "running": task.running,
                "completed": task.completed,
                "failed": task.failed,
            })
        })
        .collect()
}

fn debug_value_json(value: &Value) -> serde_json::Value {
    json!({
        "debug": format!("{value:?}"),
    })
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkOptions {
    iterations: usize,
    smoke: bool,
    json: bool,
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkWorkload {
    parser_lines: usize,
    vm_ops: usize,
    dispatch_pairs: usize,
    collection_ops: usize,
    json_items: usize,
    template_exprs: usize,
    sqlite_requests: usize,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    version: &'static str,
    profile: String,
    iterations: usize,
    measurements: Vec<BenchmarkMeasurement>,
}

#[derive(Debug, Serialize)]
struct BenchmarkMeasurement {
    name: String,
    workload: String,
    operations: u64,
    iterations: usize,
    min_ms: f64,
    median_ms: f64,
    max_ms: f64,
    ops_per_second: f64,
    notes: String,
}

async fn run_benchmarks(options: BenchmarkOptions) -> Result<()> {
    let report = benchmark_report(options).await?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_benchmark_report(&report);
    }
    Ok(())
}

async fn benchmark_report(options: BenchmarkOptions) -> Result<BenchmarkReport> {
    let iterations = if options.smoke {
        1
    } else {
        options.iterations.max(1)
    };
    let workload = if options.smoke {
        BenchmarkWorkload {
            parser_lines: 200,
            vm_ops: 1_000,
            dispatch_pairs: 250,
            collection_ops: 1_000,
            json_items: 100,
            template_exprs: 100,
            sqlite_requests: 10,
        }
    } else {
        BenchmarkWorkload {
            parser_lines: 2_000,
            vm_ops: 20_000,
            dispatch_pairs: 2_500,
            collection_ops: 20_000,
            json_items: 1_000,
            template_exprs: 500,
            sqlite_requests: 100,
        }
    };

    let repo_root = benchmark_repo_root()?;
    let parse_source = generated_arithmetic_source(workload.parser_lines);
    let vm_source = generated_arithmetic_source(workload.vm_ops);
    let vm_chunk = compile_source("<bench-vm-arithmetic>", &vm_source)?;
    let dispatch_source = generated_dispatch_source(workload.dispatch_pairs);
    let dispatch_chunk = compile_source("<bench-dispatch>", &dispatch_source)?;
    let collection_source = generated_collection_source(workload.collection_ops);
    let collection_chunk = compile_source("<bench-collection>", &collection_source)?;
    let json_source = generated_json_source(workload.json_items);
    let json_chunk = compile_source("<bench-json>", &json_source)?;
    let template = generated_template(workload.template_exprs);
    let mut template_data = BTreeMap::new();
    template_data.insert(
        "title".to_string(),
        Value::String("Ricochet <Benchmark>".to_string()),
    );
    template_data.insert("count".to_string(), Value::Number(42));
    let package_root = repo_root.join("packages").join("ricochet_forms");
    let package_files = count_package_files(&package_root)? as u64;

    let mut measurements = Vec::new();
    measurements.push(measure_sync(
        "parser",
        format!("{} source lines", workload.parser_lines),
        workload.parser_lines as u64,
        iterations,
        "parse_module over generated arithmetic source",
        || {
            let parsed = parse_module(&parse_source)?;
            black_box(parsed);
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "compiler",
        format!("{} source lines", workload.parser_lines),
        workload.parser_lines as u64,
        iterations,
        "compile_source over generated arithmetic source",
        || {
            let chunk = compile_source("<bench-compile>", &parse_source)?;
            black_box(chunk.instructions.len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "vm_arithmetic",
        format!("{} additions", workload.vm_ops),
        workload.vm_ops as u64,
        iterations,
        "fresh VM running precompiled stack arithmetic",
        || {
            let mut vm = Vm::default();
            vm.run_chunk(&vm_chunk)?;
            black_box(vm.stack().len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "dispatch",
        format!("{} function + method pairs", workload.dispatch_pairs),
        (workload.dispatch_pairs * 2) as u64,
        iterations,
        "user function and OOP method calls",
        || {
            let mut vm = Vm::default();
            vm.run_chunk(&dispatch_chunk)?;
            black_box(vm.stack().len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "collection_mutation",
        format!("{} array push! calls", workload.collection_ops),
        workload.collection_ops as u64,
        iterations,
        "array mutation through collection helpers",
        || {
            let mut vm = Vm::default();
            vm.run_chunk(&collection_chunk)?;
            black_box(vm.stack().len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "json_encode_decode",
        format!("{} object items", workload.json_items),
        workload.json_items as u64,
        iterations,
        "JSON encode/decode of a generated nested map",
        || {
            let mut vm = Vm::default();
            vm.run_chunk(&json_chunk)?;
            black_box(vm.stack().len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "template_rendering",
        format!("{} template expressions", workload.template_exprs * 2),
        (workload.template_exprs * 2) as u64,
        iterations,
        "HTML template rendering with Ricochet expressions",
        || {
            let rendered = ricochet_web::render_template(
                &template,
                &template_data,
                ricochet_web::EscapeMode::Html,
            )?;
            black_box(rendered.len());
            Ok(())
        },
    )?);
    measurements.push(measure_sync(
        "package_verification",
        format!("{package_files} package files"),
        package_files,
        iterations,
        "deterministic package tree integrity hash",
        || {
            let integrity = package_tree_integrity(&package_root)?;
            black_box(integrity);
            Ok(())
        },
    )?);
    measurements
        .push(measure_sqlite_mvc_requests(&repo_root, workload.sqlite_requests, iterations).await?);

    Ok(BenchmarkReport {
        version: crate_version(),
        profile: if options.smoke {
            "smoke".to_string()
        } else {
            "local".to_string()
        },
        iterations,
        measurements,
    })
}

fn measure_sync<F>(
    name: &str,
    workload: String,
    operations: u64,
    iterations: usize,
    notes: &str,
    mut run: F,
) -> Result<BenchmarkMeasurement>
where
    F: FnMut() -> Result<()>,
{
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run()?;
        samples.push(start.elapsed());
    }
    Ok(benchmark_measurement(
        name, workload, operations, iterations, notes, samples,
    ))
}

async fn measure_sqlite_mvc_requests(
    repo_root: &Path,
    requests: usize,
    iterations: usize,
) -> Result<BenchmarkMeasurement> {
    let project = benchmark_sqlite_project_path(repo_root)?;
    create_new_project(&project, NewProjectOptions { with_sqlite: true }, false)?;
    rewrite_benchmark_sqlite_manifest(&project)?;
    let app = ricochet_web::build_served_app_from_dir(
        &project,
        false,
        false,
        &ricochet_web::ServeOptions::default(),
    )
    .await?;

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        for _ in 0..requests {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/users")
                        .body(Body::empty())
                        .context("failed to build benchmark request")?,
                )
                .await
                .context("SQLite MVC benchmark request failed")?;
            if !response.status().is_success() {
                bail!(
                    "SQLite MVC benchmark request returned {}",
                    response.status()
                );
            }
            let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .context("failed to read SQLite MVC benchmark response")?;
            black_box(body.len());
        }
        samples.push(start.elapsed());
    }

    Ok(benchmark_measurement(
        "sqlite_mvc_request",
        format!("{requests} GET /users requests"),
        requests as u64,
        iterations,
        "in-process Axum route through Ricochet MVC + SQLite Active Record",
        samples,
    ))
}

fn benchmark_measurement(
    name: &str,
    workload: String,
    operations: u64,
    iterations: usize,
    notes: &str,
    samples: Vec<Duration>,
) -> BenchmarkMeasurement {
    let mut sample_ms = samples
        .iter()
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .collect::<Vec<_>>();
    sample_ms.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let min_ms = *sample_ms.first().unwrap_or(&0.0);
    let median_ms = sample_ms[sample_ms.len() / 2];
    let max_ms = *sample_ms.last().unwrap_or(&0.0);
    let ops_per_second = if median_ms > 0.0 {
        operations as f64 / (median_ms / 1000.0)
    } else {
        0.0
    };
    BenchmarkMeasurement {
        name: name.to_string(),
        workload,
        operations,
        iterations,
        min_ms,
        median_ms,
        max_ms,
        ops_per_second,
        notes: notes.to_string(),
    }
}

fn print_benchmark_report(report: &BenchmarkReport) {
    println!(
        "Ricochet {} benchmark profile={} iterations={}",
        report.version, report.profile, report.iterations
    );
    println!("| benchmark | workload | median | throughput | notes |");
    println!("| --- | ---: | ---: | ---: | --- |");
    for measurement in &report.measurements {
        println!(
            "| {} | {} | {:.3} ms | {} ops/s | {} |",
            measurement.name,
            measurement.workload,
            measurement.median_ms,
            format_number(measurement.ops_per_second),
            measurement.notes
        );
    }
}

fn format_number(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.2}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.2}k", value / 1_000.0)
    } else {
        format!("{value:.2}")
    }
}

fn benchmark_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    if cwd.join("packages").is_dir() && cwd.join("crates").is_dir() {
        return Ok(cwd);
    }
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn benchmark_sqlite_project_path(repo_root: &Path) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock should be after Unix epoch")?
        .as_nanos();
    let root = repo_root
        .join("target")
        .join("bench")
        .join(format!("sqlite-mvc-{}-{nanos}", std::process::id()));
    Ok(root)
}

fn rewrite_benchmark_sqlite_manifest(project: &Path) -> Result<()> {
    let manifest_path = project.join("ricochet.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let database_path = path_to_slash(&project.join("db").join("development.sqlite3"));
    let manifest = manifest.replace(
        "url = \"db/development.sqlite3\"",
        &format!("url = \"{database_path}\""),
    );
    fs::write(&manifest_path, manifest)
        .with_context(|| format!("failed to write {}", manifest_path.display()))
}

fn count_package_files(path: &Path) -> Result<usize> {
    let mut count = 0usize;
    count_files_recursive(path, &mut count)?;
    Ok(count)
}

fn count_files_recursive(path: &Path, count: &mut usize) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
        let path = entry.path();
        if path.is_dir() {
            count_files_recursive(&path, count)?;
        } else if path.is_file() {
            *count += 1;
        }
    }
    Ok(())
}

fn generated_arithmetic_source(lines: usize) -> String {
    let mut source = String::new();
    for _ in 0..lines {
        source.push_str("1 2 + drop\n");
    }
    source
}

fn generated_dispatch_source(pairs: usize) -> String {
    let mut source = String::from(
        r#"Counter Object Subclass
  [
    42
  ] "next" Method
end

( value -> Number ) bump function
  value var
  $value 1 +
end

Counter new counter var
"#,
    );
    for _ in 0..pairs {
        source.push_str("1 bump drop\n");
        source.push_str("$counter next drop\n");
    }
    source
}

fn generated_collection_source(items: usize) -> String {
    let mut source = String::from("items array\n");
    for item in 0..items {
        writeln!(&mut source, "$items {item} push! drop").expect("write to string succeeds");
    }
    source.push_str("$items count\n");
    source
}

fn generated_json_source(items: usize) -> String {
    let mut source = String::from("payload map\nitems array\n");
    for item in 0..items {
        writeln!(&mut source, "item{item} map").expect("write to string succeeds");
        writeln!(&mut source, "$item{item} \"id\" {item} put! drop")
            .expect("write to string succeeds");
        writeln!(
            &mut source,
            "$item{item} \"name\" \"item-{item}\" put! drop"
        )
        .expect("write to string succeeds");
        writeln!(&mut source, "$items $item{item} push! drop").expect("write to string succeeds");
    }
    source.push_str(
        r#"$payload "items" $items put! drop
$payload json-encode encoded var
$encoded json-decode value decoded var
$decoded "items" at count
"#,
    );
    source
}

fn generated_template(expressions: usize) -> String {
    let mut template = String::new();
    for index in 0..expressions {
        writeln!(&mut template, "<p>{{ $title }} #{index}: {{ $count }}</p>")
            .expect("write to string succeeds");
    }
    template
}

#[derive(Debug, Clone)]
struct DapSetup {
    program: PathBuf,
    args: Vec<String>,
    breakpoints: Vec<DapBreakpoint>,
}

#[derive(Debug, Clone)]
struct DapBreakpoint {
    path: String,
    line: usize,
}

struct DapAdapter<R, W> {
    reader: R,
    writer: W,
    seq: i64,
    last_pause: Option<DebugPause>,
    pause_error: Option<String>,
}

impl DapAdapter<io::BufReader<io::Stdin>, io::Stdout> {
    fn stdio() -> Self {
        Self {
            reader: io::BufReader::new(io::stdin()),
            writer: io::stdout(),
            seq: 1,
            last_pause: None,
            pause_error: None,
        }
    }
}

impl<R, W> DapAdapter<R, W>
where
    R: BufRead,
    W: Write,
{
    fn read_setup(&mut self) -> Result<Option<DapSetup>> {
        let mut program = None;
        let mut args = Vec::new();
        let mut breakpoints = Vec::new();

        while let Some(request) = read_dap_message(&mut self.reader)? {
            let command = dap_request_command(&request);
            match command {
                "initialize" => {
                    self.send_response(
                        &request,
                        json!({
                            "supportsConfigurationDoneRequest": true,
                            "supportsTerminateRequest": true,
                            "supportsStepBack": false,
                            "supportsEvaluateForHovers": false,
                            "supportsSetVariable": false,
                            "supportsRestartRequest": false,
                        }),
                    )?;
                    self.send_event("initialized", json!({}))?;
                }
                "launch" => {
                    let launch_args = request.get("arguments").unwrap_or(&serde_json::Value::Null);
                    let Some(path) = launch_args.get("program").and_then(|value| value.as_str())
                    else {
                        self.send_error_response(&request, "launch requires string `program`")?;
                        continue;
                    };
                    program = Some(PathBuf::from(path));
                    args = launch_args
                        .get("args")
                        .and_then(|value| value.as_array())
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|value| value.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.send_response(&request, json!({}))?;
                }
                "setBreakpoints" => {
                    let arguments = request.get("arguments").unwrap_or(&serde_json::Value::Null);
                    let path = arguments
                        .get("source")
                        .and_then(|source| source.get("path"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                    breakpoints.retain(|breakpoint: &DapBreakpoint| breakpoint.path != path);
                    let mut response_breakpoints = Vec::new();
                    for breakpoint in arguments
                        .get("breakpoints")
                        .and_then(|value| value.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let Some(line) = breakpoint.get("line").and_then(|value| value.as_u64())
                        {
                            let line = line as usize;
                            if !path.is_empty() && line > 0 {
                                breakpoints.push(DapBreakpoint {
                                    path: path.clone(),
                                    line,
                                });
                            }
                            response_breakpoints.push(json!({
                                "verified": line > 0,
                                "line": line,
                            }));
                        }
                    }
                    self.send_response(&request, json!({ "breakpoints": response_breakpoints }))?;
                }
                "configurationDone" => {
                    self.send_response(&request, json!({}))?;
                    let Some(program) = program else {
                        bail!("DAP launch was not configured before configurationDone");
                    };
                    return Ok(Some(DapSetup {
                        program,
                        args,
                        breakpoints,
                    }));
                }
                "threads" => self.send_threads_response(&request)?,
                "setExceptionBreakpoints" => {
                    self.send_response(&request, json!({ "breakpoints": [] }))?
                }
                "disconnect" | "terminate" => {
                    self.send_response(&request, json!({}))?;
                    return Ok(None);
                }
                _ => self.send_response(&request, json!({}))?,
            }
        }

        Ok(None)
    }

    fn handle_pause(&mut self, pause: &DebugPause) -> Result<DebugAction> {
        self.last_pause = Some(pause.clone());
        self.send_event(
            "stopped",
            json!({
                "reason": match pause.reason {
                    DebugPauseReason::Step => "step",
                    DebugPauseReason::Breakpoint => "breakpoint",
                },
                "threadId": 1,
                "allThreadsStopped": true,
            }),
        )?;

        while let Some(request) = read_dap_message(&mut self.reader)? {
            match dap_request_command(&request) {
                "stackTrace" => self.send_stack_trace_response(&request)?,
                "scopes" => self.send_scopes_response(&request)?,
                "variables" => self.send_variables_response(&request)?,
                "threads" => self.send_threads_response(&request)?,
                "continue" => {
                    self.send_response(&request, json!({ "allThreadsContinued": true }))?;
                    self.send_event(
                        "continued",
                        json!({ "threadId": 1, "allThreadsContinued": true }),
                    )?;
                    return Ok(DebugAction::Continue);
                }
                "next" => {
                    self.send_response(&request, json!({}))?;
                    self.send_event(
                        "continued",
                        json!({ "threadId": 1, "allThreadsContinued": true }),
                    )?;
                    return Ok(DebugAction::StepOver);
                }
                "stepIn" => {
                    self.send_response(&request, json!({}))?;
                    self.send_event(
                        "continued",
                        json!({ "threadId": 1, "allThreadsContinued": true }),
                    )?;
                    return Ok(DebugAction::Step);
                }
                "stepOut" => {
                    self.send_response(&request, json!({}))?;
                    self.send_event(
                        "continued",
                        json!({ "threadId": 1, "allThreadsContinued": true }),
                    )?;
                    return Ok(DebugAction::StepOut);
                }
                "pause" => self.send_response(&request, json!({}))?,
                "disconnect" | "terminate" => {
                    self.send_response(&request, json!({}))?;
                    return Ok(DebugAction::Abort);
                }
                _ => self.send_response(&request, json!({}))?,
            }
        }

        Ok(DebugAction::Abort)
    }

    fn send_stack_trace_response(&mut self, request: &serde_json::Value) -> Result<()> {
        let Some(pause) = self.last_pause.as_ref() else {
            self.send_response(request, json!({ "stackFrames": [], "totalFrames": 0 }))?;
            return Ok(());
        };
        let (path, line) = dap_source_location(&pause.source);
        let source_name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&path);
        self.send_response(
            request,
            json!({
                "stackFrames": [{
                    "id": 1,
                    "name": pause.frame,
                    "source": {
                        "name": source_name,
                        "path": path,
                    },
                    "line": line,
                    "column": 1,
                }],
                "totalFrames": 1,
            }),
        )
    }

    fn send_scopes_response(&mut self, request: &serde_json::Value) -> Result<()> {
        let mut scopes = vec![
            json!({ "name": "Stack", "variablesReference": 1, "expensive": false }),
            json!({ "name": "Locals", "variablesReference": 2, "expensive": false }),
            json!({ "name": "Globals", "variablesReference": 3, "expensive": false }),
        ];
        if self
            .last_pause
            .as_ref()
            .and_then(|pause| pause.current_self.as_ref())
            .is_some()
        {
            scopes.push(json!({ "name": "Self", "variablesReference": 4, "expensive": false }));
        }
        scopes.push(json!({ "name": "Tasks", "variablesReference": 5, "expensive": false }));
        self.send_response(request, json!({ "scopes": scopes }))
    }

    fn send_variables_response(&mut self, request: &serde_json::Value) -> Result<()> {
        let reference = request
            .get("arguments")
            .and_then(|arguments| arguments.get("variablesReference"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let variables = match (reference, self.last_pause.as_ref()) {
            (1, Some(pause)) => pause
                .stack
                .iter()
                .enumerate()
                .map(|(index, value)| dap_value_variable(index.to_string(), value))
                .collect(),
            (2, Some(pause)) => dap_binding_variables(&pause.locals),
            (3, Some(pause)) => dap_binding_variables(&pause.globals),
            (4, Some(pause)) => pause
                .current_self
                .as_ref()
                .map(|value| vec![dap_value_variable("self".to_string(), value)])
                .unwrap_or_default(),
            (5, Some(pause)) => pause.tasks.iter().map(dap_task_variable).collect(),
            _ => Vec::new(),
        };
        self.send_response(request, json!({ "variables": variables }))
    }

    fn send_threads_response(&mut self, request: &serde_json::Value) -> Result<()> {
        self.send_response(request, json!({ "threads": [{ "id": 1, "name": "main" }] }))
    }

    fn send_output(&mut self, category: &str, output: &str) -> Result<()> {
        if output.is_empty() {
            return Ok(());
        }
        self.send_event(
            "output",
            json!({
                "category": category,
                "output": output,
            }),
        )
    }

    fn send_event(&mut self, event: &str, body: serde_json::Value) -> Result<()> {
        let message = json!({
            "seq": self.next_seq(),
            "type": "event",
            "event": event,
            "body": body,
        });
        write_dap_message(&mut self.writer, &message)
    }

    fn send_response(
        &mut self,
        request: &serde_json::Value,
        body: serde_json::Value,
    ) -> Result<()> {
        let message = json!({
            "seq": self.next_seq(),
            "type": "response",
            "request_seq": dap_request_seq(request),
            "success": true,
            "command": dap_request_command(request),
            "body": body,
        });
        write_dap_message(&mut self.writer, &message)
    }

    fn send_error_response(&mut self, request: &serde_json::Value, message: &str) -> Result<()> {
        let response = json!({
            "seq": self.next_seq(),
            "type": "response",
            "request_seq": dap_request_seq(request),
            "success": false,
            "command": dap_request_command(request),
            "message": message,
            "body": {
                "error": {
                    "id": 1,
                    "format": message,
                },
            },
        });
        write_dap_message(&mut self.writer, &response)
    }

    fn next_seq(&mut self) -> i64 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }
}

fn run_debug_adapter() -> Result<()> {
    let adapter = Rc::new(RefCell::new(DapAdapter::stdio()));
    let setup = {
        let mut adapter = adapter.borrow_mut();
        adapter.read_setup()?
    };
    let Some(setup) = setup else {
        return Ok(());
    };

    let chunk = match compile_source_file(&setup.program) {
        Ok(chunk) => chunk,
        Err(error) => {
            let mut adapter = adapter.borrow_mut();
            adapter.send_output("stderr", &format!("{error:#}\n"))?;
            adapter.send_event("terminated", json!({}))?;
            return Err(error);
        }
    };

    let mut vm = cli_vm(setup.args, &CapabilityOptions::default())?;
    vm.enable_debug();
    for breakpoint in setup.breakpoints {
        vm.add_line_breakpoint(breakpoint.path, breakpoint.line);
    }

    let adapter_for_controller = Rc::clone(&adapter);
    vm.set_debug_controller(move |pause| {
        let mut adapter = adapter_for_controller.borrow_mut();
        match adapter.handle_pause(pause) {
            Ok(action) => action,
            Err(error) => {
                adapter.pause_error = Some(error.to_string());
                DebugAction::Abort
            }
        }
    });

    let result = vm.run_chunk(&chunk);

    {
        let mut adapter = adapter.borrow_mut();
        adapter.send_output("stdout", vm.stdout())?;
        adapter.send_output("stderr", vm.stderr())?;
        if let Some(error) = adapter.pause_error.take() {
            adapter.send_output("stderr", &format!("debug adapter error: {error}\n"))?;
            adapter.send_event("terminated", json!({}))?;
            bail!("debug adapter error: {error}");
        }
    }

    match result {
        Ok(()) => {
            adapter.borrow_mut().send_event("terminated", json!({}))?;
            Ok(())
        }
        Err(ricochet_vm::VmError::ExitRequested { .. }) => {
            adapter.borrow_mut().send_event("terminated", json!({}))?;
            Ok(())
        }
        Err(error) => {
            let message = runtime_error_message(&vm, &error);
            let mut adapter = adapter.borrow_mut();
            adapter.send_output("stderr", &format!("{message}\n"))?;
            adapter.send_event("terminated", json!({}))?;
            bail!("{message}");
        }
    }
}

fn read_dap_message(reader: &mut impl BufRead) -> Result<Option<serde_json::Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(&['\r', '\n'][..]);
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse::<usize>()?);
            }
        }
    }

    let content_length = content_length.context("DAP message missing Content-Length header")?;
    let mut content = vec![0; content_length];
    reader.read_exact(&mut content)?;
    let message = serde_json::from_slice(&content)?;
    Ok(Some(message))
}

fn write_dap_message(writer: &mut impl Write, message: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", content.len())?;
    writer.write_all(&content)?;
    writer.flush()?;
    Ok(())
}

fn dap_request_command(request: &serde_json::Value) -> &str {
    request
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
}

fn dap_request_seq(request: &serde_json::Value) -> i64 {
    request
        .get("seq")
        .and_then(|value| value.as_i64())
        .unwrap_or(0)
}

fn dap_source_location(source: &str) -> (String, usize) {
    let before_frame = source.split_once(" [").map_or(source, |(source, _)| source);
    before_frame
        .rsplit_once(':')
        .and_then(|(path, line)| {
            line.parse::<usize>()
                .ok()
                .map(|line| (path.to_string(), line))
        })
        .unwrap_or_else(|| (before_frame.to_string(), 1))
}

fn dap_binding_variables(bindings: &[(String, Value)]) -> Vec<serde_json::Value> {
    bindings
        .iter()
        .map(|(name, value)| dap_value_variable(name.clone(), value))
        .collect()
}

fn dap_value_variable(name: String, value: &Value) -> serde_json::Value {
    json!({
        "name": name,
        "value": format!("{value:?}"),
        "variablesReference": 0,
    })
}

fn dap_task_variable(task: &DebugTask) -> serde_json::Value {
    json!({
        "name": task.id.to_string(),
        "value": format!(
            "{} pending={} running={} completed={} failed={}",
            task.status, task.pending, task.running, task.completed, task.failed
        ),
        "variablesReference": 0,
    })
}

fn cli_vm(args: Vec<String>, capabilities: &CapabilityOptions) -> Result<Vm> {
    let mut vm = Vm::default();
    capabilities.apply_to(&mut vm)?;
    vm.set_program_args(args);
    vm.set_input_reader(|| {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| error.to_string())
            .map(|read| (read > 0).then_some(line))
    });
    Ok(vm)
}

fn read_terminal_debug_action(pause: &DebugPause) -> DebugAction {
    loop {
        print!("debug> ");
        if io::stdout().flush().is_err() {
            return DebugAction::Abort;
        }

        let mut command = String::new();
        match io::stdin().read_line(&mut command) {
            Ok(0) | Err(_) => return DebugAction::Abort,
            Ok(_) => match command.trim().to_ascii_lowercase().as_str() {
                "" | "s" | "step" => return DebugAction::Step,
                "n" | "next" | "over" | "step-over" => return DebugAction::StepOver,
                "o" | "out" | "step-out" => return DebugAction::StepOut,
                "c" | "continue" => return DebugAction::Continue,
                "a" | "abort" | "q" | "quit" => return DebugAction::Abort,
                "stack" => println!("{:?}", pause.stack),
                "locals" => print_debug_bindings("locals", &pause.locals),
                "globals" => print_debug_bindings("globals", &pause.globals),
                "self" => println!("{:?}", pause.current_self),
                "tasks" => println!("{:?}", pause.tasks),
                _ => println!(
                    "commands: step, next, out, continue, abort, stack, locals, globals, self, tasks"
                ),
            },
        }
    }
}

fn print_debug_bindings(label: &str, bindings: &[(String, Value)]) {
    if bindings.is_empty() {
        println!("{label}: <empty>");
        return;
    }
    println!("{label}:");
    for (name, value) in bindings {
        println!("  {name} = {value:?}");
    }
}

fn run_tests(
    path: &str,
    debug: bool,
    filter: Option<&str>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let path = Path::new(path);
    let project_root = find_project_root_for_tests(path);
    let files = collect_test_files_for_path(path, project_root.as_deref())?;
    let mut total = 0usize;
    let mut failed = 0usize;

    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        if filter.is_some_and(|filter| !test_file_may_match_filter(&source, filter)) {
            continue;
        }
        let chunk = compile_source_file(&file)?;
        let mut vm = Vm::default();
        capabilities.apply_to(&mut vm)?;
        if debug {
            vm.enable_debug();
            vm.set_debug_sink(print_debug_event);
        }

        if let Some(project_root) = project_root.as_deref() {
            load_app_sources(&mut vm, project_root)?;
        }

        vm.run_chunk(&chunk)
            .with_context(|| format!("failed to load test file {}", file.display()))?;

        let tests = vm.test_methods();
        for (class_name, method_name) in tests {
            let test_name = format!("{class_name}.{method_name}");
            if let Some(filter) = filter {
                if !test_name.contains(filter) {
                    continue;
                }
            }

            total += 1;
            let instance = vm
                .new_instance(&class_name)
                .with_context(|| format!("failed to instantiate test case {class_name}"))?;
            match vm.call_method_value(instance, &method_name) {
                Ok(_) => println!("PASS {test_name}"),
                Err(error) => {
                    failed += 1;
                    println!("FAIL {test_name}: {error}");
                }
            }
        }
    }

    println!("{total} tests, {failed} failed");
    if failed > 0 {
        bail!(
            "{} Ricochet test{} failed",
            failed,
            if failed == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

fn test_file_may_match_filter(source: &str, filter: &str) -> bool {
    source.contains(filter)
        || filter.split_once('.').is_some_and(|(class, method)| {
            !class.is_empty()
                && source.contains(class)
                && !method.is_empty()
                && source.contains(method)
        })
}

fn find_project_root_for_tests(path: &Path) -> Option<PathBuf> {
    let canonical = fs::canonicalize(path).ok()?;
    let start: &Path = if canonical.is_file() {
        canonical.parent()?
    } else {
        &canonical
    };

    start
        .ancestors()
        .find(|ancestor| ancestor.join("ricochet.toml").is_file())
        .map(Path::to_path_buf)
}

fn collect_test_files_for_path(path: &Path, project_root: Option<&Path>) -> Result<Vec<PathBuf>> {
    if let Some(project_root) = project_root {
        if paths_point_to_same_location(path, project_root) {
            let tests_path = project_root.join("tests");
            if !tests_path.exists() {
                return Ok(Vec::new());
            }
            return collect_test_files(&tests_path);
        }
    }

    collect_test_files(path)
}

fn paths_point_to_same_location(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn load_app_sources(vm: &mut Vm, project_root: &Path) -> Result<()> {
    for file in collect_app_source_files(project_root)? {
        let chunk = compile_source_file(&file)
            .with_context(|| format!("failed to compile app source {}", file.display()))?;
        vm.run_chunk(&chunk)
            .with_context(|| format!("failed to load app source {}", file.display()))?;
    }

    Ok(())
}

fn collect_app_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let app_path = project_root.join("app");
    if !app_path.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    for directory in ["Models", "Services", "Controllers"] {
        let source_path = app_path.join(directory);
        if source_path.is_dir() {
            push_unique_rco_files(&source_path, &mut seen, &mut files)?;
        }
    }

    push_unique_rco_files(&app_path, &mut seen, &mut files)?;
    Ok(files)
}

fn push_unique_rco_files(
    path: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut source_files = Vec::new();
    collect_rco_files(path, &mut source_files)?;
    source_files.sort();
    for file in source_files {
        if seen.insert(file.clone()) {
            files.push(file);
        }
    }

    Ok(())
}

fn print_debug_event(event: &DebugEvent) {
    match event {
        DebugEvent::Paused(pause) => {
            let reason = match pause.reason {
                DebugPauseReason::Step => "step",
                DebugPauseReason::Breakpoint => "breakpoint",
            };
            println!(
                "PAUSE {reason} {} [{}] {}",
                pause.source, pause.frame, pause.opcode
            );
            println!("  stack:  {:?}", pause.stack);
            if !pause.locals.is_empty() {
                println!("  locals: {:?}", pause.locals);
            }
            if !pause.globals.is_empty() {
                println!("  globals: {:?}", pause.globals);
            }
            if let Some(current_self) = &pause.current_self {
                println!("  self:   {current_self:?}");
            }
            if !pause.tasks.is_empty() {
                println!("  tasks:  {:?}", pause.tasks);
            }
        }
        DebugEvent::Instruction {
            frame,
            source,
            opcode,
            stack_before,
            stack_after,
        } => {
            println!("TRACE {source} [{frame}] {opcode}");
            println!("  before: {stack_before:?}");
            println!("  after:  {stack_after:?}");
        }
        DebugEvent::Fault {
            frame,
            message,
            stack,
        } => {
            println!("FAULT [{frame}] {message}");
            println!("  stack:  {stack:?}");
        }
    }
}

fn build(path: &str) -> Result<()> {
    let chunk = compile_source_file(Path::new(path))?;
    let output = Path::new(BUILD_OUTPUT);
    let parent = output
        .parent()
        .expect("build output path should include parent directory");

    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(output, chunk.to_bytes()?)
        .with_context(|| format!("failed to write {}", output.display()))?;

    println!("built {}", output.display());

    Ok(())
}

fn package(path: &str, output: &Path, options: PackageOptions<'_>) -> Result<()> {
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
            options.package_description,
            options.gui,
        )?;
    }

    Ok(())
}

struct PackageOptions<'a> {
    tui: bool,
    gui: bool,
    mvc: bool,
    gui_launcher: Option<&'a Path>,
    linux_packages: &'a [LinuxPackageFormat],
    package_name: Option<&'a str>,
    package_version: &'a str,
    package_description: &'a str,
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

fn extract_embedded_mvc_bundle(bundle: &MvcBundle) -> Result<PathBuf> {
    let root = unique_mvc_extract_dir()?;
    bundle.extract_to(&root)?;
    Ok(root)
}

fn unique_mvc_extract_dir() -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let base = std::env::temp_dir();
    for attempt in 0..100 {
        let path = base.join(format!(
            "ricochet-mvc-{}-{millis}-{attempt}",
            std::process::id()
        ));
        if !path.exists() {
            fs::create_dir_all(&path)
                .with_context(|| format!("failed to create {}", path.display()))?;
            return Ok(path);
        }
    }
    bail!("failed to find an unused MVC extraction directory")
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

    for format in unique_formats {
        match format {
            LinuxPackageFormat::Tar => create_linux_tarball(
                executable,
                &artifact_dir,
                &staging_root,
                &name,
                package_version,
                &description,
                gui,
            )?,
            LinuxPackageFormat::Deb => create_linux_deb(
                executable,
                &artifact_dir,
                &staging_root,
                &name,
                package_version,
                &description,
                gui,
            )?,
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
    Ok(())
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
    let root = Path::new("target")
        .join("ricochet-package")
        .join(format!("{name}-{version}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&root).with_context(|| format!("failed to create {}", root.display()))?;
    Ok(root)
}

fn create_linux_tarball(
    executable: &Path,
    artifact_dir: &Path,
    staging_root: &Path,
    name: &str,
    version: &str,
    description: &str,
    gui: bool,
) -> Result<()> {
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
                "\nLinux GUI apps require the WebKitGTK 4.1 runtime package, for example `libwebkit2gtk-4.1-0` on Debian/Ubuntu.\n"
            } else {
                ""
            }
        ),
    )
    .with_context(|| format!("failed to write {}", package_dir.join("README.txt").display()))?;
    write_linux_install_script(&package_dir.join("install.sh"), name)?;

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
    name: &str,
    version: &str,
    description: &str,
    gui: bool,
) -> Result<()> {
    let deb_path = artifact_dir.join(format!("{name}_{version}_amd64.deb"));
    assert_new_artifact(&deb_path)?;

    let deb_root = staging_root.join(format!("{name}-deb-root"));
    let control_dir = deb_root.join("DEBIAN");
    let bin_dir = deb_root.join("usr/bin");
    let doc_dir = deb_root.join("usr/share/doc").join(name);

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
        control_dir.join("control"),
        format!(
            "Package: {name}\nVersion: {version}\nSection: devel\nPriority: optional\nArchitecture: amd64\n{}Maintainer: Ricochet Packager <noreply@ricochet.today>\nDescription: {description}\n",
            if gui {
                "Depends: libwebkit2gtk-4.1-0, libgtk-3-0\n"
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

fn write_linux_install_script(path: &Path, binary_name: &str) -> Result<()> {
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

fn embedded_app_from_current_exe() -> Result<Option<EmbeddedApp>> {
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

fn format_path(path: &str, check: bool) -> Result<()> {
    let path = Path::new(path);
    if path.is_file() {
        let changed = format_file(path, check)?;
        if check && changed {
            bail!("format check failed");
        }
        return Ok(());
    }
    if !path.is_dir() {
        bail!("fmt path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();

    let mut changed = false;
    for file in files {
        changed |= format_file(&file, check)?;
    }
    if check && changed {
        bail!("format check failed");
    }

    Ok(())
}

fn format_file(path: &Path, check: bool) -> Result<bool> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let formatted =
        format_source(&source).with_context(|| format!("failed to format {}", path.display()))?;
    if source == formatted {
        if check {
            println!("checked {}", path.display());
        }
        return Ok(false);
    }

    if check {
        eprintln!("{} would reformat", path.display());
        return Ok(true);
    }

    fs::write(path, formatted).with_context(|| format!("failed to write {}", path.display()))?;
    println!("formatted {}", path.display());
    Ok(true)
}

fn compile_source_file(source_path: &Path) -> Result<Chunk> {
    compile_file_with_imports(source_path)
}

fn collect_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("test path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rco_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_rco_files(&entry_path, files)?;
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("rco") {
            files.push(entry_path);
        }
    }
    Ok(())
}
