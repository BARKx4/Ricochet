use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::convert::Infallible;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::io::{self, BufRead, IsTerminal, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use ricochet_bytecode::{Chunk, Op, SourceSpan};
use ricochet_compiler::{
    compile_file_with_imports, compile_source, expand_file_with_imports,
    resolve_import_with_metadata, verify_runtime_import_locks_for_parent, CompileError,
};
use ricochet_sandbox::DestinationGrant;
use ricochet_syntax::formatter::format_module;
use ricochet_syntax::{
    format_source, lex, line_column, line_starts, parse_module, utf16_range_for_span, ArgsDecl,
    Expr, Item as SyntaxItem, LexError, Module, ParseError, SourceDiagnostic, Span, SpannedExpr,
    TokenKind,
};
use ricochet_vm::{
    DebugAction, DebugControl, DebugEvent, DebugPause, DebugPauseReason, DebugTask, DebugTaskFrame,
    DynamicModuleSource, MapValue, RicochetResult, StrictnessConfig, StrictnessDiagnostic, Value,
    Vm, VmImage,
};
use ricochet_web::{
    install_project_database_runtime, DatabaseBackend, MysqlDatabase, PostgresDatabase,
    SqliteDatabase,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use toml_edit::{value, DocumentMut, Item, Table};
use tower::ServiceExt;

mod commands;
mod debug_protocol;
mod hosted_registry;
mod hosted_registry_server;
mod lsp;
mod migration_dsl;
pub mod secure_action;
pub mod secure_prompt;
mod static_registry;

#[cfg(feature = "test-host")]
pub use commands::gui::test_host as gui_test_host;

use commands::package::{EmbeddedAppKind, EmbeddedAppPayload, LinuxPackageFormat, PackageOptions};
use debug_protocol::{
    apply_debug_control_command, debug_command_from_command, debug_command_from_web_request,
    debug_command_label, debug_command_resumes, debug_event_json, debug_value_label, DebugCommand,
    DebugWebControlRequest,
};

const DEFAULT_BUILD_SOURCE: &str = "main.rco";
const BUILD_OUTPUT: &str = "build/app.rcob";
const EXPAND_JSON_SCHEMA: &str = "ricochet.expand.v1";
const EXPAND_JSON_SCHEMA_VERSION: u32 = 1;
const EMBEDDED_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_APP_V1\0";
const EMBEDDED_TUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_TUI_APP_V1\0";
const EMBEDDED_GUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_GUI_APP_V1\0";
const EMBEDDED_MVC_GUI_APP_MARKER: &[u8] = b"\nRICOCHET_EMBEDDED_MVC_GUI_APP_V1\0";
const MVC_BUNDLE_MAGIC: &[u8] = b"RICOCHET_MVC_BUNDLE_V1\0";
const GUI_EXPORT_HTML_ENV: &str = "RICOCHET_GUI_EXPORT_HTML";
const GUI_EXPORT_PATH_ENV: &str = "RICOCHET_GUI_EXPORT_PATH";
const GUI_EVENT_ENV: &str = "RICOCHET_GUI_EVENT";
#[cfg(target_os = "linux")]
const GUI_EXTERNAL_BROWSER_ENV: &str = "RICOCHET_GUI_EXTERNAL_BROWSER";
const RICOCHET_QUIT_ACTION: &str = "__ricochet_quit";
const RICOCHET_COPY_ACTION: &str = "__ricochet_copy";
const RICOCHET_PASTE_ACTION: &str = "__ricochet_paste";
const DEFAULT_MVC_GUI_TITLE: &str = "Ricochet MVC App";
const DEFAULT_MVC_GUI_WIDTH: u32 = 1100;
const DEFAULT_MVC_GUI_HEIGHT: u32 = 760;
const CLI_PARSE_STACK_SIZE: usize = 8 * 1024 * 1024;
#[derive(Debug, Parser)]
#[command(name = "rco")]
#[command(about = "Ricochet language toolchain")]
#[command(version)]
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
        #[arg(
            long = "strict",
            help = "Emit strictness warnings for dynamic convenience fallbacks"
        )]
        strict: bool,
        path: Option<String>,
    },
    Expand {
        path: String,
        #[arg(long, help = "Emit macro expansion details as JSON")]
        json: bool,
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
    DebugTui {
        #[arg(long, help = "Render one read-only debugger snapshot and exit")]
        smoke: bool,
        #[arg(
            long = "command",
            value_name = "ACTION",
            help = "Run a scripted debugger action; repeat for step, next, out, continue, or abort"
        )]
        commands: Vec<String>,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    DebugWeb {
        #[arg(
            long,
            help = "Render one read-only debugger HTML snapshot to stdout and exit"
        )]
        smoke: bool,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
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
    Seed {
        path: Option<String>,
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
        #[arg(long, help = "Package as a desktop GUI app using the rco-gui launcher")]
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
            long = "package-license",
            value_name = "SPDX",
            help = "Set the AppStream project license for GUI Linux packages; required with --gui and --linux-package"
        )]
        package_license: Option<String>,
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
            help = "Use a static registry index URL or hosted registry base URL for registry:name dependencies"
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
        registry: Option<PathBuf>,
        #[arg(
            long = "registry-url",
            value_name = "URL",
            help = "Publish to a hosted registry base URL"
        )]
        registry_url: Option<String>,
        #[arg(
            long = "token-env",
            value_name = "NAME",
            help = "Resolve the hosted registry bearer token from environment variable NAME"
        )]
        token_env: Option<String>,
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
            help = "Search a static registry index URL or hosted registry base URL"
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
    Lint {
        path: Option<String>,
        #[arg(long, help = "Emit lint diagnostics as JSON")]
        json: bool,
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
        #[arg(
            long = "allow-http-destination",
            value_name = "HOST:PORT",
            value_parser = parse_http_destination_arg,
            help = "Allow deferred HTTP credentials only for exact public HOST:PORT; repeat for multiple destinations"
        )]
        http_destinations: Vec<String>,
        #[arg(
            long = "ai-allow-host",
            value_name = "HOST",
            help = "Allow MVC AI providers to send requests only to HOST; repeat for multiple hosts"
        )]
        ai_allow_hosts: Vec<String>,
        #[arg(
            long = "database-allow-host",
            value_name = "HOST",
            help = "Allow MVC remote database connections only to HOST; repeat for multiple hosts"
        )]
        database_allow_hosts: Vec<String>,
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

#[derive(Debug)]
enum ImageCommand {
    Save {
        path: PathBuf,
        source: Option<PathBuf>,
    },
    Inspect {
        path: PathBuf,
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum MigrateCommand {
    New {
        name: String,
        #[arg(long, help = "Create paired Ricochet migration DSL files")]
        dsl: bool,
        path: Option<String>,
    },
    Status {
        path: Option<String>,
    },
    Apply {
        path: Option<String>,
    },
    Rollback {
        path: Option<String>,
        #[arg(long, default_value_t = 1)]
        steps: usize,
    },
    Dump {
        path: Option<String>,
        #[arg(long, value_name = "PATH", default_value = "db/schema.sql")]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    Rebuild {
        path: PathBuf,
    },
    Check {
        path: PathBuf,
    },
    Mirror {
        #[arg(value_name = "REGISTRY_URL")]
        registry_url: String,
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    Serve {
        path: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 3001)]
        port: u16,
        #[arg(
            long = "token-env",
            value_name = "NAME",
            help = "Allow the bearer token from environment variable NAME to publish or yank any package"
        )]
        token_env: Vec<String>,
        #[arg(
            long = "publisher",
            value_name = "PACKAGE=ENV",
            help = "Allow the bearer token from ENV to publish or yank PACKAGE or @scope/*; repeat for multiple publishers"
        )]
        publisher: Vec<String>,
    },
    Yank {
        package: String,
        #[arg(value_name = "VERSION")]
        package_version: String,
        #[arg(
            long = "registry-url",
            value_name = "URL",
            help = "Use a hosted registry base URL"
        )]
        registry_url: String,
        #[arg(
            long = "token-env",
            value_name = "NAME",
            help = "Resolve the hosted registry bearer token from environment variable NAME"
        )]
        token_env: String,
    },
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
    #[arg(
        long = "allow-http-destination",
        value_name = "HOST:PORT",
        value_parser = parse_http_destination_arg,
        help = "Allow deferred HTTP credentials only for exact public HOST:PORT; repeat for multiple destinations"
    )]
    http_destinations: Vec<String>,
    #[arg(
        long,
        help = "Enable outbound TCP and WebSocket socket capabilities for this run"
    )]
    allow_sockets: bool,
    #[arg(
        long = "socket-allow-host",
        value_name = "HOST",
        help = "Allow TCP/WebSocket connects and listener binds only for HOST; repeat for multiple hosts"
    )]
    socket_allow_hosts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum CapabilityProfile {
    #[default]
    Trusted,
    Sandboxed,
}

fn parse_http_destination_arg(value: &str) -> std::result::Result<String, String> {
    DestinationGrant::parse(value)
        .map(|destination| destination.to_string())
        .map_err(|error| error.to_string())
}

fn parsed_http_destinations(values: &[String]) -> Vec<DestinationGrant> {
    values
        .iter()
        .map(|value| {
            DestinationGrant::parse(value)
                .expect("CLI destination values are validated and canonicalized by clap")
        })
        .collect()
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
        if self.no_http && !self.http_destinations.is_empty() {
            bail!("--allow-http-destination cannot be used with --no-http");
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
        let socket_enabled = self.allow_sockets || !self.socket_allow_hosts.is_empty();
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
        vm.set_socket_enabled(socket_enabled);
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
        vm.set_http_allowed_destinations(parsed_http_destinations(&self.http_destinations));
        if self.socket_allow_hosts.is_empty() {
            vm.clear_socket_allowed_hosts();
        } else {
            vm.set_socket_allowed_hosts(self.socket_allow_hosts.clone());
        }
        Ok(())
    }
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn run_on_cli_parse_stack<T, F>(name: &'static str, f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(CLI_PARSE_STACK_SIZE)
        .spawn(f)
        .expect("failed to spawn CLI parser thread");
    match handle.join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn parse_cli() -> Cli {
    run_on_cli_parse_stack("rco-cli-parse", Cli::parse)
}

fn parse_cli_from(args: Vec<OsString>) -> Cli {
    run_on_cli_parse_stack("rco-cli-parse-from", move || Cli::parse_from(args))
}

pub async fn run_cli() -> Result<()> {
    if let Some(app) = commands::package::embedded_app_from_current_exe()? {
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
                        dynamic_import_parent: current_dir_for_dynamic_imports()?,
                    },
                )?
            }
            EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Tui => {
                run_embedded_tui_app(&chunk, std::env::args().skip(1).collect())?
            }
            EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Gui => {
                commands::gui::run_embedded_gui_app(&chunk, std::env::args().skip(1).collect())?
            }
            EmbeddedAppPayload::MvcBundle(bundle) if app.kind == EmbeddedAppKind::MvcGui => {
                commands::gui::run_embedded_mvc_gui_app(bundle, std::env::args().skip(1).collect())
                    .await?
            }
            _ => bail!("embedded Ricochet app payload does not match its marker"),
        }
        return Ok(());
    }

    if dispatch_manual_command()? {
        return Ok(());
    }

    let cli = parse_cli();
    match cli.command {
        Command::New { path, with_sqlite } => {
            new_project(Path::new(&path), NewProjectOptions { with_sqlite })?
        }
        Command::Check { path, strict } => check(path.as_deref().unwrap_or("."), strict)?,
        Command::Expand { path, json } => expand_path(&path, json)?,
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
                None,
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
        Command::DebugTui {
            smoke,
            commands,
            step,
            breakpoints,
            capabilities,
            path,
            args,
        } => run_debug_tui(
            &path,
            DebugTuiOptions {
                smoke,
                commands,
                step,
                breakpoints: &breakpoints,
            },
            args,
            capabilities,
        )?,
        Command::DebugWeb {
            smoke,
            host,
            port,
            step,
            breakpoints,
            capabilities,
            path,
            args,
        } => {
            run_debug_web(
                &path,
                DebugWebOptions {
                    smoke,
                    host: &host,
                    port,
                    step,
                    breakpoints: &breakpoints,
                },
                args,
                capabilities,
            )
            .await?
        }
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
        Command::Seed { path } => seed(Path::new(path.as_deref().unwrap_or("."))).await?,
        Command::Gui {
            capabilities,
            path,
            args,
        } => commands::gui::run_gui_file(&path, args, capabilities)?,
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
            package_license,
            package_description,
        } => commands::package::package(
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
                package_license: package_license.as_deref(),
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
            registry_url,
            token_env,
            provenance_file,
            signature_file,
            signature_kind,
            dry_run,
        } => publish_package(
            path.as_deref(),
            registry.as_deref(),
            registry_url.as_deref(),
            PublishRegistryOptions {
                dry_run,
                token_env: token_env.as_deref(),
                provenance_file: provenance_file.as_deref(),
                signature_file: signature_file.as_deref(),
                signature_kind: signature_kind.as_deref(),
            },
        )?,
        Command::Registry { command } => match command {
            RegistryCommand::Rebuild { path } => static_registry::rebuild(&path)?,
            RegistryCommand::Check { path } => static_registry::check(&path)?,
            RegistryCommand::Mirror { registry_url, path } => {
                hosted_registry::mirror(&registry_url, &path)?
            }
            RegistryCommand::Serve {
                path,
                host,
                port,
                token_env,
                publisher,
            } => {
                hosted_registry_server::serve(hosted_registry_server::HostedRegistryServeOptions {
                    root: &path,
                    host: &host,
                    port,
                    token_envs: &token_env,
                    publishers: &publisher,
                })
                .await?
            }
            RegistryCommand::Yank {
                package,
                package_version,
                registry_url,
                token_env,
            } => hosted_registry::yank(&package, &package_version, &registry_url, &token_env)?,
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
        Command::Lint { path, json } => lint_path(path.as_deref().unwrap_or("."), json)?,
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
            http_destinations,
            ai_allow_hosts,
            database_allow_hosts,
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
                sqlite_data_root: None,
                http_allow_hosts,
                http_destinations: parsed_http_destinations(&http_destinations),
                ai_allow_hosts,
                database_allow_hosts,
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
    let app = commands::package::embedded_app_from_current_exe()?
        .context("rco-gui must be packaged with `rco package --gui` before it can launch an app")?;
    match app.payload {
        EmbeddedAppPayload::Chunk(chunk) if app.kind == EmbeddedAppKind::Gui => {
            commands::gui::run_embedded_gui_app(&chunk, std::env::args().skip(1).collect())
        }
        EmbeddedAppPayload::MvcBundle(bundle) if app.kind == EmbeddedAppKind::MvcGui => {
            commands::gui::run_embedded_mvc_gui_app(bundle, std::env::args().skip(1).collect())
                .await
        }
        _ => {
            bail!("rco-gui can only launch apps packaged with `rco package --gui`");
        }
    }
}

fn dispatch_manual_command() -> Result<bool> {
    let mut args = std::env::args_os();
    let executable = args.next().unwrap_or_else(|| OsString::from("rco"));
    let Some(command) = args.next() else {
        return Ok(false);
    };

    if is_help_arg(&command) {
        print_root_help_with_manual_commands()?;
        return Ok(true);
    }

    if command == OsStr::new("help") {
        let rest = args.collect::<Vec<_>>();
        if rest.is_empty() {
            print_root_help_with_manual_commands()?;
            return Ok(true);
        }
        if rest.len() == 1 && rest[0] == OsStr::new("image") {
            print_image_help();
            return Ok(true);
        }
        if rest.len() == 1 && rest[0] == OsStr::new("emit-source") {
            print_emit_source_help();
            return Ok(true);
        }
        return Ok(false);
    }

    if command == OsStr::new("repl") {
        return dispatch_repl_with_image(executable, args.collect());
    }

    if command == OsStr::new("image") {
        dispatch_image_command(args.collect())?;
        return Ok(true);
    }

    if command == OsStr::new("emit-source") {
        dispatch_emit_source_command(args.collect())?;
        return Ok(true);
    }

    Ok(false)
}

fn dispatch_repl_with_image(executable: OsString, args: Vec<OsString>) -> Result<bool> {
    if !args
        .iter()
        .any(|arg| arg == OsStr::new("--image") || arg.to_string_lossy().starts_with("--image="))
    {
        return Ok(false);
    }

    let mut image_path = None;
    let mut filtered = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let argument = &args[index];
        if argument == OsStr::new("--image") {
            if image_path.is_some() {
                bail!("--image can only be provided once");
            }
            index += 1;
            let Some(value) = args.get(index) else {
                bail!("--image requires a path");
            };
            image_path = Some(PathBuf::from(value));
            index += 1;
            continue;
        }
        if let Some(value) = argument.to_string_lossy().strip_prefix("--image=") {
            if image_path.is_some() {
                bail!("--image can only be provided once");
            }
            image_path = Some(PathBuf::from(value));
            index += 1;
            continue;
        }
        filtered.push(argument.clone());
        index += 1;
    }

    let cli_args = std::iter::once(executable)
        .chain(std::iter::once(OsString::from("repl")))
        .chain(filtered)
        .collect::<Vec<_>>();
    let cli = parse_cli_from(cli_args);
    let Command::Repl {
        debug,
        capabilities,
    } = cli.command
    else {
        unreachable!("manual repl dispatch must parse as repl");
    };
    let image_path = image_path.expect("--image presence was checked");
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let stdout = io::stdout();
    run_repl(
        stdin.lock(),
        stdout.lock(),
        debug,
        interactive,
        Some(&image_path),
        capabilities,
    )?;
    Ok(true)
}

fn dispatch_image_command(args: Vec<OsString>) -> Result<()> {
    let Some(command) = args.first() else {
        print_image_help();
        return Ok(());
    };
    if is_help_arg(command) {
        print_image_help();
        return Ok(());
    }

    match command.to_string_lossy().as_ref() {
        "save" => dispatch_image_save(args[1..].to_vec()),
        "inspect" => dispatch_image_inspect(args[1..].to_vec()),
        "help" => {
            print_image_help();
            Ok(())
        }
        other => bail!("unknown image command: {other}"),
    }
}

fn dispatch_image_save(args: Vec<OsString>) -> Result<()> {
    if args.iter().any(is_help_arg) {
        print_image_save_help();
        return Ok(());
    }
    let Some(path) = args.first() else {
        bail!("usage: rco image save <PATH> [--source <FILE>]");
    };
    if is_flag(path) {
        bail!("image save path is required before options");
    }
    let mut source = None;
    let mut index = 1;
    while index < args.len() {
        if args[index] == OsStr::new("--source") {
            index += 1;
            let Some(value) = args.get(index) else {
                bail!("--source requires a file path");
            };
            source = Some(PathBuf::from(value));
            index += 1;
        } else {
            bail!(
                "unknown image save option: {}",
                args[index].to_string_lossy()
            );
        }
    }
    image_command(ImageCommand::Save {
        path: PathBuf::from(path),
        source,
    })
}

fn dispatch_image_inspect(args: Vec<OsString>) -> Result<()> {
    if args.iter().any(is_help_arg) {
        print_image_inspect_help();
        return Ok(());
    }
    let Some(path) = args.first() else {
        bail!("usage: rco image inspect <PATH> [--json]");
    };
    if is_flag(path) {
        bail!("image inspect path is required before options");
    }
    let mut json = false;
    for option in &args[1..] {
        if option == OsStr::new("--json") {
            json = true;
        } else {
            bail!("unknown image inspect option: {}", option.to_string_lossy());
        }
    }
    image_command(ImageCommand::Inspect {
        path: PathBuf::from(path),
        json,
    })
}

fn dispatch_emit_source_command(args: Vec<OsString>) -> Result<()> {
    if args.is_empty() || args.iter().any(is_help_arg) {
        print_emit_source_help();
        return Ok(());
    }
    if args.len() != 1 {
        bail!("usage: rco emit-source <FILE_OR_BYTECODE>");
    }
    emit_source(Path::new(&args[0]))
}

fn is_help_arg(value: &OsString) -> bool {
    value == OsStr::new("-h") || value == OsStr::new("--help")
}

fn is_flag(value: &OsString) -> bool {
    value.to_string_lossy().starts_with('-')
}

fn print_root_help_with_manual_commands() -> Result<()> {
    let bytes = run_on_cli_parse_stack("rco-cli-help", || {
        let mut bytes = Vec::new();
        Cli::command()
            .write_help(&mut bytes)
            .context("failed to render CLI help")?;
        Ok::<_, anyhow::Error>(bytes)
    })?;
    let mut help = String::from_utf8(bytes).context("CLI help was not valid UTF-8")?;
    let manual_commands =
        "  image            Save and inspect persistent Ricochet VM images\n  emit-source      Emit readable Ricochet-like source from source or bytecode\n";

    if let Some(index) = help.find("  help             Print this message") {
        help.insert_str(index, manual_commands);
    } else if let Some(index) = help.find("\nOptions:") {
        help.insert_str(index, manual_commands);
    } else {
        help.push_str(manual_commands);
    }

    print!("{help}");
    Ok(())
}

fn print_image_help() {
    println!("Save and inspect persistent Ricochet VM images");
    println!();
    println!("Usage: rco image <COMMAND>");
    println!();
    println!("Commands:");
    println!("  save <PATH> [--source <FILE>]");
    println!("  inspect <PATH> [--json]");
}

fn print_image_save_help() {
    println!("Save a persistent Ricochet VM image");
    println!();
    println!("Usage: rco image save <PATH> [--source <FILE>]");
}

fn print_image_inspect_help() {
    println!("Inspect a persistent Ricochet VM image");
    println!();
    println!("Usage: rco image inspect <PATH> [--json]");
}

fn print_emit_source_help() {
    println!("Emit readable Ricochet-like source from source or bytecode");
    println!();
    println!("Usage: rco emit-source <FILE_OR_BYTECODE>");
}

fn run_repl<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    debug: bool,
    interactive: bool,
    image_path: Option<&Path>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let mut vm = Vm::default();
    capabilities.apply_to(&mut vm)?;
    install_dynamic_module_loader(&mut vm, current_dir_for_dynamic_imports()?);
    if let Some(path) = image_path {
        if path.is_file() {
            let image = read_vm_image(path)?;
            vm.restore_image(image)
                .with_context(|| format!("failed to load REPL image {}", path.display()))?;
        } else if path.exists() {
            bail!("REPL image path is not a file: {}", path.display());
        }
    }
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
                save_repl_image_if_requested(&vm, image_path)?;
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
                    save_repl_image_if_requested(&vm, image_path)?;
                    Ok(())
                }
            };
        }

        source.push_str(&line);
        if source.trim_start().starts_with(':')
            && handle_repl_command(&source, &mut vm, image_path, &mut output)?
        {
            source.clear();
            continue;
        }
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

fn handle_repl_command<W: Write>(
    source: &str,
    vm: &mut Vm,
    image_path: Option<&Path>,
    output: &mut W,
) -> Result<bool> {
    let command = source.trim();
    let Some(command) = command.strip_prefix(':') else {
        return Ok(false);
    };
    let mut parts = command.split_whitespace();
    let Some(name) = parts.next() else {
        return Ok(false);
    };

    match name {
        "save" => {
            let path = repl_command_image_path(parts.next(), image_path, ":save")?;
            if parts.next().is_some() {
                bail!(":save accepts at most one image path");
            }
            write_vm_image(&path, &vm.to_image()?)?;
            writeln!(output, "saved image {}", path.display())?;
            output.flush()?;
            Ok(true)
        }
        "load" => {
            let path = repl_command_image_path(parts.next(), image_path, ":load")?;
            if parts.next().is_some() {
                bail!(":load accepts at most one image path");
            }
            let image = read_vm_image(&path)?;
            vm.restore_image(image)
                .with_context(|| format!("failed to load REPL image {}", path.display()))?;
            writeln!(output, "loaded image {}", path.display())?;
            output.flush()?;
            Ok(true)
        }
        "bindings" => {
            let image = vm.to_image()?;
            writeln!(
                output,
                "stack={} variables=[{}] functions=[{}] classes=[{}]",
                image.stack.len(),
                image
                    .variables
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                image
                    .functions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                image.classes.keys().cloned().collect::<Vec<_>>().join(", ")
            )?;
            output.flush()?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn repl_command_image_path(
    explicit: Option<&str>,
    image_path: Option<&Path>,
    command: &str,
) -> Result<PathBuf> {
    explicit
        .map(PathBuf::from)
        .or_else(|| image_path.map(Path::to_path_buf))
        .with_context(|| {
            format!("{command} requires a path when the REPL was not started with --image")
        })
}

fn save_repl_image_if_requested(vm: &Vm, image_path: Option<&Path>) -> Result<()> {
    if let Some(path) = image_path {
        write_vm_image(path, &vm.to_image()?)?;
    }
    Ok(())
}

fn read_vm_image(path: &Path) -> Result<VmImage> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read image {}", path.display()))?;
    let image: VmImage = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to decode image {}", path.display()))?;
    image
        .validate_format()
        .with_context(|| format!("unsupported image {}", path.display()))?;
    Ok(image)
}

fn write_vm_image(path: &Path, image: &VmImage) -> Result<()> {
    image
        .validate_format()
        .context("refusing to write unsupported Ricochet image format")?;
    let bytes = serde_json::to_vec_pretty(image).context("failed to encode Ricochet image")?;
    fs::write(path, bytes).with_context(|| format!("failed to write image {}", path.display()))
}

fn image_command(command: ImageCommand) -> Result<()> {
    match command {
        ImageCommand::Save { path, source } => {
            let mut vm = Vm::default();
            CapabilityOptions::default().apply_to(&mut vm)?;
            if let Some(source) = source {
                let chunk = compile_source_file(&source)?;
                install_dynamic_module_loader(&mut vm, dynamic_import_parent_for_source(&source)?);
                vm.run_chunk(&chunk)
                    .with_context(|| format!("failed to run {}", source.display()))?;
            }
            let image = vm.to_image()?;
            write_vm_image(&path, &image)?;
            println!(
                "saved image {} ({} bindings, {} functions, {} classes)",
                path.display(),
                image.variables.len(),
                image.functions.len(),
                image.classes.len()
            );
            Ok(())
        }
        ImageCommand::Inspect { path, json } => {
            let image = read_vm_image(&path)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&image_summary_json(&image))?
                );
            } else {
                println!("format {}", image.format);
                println!("format_version {}", image.format_version);
                println!("ricochet_version {}", image.ricochet_version);
                println!("stack {}", image.stack.len());
                println!(
                    "variables {}",
                    image
                        .variables
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                println!(
                    "functions {}",
                    image
                        .functions
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                println!(
                    "classes {}",
                    image.classes.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            Ok(())
        }
    }
}

fn image_summary_json(image: &VmImage) -> serde_json::Value {
    json!({
        "format": image.format,
        "format_version": image.format_version,
        "ricochet_version": image.ricochet_version,
        "stack_len": image.stack.len(),
        "variables": image.variables.keys().cloned().collect::<Vec<_>>(),
        "functions": image.functions.keys().cloned().collect::<Vec<_>>(),
        "classes": image.classes.keys().cloned().collect::<Vec<_>>(),
    })
}

fn emit_source(path: &Path) -> Result<()> {
    let chunk = load_chunk_for_source_emission(path)?;
    print!("{}", emit_chunk_source_like(&chunk));
    Ok(())
}

fn load_chunk_for_source_emission(path: &Path) -> Result<Chunk> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if is_bytecode_path(path) {
        return Chunk::from_bytes(&bytes)
            .with_context(|| format!("failed to decode bytecode {}", path.display()));
    }

    match Chunk::from_bytes(&bytes) {
        Ok(chunk) => Ok(chunk),
        Err(_) => compile_source_file(path),
    }
}

fn is_bytecode_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rcob"))
}

fn emit_chunk_source_like(chunk: &Chunk) -> String {
    let mut lines = vec![format!(
        "(( emitted from bytecode chunk {} ))",
        ricochet_string_literal(&chunk.file)
    )];
    emit_chunk_body(chunk, 0, &mut lines);
    lines.push(String::new());
    lines.join("\n")
}

fn emit_chunk_body(chunk: &Chunk, indent: usize, lines: &mut Vec<String>) {
    for instruction in &chunk.instructions {
        emit_instruction_source_like(&instruction.op, chunk, indent, lines);
    }
}

fn emit_instruction_source_like(op: &Op, chunk: &Chunk, indent: usize, lines: &mut Vec<String>) {
    match op {
        Op::PushNil => push_source_line(lines, indent, "nil"),
        Op::PushBool(value) => {
            push_source_line(lines, indent, if *value { "true" } else { "false" })
        }
        Op::PushNumber(value) => push_source_line(lines, indent, value.to_string()),
        Op::PushFloat(value) => push_source_line(lines, indent, format_float_literal(*value)),
        Op::PushString(value) => push_source_line(lines, indent, ricochet_string_literal(value)),
        Op::PushBlock(index) => {
            push_source_line(lines, indent, "[");
            if let Some(block) = chunk.blocks.get(*index) {
                emit_chunk_body(block, indent + 1, lines);
            } else {
                push_source_line(lines, indent + 1, format!("(( invalid block {index} ))"));
            }
            push_source_line(lines, indent, "]");
        }
        Op::CallWord(word) => push_source_line(lines, indent, word),
        Op::CallMethod(method) => push_source_line(lines, indent, method),
        Op::Send => push_source_line(lines, indent, "send"),
        Op::GetVar(name) => push_source_line(lines, indent, format!("${name}")),
        Op::SetVar(name) => push_source_line(
            lines,
            indent,
            format!("(( set_var {} ))", source_name(name)),
        ),
        Op::DeclareVar(name) => {
            push_source_line(lines, indent, format!("{} var", source_name(name)))
        }
        Op::BeginClass { name, superclass } => push_source_line(
            lines,
            indent,
            format!("{} {} Subclass", source_name(name), source_name(superclass)),
        ),
        Op::EndClass => push_source_line(lines, indent, "end"),
        Op::AddField(name) => push_source_line(
            lines,
            indent,
            format!("{} Field", ricochet_string_literal(name)),
        ),
        Op::AddAccessor(name) => push_source_line(
            lines,
            indent,
            format!("{} Accessor", ricochet_string_literal(name)),
        ),
        Op::AddMethod { name, block, args } => {
            let prefix = args
                .as_ref()
                .map(|args| format!("{} ", format_args_spec(args)))
                .unwrap_or_default();
            push_source_line(lines, indent, format!("{prefix}["));
            if let Some(block) = chunk.blocks.get(*block) {
                emit_chunk_body(block, indent + 1, lines);
            } else {
                push_source_line(lines, indent + 1, format!("(( invalid block {block} ))"));
            }
            push_source_line(
                lines,
                indent,
                format!("] {} Method", ricochet_string_literal(name)),
            );
        }
        Op::AddFunction { name, block, args } => {
            let prefix = args
                .as_ref()
                .map(|args| format!("{} ", format_args_spec(args)))
                .unwrap_or_default();
            push_source_line(
                lines,
                indent,
                format!("{prefix}{} function", source_name(name)),
            );
            if let Some(block) = chunk.blocks.get(*block) {
                emit_chunk_body(block, indent + 1, lines);
            } else {
                push_source_line(lines, indent + 1, format!("(( invalid block {block} ))"));
            }
            push_source_line(lines, indent, "end");
        }
        Op::Return => push_source_line(lines, indent, "return"),
        Op::JumpIfFalse(target) => {
            push_source_line(lines, indent, format!("(( jump_if_false {target} ))"))
        }
        Op::Jump(target) => push_source_line(lines, indent, format!("(( jump {target} ))")),
        Op::Pop => push_source_line(lines, indent, "drop"),
    }
}

fn push_source_line(lines: &mut Vec<String>, indent: usize, line: impl AsRef<str>) {
    lines.push(format!("{}{}", "  ".repeat(indent), line.as_ref()));
}

fn format_args_spec(args: &ricochet_bytecode::ArgsSpec) -> String {
    let inputs = args.inputs.join(" ");
    let outputs = args.outputs.join(" ");
    format!("( {inputs} -> {outputs} )")
}

fn source_name(name: &str) -> String {
    if is_plain_source_name(name) {
        name.to_string()
    } else {
        ricochet_string_literal(name)
    }
}

fn is_plain_source_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(char::is_whitespace)
        && !name.contains('"')
        && !name.contains('\\')
        && !name.starts_with('$')
}

fn ricochet_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn format_float_literal(value: f64) -> String {
    if value.is_nan() {
        return "(( float NaN ))".to_string();
    }
    if value == f64::INFINITY {
        return "(( float Infinity ))".to_string();
    }
    if value == f64::NEG_INFINITY {
        return "(( float -Infinity ))".to_string();
    }

    let literal = value.to_string();
    if literal.contains(['.', 'e', 'E']) {
        literal
    } else {
        format!("{literal}.0")
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

const STRICT_CHECK_INSTRUCTION_LIMIT: u64 = 100_000;

fn check(path: &str, strict: bool) -> Result<()> {
    let path = Path::new(path);
    if path.is_file() {
        check_source_file(path, strict)?;
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
        check_source_file(file, strict)?;
    }

    println!(
        "checked {} Ricochet files in {}",
        files.len(),
        path.display()
    );
    Ok(())
}

fn check_source_file(path: &Path, strict: bool) -> Result<()> {
    let chunk = compile_source_file(path)
        .with_context(|| format!("failed to compile {}", path.display()))?;
    if strict {
        emit_strictness_warnings(path, &chunk);
    }
    Ok(())
}

fn emit_strictness_warnings(path: &Path, chunk: &Chunk) {
    let mut vm = Vm::default();
    vm.set_instruction_limit(STRICT_CHECK_INSTRUCTION_LIMIT);
    vm.set_strictness(StrictnessConfig {
        warn_unknown_question_word_fallback: true,
        warn_nil_producing_lookup: true,
    });

    let _ = vm.run_chunk(chunk);
    print_strictness_warnings(path, vm.strictness_diagnostics());
}

fn print_strictness_warnings(path: &Path, diagnostics: &[StrictnessDiagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("strict warning: {}: {}", path.display(), diagnostic.message);
    }
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
            check_source_file(path, false)?;
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
                check_source_file(file, false)?;
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
            check_source_file(file, false)?;
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
            "word inventory check passed: {} documented words, {} TextMate token literals, {} built-in LSP entries, {} registered VM words ({} documented token words missing from the embedded LSP inventory, {} duplicate reference entries)",
            summary.documented_words,
            summary.grammar_token_words,
            summary.lsp_words,
            summary.registered_words,
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
    registered_words: usize,
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
    let registered_words = registered_builtin_word_names();
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
    let registry_missing_from_docs = registered_words
        .iter()
        .filter(|word| !documented_all_names.contains(*word))
        .cloned()
        .collect::<Vec<_>>();
    let registry_missing_from_lsp = registered_words
        .iter()
        .filter(|word| !lsp_words.contains(*word))
        .cloned()
        .collect::<Vec<_>>();
    let registry_missing_from_grammar = registered_words
        .iter()
        .filter(|word| !grammar_builtin_words.contains(*word))
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
    if !registry_missing_from_docs.is_empty() {
        failures.push(format!(
            "registered built-in words missing from docs/reference/app.js: {}",
            registry_missing_from_docs.join(", ")
        ));
    }
    if !registry_missing_from_lsp.is_empty() {
        failures.push(format!(
            "registered built-in words missing from LSP inventory: {}",
            registry_missing_from_lsp.join(", ")
        ));
    }
    if !registry_missing_from_grammar.is_empty() {
        failures.push(format!(
            "registered built-in words missing from TextMate grammar: {}",
            registry_missing_from_grammar.join(", ")
        ));
    }
    if failures.is_empty() {
        Ok(WordInventoryCheckSummary {
            documented_words: documented_primary.len(),
            grammar_token_words: token_words.len(),
            lsp_words: lsp_words.len(),
            registered_words: registered_words.len(),
            documented_only_words,
            duplicate_reference_entries: duplicate_words.len(),
        })
    } else {
        bail!("word inventory check failed:\n{}", failures.join("\n"));
    }
}

fn registered_builtin_word_names() -> BTreeSet<String> {
    ricochet_vm::builtin_words()
        .iter()
        .map(|word| word.name.to_string())
        .collect()
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
    let syntax_diagnostics = syntax_lsp_diagnostics(file, source);
    match compile_source(file, source) {
        Ok(_) => syntax_diagnostics,
        Err(error) => {
            let mut diagnostics = vec![compile_error_lsp_diagnostic(file, source, &error)];
            for diagnostic in syntax_diagnostics {
                if !has_same_lsp_code_and_range(&diagnostics, &diagnostic) {
                    diagnostics.push(diagnostic);
                }
            }
            diagnostics
        }
    }
}

fn has_same_lsp_code_and_range(
    diagnostics: &[serde_json::Value],
    candidate: &serde_json::Value,
) -> bool {
    let Some(candidate_code) = candidate.get("code") else {
        return false;
    };
    let Some(candidate_range) = candidate.get("range") else {
        return false;
    };

    diagnostics.iter().any(|diagnostic| {
        diagnostic.get("code") == Some(candidate_code)
            && diagnostic.get("range") == Some(candidate_range)
    })
}

fn lint_path(path: &str, json_output: bool) -> Result<()> {
    let path = Path::new(path);
    let files = lint_files(path)?;
    let mut entries = Vec::new();
    let mut diagnostic_count = 0usize;

    for file in &files {
        let source = fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let file_name = file.to_string_lossy().into_owned();
        let diagnostics = source_lsp_diagnostics(&file_name, &source);
        diagnostic_count += diagnostics.len();
        entries.push((file.clone(), diagnostics));
    }

    if json_output {
        let files = entries
            .iter()
            .map(|(path, diagnostics)| {
                json!({
                    "path": path.to_string_lossy(),
                    "diagnostics": diagnostics,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "file_count": files.len(),
                "diagnostic_count": diagnostic_count,
                "files": files,
            }))?
        );
    } else if diagnostic_count == 0 {
        println!("linted {} Ricochet file(s); no diagnostics", entries.len());
    } else {
        for (path, diagnostics) in &entries {
            for diagnostic in diagnostics {
                print_lint_diagnostic(path, diagnostic);
            }
        }
    }

    if diagnostic_count > 0 {
        bail!("lint found {diagnostic_count} diagnostic(s)");
    }

    Ok(())
}

fn lint_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("lint path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_rco_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn print_lint_diagnostic(path: &Path, diagnostic: &serde_json::Value) {
    let start = diagnostic
        .get("range")
        .and_then(|range| range.get("start"))
        .unwrap_or(&serde_json::Value::Null);
    let line = start
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .map(|line| line + 1)
        .unwrap_or(1);
    let character = start
        .get("character")
        .and_then(serde_json::Value::as_u64)
        .map(|character| character + 1)
        .unwrap_or(1);
    let severity = diagnostic
        .get("severity")
        .and_then(serde_json::Value::as_i64)
        .map(lint_severity_label)
        .unwrap_or("diagnostic");
    let message = diagnostic
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("diagnostic");
    let code = diagnostic
        .get("code")
        .and_then(serde_json::Value::as_str)
        .map(|code| format!("[{code}]"))
        .unwrap_or_default();
    eprintln!(
        "{}:{line}:{character}: {severity}{code}: {message}",
        path.display()
    );
    if let Some(help) = diagnostic
        .get("data")
        .and_then(|data| data.get("help"))
        .and_then(serde_json::Value::as_str)
    {
        eprintln!("  help: {help}");
    }
}

fn lint_severity_label(severity: i64) -> &'static str {
    match severity {
        1 => "error",
        2 => "warning",
        3 => "information",
        4 => "hint",
        _ => "diagnostic",
    }
}

fn compile_error_lsp_diagnostic(
    file: &str,
    source: &str,
    error: &CompileError,
) -> serde_json::Value {
    let (span, message, help, code, fix) = match error {
        CompileError::Parse(error) => {
            let diagnostic = ricochet_syntax::parse_error_diagnostic(file, source, error);
            (
                diagnostic.span,
                diagnostic.message,
                diagnostic.help,
                None,
                None,
            )
        }
        CompileError::Unsupported {
            feature,
            span,
            help,
        } => {
            let fix = if feature.starts_with("leading-dot method syntax ") {
                leading_dot_fix(source, *span)
            } else {
                None
            };
            (
                fix.as_ref().map(|fix| fix.span).unwrap_or(*span),
                format!("unsupported compiler feature: {feature}"),
                help.clone(),
                fix.as_ref().map(|_| "leading-dot-syntax"),
                fix,
            )
        }
        CompileError::LoopControlOutsideLoop { word, span } => (
            *span,
            format!("{word} can only be used inside a loop"),
            None,
            None,
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
    if let Some(code) = code {
        diagnostic["code"] = json!(code);
    }
    if let Some(help) = help {
        diagnostic["codeDescription"] = json!({ "href": "https://github.com/BARKx4/Ricochet" });
        diagnostic["data"] = json!({ "help": help });
    }
    if let Some(fix) = fix {
        diagnostic["codeDescription"] = json!({ "href": "https://github.com/BARKx4/Ricochet" });
        if diagnostic.get("data").is_none() {
            diagnostic["data"] = json!({});
        }
        diagnostic["data"]["replacement"] = json!(fix.replacement);
    }
    diagnostic
}

struct DiagnosticFix {
    span: Span,
    replacement: String,
}

fn leading_dot_fix(source: &str, span: Span) -> Option<DiagnosticFix> {
    let tokens = lex(source).ok()?;
    let index = tokens.iter().position(|token| token.span == span)?;
    let TokenKind::DotWord(word) = &tokens[index].kind else {
        return None;
    };
    let selector = word.strip_prefix('.')?;

    if let Some(next) = next_non_newline_token(&tokens, index) {
        if matches!(&next.kind, TokenKind::Symbol(next_word) if next_word == "get") {
            return Some(DiagnosticFix {
                span: Span {
                    start: span.start,
                    end: next.span.end,
                },
                replacement: format!("{selector}.get"),
            });
        }
    }

    if let Some(previous) = previous_non_newline_token(&tokens, index) {
        if let TokenKind::Symbol(namespace) = &previous.kind {
            if is_host_namespace(namespace) {
                return Some(DiagnosticFix {
                    span: Span {
                        start: previous.span.start,
                        end: span.end,
                    },
                    replacement: host_namespace_word(namespace, selector),
                });
            }
        }
    }

    Some(DiagnosticFix {
        span,
        replacement: selector.to_string(),
    })
}

fn previous_non_newline_token(
    tokens: &[ricochet_syntax::Token],
    index: usize,
) -> Option<&ricochet_syntax::Token> {
    tokens[..index]
        .iter()
        .rev()
        .find(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
}

fn next_non_newline_token(
    tokens: &[ricochet_syntax::Token],
    index: usize,
) -> Option<&ricochet_syntax::Token> {
    tokens
        .get(index + 1..)?
        .iter()
        .find(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof))
}

fn is_host_namespace(namespace: &str) -> bool {
    matches!(
        namespace,
        "fs" | "workspace" | "http" | "process" | "pty" | "tui" | "webview"
    )
}

fn host_namespace_word(namespace: &str, selector: &str) -> String {
    let selector = selector.trim_end_matches('!').replace('-', "_");
    format!("{namespace}_{selector}")
}

struct SyntaxLint {
    span: Span,
    message: String,
    help: String,
    replacement: Option<String>,
    code: &'static str,
}

fn syntax_lsp_diagnostics(file: &str, source: &str) -> Vec<serde_json::Value> {
    let Ok(module) = parse_module(source) else {
        return Vec::new();
    };
    let mut lints = Vec::new();
    collect_module_lints(&module, source, &mut lints);
    lints
        .into_iter()
        .map(|lint| syntax_lint_lsp_diagnostic(file, source, lint))
        .collect()
}

fn collect_module_lints(module: &Module, source: &str, lints: &mut Vec<SyntaxLint>) {
    for item in &module.items {
        collect_item_lints(item, source, lints);
    }
}

fn collect_item_lints(item: &SyntaxItem, source: &str, lints: &mut Vec<SyntaxLint>) {
    match item {
        SyntaxItem::Class(class) => {
            for item in &class.body {
                collect_item_lints(item, source, lints);
            }
        }
        SyntaxItem::Method(method) => collect_expr_list_lints(&method.body, source, lints),
        SyntaxItem::Function(function) => collect_expr_list_lints(&function.body, source, lints),
        SyntaxItem::Macro(macro_decl) => collect_expr_list_lints(&macro_decl.body, source, lints),
        SyntaxItem::Expr { expr, span, .. } => {
            collect_spanned_expr_lints(expr, *span, source, lints)
        }
    }
}

fn collect_expr_list_lints(exprs: &[SpannedExpr], source: &str, lints: &mut Vec<SyntaxLint>) {
    for expr in exprs {
        collect_spanned_expr_lints(&expr.expr, expr.span, source, lints);
    }
}

fn collect_spanned_expr_lints(expr: &Expr, span: Span, source: &str, lints: &mut Vec<SyntaxLint>) {
    if let Expr::DotWord(word) = expr {
        lints.push(leading_dot_lint(source, word, span));
    }
    collect_expr_lints(expr, source, lints);
}

fn collect_expr_lints(expr: &Expr, source: &str, lints: &mut Vec<SyntaxLint>) {
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
            collect_expr_list_lints(exprs, source, lints);
        }
        Expr::Block(exprs) => collect_expr_list_lints(exprs, source, lints),
        Expr::If {
            then_body,
            else_body,
        } => {
            collect_expr_list_lints(then_body, source, lints);
            collect_expr_list_lints(else_body, source, lints);
        }
        Expr::While { condition, body } => {
            collect_expr_list_lints(condition, source, lints);
            collect_expr_list_lints(body, source, lints);
        }
        Expr::Symbol(_)
        | Expr::DotWord(_)
        | Expr::Reference(_)
        | Expr::String(_)
        | Expr::Number(_)
        | Expr::Float(_)
        | Expr::Args(_) => {}
    }
}

fn leading_dot_lint(source: &str, word: &str, span: Span) -> SyntaxLint {
    let fix = leading_dot_fix(source, span);
    SyntaxLint {
        span: fix.as_ref().map(|fix| fix.span).unwrap_or(span),
        message: format!("avoid leading-dot method syntax {word:?}"),
        help: "Use postfix selectors, for example: user email.get or http_request".to_string(),
        replacement: fix.map(|fix| fix.replacement),
        code: "leading-dot-syntax",
    }
}

fn prefer_reference_lint(name: &str, span: Span) -> SyntaxLint {
    SyntaxLint {
        span,
        message: format!("prefer ${name} for variable reads"),
        help: format!(
            "Use ${name} for ordinary variable reads. Keep \"{name}\" get only when the variable name is data on the stack."
        ),
        replacement: Some(format!("${name}")),
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
        "data": { "help": lint.help, "file": file, "replacement": lint.replacement },
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
        MigrateCommand::New { name, dsl, path } => {
            migrate_new(Path::new(path.as_deref().unwrap_or(".")), &name, dsl)
        }
        MigrateCommand::Status { path } => {
            migrate_status(Path::new(path.as_deref().unwrap_or("."))).await
        }
        MigrateCommand::Apply { path } => {
            migrate_apply(Path::new(path.as_deref().unwrap_or("."))).await
        }
        MigrateCommand::Rollback { path, steps } => {
            migrate_rollback(Path::new(path.as_deref().unwrap_or(".")), steps).await
        }
        MigrateCommand::Dump { path, output } => {
            migrate_dump(Path::new(path.as_deref().unwrap_or(".")), &output).await
        }
    }
}

async fn seed(path: &Path) -> Result<()> {
    let project_root = migration_project_root_for_command("seed", path)?;
    let Some(database) = project_database_config(&project_root)? else {
        bail!("No [database.default] configured.");
    };
    let seed_files = discover_seed_files(&project_root)?;
    if seed_files.is_empty() {
        println!("No seed files found in db/seeds.");
        return Ok(());
    }

    match database.adapter.as_str() {
        "sqlite" => seed_sqlite(&project_root, &database, seed_files),
        "postgres" | "postgresql" => seed_postgres(&project_root, &database, seed_files).await,
        "mysql" | "mariadb" => seed_mysql(&project_root, &database, seed_files).await,
        adapter => bail!(
            "rco seed supports sqlite, postgres, and mysql projects; found adapter {adapter:?}"
        ),
    }
}

fn migrate_new(path: &Path, name: &str, dsl: bool) -> Result<()> {
    let project_root = migration_project_root(path)?;
    let migrations_dir = project_root.join("db").join("migrations");
    fs::create_dir_all(&migrations_dir)
        .with_context(|| format!("failed to create {}", migrations_dir.display()))?;

    let version = format!("{}_{}", migration_timestamp(), migration_name_slug(name)?);
    if dsl {
        let up_path = migrations_dir.join(format!("{version}.up.rco"));
        let down_path = migrations_dir.join(format!("{version}.down.rco"));
        ensure_new_file_path(&up_path)?;
        ensure_new_file_path(&down_path)?;
        write_new_file(&up_path, migration_dsl_up_template())?;
        write_new_file(&down_path, migration_dsl_down_template())?;
        println!("created {}", up_path.display());
        println!("created {}", down_path.display());
    } else {
        let sql_path = migrations_dir.join(format!("{version}.sql"));
        ensure_new_file_path(&sql_path)?;
        write_new_file(&sql_path, migration_sql_template())?;
        println!("created {}", sql_path.display());
    }
    Ok(())
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

async fn migrate_rollback(path: &Path, steps: usize) -> Result<()> {
    if steps == 0 {
        bail!("rollback --steps must be greater than 0");
    }
    let project_root = migration_project_root(path)?;
    let Some(database) = project_database_config(&project_root)? else {
        bail!("No [database.default] configured.");
    };
    let migrations = discover_migrations(&project_root)?;

    match database.adapter.as_str() {
        "sqlite" => migrate_rollback_sqlite(&project_root, &database, migrations, steps),
        "postgres" | "postgresql" => {
            migrate_rollback_postgres(&database, migrations, steps).await
        }
        "mysql" | "mariadb" => migrate_rollback_mysql(&database, migrations, steps).await,
        adapter => bail!("rco migrate rollback supports sqlite, postgres, and mysql projects; found adapter {adapter:?}"),
    }
}

async fn migrate_dump(path: &Path, output: &Path) -> Result<()> {
    let project_root = migration_project_root(path)?;
    let Some(database) = project_database_config(&project_root)? else {
        bail!("No [database.default] configured.");
    };

    match database.adapter.as_str() {
        "sqlite" => migrate_dump_sqlite(&project_root, &database, output),
        "postgres" | "postgresql" => migrate_dump_postgres(&project_root, &database, output).await,
        "mysql" | "mariadb" => migrate_dump_mysql(&project_root, &database, output).await,
        adapter => bail!("rco migrate dump supports sqlite, postgres, and mysql projects; found adapter {adapter:?}"),
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
        let sql = migration_sql(&migration.source, database)
            .with_context(|| format!("failed to prepare migration {}", migration.version))?;
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

fn migrate_apply_packaged_sqlite_at_path(
    database_path: &Path,
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
) -> Result<()> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    ensure_private_packaged_sqlite_file(database_path)?;
    let mut connection = rusqlite::Connection::open(database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .with_context(|| {
            format!(
                "failed to configure SQLite locking for {}",
                database_path.display()
            )
        })?;
    ensure_schema_migrations_table(&connection)?;
    for migration in migrations {
        let sql = migration_sql(&migration.source, database)
            .with_context(|| format!("failed to prepare migration {}", migration.version))?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .with_context(|| format!("failed to start migration {}", migration.version))?;
        let already_applied: bool = tx
            .query_row(
                "select exists(select 1 from schema_migrations where version = ?1)",
                [&migration.version],
                |row| row.get(0),
            )
            .with_context(|| format!("failed to check migration {}", migration.version))?;
        if already_applied {
            tx.commit().with_context(|| {
                format!("failed to finish migration check {}", migration.version)
            })?;
            continue;
        }
        tx.execute_batch(&sql)
            .with_context(|| format!("failed to apply migration {}", migration.version))?;
        tx.execute(
            "insert into schema_migrations (version, applied_at) values (?1, ?2)",
            (&migration.version, migration_timestamp()),
        )
        .with_context(|| format!("failed to record migration {}", migration.version))?;
        tx.commit()
            .with_context(|| format!("failed to commit migration {}", migration.version))?;
    }
    restrict_packaged_sqlite_file_permissions(database_path)
}

fn ensure_private_packaged_sqlite_file(database_path: &Path) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(database_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to create {}", database_path.display()));
        }
    }
    restrict_packaged_sqlite_file_permissions(database_path)
}

#[cfg(unix)]
fn restrict_packaged_sqlite_file_permissions(database_path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(database_path, fs::Permissions::from_mode(0o600)).with_context(|| {
        format!(
            "failed to restrict packaged SQLite permissions on {}",
            database_path.display()
        )
    })
}

#[cfg(not(unix))]
fn restrict_packaged_sqlite_file_permissions(_database_path: &Path) -> Result<()> {
    Ok(())
}

pub(crate) fn prepare_packaged_mvc_sqlite(project_root: &Path, data_root: &Path) -> Result<()> {
    let Some(database) = project_database_config(project_root)? else {
        return Ok(());
    };
    if !database.adapter.eq_ignore_ascii_case("sqlite") {
        return Ok(());
    }
    let url = database.url.trim();
    if url == ":memory:" || url == "sqlite::memory:" {
        return Ok(());
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
        return Ok(());
    }
    validate_project_relative_path(path, "database.default.url")?;
    let database_path = data_root.join(path);
    ensure_contained_candidate(data_root, &database_path, "packaged SQLite database")?;
    let migrations = discover_migrations(project_root)?;
    if migrations.is_empty() {
        bail!(
            "packaged file-backed SQLite requires at least one db/migrations migration; the development database is not a package schema source"
        );
    }
    migrate_apply_packaged_sqlite_at_path(&database_path, &database, migrations)
}

fn migrate_rollback_sqlite(
    project_root: &Path,
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
    steps: usize,
) -> Result<()> {
    let database_path = sqlite_database_path(project_root, &database.url);
    if !database_path.is_file() {
        bail!(
            "SQLite database does not exist: {}",
            database_path.display()
        );
    }
    let mut connection = rusqlite::Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    ensure_schema_migrations_table(&connection)?;
    let applied = sqlite_applied_migrations(&connection)?;
    if applied.is_empty() {
        println!("No applied migrations to roll back.");
        return Ok(());
    }

    let migrations_by_version = migrations
        .iter()
        .map(|migration| (migration.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let mut rolled_back = 0usize;
    for version in applied.iter().rev().take(steps) {
        let migration = migrations_by_version.get(version.as_str()).with_context(|| {
            format!(
                "cannot roll back migration {version}: no matching migration file found in db/migrations"
            )
        })?;
        let down = migration.down.as_ref().with_context(|| {
            format!(
                "cannot roll back migration {version}: no down SQL or DSL migration found; add db/migrations/{version}.down.sql or db/migrations/{version}.down.rco"
            )
        })?;
        let sql = migration_sql(down, database)
            .with_context(|| format!("failed to prepare rollback {}", migration.version))?;
        let tx = connection
            .transaction()
            .with_context(|| format!("failed to start rollback {}", migration.version))?;
        tx.execute_batch(&sql)
            .with_context(|| format!("failed to roll back migration {}", migration.version))?;
        tx.execute(
            "delete from schema_migrations where version = ?1",
            [&migration.version],
        )
        .with_context(|| format!("failed to forget migration {}", migration.version))?;
        tx.commit()
            .with_context(|| format!("failed to commit rollback {}", migration.version))?;
        rolled_back += 1;
        println!("rolled back {}", migration.version);
    }

    if rolled_back == 0 {
        println!("No applied migrations to roll back.");
    } else {
        println!("Rolled back {rolled_back} migration(s).");
    }
    Ok(())
}

async fn migrate_rollback_postgres(
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
    steps: usize,
) -> Result<()> {
    let backend = PostgresDatabase::connect(&database.url)
        .await
        .context("failed to connect to PostgreSQL for rollback")?;
    backend
        .ensure_schema_migrations_table()
        .await
        .context("failed to create PostgreSQL schema_migrations")?;
    let applied = backend
        .migration_versions()
        .await
        .context("failed to read PostgreSQL schema_migrations")?
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    if applied.is_empty() {
        println!("No applied migrations to roll back.");
        return Ok(());
    }

    let migrations_by_version = migrations
        .iter()
        .map(|migration| (migration.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let mut rolled_back = 0usize;
    for version in applied.iter().rev().take(steps) {
        let migration = migrations_by_version.get(version.as_str()).with_context(|| {
            format!(
                "cannot roll back migration {version}: no matching migration file found in db/migrations"
            )
        })?;
        let down = migration.down.as_ref().with_context(|| {
            format!(
                "cannot roll back migration {version}: no down SQL or DSL migration found; add db/migrations/{version}.down.sql or db/migrations/{version}.down.rco"
            )
        })?;
        let sql = migration_sql(down, database)
            .with_context(|| format!("failed to prepare rollback {}", migration.version))?;
        backend
            .rollback_migration(&migration.version, &sql)
            .await
            .with_context(|| format!("failed to roll back migration {}", migration.version))?;
        rolled_back += 1;
        println!("rolled back {}", migration.version);
    }

    if rolled_back == 0 {
        println!("No applied migrations to roll back.");
    } else {
        println!("Rolled back {rolled_back} migration(s).");
    }
    Ok(())
}

async fn migrate_rollback_mysql(
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
    steps: usize,
) -> Result<()> {
    let backend = MysqlDatabase::connect(&database.url)
        .await
        .context("failed to connect to MySQL for rollback")?;
    backend
        .ensure_schema_migrations_table()
        .await
        .context("failed to create MySQL schema_migrations")?;
    let applied = backend
        .migration_versions()
        .await
        .context("failed to read MySQL schema_migrations")?
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeSet<_>>();
    if applied.is_empty() {
        println!("No applied migrations to roll back.");
        return Ok(());
    }

    let migrations_by_version = migrations
        .iter()
        .map(|migration| (migration.version.as_str(), migration))
        .collect::<BTreeMap<_, _>>();
    let mut rolled_back = 0usize;
    for version in applied.iter().rev().take(steps) {
        let migration = migrations_by_version.get(version.as_str()).with_context(|| {
            format!(
                "cannot roll back migration {version}: no matching migration file found in db/migrations"
            )
        })?;
        let down = migration.down.as_ref().with_context(|| {
            format!(
                "cannot roll back migration {version}: no down SQL or DSL migration found; add db/migrations/{version}.down.sql or db/migrations/{version}.down.rco"
            )
        })?;
        let sql = migration_sql(down, database)
            .with_context(|| format!("failed to prepare rollback {}", migration.version))?;
        backend
            .rollback_migration(&migration.version, &sql)
            .await
            .with_context(|| format!("failed to roll back migration {}", migration.version))?;
        rolled_back += 1;
        println!("rolled back {}", migration.version);
    }

    if rolled_back == 0 {
        println!("No applied migrations to roll back.");
    } else {
        println!("Rolled back {rolled_back} migration(s).");
    }
    Ok(())
}

fn migrate_dump_sqlite(
    project_root: &Path,
    database: &MigrationDatabase,
    output: &Path,
) -> Result<()> {
    let database_path = sqlite_database_path(project_root, &database.url);
    if !database_path.is_file() {
        bail!(
            "SQLite database does not exist: {}",
            database_path.display()
        );
    }
    let connection = rusqlite::Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    let dump = sqlite_schema_dump(&connection)?;
    write_schema_dump(project_root, output, dump)
}

async fn migrate_dump_postgres(
    project_root: &Path,
    database: &MigrationDatabase,
    output: &Path,
) -> Result<()> {
    let backend = PostgresDatabase::connect(&database.url)
        .await
        .context("failed to connect to PostgreSQL for schema dump")?;
    let dump = backend
        .schema_dump()
        .await
        .context("failed to dump PostgreSQL schema")?;
    write_schema_dump(project_root, output, dump)
}

async fn migrate_dump_mysql(
    project_root: &Path,
    database: &MigrationDatabase,
    output: &Path,
) -> Result<()> {
    let backend = MysqlDatabase::connect(&database.url)
        .await
        .context("failed to connect to MySQL for schema dump")?;
    let dump = backend
        .schema_dump()
        .await
        .context("failed to dump MySQL schema")?;
    write_schema_dump(project_root, output, dump)
}

fn write_schema_dump(project_root: &Path, output: &Path, dump: String) -> Result<()> {
    let output_path = if output.is_absolute() {
        output.to_path_buf()
    } else {
        project_root.join(output)
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&output_path, dump)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    println!("dumped schema to {}", output_path.display());
    Ok(())
}

fn seed_sqlite(
    project_root: &Path,
    database: &MigrationDatabase,
    seed_files: Vec<SeedFile>,
) -> Result<()> {
    let database_path = sqlite_database_path(project_root, &database.url);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut connection = rusqlite::Connection::open(&database_path)
        .with_context(|| format!("failed to open {}", database_path.display()))?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .context("failed to configure SQLite foreign keys")?;
    eprintln!(
        "Seed files are not tracked; make them idempotent because rco seed runs them every time."
    );

    let mut seeded_count = 0usize;
    for seed_file in seed_files {
        match seed_file.kind {
            SeedFileKind::Sql => {
                let sql = fs::read_to_string(&seed_file.path)
                    .with_context(|| format!("failed to read {}", seed_file.path.display()))?;
                let tx = connection
                    .transaction()
                    .with_context(|| format!("failed to start seed {}", seed_file.name))?;
                tx.execute_batch(&sql)
                    .with_context(|| format!("failed to run SQL seed {}", seed_file.name))?;
                tx.commit()
                    .with_context(|| format!("failed to commit seed {}", seed_file.name))?;
            }
            SeedFileKind::Ricochet => {
                let database_url = database_path.to_string_lossy().to_string();
                let backend: Arc<dyn DatabaseBackend> =
                    Arc::new(SqliteDatabase::connect(&database_url).with_context(|| {
                        format!("failed to connect to {}", database_path.display())
                    })?);
                run_ricochet_seed(project_root, backend, &seed_file.path)
                    .with_context(|| format!("failed to run Ricochet seed {}", seed_file.name))?;
            }
        }
        seeded_count += 1;
        println!("seeded {}", seed_file.name);
    }

    println!("Ran {seeded_count} seed file(s).");
    Ok(())
}

async fn seed_postgres(
    project_root: &Path,
    database: &MigrationDatabase,
    seed_files: Vec<SeedFile>,
) -> Result<()> {
    let backend = Arc::new(
        PostgresDatabase::connect(&database.url)
            .await
            .context("failed to connect to PostgreSQL for seeds")?,
    );
    eprintln!(
        "Seed files are not tracked; make them idempotent because rco seed runs them every time."
    );

    let mut seeded_count = 0usize;
    for seed_file in seed_files {
        match seed_file.kind {
            SeedFileKind::Sql => {
                let sql = fs::read_to_string(&seed_file.path)
                    .with_context(|| format!("failed to read {}", seed_file.path.display()))?;
                backend
                    .execute_seed(&sql)
                    .await
                    .with_context(|| format!("failed to run SQL seed {}", seed_file.name))?;
            }
            SeedFileKind::Ricochet => {
                let seed_backend: Arc<dyn DatabaseBackend> = backend.clone();
                run_ricochet_seed(project_root, seed_backend, &seed_file.path)
                    .with_context(|| format!("failed to run Ricochet seed {}", seed_file.name))?;
            }
        }
        seeded_count += 1;
        println!("seeded {}", seed_file.name);
    }

    println!("Ran {seeded_count} seed file(s).");
    Ok(())
}

async fn seed_mysql(
    project_root: &Path,
    database: &MigrationDatabase,
    seed_files: Vec<SeedFile>,
) -> Result<()> {
    let backend = Arc::new(
        MysqlDatabase::connect(&database.url)
            .await
            .context("failed to connect to MySQL for seeds")?,
    );
    eprintln!(
        "Seed files are not tracked; make them idempotent because rco seed runs them every time."
    );

    let mut seeded_count = 0usize;
    for seed_file in seed_files {
        match seed_file.kind {
            SeedFileKind::Sql => {
                let sql = fs::read_to_string(&seed_file.path)
                    .with_context(|| format!("failed to read {}", seed_file.path.display()))?;
                backend
                    .execute_seed(&sql)
                    .await
                    .with_context(|| format!("failed to run SQL seed {}", seed_file.name))?;
            }
            SeedFileKind::Ricochet => {
                let seed_backend: Arc<dyn DatabaseBackend> = backend.clone();
                run_ricochet_seed(project_root, seed_backend, &seed_file.path)
                    .with_context(|| format!("failed to run Ricochet seed {}", seed_file.name))?;
            }
        }
        seeded_count += 1;
        println!("seeded {}", seed_file.name);
    }

    println!("Ran {seeded_count} seed file(s).");
    Ok(())
}

fn run_ricochet_seed(
    project_root: &Path,
    backend: Arc<dyn DatabaseBackend>,
    seed_path: &Path,
) -> Result<()> {
    let chunk = compile_source_file(seed_path)?;
    let mut vm = cli_vm(Vec::new(), &CapabilityOptions::default())?;
    let capabilities = install_project_database_runtime(&mut vm, project_root, backend)?;
    for (name, value) in capabilities {
        vm.set_variable(name, value);
    }

    let result = vm.run_chunk(&chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
    if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
        std::process::exit(code);
    }
    if let Err(error) = result {
        bail!("{}", runtime_error_message(&vm, &error));
    }
    Ok(())
}

async fn migrate_apply_postgres(
    database: &MigrationDatabase,
    migrations: Vec<MigrationFile>,
) -> Result<()> {
    let backend = PostgresDatabase::connect(&database.url)
        .await
        .context("failed to connect to PostgreSQL for migrations")?;
    backend
        .ensure_schema_migrations_table()
        .await
        .context("failed to create PostgreSQL schema_migrations")?;
    let mut applied = backend
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
        let sql = migration_sql(&migration.source, database)
            .with_context(|| format!("failed to prepare migration {}", migration.version))?;
        let applied_at = migration_timestamp();
        backend
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
    let backend = MysqlDatabase::connect(&database.url)
        .await
        .context("failed to connect to MySQL for migrations")?;
    backend
        .ensure_schema_migrations_table()
        .await
        .context("failed to create MySQL schema_migrations")?;
    let mut applied = backend
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
        let sql = migration_sql(&migration.source, database)
            .with_context(|| format!("failed to prepare migration {}", migration.version))?;
        let applied_at = migration_timestamp();
        backend
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
    source: MigrationSource,
    down: Option<MigrationSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationSource {
    path: PathBuf,
    kind: MigrationFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MigrationFileKind {
    Sql,
    RicochetDsl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MigrationDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedFileKind {
    Sql,
    Ricochet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeedFile {
    name: String,
    path: PathBuf,
    kind: SeedFileKind,
}

fn migration_project_root(path: &Path) -> Result<PathBuf> {
    migration_project_root_for_command("migrate", path)
}

fn migration_project_root_for_command(command: &str, path: &Path) -> Result<PathBuf> {
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
        "{command} must be run inside a Ricochet project with ricochet.toml: {}",
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

fn migration_sql(source: &MigrationSource, database: &MigrationDatabase) -> Result<String> {
    let contents = fs::read_to_string(&source.path)
        .with_context(|| format!("failed to read {}", source.path.display()))?;
    match source.kind {
        MigrationFileKind::Sql => Ok(contents),
        MigrationFileKind::RicochetDsl => migration_dsl::compile(&database.adapter, &contents)
            .with_context(|| format!("failed to compile {}", source.path.display())),
    }
}

fn migration_source_for_path(
    path: &Path,
) -> Result<Option<(String, MigrationDirection, MigrationSource)>> {
    let kind = match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("sql") => MigrationFileKind::Sql,
        Some("rco") => MigrationFileKind::RicochetDsl,
        _ => return Ok(None),
    };

    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("migration file name must be UTF-8")?;

    let (version, direction) = if let Some(version) = stem.strip_suffix(".down") {
        (version.to_string(), MigrationDirection::Down)
    } else if let Some(version) = stem.strip_suffix(".up") {
        (version.to_string(), MigrationDirection::Up)
    } else if kind == MigrationFileKind::Sql {
        (stem.to_string(), MigrationDirection::Up)
    } else {
        bail!(
            "Ricochet migration DSL files must use .up.rco or .down.rco: {}",
            path.display()
        );
    };

    Ok(Some((
        version,
        direction,
        MigrationSource {
            path: path.to_path_buf(),
            kind,
        },
    )))
}

fn discover_migrations(project_root: &Path) -> Result<Vec<MigrationFile>> {
    let migrations_dir = project_root.join("db").join("migrations");
    if !migrations_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut up_sources = BTreeMap::new();
    let mut down_sources = BTreeMap::new();
    for entry in fs::read_dir(&migrations_dir)
        .with_context(|| format!("failed to read {}", migrations_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", migrations_dir.display()))?;
        let path = entry.path();
        let Some((version, direction, source)) = migration_source_for_path(&path)? else {
            continue;
        };
        validate_migration_version(&version, &path)?;
        match direction {
            MigrationDirection::Up => {
                if let Some(existing) = up_sources.insert(version.clone(), source) {
                    bail!(
                        "duplicate migration for version {version}: {}",
                        existing.path.display()
                    );
                }
            }
            MigrationDirection::Down => {
                if let Some(existing) = down_sources.insert(version.clone(), source) {
                    bail!(
                        "duplicate down migration for version {version}: {}",
                        existing.path.display()
                    );
                }
            }
        }
    }

    let mut migrations = Vec::new();
    for (version, source) in up_sources {
        let down = down_sources.remove(&version);
        migrations.push(MigrationFile {
            version,
            source,
            down,
        });
    }
    if let Some((version, source)) = down_sources.into_iter().next() {
        bail!(
            "down migration {} has no matching up migration: {}",
            version,
            source.path.display()
        );
    }
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

fn sqlite_schema_dump(connection: &rusqlite::Connection) -> Result<String> {
    let mut statement = connection
        .prepare(
            r#"
select type, name, sql
from sqlite_schema
where sql is not null
  and name not like 'sqlite_%'
  and name != 'schema_migrations'
  and tbl_name != 'schema_migrations'
  and type in ('table', 'index', 'view', 'trigger')
order by case type
  when 'table' then 0
  when 'index' then 1
  when 'view' then 2
  when 'trigger' then 3
  else 4
end, name
"#,
        )
        .context("failed to read SQLite schema")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut dump = String::from("-- Ricochet SQLite schema dump\n\n");
    for row in rows {
        let (kind, name, sql) = row?;
        let sql = sql.trim().trim_end_matches(';').trim();
        writeln!(dump, "-- {kind}: {name}")?;
        writeln!(dump, "{sql};\n")?;
    }
    Ok(dump)
}

fn discover_seed_files(project_root: &Path) -> Result<Vec<SeedFile>> {
    let seeds_dir = project_root.join("db").join("seeds");
    if !seeds_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut seed_files = Vec::new();
    for entry in fs::read_dir(&seeds_dir)
        .with_context(|| format!("failed to read {}", seeds_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", seeds_dir.display()))?;
        let path = entry.path();
        let kind = match path.extension().and_then(|extension| extension.to_str()) {
            Some("sql") => SeedFileKind::Sql,
            Some("rco") => SeedFileKind::Ricochet,
            _ => continue,
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("seed file name must be UTF-8")?
            .to_string();
        seed_files.push(SeedFile { name, path, kind });
    }
    seed_files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(seed_files)
}

fn migration_timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    millis.to_string()
}

fn migration_name_slug(name: &str) -> Result<String> {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in name.trim().chars() {
        let next = match character {
            'a'..='z' | '0'..='9' => character,
            'A'..='Z' => character.to_ascii_lowercase(),
            '_' | '-' => character,
            character if character.is_whitespace() => '_',
            _ => '_',
        };
        if matches!(next, '_' | '-') {
            if slug.is_empty() || last_was_separator {
                continue;
            }
            last_was_separator = true;
        } else {
            last_was_separator = false;
        }
        slug.push(next);
    }
    while slug.ends_with('_') || slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        bail!("migration name must contain at least one letter or digit");
    }
    Ok(slug)
}

fn write_new_file(path: &Path, contents: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(contents.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn ensure_new_file_path(path: &Path) -> Result<()> {
    if path.exists() {
        bail!("migration file already exists: {}", path.display());
    }
    Ok(())
}

fn migration_sql_template() -> &'static str {
    "-- Write migration SQL here.\n"
}

fn migration_dsl_up_template() -> &'static str {
    r#"(( Write migration DSL here. Example:
"items" table_create
"id" "integer" column primary_key
))
"#
}

fn migration_dsl_down_template() -> &'static str {
    r#"(( Write rollback DSL here. Example:
"items" table_drop
))
"#
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
        SyntaxItem::Macro(_) => {}
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
    "ada@example.com" assert_equals
  ] "testUserDisplayNameFallsBackToEmail" Method

  [
    users array
    User new
    "grace@example.com" swap email.set
    $users swap push drop
    $users count
    1 assert_equals
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
    $session "last_page" "users" put drop
    User default_page
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
    $users swap push drop
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
        $session "user_email" $email put drop
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
    $session "user_email" remove drop
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
    archive_integrity: Option<String>,
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
    archive_integrity: Option<String>,
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
    token_env: Option<&'a str>,
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
    registry: Option<&Path>,
    registry_url: Option<&str>,
    options: PublishRegistryOptions<'_>,
) -> Result<()> {
    if registry.is_some() == registry_url.is_some() {
        bail!("use exactly one of --registry or --registry-url for publish");
    }
    if registry.is_some() && options.token_env.is_some() {
        bail!("--token-env can only be used with hosted --registry-url publish");
    }
    if registry_url.is_some() && options.token_env.is_none() && !options.dry_run {
        bail!("--token-env is required for hosted publish unless --dry-run is used");
    }

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
    if let Some(registry_url) = registry_url {
        return hosted_registry::publish(hosted_registry::HostedPublishOptions {
            package_root,
            package: name,
            version,
            package_integrity: &package_integrity,
            registry_url,
            token_env: options.token_env,
            dry_run: options.dry_run,
            provenance: provenance.as_ref(),
            signature: signature.as_ref(),
            signature_kind,
        });
    }

    let registry = registry.expect("publish target cardinality checked above");
    let registry_root = absolute_path_from_current(registry)?;
    let package_root_canonical = fs::canonicalize(package_root)
        .with_context(|| format!("failed to resolve {}", package_root.display()))?;
    if registry_root.starts_with(&package_root_canonical) {
        bail!("publish registry must not be inside the package being published");
    }

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
        let registry = normalize_manifest_registry_value(registry)?;
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
            archive_integrity: None,
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
    let registry = normalize_manifest_registry_value(registry)?;
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
        archive_integrity: None,
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

fn normalize_manifest_registry_value(registry: Option<String>) -> Result<Option<String>> {
    let Some(registry) = registry else {
        return Ok(None);
    };
    if static_registry::is_static_source(&registry) {
        return static_registry::validate_url(&registry)
            .map(str::to_string)
            .map(Some);
    }
    if hosted_registry::is_hosted_source(&registry) {
        return hosted_registry::validate_base_url(&registry).map(Some);
    }
    Ok(Some(registry))
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
    let archive_integrity = table
        .get("archive_integrity")
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(archive_integrity) = archive_integrity.as_deref() {
        validate_package_integrity(archive_integrity)?;
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
        archive_integrity,
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
            Some(registry_url) if static_registry::is_static_source(registry_url) => {
                static_registry::validate_url(registry_url)?.to_string()
            }
            Some(registry_url) => hosted_registry::validate_base_url(registry_url)?,
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
                archive_integrity: None,
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
                archive_integrity: None,
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
                archive_integrity: None,
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
    command.arg("--").arg(git).arg(&package_dir);

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

fn search_registry(query: &str, registry: Option<&Path>, registry_url: Option<&str>) -> Result<()> {
    if registry.is_some() && registry_url.is_some() {
        bail!("use either --registry or --registry-url, not both");
    }
    if let Some(registry_url) = registry_url {
        if static_registry::is_static_source(registry_url) {
            return static_registry::search(query, None, Some(registry_url));
        }
        if hosted_registry::is_hosted_source(registry_url) {
            return hosted_registry::search(query, registry_url);
        }
        bail!(
            "registry URL {registry_url:?} must be a static index URL or hosted registry base URL"
        );
    }
    static_registry::search(query, registry, None)
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
    if static_registry::is_static_source(registry) {
        return static_registry::install_dependency(project_root, spec, locked);
    }
    if hosted_registry::is_hosted_source(registry) {
        return hosted_registry::install_dependency(project_root, spec, locked);
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
    if let Some(archive_integrity) = &spec.archive_integrity {
        validate_package_integrity(archive_integrity)?;
        package["archive_integrity"] = value(archive_integrity.clone());
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
    let source_path = Path::new(path);
    let chunk = compile_source_file(source_path)?;
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
            dynamic_import_parent: dynamic_import_parent_for_source(source_path)?,
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
    let source_path = Path::new(path);
    let chunk = compile_source_file(source_path)?;
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
            dynamic_import_parent: dynamic_import_parent_for_source(source_path)?,
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
            dynamic_import_parent: current_dir_for_dynamic_imports()?,
        },
    )
}

fn run_tui_file(path: &str, args: Vec<String>, capabilities: CapabilityOptions) -> Result<()> {
    let source_path = Path::new(path);
    let chunk = compile_source_file(source_path)?;
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
            dynamic_import_parent: dynamic_import_parent_for_source(source_path)?,
        },
    )
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
            dynamic_import_parent: current_dir_for_dynamic_imports()?,
        },
    )
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
    dynamic_import_parent: PathBuf,
}

fn run_chunk_cli(chunk: &Chunk, options: RunChunkCliOptions<'_>) -> Result<()> {
    let mut vm = cli_vm(options.args, &options.capabilities)?;
    install_dynamic_module_loader(&mut vm, options.dynamic_import_parent);
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
        vm.set_debug_controller_with_control(read_terminal_debug_action);
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
    let trace = events
        .iter()
        .map(debug_event_json)
        .collect::<Result<Vec<_>>>()?;
    let json = serde_json::to_string_pretty(&trace)?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

fn print_debug_event_json_line(event: &DebugEvent) {
    let value = debug_event_json(event).unwrap_or_else(|_| {
        json!({
            "event": "error",
            "error": "debug protocol cannot serialize non-serializable value",
        })
    });
    println!(
        "{}",
        serde_json::to_string(&value).expect("debug event JSON should serialize")
    );
}

fn emit_json_line(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

struct DebugUiSnapshot {
    pause: DebugPause,
    source_line: Option<String>,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Copy)]
enum DebugUiRenderMode {
    Preview,
    Interactive,
}

struct DebugTuiOptions<'a> {
    smoke: bool,
    commands: Vec<String>,
    step: bool,
    breakpoints: &'a [usize],
}

struct DebugWebOptions<'a> {
    smoke: bool,
    host: &'a str,
    port: u16,
    step: bool,
    breakpoints: &'a [usize],
}

#[derive(Clone)]
struct DebugWebLiveState {
    events: tokio::sync::broadcast::Sender<String>,
    latest_event: Arc<Mutex<Option<String>>>,
    status: Arc<Mutex<DebugWebSessionStatus>>,
    actions: Arc<Mutex<std_mpsc::Sender<DebugCommand>>>,
}

#[derive(Default)]
struct DebugWebSessionStatus {
    paused: bool,
    completed: bool,
    pause_id: usize,
}

fn run_debug_tui(
    path: &str,
    options: DebugTuiOptions<'_>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    if options.smoke {
        let snapshot =
            collect_debug_ui_snapshot(path, options.step, options.breakpoints, args, capabilities)?;
        print!(
            "{}",
            render_debug_tui_snapshot(&snapshot, DebugUiRenderMode::Preview)
        );
        return Ok(());
    }

    run_debug_tui_session(path, options, args, capabilities)
}

async fn run_debug_web(
    path: &str,
    options: DebugWebOptions<'_>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let bind_address = if options.smoke {
        None
    } else {
        let ip: IpAddr = options.host.parse().with_context(|| {
            format!(
                "debug-web host must be an IP address, got {:?}",
                options.host
            )
        })?;
        if !ip.is_loopback() {
            bail!("debug-web only binds loopback addresses by default");
        }
        Some(SocketAddr::new(ip, options.port))
    };

    if options.smoke {
        let snapshot =
            collect_debug_ui_snapshot(path, options.step, options.breakpoints, args, capabilities)?;
        let html = render_debug_web_snapshot(&snapshot);
        println!("{html}");
        return Ok(());
    }

    let bind_address = bind_address.expect("debug-web bind address set for non-smoke mode");
    run_debug_web_live_session(path, options, args, capabilities, bind_address).await
}

async fn run_debug_web_live_session(
    path: &str,
    options: DebugWebOptions<'_>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    bind_address: SocketAddr,
) -> Result<()> {
    let (source_path, chunk, mut vm) =
        prepare_debug_ui_run(path, options.step, options.breakpoints, args, capabilities)?;
    let (action_tx, action_rx) = std_mpsc::channel::<DebugCommand>();
    let (event_tx, _) = tokio::sync::broadcast::channel::<String>(64);
    let latest_event = Arc::new(Mutex::new(None::<String>));
    let status = Arc::new(Mutex::new(DebugWebSessionStatus::default()));
    let app_state = DebugWebLiveState {
        events: event_tx.clone(),
        latest_event: Arc::clone(&latest_event),
        status: Arc::clone(&status),
        actions: Arc::new(Mutex::new(action_tx)),
    };

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind debug-web server on {bind_address}"))?;
    let address = listener
        .local_addr()
        .context("failed to read debug-web listener address")?;
    let app = Router::new()
        .route("/", get(debug_web_live_page))
        .route("/events", get(debug_web_events))
        .route("/control", post(debug_web_control))
        .with_state(app_state);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });
    println!("Ricochet debug web listening on http://{address}/");

    let session_result = tokio::task::block_in_place(|| {
        run_debug_web_vm_session(
            source_path,
            chunk,
            &mut vm,
            action_rx,
            event_tx,
            latest_event,
            status,
        )
    });
    let _ = shutdown_tx.send(());
    match server_task.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) if session_result.is_ok() => {
            bail!("debug-web server stopped unexpectedly: {error}");
        }
        Ok(Err(_)) => {}
        Err(error) if session_result.is_ok() => {
            bail!("debug-web server task failed: {error}");
        }
        Err(_) => {}
    }
    session_result
}

fn run_debug_web_vm_session(
    source_path: PathBuf,
    chunk: Chunk,
    vm: &mut Vm,
    action_rx: std_mpsc::Receiver<DebugCommand>,
    event_tx: tokio::sync::broadcast::Sender<String>,
    latest_event: Arc<Mutex<Option<String>>>,
    status: Arc<Mutex<DebugWebSessionStatus>>,
) -> Result<()> {
    let pause_count = Rc::new(Cell::new(0_usize));
    let controller_pause_count = Rc::clone(&pause_count);
    let controller_source_path = source_path.clone();
    let controller_event_tx = event_tx.clone();
    let controller_latest_event = Arc::clone(&latest_event);
    let controller_status = Arc::clone(&status);
    vm.set_debug_controller_with_control(move |pause, control| {
        let count = controller_pause_count.get() + 1;
        controller_pause_count.set(count);
        let snapshot = debug_ui_snapshot_for_pause(
            &controller_source_path,
            pause.clone(),
            String::new(),
            String::new(),
        );
        {
            let mut status = controller_status
                .lock()
                .expect("debug-web session status lock");
            status.paused = true;
            status.completed = false;
            status.pause_id = count;
        }
        publish_debug_web_event(
            &controller_event_tx,
            &controller_latest_event,
            debug_web_pause_event(&snapshot, count),
        );
        loop {
            let command = action_rx
                .recv()
                .unwrap_or(DebugCommand::Resume(DebugAction::Abort));
            if let Some(action) = apply_debug_control_command(command, control, |event| {
                publish_debug_web_event(&controller_event_tx, &controller_latest_event, event)
            }) {
                {
                    let mut status = controller_status
                        .lock()
                        .expect("debug-web session status lock");
                    status.paused = false;
                }
                return action;
            }
        }
    });

    let result = vm.run_chunk(&chunk);
    match result {
        Ok(()) => {
            mark_debug_web_session_completed(&status);
            publish_debug_web_event(
                &event_tx,
                &latest_event,
                json!({
                    "event": "completed",
                    "pause_count": pause_count.get(),
                    "stdout": vm.stdout(),
                    "stderr": vm.stderr(),
                }),
            );
            Ok(())
        }
        Err(ricochet_vm::VmError::ExecutionAborted { .. }) => {
            mark_debug_web_session_completed(&status);
            publish_debug_web_event(
                &event_tx,
                &latest_event,
                json!({
                    "event": "aborted",
                    "pause_count": pause_count.get(),
                    "stdout": vm.stdout(),
                    "stderr": vm.stderr(),
                }),
            );
            Ok(())
        }
        Err(error) => {
            let message = runtime_error_message(vm, &error);
            mark_debug_web_session_completed(&status);
            publish_debug_web_event(
                &event_tx,
                &latest_event,
                json!({
                    "event": "fault",
                    "pause_count": pause_count.get(),
                    "message": message,
                    "stdout": vm.stdout(),
                    "stderr": vm.stderr(),
                }),
            );
            bail!("{message}")
        }
    }
}

async fn debug_web_live_page() -> Html<String> {
    Html(render_debug_web_live_page())
}

async fn debug_web_events(
    State(state): State<DebugWebLiveState>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let live_events = state.events.subscribe();
    let initial = state
        .latest_event
        .lock()
        .expect("debug-web latest event lock")
        .clone();
    let (events_tx, events_rx) = tokio::sync::mpsc::channel::<String>(16);
    tokio::spawn(forward_debug_web_sse_events(
        initial,
        live_events,
        events_tx,
    ));
    let stream =
        ReceiverStream::new(events_rx).map(|data| Ok(Event::default().event("debug").data(data)));

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn forward_debug_web_sse_events(
    initial: Option<String>,
    mut live_events: tokio::sync::broadcast::Receiver<String>,
    events_tx: tokio::sync::mpsc::Sender<String>,
) {
    let mut last_sent = None::<String>;
    if let Some(data) = initial {
        let terminal = debug_web_event_is_terminal(&data);
        if send_debug_web_sse_event(&events_tx, data, &mut last_sent)
            .await
            .is_err()
            || terminal
        {
            return;
        }
    }

    loop {
        match live_events.recv().await {
            Ok(data) => {
                if last_sent.as_deref() == Some(data.as_str()) {
                    continue;
                }
                let terminal = debug_web_event_is_terminal(&data);
                if send_debug_web_sse_event(&events_tx, data, &mut last_sent)
                    .await
                    .is_err()
                    || terminal
                {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                let data = json!({
                    "event": "lagged",
                    "skipped": skipped,
                })
                .to_string();
                if send_debug_web_sse_event(&events_tx, data, &mut last_sent)
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn send_debug_web_sse_event(
    events_tx: &tokio::sync::mpsc::Sender<String>,
    data: String,
    last_sent: &mut Option<String>,
) -> std::result::Result<(), ()> {
    *last_sent = Some(data.clone());
    events_tx.send(data).await.map_err(|_| ())
}

fn debug_web_event_is_terminal(data: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(data)
        .ok()
        .and_then(|value| {
            value
                .get("event")
                .and_then(serde_json::Value::as_str)
                .map(|event| matches!(event, "completed" | "aborted" | "fault"))
        })
        .unwrap_or(false)
}

async fn debug_web_control(
    State(state): State<DebugWebLiveState>,
    Json(request): Json<DebugWebControlRequest>,
) -> impl IntoResponse {
    let command = match debug_command_from_web_request(&request) {
        Ok(command) => command,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "ok": false,
                    "error": error,
                    "expected": [
                        "step",
                        "next",
                        "out",
                        "continue",
                        "abort",
                        "breakpoint_add",
                        "breakpoint_remove",
                        "breakpoint_clear",
                        "breakpoints"
                    ],
                })),
            )
                .into_response();
        }
    };
    let resumes = debug_command_resumes(&command);

    {
        let mut status = state.status.lock().expect("debug-web session status lock");
        if status.completed {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "ok": false,
                    "error": "debug session is already complete",
                })),
            )
                .into_response();
        }
        if !status.paused {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "ok": false,
                    "error": "debug session is not paused",
                })),
            )
                .into_response();
        }
        if let Some(pause_id) = request.pause_id {
            if pause_id != status.pause_id {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "ok": false,
                        "error": "stale pause id",
                        "current_pause_id": status.pause_id,
                    })),
                )
                    .into_response();
            }
        }
        if resumes {
            status.paused = false;
        }
    }

    let command_label = debug_command_label(&command);
    let send_result = state
        .actions
        .lock()
        .expect("debug-web action channel lock")
        .send(command);
    match send_result {
        Ok(()) => Json(json!({
            "ok": true,
            "action": command_label,
        }))
        .into_response(),
        Err(_) => {
            mark_debug_web_session_completed(&state.status);
            (
                StatusCode::CONFLICT,
                Json(json!({
                    "ok": false,
                    "error": "debug session is already complete",
                })),
            )
                .into_response()
        }
    }
}

fn collect_debug_ui_snapshot(
    path: &str,
    step: bool,
    breakpoints: &[usize],
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<DebugUiSnapshot> {
    let (source_path, chunk, mut vm) =
        prepare_debug_ui_run(path, step, breakpoints, args, capabilities)?;

    let first_pause = Rc::new(RefCell::new(None::<DebugPause>));
    let pause_slot = Rc::clone(&first_pause);
    vm.set_debug_controller(move |pause| {
        let mut slot = pause_slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(pause.clone());
        }
        DebugAction::Abort
    });

    let result = vm.run_chunk(&chunk);
    match result {
        Ok(()) | Err(ricochet_vm::VmError::ExecutionAborted { .. }) => {}
        Err(error) => bail!("{}", runtime_error_message(&vm, &error)),
    }

    let pause = first_pause
        .borrow_mut()
        .take()
        .context("debug UI did not observe a debugger pause")?;
    Ok(debug_ui_snapshot_for_pause(
        &source_path,
        pause,
        vm.stdout().to_string(),
        vm.stderr().to_string(),
    ))
}

fn run_debug_tui_session(
    path: &str,
    options: DebugTuiOptions<'_>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<()> {
    let (source_path, chunk, mut vm) =
        prepare_debug_ui_run(path, options.step, options.breakpoints, args, capabilities)?;
    let scripted = !options.commands.is_empty();
    let commands = Rc::new(RefCell::new(VecDeque::from(options.commands)));
    let command_error = Rc::new(RefCell::new(None::<String>));
    let pause_count = Rc::new(Cell::new(0_usize));
    let controller_source_path = source_path.clone();
    let controller_commands = Rc::clone(&commands);
    let controller_error = Rc::clone(&command_error);
    let controller_pause_count = Rc::clone(&pause_count);
    vm.set_debug_controller_with_control(move |pause, control| {
        controller_pause_count.set(controller_pause_count.get() + 1);
        let snapshot = debug_ui_snapshot_for_pause(
            &controller_source_path,
            pause.clone(),
            String::new(),
            String::new(),
        );
        print!(
            "{}",
            render_debug_tui_snapshot(&snapshot, DebugUiRenderMode::Interactive)
        );
        if io::stdout().flush().is_err() {
            *controller_error.borrow_mut() = Some("failed to flush debug-tui output".to_string());
            return DebugAction::Abort;
        }

        if scripted {
            loop {
                let Some(command) = controller_commands.borrow_mut().pop_front() else {
                    *controller_error.borrow_mut() = Some(
                        "debug-tui command script ended while the VM was still paused; add another --command"
                            .to_string(),
                    );
                    return DebugAction::Abort;
                };
                match debug_command_from_command(&command) {
                    Ok(command_action) => {
                        println!("debug-tui command: {}", command.trim());
                        if let Some(action) =
                            apply_debug_control_command(command_action, control, |event| {
                                println!("debug-tui event: {event}");
                            })
                        {
                            return action;
                        }
                    }
                    Err(error) => {
                        *controller_error.borrow_mut() = Some(format!(
                            "unknown debug-tui command {:?}: {error}; expected step, next, out, continue, abort, break <line>, clear <line>, clear_breakpoints, or breakpoints",
                            command.trim()
                        ));
                        return DebugAction::Abort;
                    }
                }
            }
        } else {
            read_debug_tui_action_from_stdin(control)
        }
    });

    let result = vm.run_chunk(&chunk);
    if let Some(error) = command_error.borrow_mut().take() {
        bail!("{error}");
    }
    print_debug_tui_program_output(&vm);
    match result {
        Ok(()) => {
            println!(
                "debug-tui: program completed after {} pause(s)",
                pause_count.get()
            );
            Ok(())
        }
        Err(ricochet_vm::VmError::ExecutionAborted { .. }) => {
            println!(
                "debug-tui: session aborted after {} pause(s)",
                pause_count.get()
            );
            Ok(())
        }
        Err(error) => bail!("{}", runtime_error_message(&vm, &error)),
    }
}

fn prepare_debug_ui_run(
    path: &str,
    step: bool,
    breakpoints: &[usize],
    args: Vec<String>,
    capabilities: CapabilityOptions,
) -> Result<(PathBuf, Chunk, Vm)> {
    let source_path = Path::new(path).to_path_buf();
    let chunk = compile_source_file(&source_path)?;
    let mut vm = cli_vm(args, &capabilities)?;
    install_dynamic_module_loader(&mut vm, dynamic_import_parent_for_source(&source_path)?);
    vm.enable_debug();
    if step || breakpoints.is_empty() {
        vm.enable_step_debugging();
    }
    for &line in breakpoints {
        if line == 0 {
            bail!("breakpoint lines are 1-based");
        }
        vm.add_line_breakpoint(chunk.file.clone(), line);
    }
    Ok((source_path, chunk, vm))
}

fn debug_ui_snapshot_for_pause(
    source_path: &Path,
    pause: DebugPause,
    stdout: String,
    stderr: String,
) -> DebugUiSnapshot {
    let source_line = debug_source_line(source_path, &pause);
    DebugUiSnapshot {
        pause,
        source_line,
        stdout,
        stderr,
    }
}

fn debug_web_pause_event(snapshot: &DebugUiSnapshot, pause_id: usize) -> serde_json::Value {
    let mut value =
        debug_event_json(&DebugEvent::Paused(snapshot.pause.clone())).unwrap_or_else(|_| {
            json!({
                "event": "error",
                "error": "debug protocol cannot serialize non-serializable value",
            })
        });
    if let serde_json::Value::Object(fields) = &mut value {
        fields.insert("pause_id".to_string(), json!(pause_id));
        fields.insert("source_line".to_string(), json!(snapshot.source_line));
        if !snapshot.stdout.is_empty() {
            fields.insert("stdout".to_string(), json!(snapshot.stdout));
        }
        if !snapshot.stderr.is_empty() {
            fields.insert("stderr".to_string(), json!(snapshot.stderr));
        }
    }
    value
}

fn publish_debug_web_event(
    events: &tokio::sync::broadcast::Sender<String>,
    latest_event: &Arc<Mutex<Option<String>>>,
    value: serde_json::Value,
) {
    let data = value.to_string();
    *latest_event.lock().expect("debug-web latest event lock") = Some(data.clone());
    let _ = events.send(data);
}

fn mark_debug_web_session_completed(status: &Arc<Mutex<DebugWebSessionStatus>>) {
    let mut status = status.lock().expect("debug-web session status lock");
    status.paused = false;
    status.completed = true;
}

fn debug_source_line(source_path: &Path, pause: &DebugPause) -> Option<String> {
    let line_number = pause.source.rsplit_once(':')?.1.parse::<usize>().ok()?;
    let source = fs::read_to_string(source_path).ok()?;
    source
        .lines()
        .nth(line_number.saturating_sub(1))
        .map(str::trim_end)
        .map(str::to_string)
}

fn render_debug_tui_snapshot(snapshot: &DebugUiSnapshot, mode: DebugUiRenderMode) -> String {
    let pause = &snapshot.pause;
    let mut output = String::new();
    let reason = match pause.reason {
        DebugPauseReason::Step => "step",
        DebugPauseReason::Breakpoint => "breakpoint",
    };
    writeln!(&mut output, "Ricochet Debug TUI").expect("write to string");
    writeln!(&mut output, "status: paused ({reason})").expect("write to string");
    writeln!(&mut output, "source: {}", pause.source).expect("write to string");
    if let Some(source_line) = &snapshot.source_line {
        writeln!(&mut output, "source line: {source_line}").expect("write to string");
    }
    writeln!(&mut output, "frame: {}", pause.frame).expect("write to string");
    writeln!(&mut output, "opcode: {}", pause.opcode).expect("write to string");
    write_debug_tui_values(&mut output, "stack", &pause.stack);
    write_debug_tui_bindings(&mut output, "locals", &pause.locals);
    write_debug_tui_bindings(&mut output, "globals", &pause.globals);
    if let Some(current_self) = &pause.current_self {
        writeln!(&mut output, "self: {current_self:?}").expect("write to string");
    }
    write_debug_tui_tasks(&mut output, &pause.tasks);
    if !snapshot.stdout.is_empty() {
        writeln!(&mut output, "stdout:\n{}", snapshot.stdout).expect("write to string");
    }
    if !snapshot.stderr.is_empty() {
        writeln!(&mut output, "stderr:\n{}", snapshot.stderr).expect("write to string");
    }
    match mode {
        DebugUiRenderMode::Preview => {
            writeln!(
                &mut output,
                "preview: read-only snapshot; run without --smoke for interactive controls"
            )
            .expect("write to string");
        }
        DebugUiRenderMode::Interactive => {
            writeln!(
                &mut output,
                "controls: step | next | out | continue | abort (s/n/o/c/q)"
            )
            .expect("write to string");
        }
    }
    output
}

fn write_debug_tui_values(output: &mut String, label: &str, values: &[Value]) {
    writeln!(output, "{label}:").expect("write to string");
    if values.is_empty() {
        writeln!(output, "  <empty>").expect("write to string");
    } else {
        for (index, value) in values.iter().enumerate() {
            writeln!(output, "  [{index}] {}", debug_value_label(value)).expect("write to string");
        }
    }
}

fn write_debug_tui_bindings(output: &mut String, label: &str, bindings: &[(String, Value)]) {
    writeln!(output, "{label}:").expect("write to string");
    if bindings.is_empty() {
        writeln!(output, "  <empty>").expect("write to string");
    } else {
        for (name, value) in bindings {
            writeln!(output, "  {name} = {}", debug_value_label(value)).expect("write to string");
        }
    }
}

fn write_debug_tui_tasks(output: &mut String, tasks: &[DebugTask]) {
    writeln!(output, "tasks:").expect("write to string");
    if tasks.is_empty() {
        writeln!(output, "  <empty>").expect("write to string");
        return;
    }
    for task in tasks {
        writeln!(
            output,
            "  task {}: {} operation={} pending={} running={} completed={} failed={} frames={}",
            task.id,
            task.status,
            task.operation,
            task.pending,
            task.running,
            task.completed,
            task.failed,
            task.frames.len()
        )
        .expect("write to string");
        if let Some(fault) = &task.fault {
            writeln!(output, "    fault: {fault}").expect("write to string");
        }
        for (index, frame) in task.frames.iter().enumerate() {
            writeln!(
                output,
                "    frame {index}: {} {} {}",
                frame.frame, frame.source, frame.opcode
            )
            .expect("write to string");
        }
    }
}

fn render_debug_web_snapshot(snapshot: &DebugUiSnapshot) -> String {
    let pause = &snapshot.pause;
    let reason = match pause.reason {
        DebugPauseReason::Step => "step",
        DebugPauseReason::Breakpoint => "breakpoint",
    };
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Ricochet Debug Web</title>");
    html.push_str(
        "<style>body{font:14px system-ui,sans-serif;margin:24px;max-width:1100px}\
         header{border-bottom:1px solid #ccc;margin-bottom:16px}\
         section{margin:16px 0}pre{background:#111;color:#f5f5f5;padding:12px;overflow:auto}\
         table{border-collapse:collapse;width:100%}td,th{border:1px solid #ddd;padding:6px;text-align:left}\
         .muted{color:#666}</style>",
    );
    html.push_str("</head><body><header><h1>Ricochet Debug Web</h1>");
    write!(
        &mut html,
        "<p>Paused: {} at <code>{}</code></p>",
        html_escape(reason),
        html_escape(&pause.source)
    )
    .expect("write to string");
    if let Some(source_line) = &snapshot.source_line {
        write!(
            &mut html,
            "<p><strong>Source line:</strong> <code>{}</code></p>",
            html_escape(source_line)
        )
        .expect("write to string");
    }
    html.push_str("</header>");
    write!(
        &mut html,
        "<section><h2>Frame</h2><table><tr><th>Frame</th><td>{}</td></tr>\
         <tr><th>Opcode</th><td><code>{}</code></td></tr></table></section>",
        html_escape(&pause.frame),
        html_escape(&pause.opcode)
    )
    .expect("write to string");
    write_debug_web_values(&mut html, "Stack", &pause.stack);
    write_debug_web_bindings(&mut html, "Locals", &pause.locals);
    write_debug_web_bindings(&mut html, "Globals", &pause.globals);
    write_debug_web_tasks(&mut html, &pause.tasks);
    if !snapshot.stdout.is_empty() {
        write!(
            &mut html,
            "<section><h2>Stdout</h2><pre>{}</pre></section>",
            html_escape(&snapshot.stdout)
        )
        .expect("write to string");
    }
    if !snapshot.stderr.is_empty() {
        write!(
            &mut html,
            "<section><h2>Stderr</h2><pre>{}</pre></section>",
            html_escape(&snapshot.stderr)
        )
        .expect("write to string");
    }
    html.push_str("<p class=\"muted\">Read-only debugger web preview. Run without --smoke for live loopback controls and SSE events.</p>");
    html.push_str("</body></html>");
    html
}

fn render_debug_web_live_page() -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Ricochet Debug Web</title>");
    html.push_str(
        "<style>:root{color-scheme:light dark;--border:#c8d0d8;--panel:#f7f9fb;--ink:#17202a;--muted:#5f6f7c;--accent:#0b6bcb}\
         body{font:14px system-ui,sans-serif;margin:0;color:var(--ink);background:#fff}\
         main{display:grid;grid-template-columns:minmax(220px,300px) minmax(0,1fr);min-height:100vh}\
         aside{border-right:1px solid var(--border);padding:16px;background:var(--panel)}\
         section{border-bottom:1px solid var(--border);padding:14px 16px}\
         h1{font-size:20px;margin:0 0 4px}h2{font-size:13px;margin:0 0 10px;text-transform:uppercase;color:var(--muted)}\
         button{margin:0 6px 8px 0;padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:#fff;color:var(--ink);cursor:pointer}\
         button:focus,input:focus{outline:2px solid var(--accent);outline-offset:2px}\
         input{width:84px;margin:0 6px 8px 0;padding:6px;border:1px solid var(--border);border-radius:6px}\
         pre{background:#111;color:#f5f5f5;padding:12px;overflow:auto;white-space:pre-wrap}\
         code{font-family:ui-monospace,SFMono-Regular,Consolas,monospace}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:0}\
         .value-list{display:grid;gap:6px}.value-row{display:grid;grid-template-columns:100px minmax(0,1fr);gap:8px}\
         .muted{color:var(--muted)}@media (max-width:760px){main{display:block}.grid{grid-template-columns:1fr}aside{border-right:0;border-bottom:1px solid var(--border)}}\
         @media (prefers-color-scheme:dark){:root{--border:#344452;--panel:#121820;--ink:#e6edf3;--muted:#9fb1c1;--accent:#58a6ff}body,button,input{background:#0d1117;color:var(--ink)}}</style>",
    );
    html.push_str("</head><body><main><aside><header><h1>Ricochet Debug Web</h1>");
    html.push_str(
        "<p id=\"session-status\" class=\"muted\" aria-live=\"polite\">connecting</p></header>",
    );
    html.push_str("<section aria-label=\"Debugger controls\"><h2>Controls</h2>");
    for action in ["step", "next", "out", "continue", "abort"] {
        let shortcut = match action {
            "step" => "s",
            "next" => "n",
            "out" => "o",
            "continue" => "c",
            "abort" => "q",
            _ => "",
        };
        write!(
            &mut html,
            "<button type=\"button\" data-action=\"{action}\" title=\"Shortcut: {shortcut}\">{action}</button>"
        )
        .expect("write to string");
    }
    html.push_str("</section><section aria-label=\"Breakpoint controls\"><h2>Breakpoints</h2>");
    html.push_str("<label>Line <input id=\"breakpoint-line\" type=\"number\" min=\"1\"></label>");
    for action in [
        ("breakpoint_add", "add breakpoint"),
        ("breakpoint_remove", "remove breakpoint"),
        ("breakpoint_clear", "clear breakpoints"),
        ("breakpoints", "list breakpoints"),
    ] {
        write!(
            &mut html,
            "<button type=\"button\" data-breakpoint-action=\"{}\">{}</button>",
            action.0, action.1
        )
        .expect("write to string");
    }
    html.push_str("<div id=\"breakpoints\" class=\"value-list muted\">none</div>");
    html.push_str("</section></aside><div>");
    html.push_str("<section aria-label=\"Source\"><h2>Source</h2><div id=\"source-line\" class=\"muted\">waiting for pause</div></section>");
    html.push_str("<section aria-label=\"Current instruction\"><h2>Current Instruction</h2><div id=\"current-instruction\" class=\"value-list muted\">waiting for pause</div></section>");
    html.push_str("<div class=\"grid\"><section aria-label=\"Stack\"><h2>Stack</h2><div id=\"stack\" class=\"value-list muted\">empty</div></section>");
    html.push_str("<section aria-label=\"Locals\"><h2>Locals</h2><div id=\"locals\" class=\"value-list muted\">empty</div></section>");
    html.push_str("<section aria-label=\"Globals\"><h2>Globals</h2><div id=\"globals\" class=\"value-list muted\">empty</div></section>");
    html.push_str("<section aria-label=\"Self\"><h2>Self</h2><div id=\"self-value\" class=\"value-list muted\">nil</div></section>");
    html.push_str("<section aria-label=\"Tasks\"><h2>Tasks</h2><div id=\"tasks\" class=\"value-list muted\">empty</div></section></div>");
    html.push_str("<section aria-label=\"Program output\"><h2>Output</h2><pre id=\"program-output\"></pre></section>");
    html.push_str("<section aria-label=\"Event log\"><h2>Events</h2><pre id=\"events\">connecting...</pre></section>");
    html.push_str("</div></main>");
    html.push_str(
        "<script>
const eventLog = document.getElementById('events');
const panes = {
  status: document.getElementById('session-status'),
  source: document.getElementById('source-line'),
  instruction: document.getElementById('current-instruction'),
  stack: document.getElementById('stack'),
  locals: document.getElementById('locals'),
  globals: document.getElementById('globals'),
  selfValue: document.getElementById('self-value'),
  tasks: document.getElementById('tasks'),
  output: document.getElementById('program-output'),
  breakpoints: document.getElementById('breakpoints'),
};
let latestPauseId = null;
function append(line) {
  if (eventLog.textContent === 'connecting...') eventLog.textContent = '';
  eventLog.textContent += line + '\\n';
  eventLog.scrollTop = eventLog.scrollHeight;
}
function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>\"']/g, (char) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '\"': '&quot;', \"'\": '&#39;'
  }[char]));
}
function valueLabel(value) {
  return value && typeof value === 'object' && 'debug' in value ? value.debug : JSON.stringify(value);
}
function renderPairs(items, keyName) {
  if (!items || items.length === 0) return '<span class=\"muted\">empty</span>';
  return items.map((item, index) => {
    const key = keyName ? item[keyName] : `[${index}]`;
    const value = keyName ? item.value : item;
    return `<div class=\"value-row\"><span>${escapeHtml(key)}</span><code>${escapeHtml(valueLabel(value))}</code></div>`;
  }).join('');
}
function renderTasks(tasks) {
  if (!tasks || tasks.length === 0) return '<span class=\"muted\">empty</span>';
  return tasks.map((task) => {
    const frames = (task.frames || []).map((frame, index) =>
      `<div class=\"value-row\"><span>frame ${index}</span><code>${escapeHtml(frame.opcode)}</code></div><div class=\"muted\">${escapeHtml(frame.source)}</div>`
    ).join('');
    return `<div><strong>task ${escapeHtml(task.id)}</strong> ${escapeHtml(task.status)} <span class=\"muted\">${escapeHtml(task.operation)}</span>${frames}</div>`;
  }).join('');
}
function renderBreakpoints(payload) {
  if (!payload.breakpoints || payload.breakpoints.length === 0) {
    panes.breakpoints.innerHTML = '<span class=\"muted\">none</span>';
    return;
  }
  panes.breakpoints.innerHTML = payload.breakpoints
    .map((bp) => `<div><code>${escapeHtml(bp.file)}:${escapeHtml(bp.line)}</code></div>`)
    .join('');
}
function renderPaused(payload) {
  latestPauseId = payload.pause_id;
  panes.status.textContent = `paused (${payload.reason})`;
  panes.source.innerHTML = `<code>${escapeHtml(payload.source)}</code><br><strong>${escapeHtml(payload.source_line ?? '')}</strong>`;
  panes.instruction.innerHTML =
    `<div class=\"value-row\"><span>frame</span><code>${escapeHtml(payload.frame)}</code></div>` +
    `<div class=\"value-row\"><span>opcode</span><code>${escapeHtml(payload.opcode)}</code></div>` +
    `<div class=\"value-row\"><span>pause</span><code>${escapeHtml(payload.pause_id)}</code></div>`;
  panes.stack.innerHTML = renderPairs(payload.stack);
  panes.locals.innerHTML = renderPairs(payload.locals, 'name');
  panes.globals.innerHTML = renderPairs(payload.globals, 'name');
  panes.selfValue.innerHTML = payload.self
    ? `<code>${escapeHtml(valueLabel(payload.self))}</code>`
    : '<span class=\"muted\">nil</span>';
  panes.tasks.innerHTML = renderTasks(payload.tasks);
  panes.output.textContent = [payload.stdout, payload.stderr].filter(Boolean).join('\\n');
}
function renderPayload(payload) {
  if (payload.event === 'paused') renderPaused(payload);
  if (payload.event === 'breakpoints' || payload.event === 'breakpoint_added' || payload.event === 'breakpoint_removed' || payload.event === 'breakpoints_cleared') renderBreakpoints(payload);
  if (payload.event === 'completed' || payload.event === 'aborted' || payload.event === 'fault') panes.status.textContent = payload.event;
}
const events = new EventSource('/events');
events.addEventListener('debug', (event) => {
  append(event.data);
  try {
    const payload = JSON.parse(event.data);
    renderPayload(payload);
  } catch (_) {
    panes.status.textContent = 'event parse error';
  }
});
events.onerror = () => {
  panes.status.textContent = 'connection error';
  append('{\"event\":\"connection_error\"}');
};
for (const button of document.querySelectorAll('button[data-action]')) {
  button.addEventListener('click', async () => {
    const body = { action: button.dataset.action };
    if (latestPauseId !== null) body.pause_id = latestPauseId;
    await sendControl(body);
  });
}
for (const button of document.querySelectorAll('button[data-breakpoint-action]')) {
  button.addEventListener('click', async () => {
    const body = { action: button.dataset.breakpointAction };
    if (latestPauseId !== null) body.pause_id = latestPauseId;
    const line = Number(document.getElementById('breakpoint-line').value);
    if (Number.isInteger(line) && line > 0) body.line = line;
    await sendControl(body);
  });
}
async function sendControl(body) {
  const response = await fetch('/control', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const responseBody = await response.json();
  append(JSON.stringify({ event: 'control_response', status: response.status, body: responseBody }));
  if (!response.ok) panes.status.textContent = responseBody.error || 'control error';
}
document.addEventListener('keydown', (event) => {
  if (event.target instanceof HTMLInputElement || event.ctrlKey || event.metaKey || event.altKey) return;
  const shortcuts = { s: 'step', n: 'next', o: 'out', c: 'continue', q: 'abort' };
  const action = shortcuts[event.key.toLowerCase()];
  if (!action) return;
  event.preventDefault();
  document.querySelector(`button[data-action=\"${action}\"]`)?.click();
});
</script>",
    );
    html.push_str("</body></html>");
    html
}

fn write_debug_web_values(html: &mut String, label: &str, values: &[Value]) {
    write!(html, "<section><h2>{}</h2>", html_escape(label)).expect("write to string");
    if values.is_empty() {
        html.push_str("<p class=\"muted\">&lt;empty&gt;</p></section>");
        return;
    }
    html.push_str("<table><tr><th>Index</th><th>Value</th></tr>");
    for (index, value) in values.iter().enumerate() {
        write!(
            html,
            "<tr><td>{index}</td><td><code>{}</code></td></tr>",
            html_escape(&debug_value_label(value))
        )
        .expect("write to string");
    }
    html.push_str("</table></section>");
}

fn write_debug_web_bindings(html: &mut String, label: &str, bindings: &[(String, Value)]) {
    write!(html, "<section><h2>{}</h2>", html_escape(label)).expect("write to string");
    if bindings.is_empty() {
        html.push_str("<p class=\"muted\">&lt;empty&gt;</p></section>");
        return;
    }
    html.push_str("<table><tr><th>Name</th><th>Value</th></tr>");
    for (name, value) in bindings {
        write!(
            html,
            "<tr><td>{}</td><td><code>{}</code></td></tr>",
            html_escape(name),
            html_escape(&debug_value_label(value))
        )
        .expect("write to string");
    }
    html.push_str("</table></section>");
}

fn write_debug_web_tasks(html: &mut String, tasks: &[DebugTask]) {
    html.push_str("<section><h2>Tasks</h2>");
    if tasks.is_empty() {
        html.push_str("<p class=\"muted\">&lt;empty&gt;</p></section>");
        return;
    }
    html.push_str("<table><tr><th>ID</th><th>Status</th><th>Operation</th><th>Frames</th></tr>");
    for task in tasks {
        write!(
            html,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            task.id,
            html_escape(&task.status),
            html_escape(&task.operation),
            task.frames.len()
        )
        .expect("write to string");
        for (index, frame) in task.frames.iter().enumerate() {
            write!(
                html,
                "<tr><td></td><td colspan=\"3\"><strong>frame {index}</strong>: {} \
                 <code>{}</code><br><span class=\"muted\">{}</span></td></tr>",
                html_escape(&frame.frame),
                html_escape(&frame.opcode),
                html_escape(&frame.source)
            )
            .expect("write to string");
        }
    }
    html.push_str("</table></section>");
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn read_debug_tui_action_from_stdin(control: &mut DebugControl<'_>) -> DebugAction {
    loop {
        print!("debug-tui> ");
        if io::stdout().flush().is_err() {
            return DebugAction::Abort;
        }
        let mut command = String::new();
        match io::stdin().read_line(&mut command) {
            Ok(0) | Err(_) => return DebugAction::Abort,
            Ok(_) => match debug_command_from_command(&command) {
                Ok(command) => {
                    if let Some(action) = apply_debug_control_command(command, control, |event| {
                        println!("debug-tui event: {event}");
                    }) {
                        return action;
                    }
                }
                Err(_) => {
                    println!(
                        "commands: step, next, out, continue, abort, break <line>, clear <line>, clear_breakpoints, breakpoints (aliases: s, n, o, c, q)"
                    )
                }
            },
        }
    }
}

fn print_debug_tui_program_output(vm: &Vm) {
    if !vm.stdout().is_empty() {
        print!("stdout:\n{}", vm.stdout());
    }
    if !vm.stderr().is_empty() {
        eprint!("stderr:\n{}", vm.stderr());
    }
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
        format!("{} array push calls", workload.collection_ops),
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
        writeln!(&mut source, "$items {item} push drop").expect("write to string succeeds");
    }
    source.push_str("$items count\n");
    source
}

fn generated_json_source(items: usize) -> String {
    let mut source = String::from("payload map\nitems array\n");
    for item in 0..items {
        writeln!(&mut source, "item{item} map").expect("write to string succeeds");
        writeln!(&mut source, "$item{item} \"id\" {item} put drop")
            .expect("write to string succeeds");
        writeln!(&mut source, "$item{item} \"name\" \"item-{item}\" put drop")
            .expect("write to string succeeds");
        writeln!(&mut source, "$items $item{item} push drop").expect("write to string succeeds");
    }
    source.push_str(
        r#"$payload "items" $items put drop
$payload json_encode encoded var
$encoded json_decode value decoded var
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

const DAP_TASK_REFERENCE_BASE: u64 = 10_000;
const DAP_TASK_FRAME_REFERENCE_BASE: u64 = 1_000_000;
const DAP_TASK_FRAME_REFERENCE_STRIDE: u64 = 1_000;

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
        if self
            .last_pause
            .as_ref()
            .is_some_and(|pause| dap_selected_values_contain_opaque(pause, reference))
        {
            return self.send_error_response(
                request,
                "debug adapter cannot serialize non-serializable value",
            );
        }
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
            (reference, Some(pause)) => {
                if let Some(task_id) = dap_task_reference_id(reference) {
                    dap_task_detail_variables(pause, task_id)
                } else if let Some((task_id, frame_index)) = dap_task_frame_reference_id(reference)
                {
                    dap_task_frame_variables(pause, task_id, frame_index)
                } else {
                    Vec::new()
                }
            }
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

    let dynamic_import_parent = dynamic_import_parent_for_source(&setup.program)?;
    let mut vm = cli_vm(setup.args, &CapabilityOptions::default())?;
    install_dynamic_module_loader(&mut vm, dynamic_import_parent);
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

fn dap_selected_values_contain_opaque(pause: &DebugPause, reference: u64) -> bool {
    let contains = |value: &Value| value.opaque_value_kind().is_some();
    match reference {
        1 => pause.stack.iter().any(contains),
        2 => pause.locals.iter().any(|(_, value)| contains(value)),
        3 => pause.globals.iter().any(|(_, value)| contains(value)),
        4 => pause.current_self.as_ref().is_some_and(contains),
        5 => pause.tasks.iter().any(dap_task_contains_opaque),
        reference => {
            if let Some(task_id) = dap_task_reference_id(reference) {
                pause
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .is_some_and(dap_task_contains_opaque)
            } else if let Some((task_id, frame_index)) = dap_task_frame_reference_id(reference) {
                pause
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .and_then(|task| task.frames.get(frame_index))
                    .is_some_and(dap_task_frame_contains_opaque)
            } else {
                false
            }
        }
    }
}

fn dap_task_contains_opaque(task: &DebugTask) -> bool {
    task.frames.iter().any(dap_task_frame_contains_opaque)
}

fn dap_task_frame_contains_opaque(frame: &DebugTaskFrame) -> bool {
    frame
        .stack
        .iter()
        .any(|value| value.opaque_value_kind().is_some())
        || frame
            .locals
            .iter()
            .any(|(_, value)| value.opaque_value_kind().is_some())
        || frame
            .current_self
            .as_ref()
            .is_some_and(|value| value.opaque_value_kind().is_some())
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
            "{} operation={} pending={} running={} completed={} failed={} frames={}",
            task.status,
            task.operation,
            task.pending,
            task.running,
            task.completed,
            task.failed,
            task.frames.len()
        ),
        "variablesReference": dap_task_reference(task.id),
    })
}

fn dap_task_detail_variables(pause: &DebugPause, task_id: u64) -> Vec<serde_json::Value> {
    let Some(task) = pause.tasks.iter().find(|task| task.id == task_id) else {
        return Vec::new();
    };
    let mut variables = vec![
        dap_scalar_variable("operation", &task.operation),
        dap_scalar_variable("status", &task.status),
        dap_scalar_variable("pending", task.pending),
        dap_scalar_variable("running", task.running),
        dap_scalar_variable("completed", task.completed),
        dap_scalar_variable("failed", task.failed),
        dap_scalar_variable("frame_count", task.frames.len()),
    ];
    if let Some(fault) = &task.fault {
        variables.push(dap_scalar_variable("fault", fault));
    }
    variables.extend(task.frames.iter().enumerate().map(|(index, frame)| {
        json!({
            "name": format!("frame {index}"),
            "value": format!("{} {} {}", frame.frame, frame.source, frame.opcode),
            "variablesReference": dap_task_frame_reference(task.id, index),
        })
    }));
    variables
}

fn dap_task_frame_variables(
    pause: &DebugPause,
    task_id: u64,
    frame_index: usize,
) -> Vec<serde_json::Value> {
    let Some(frame) = pause
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .and_then(|task| task.frames.get(frame_index))
    else {
        return Vec::new();
    };
    dap_task_frame_detail_variables(frame)
}

fn dap_task_frame_detail_variables(frame: &DebugTaskFrame) -> Vec<serde_json::Value> {
    let mut variables = vec![
        dap_scalar_variable("frame", &frame.frame),
        dap_scalar_variable("source", &frame.source),
        dap_scalar_variable("opcode", &frame.opcode),
    ];
    variables.extend(
        frame
            .stack
            .iter()
            .enumerate()
            .map(|(index, value)| dap_value_variable(format!("stack[{index}]"), value)),
    );
    variables.extend(
        frame
            .locals
            .iter()
            .map(|(name, value)| dap_value_variable(format!("local {name}"), value)),
    );
    if let Some(current_self) = &frame.current_self {
        variables.push(dap_value_variable("self".to_string(), current_self));
    }
    variables
}

fn dap_scalar_variable(name: impl Into<String>, value: impl ToString) -> serde_json::Value {
    json!({
        "name": name.into(),
        "value": value.to_string(),
        "variablesReference": 0,
    })
}

fn dap_task_reference(task_id: u64) -> u64 {
    DAP_TASK_REFERENCE_BASE.saturating_add(task_id)
}

fn dap_task_reference_id(reference: u64) -> Option<u64> {
    if (DAP_TASK_REFERENCE_BASE..DAP_TASK_FRAME_REFERENCE_BASE).contains(&reference) {
        Some(reference - DAP_TASK_REFERENCE_BASE)
    } else {
        None
    }
}

fn dap_task_frame_reference(task_id: u64, frame_index: usize) -> u64 {
    DAP_TASK_FRAME_REFERENCE_BASE
        .saturating_add(task_id.saturating_mul(DAP_TASK_FRAME_REFERENCE_STRIDE))
        .saturating_add(frame_index as u64)
}

fn dap_task_frame_reference_id(reference: u64) -> Option<(u64, usize)> {
    if reference < DAP_TASK_FRAME_REFERENCE_BASE {
        return None;
    }
    let offset = reference - DAP_TASK_FRAME_REFERENCE_BASE;
    let task_id = offset / DAP_TASK_FRAME_REFERENCE_STRIDE;
    let frame_index = offset % DAP_TASK_FRAME_REFERENCE_STRIDE;
    usize::try_from(frame_index)
        .ok()
        .map(|frame_index| (task_id, frame_index))
}

fn cli_vm(args: Vec<String>, capabilities: &CapabilityOptions) -> Result<Vm> {
    let mut vm = Vm::default();
    capabilities.apply_to(&mut vm)?;
    install_dynamic_module_loader(&mut vm, current_dir_for_dynamic_imports()?);
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

fn dynamic_import_parent_for_source(source_path: &Path) -> Result<PathBuf> {
    let path = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to determine current directory")?
            .join(source_path)
    };
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::canonicalize(parent)
        .with_context(|| format!("failed to resolve dynamic import root {}", parent.display()))
}

fn current_dir_for_dynamic_imports() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to determine current directory")?;
    fs::canonicalize(&current_dir).with_context(|| {
        format!(
            "failed to resolve dynamic import root {}",
            current_dir.display()
        )
    })
}

fn read_terminal_debug_action(pause: &DebugPause, control: &mut DebugControl<'_>) -> DebugAction {
    loop {
        print!("debug> ");
        if io::stdout().flush().is_err() {
            return DebugAction::Abort;
        }

        let mut command = String::new();
        match io::stdin().read_line(&mut command) {
            Ok(0) | Err(_) => return DebugAction::Abort,
            Ok(_) => {
                let command = command.trim().to_ascii_lowercase();
                if let Ok(debug_command) = debug_command_from_command(&command) {
                    if let Some(action) =
                        apply_debug_control_command(debug_command, control, |event| {
                            println!("debug event: {event}");
                        })
                    {
                        return action;
                    }
                    continue;
                }
                match command.as_str() {
                    "stack" => println!("{:?}", pause.stack),
                    "locals" => print_debug_bindings("locals", &pause.locals),
                    "globals" => print_debug_bindings("globals", &pause.globals),
                    "self" => println!("{:?}", pause.current_self),
                    "tasks" => print_debug_tasks(&pause.tasks),
                    "tasks --tree" => print_debug_task_tree(&pause.tasks),
                    _ if handle_debug_task_command(&command, pause) => {}
                    _ => println!(
                        "commands: step, next, out, continue, abort, break <line>, clear <line>, clear_breakpoints, breakpoints, stack, locals, globals, self, tasks, tasks --tree, task <id> stack, task <id> locals"
                    ),
                }
            }
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

fn print_debug_tasks(tasks: &[DebugTask]) {
    if tasks.is_empty() {
        println!("tasks: <empty>");
        return;
    }
    println!("tasks:");
    for task in tasks {
        println!(
            "  task {}: {} operation={} pending={} running={} completed={} failed={} frames={}",
            task.id,
            task.status,
            task.operation,
            task.pending,
            task.running,
            task.completed,
            task.failed,
            task.frames.len()
        );
        if let Some(fault) = &task.fault {
            println!("    fault: {fault}");
        }
    }
}

fn print_debug_task_tree(tasks: &[DebugTask]) {
    if tasks.is_empty() {
        println!("tasks: <empty>");
        return;
    }
    println!("tasks:");
    for task in tasks {
        println!(
            "  task {}: {} operation={}",
            task.id, task.status, task.operation
        );
        for (index, frame) in task.frames.iter().enumerate() {
            println!(
                "    frame {index}: {} {} {}",
                frame.frame, frame.source, frame.opcode
            );
        }
    }
}

fn handle_debug_task_command(command: &str, pause: &DebugPause) -> bool {
    let mut parts = command.split_whitespace();
    if parts.next() != Some("task") {
        return false;
    }
    let Some(id) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
        println!("usage: task <id> stack|locals");
        return true;
    };
    let Some(task) = pause.tasks.iter().find(|task| task.id == id) else {
        println!("task {id}: <unknown>");
        return true;
    };
    match parts.next() {
        Some("stack") => print_debug_task_stack(task),
        Some("locals") => print_debug_task_locals(task),
        Some("info") | None => print_debug_tasks(std::slice::from_ref(task)),
        _ => println!("usage: task <id> stack|locals"),
    }
    true
}

fn print_debug_task_stack(task: &DebugTask) {
    if task.frames.is_empty() {
        println!("task {} stack: <empty>", task.id);
        return;
    }
    println!("task {} stack:", task.id);
    for (index, frame) in task.frames.iter().enumerate() {
        println!(
            "  frame {index}: {} {} {}",
            frame.frame, frame.source, frame.opcode
        );
        println!("    stack: {:?}", frame.stack);
    }
}

fn print_debug_task_locals(task: &DebugTask) {
    if task.frames.is_empty() {
        println!("task {} locals: <empty>", task.id);
        return;
    }
    println!("task {} locals:", task.id);
    for (index, frame) in task.frames.iter().enumerate() {
        println!("  frame {index}: {} {}", frame.frame, frame.source);
        if frame.locals.is_empty() {
            println!("    <empty>");
        } else {
            for (name, value) in &frame.locals {
                println!("    {name} = {value:?}");
            }
        }
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

fn expand_path(path: &str, json_output: bool) -> Result<()> {
    let path = Path::new(path);
    let module_id = module_id_for_path(path);

    if json_output {
        match expand_file_with_imports(path) {
            Ok(file_expansion) => {
                let payload = expand_json_payload_from_expansion(
                    &file_expansion.source,
                    &file_expansion.expansion,
                );
                println!("{}", serde_json::to_string_pretty(&payload)?);
                Ok(())
            }
            Err(error) => {
                let source = fs::read_to_string(path).unwrap_or_default();
                let payload = expand_json_error_message_payload(&module_id, &source, &error);
                println!("{}", serde_json::to_string_pretty(&payload)?);
                bail!("expand failed")
            }
        }
    } else {
        match expand_file_with_imports(path) {
            Ok(file_expansion) => {
                print!("{}", format_module(&file_expansion.expansion.module));
                Ok(())
            }
            Err(error) => bail!("{error}"),
        }
    }
}

#[cfg(test)]
fn expand_json_payload(
    module_id: &str,
    source: &str,
) -> std::result::Result<serde_json::Value, CompileError> {
    let expansion = ricochet_compiler::expand_source(module_id, source)?;
    Ok(expand_json_payload_from_expansion(source, &expansion))
}

#[derive(Clone)]
struct ExpandSourceEntry {
    id: String,
    module_id: String,
    kind: &'static str,
    source_hash: String,
    package: Option<ricochet_compiler::MacroPackageMetadata>,
}

fn expand_json_payload_from_expansion(
    source: &str,
    expansion: &ricochet_compiler::MacroExpansion,
) -> serde_json::Value {
    let expanded_source = format_module(&expansion.module);
    let source_line_starts = line_starts(source);
    let sources = expand_sources(expansion);
    let source_lookup = expand_source_lookup(&sources);
    let cache = expand_cache_json(expansion.module_id.as_str(), &sources);
    let cache_hash = cache["key"].clone();
    json!({
        "schema": EXPAND_JSON_SCHEMA,
        "schema_version": EXPAND_JSON_SCHEMA_VERSION,
        "module_id": &expansion.module_id,
        "source_hash": sha256_text(source),
        "compiler_version": ricochet_compiler::crate_version(),
        "formatter_version": ricochet_syntax::crate_version(),
        "imports": imports_json(&expansion.imports, &source_lookup),
        "sources": sources_json(&sources),
        "source_map": source_map_json(expansion),
        "macro_tables": macro_tables_json(&expansion.macro_tables),
        "expanded_ast": module_ast_json(source, &source_line_starts, &expansion.module),
        "expanded_source": &expanded_source,
        "trace": trace_json(
            source,
            &source_line_starts,
            &expansion.macro_tables,
            &expansion.trace,
        ),
        "diagnostics": [],
        "cache": cache,
        "cache_hash": cache_hash,
        "output_hash": sha256_text(&expanded_source),
    })
}

fn expand_json_error_message_payload(
    module_id: &str,
    source: &str,
    error: &anyhow::Error,
) -> serde_json::Value {
    let source_hash = sha256_text(source);
    let sources = vec![ExpandSourceEntry {
        id: module_id.to_string(),
        module_id: module_id.to_string(),
        kind: "local",
        source_hash: source_hash.clone(),
        package: None,
    }];
    let cache = expand_cache_json(module_id, &sources);
    let cache_hash = cache["key"].clone();
    json!({
        "schema": EXPAND_JSON_SCHEMA,
        "schema_version": EXPAND_JSON_SCHEMA_VERSION,
        "module_id": module_id,
        "source_hash": source_hash,
        "compiler_version": ricochet_compiler::crate_version(),
        "formatter_version": ricochet_syntax::crate_version(),
        "imports": [],
        "sources": sources_json(&sources),
        "source_map": {
            "root_source_id": module_id,
            "macro_tables": [],
            "trace": [],
        },
        "macro_tables": [],
        "expanded_ast": serde_json::Value::Null,
        "expanded_source": serde_json::Value::Null,
        "trace": [],
        "diagnostics": [{
            "severity": "error",
            "message": error.to_string(),
        }],
        "cache": cache,
        "cache_hash": cache_hash,
        "output_hash": serde_json::Value::Null,
    })
}

fn expand_sources(expansion: &ricochet_compiler::MacroExpansion) -> Vec<ExpandSourceEntry> {
    let mut root = None;
    let mut imported = BTreeMap::new();
    for table in &expansion.macro_tables {
        let entry = ExpandSourceEntry {
            id: table.module_id.clone(),
            module_id: table.module_id.clone(),
            kind: source_kind_name(&table.source_kind),
            source_hash: table.source_hash.clone(),
            package: table.package.clone(),
        };
        if table.module_id == expansion.module_id && table.import_specifier.is_none() {
            root = Some(entry);
        } else {
            imported.entry(entry.id.clone()).or_insert(entry);
        }
    }

    let mut sources = Vec::new();
    if let Some(root) = root {
        sources.push(root);
    }
    sources.extend(imported.into_values());
    sources
}

fn expand_source_lookup(sources: &[ExpandSourceEntry]) -> BTreeMap<String, ExpandSourceEntry> {
    sources
        .iter()
        .cloned()
        .map(|source| (source.id.clone(), source))
        .collect()
}

fn source_kind_name(source_kind: &ricochet_compiler::MacroSourceKind) -> &'static str {
    match source_kind {
        ricochet_compiler::MacroSourceKind::Local => "local",
        ricochet_compiler::MacroSourceKind::Package => "package",
    }
}

fn sources_json(sources: &[ExpandSourceEntry]) -> Vec<serde_json::Value> {
    sources
        .iter()
        .map(|source| {
            json!({
                "id": &source.id,
                "module_id": &source.module_id,
                "kind": source.kind,
                "source_hash": &source.source_hash,
                "package": source.package.as_ref().map(package_json),
            })
        })
        .collect()
}

fn package_json(package: &ricochet_compiler::MacroPackageMetadata) -> serde_json::Value {
    json!({
        "name": &package.name,
        "package": &package.package,
        "module_path": &package.module_path,
        "version": &package.version,
        "integrity": &package.integrity,
        "source_kind": &package.source_kind,
        "commit": &package.commit,
    })
}

fn expand_cache_json(root_module_id: &str, sources: &[ExpandSourceEntry]) -> serde_json::Value {
    let imported_sources = sources
        .iter()
        .filter(|source| source.id != root_module_id)
        .map(|source| {
            json!({
                "id": &source.id,
                "source_hash": &source.source_hash,
                "kind": source.kind,
                "package": source.package.as_ref().map(package_json),
            })
        })
        .collect::<Vec<_>>();
    let cache_inputs = json!({
        "schema": EXPAND_JSON_SCHEMA,
        "schema_version": EXPAND_JSON_SCHEMA_VERSION,
        "compiler_version": ricochet_compiler::crate_version(),
        "formatter_version": ricochet_syntax::crate_version(),
        "root_module_id": root_module_id,
        "root_source_hash": sources
            .iter()
            .find(|source| source.id == root_module_id)
            .map(|source| source.source_hash.clone())
            .unwrap_or_default(),
        "imported_sources": imported_sources,
    });
    let cache_key =
        sha256_text(&serde_json::to_string(&cache_inputs).expect("cache inputs should serialize"));
    json!({
        "algorithm": "sha256",
        "key": cache_key,
        "inputs": cache_inputs,
    })
}

fn imports_json(
    imports: &[ricochet_compiler::MacroImportSummary],
    source_lookup: &BTreeMap<String, ExpandSourceEntry>,
) -> Vec<serde_json::Value> {
    imports
        .iter()
        .map(|import| {
            let source = source_lookup
                .get(import.module_id.as_str())
                .expect("import source should exist in source inventory");
            json!({
                "specifier": &import.specifier,
                "module_id": &import.module_id,
                "kind": source.kind,
                "source_hash": &source.source_hash,
                "package": source.package.as_ref().map(package_json),
            })
        })
        .collect()
}

fn macro_tables_json(tables: &[ricochet_compiler::MacroTableSummary]) -> Vec<serde_json::Value> {
    tables
        .iter()
        .map(|table| {
            let source_id = table.module_id.as_str();
            let macros = table
                .macros
                .iter()
                .map(|macro_summary| {
                    json!({
                        "name": &macro_summary.name,
                        "args": {
                            "inputs": &macro_summary.inputs,
                            "outputs": &macro_summary.outputs,
                        },
                        "docs": &macro_summary.docs,
                        "span": span_json_from_source_map_with_source_id(
                            table.source_len,
                            &table.line_starts,
                            macro_summary.span,
                            source_id,
                        ),
                        "body_span": macro_summary
                            .body_span
                            .map(|span| span_json_from_source_map_with_source_id(
                                table.source_len,
                                &table.line_starts,
                                span,
                                source_id,
                            )),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "module_id": &table.module_id,
                "scope": if table.import_specifier.is_some() { "import" } else { "local" },
                "import_specifier": &table.import_specifier,
                "macros": macros,
            })
        })
        .collect()
}

fn source_map_json(expansion: &ricochet_compiler::MacroExpansion) -> serde_json::Value {
    json!({
        "root_source_id": &expansion.module_id,
        "macro_tables": expansion
            .macro_tables
            .iter()
            .map(|table| {
                json!({
                    "module_id": &table.module_id,
                    "import_specifier": &table.import_specifier,
                    "source_id": &table.module_id,
                })
            })
            .collect::<Vec<_>>(),
        "trace": expansion
            .trace
            .iter()
            .map(|entry| {
                json!({
                    "id": &entry.id,
                    "invocation_source_id": &entry.invocation_module_id,
                    "name_source_id": &entry.invocation_module_id,
                    "definition_source_id": &entry.module_id,
                })
            })
            .collect::<Vec<_>>(),
    })
}

fn trace_json(
    source: &str,
    source_line_starts: &[usize],
    macro_tables: &[ricochet_compiler::MacroTableSummary],
    trace: &[ricochet_compiler::MacroExpansionTraceEntry],
) -> Vec<serde_json::Value> {
    trace
        .iter()
        .map(|entry| {
            let (invocation_source_len, invocation_line_starts) = trace_source_map_for_module(
                source,
                source_line_starts,
                macro_tables,
                &entry.invocation_module_id,
            );
            let (definition_source_len, definition_line_starts) =
                trace_definition_source_map(source, source_line_starts, macro_tables, entry);
            json!({
                "id": &entry.id,
                "module_id": &entry.module_id,
                "import_specifier": &entry.import_specifier,
                "macro_name": &entry.macro_name,
                "depth": entry.depth,
                "argument_count": entry.argument_count,
                "output_node_count": entry.output_node_count,
                "invocation_span": span_json_from_source_map_with_source_id(
                    invocation_source_len,
                    invocation_line_starts,
                    entry.invocation_span,
                    &entry.invocation_module_id,
                ),
                "name_span": span_json_from_source_map_with_source_id(
                    invocation_source_len,
                    invocation_line_starts,
                    entry.name_span,
                    &entry.invocation_module_id,
                ),
                "definition_span": span_json_from_source_map_with_source_id(
                    definition_source_len,
                    definition_line_starts,
                    entry.definition_span,
                    &entry.module_id,
                ),
            })
        })
        .collect()
}

fn trace_source_map_for_module<'a>(
    source: &'a str,
    source_line_starts: &'a [usize],
    macro_tables: &'a [ricochet_compiler::MacroTableSummary],
    module_id: &str,
) -> (usize, &'a [usize]) {
    macro_tables
        .iter()
        .find(|table| table.module_id == module_id)
        .map(|table| (table.source_len, table.line_starts.as_slice()))
        .unwrap_or((source.len(), source_line_starts))
}

fn trace_definition_source_map<'a>(
    source: &'a str,
    source_line_starts: &'a [usize],
    macro_tables: &'a [ricochet_compiler::MacroTableSummary],
    entry: &ricochet_compiler::MacroExpansionTraceEntry,
) -> (usize, &'a [usize]) {
    macro_tables
        .iter()
        .find(|table| {
            table.module_id == entry.module_id && table.import_specifier == entry.import_specifier
        })
        .map(|table| (table.source_len, table.line_starts.as_slice()))
        .unwrap_or((source.len(), source_line_starts))
}

fn module_ast_json(
    source: &str,
    source_line_starts: &[usize],
    module: &Module,
) -> serde_json::Value {
    json!({
        "type": "module",
        "items": module
            .items
            .iter()
            .map(|item| item_ast_json(source, source_line_starts, item))
            .collect::<Vec<_>>(),
    })
}

fn item_ast_json(
    source: &str,
    source_line_starts: &[usize],
    item: &SyntaxItem,
) -> serde_json::Value {
    match item {
        SyntaxItem::Class(class) => json!({
            "type": "class",
            "name": &class.name,
            "superclass": &class.superclass,
            "docs": &class.docs,
            "span": span_json(source, source_line_starts, class.span),
            "body": class
                .body
                .iter()
                .map(|item| item_ast_json(source, source_line_starts, item))
                .collect::<Vec<_>>(),
        }),
        SyntaxItem::Method(method) => json!({
            "type": "method",
            "name": &method.name,
            "args": args_json(method.args.as_ref()),
            "docs": &method.docs,
            "span": span_json(source, source_line_starts, method.span),
            "body": spanned_exprs_ast_json(source, source_line_starts, &method.body),
        }),
        SyntaxItem::Function(function) => json!({
            "type": "function",
            "name": &function.name,
            "args": args_json(function.args.as_ref()),
            "docs": &function.docs,
            "span": span_json(source, source_line_starts, function.span),
            "body": spanned_exprs_ast_json(source, source_line_starts, &function.body),
        }),
        SyntaxItem::Macro(macro_decl) => json!({
            "type": "macro",
            "name": &macro_decl.name,
            "args": args_json(macro_decl.args.as_ref()),
            "docs": &macro_decl.docs,
            "span": span_json(source, source_line_starts, macro_decl.span),
            "body": spanned_exprs_ast_json(source, source_line_starts, &macro_decl.body),
        }),
        SyntaxItem::Expr { expr, span, docs } => json!({
            "type": "expr",
            "docs": docs,
            "span": span_json(source, source_line_starts, *span),
            "expr": expr_ast_json(source, source_line_starts, expr),
        }),
    }
}

fn spanned_exprs_ast_json(
    source: &str,
    source_line_starts: &[usize],
    exprs: &[SpannedExpr],
) -> Vec<serde_json::Value> {
    exprs
        .iter()
        .map(|expr| spanned_expr_ast_json(source, source_line_starts, expr))
        .collect()
}

fn spanned_expr_ast_json(
    source: &str,
    source_line_starts: &[usize],
    expr: &SpannedExpr,
) -> serde_json::Value {
    json!({
        "span": span_json(source, source_line_starts, expr.span),
        "expr": expr_ast_json(source, source_line_starts, &expr.expr),
    })
}

fn expr_ast_json(source: &str, source_line_starts: &[usize], expr: &Expr) -> serde_json::Value {
    match expr {
        Expr::Symbol(name) => json!({ "kind": "symbol", "name": name }),
        Expr::DotWord(name) => json!({ "kind": "dot_word", "name": name }),
        Expr::Reference(name) => json!({ "kind": "reference", "name": name }),
        Expr::String(value) => json!({ "kind": "string", "value": value }),
        Expr::Number(value) => json!({ "kind": "number", "value": value }),
        Expr::Float(value) => json!({ "kind": "float", "value": value }),
        Expr::Args(args) => json!({
            "kind": "args",
            "value": args_json(Some(args)),
        }),
        Expr::Block(body) => json!({
            "kind": "block",
            "body": spanned_exprs_ast_json(source, source_line_starts, body),
        }),
        Expr::Sequence(exprs) => json!({
            "kind": "sequence",
            "exprs": spanned_exprs_ast_json(source, source_line_starts, exprs),
        }),
        Expr::If {
            then_body,
            else_body,
        } => json!({
            "kind": "if",
            "then": spanned_exprs_ast_json(source, source_line_starts, then_body),
            "else": spanned_exprs_ast_json(source, source_line_starts, else_body),
        }),
        Expr::While { condition, body } => json!({
            "kind": "while",
            "condition": spanned_exprs_ast_json(source, source_line_starts, condition),
            "body": spanned_exprs_ast_json(source, source_line_starts, body),
        }),
    }
}

fn args_json(args: Option<&ArgsDecl>) -> serde_json::Value {
    args.map_or_else(
        || serde_json::Value::Null,
        |args| {
            json!({
                "inputs": &args.inputs,
                "outputs": &args.outputs,
            })
        },
    )
}

fn span_json(source: &str, source_line_starts: &[usize], span: Span) -> serde_json::Value {
    span_json_from_source_map(source.len(), source_line_starts, span)
}

fn span_json_from_source_map(
    source_len: usize,
    source_line_starts: &[usize],
    span: Span,
) -> serde_json::Value {
    let bounded_start = span.start.min(source_len);
    let bounded_end = span.end.min(source_len);
    let (start_line, start_column) = line_column(source_line_starts, bounded_start);
    let (end_line, end_column) = line_column(source_line_starts, bounded_end);
    json!({
        "start": span.start,
        "end": span.end,
        "start_line": start_line,
        "start_column": start_column,
        "end_line": end_line,
        "end_column": end_column,
    })
}

fn span_json_from_source_map_with_source_id(
    source_len: usize,
    source_line_starts: &[usize],
    span: Span,
    source_id: &str,
) -> serde_json::Value {
    let mut span_json = span_json_from_source_map(source_len, source_line_starts, span);
    span_json["source_id"] = json!(source_id);
    span_json
}

fn module_id_for_path(path: &Path) -> String {
    let relative = if path.is_absolute() {
        std::env::current_dir()
            .ok()
            .and_then(|current_dir| path.strip_prefix(current_dir).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| {
                path.file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("source.rco"))
            })
    } else {
        path.to_path_buf()
    };
    let module_id = relative.to_string_lossy().replace('\\', "/");
    let module_id = module_id.strip_prefix("./").unwrap_or(&module_id);
    if module_id.is_empty() {
        ".".to_string()
    } else {
        module_id.to_string()
    }
}

fn sha256_text(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    let digest = hasher.finalize();
    format!("sha256:{}", hex_digest(&digest))
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
    files.retain(|file| {
        file.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("Test.rco"))
    });
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn nested_deferred_credentials_for_dap(sentinel: &str) -> Value {
        Value::Array(ricochet_vm::ArrayValue::from(vec![
            Value::DeferredHttpCredentials(ricochet_secrets::DeferredHttpCredentials::bearer(
                ricochet_secrets::DeferredSecretSource::literal(sentinel.to_string())
                    .expect("synthetic literal"),
            )),
        ]))
    }

    fn dap_variables_response(pause: DebugPause, reference: u64) -> serde_json::Value {
        let request = json!({
            "seq": 77,
            "type": "request",
            "command": "variables",
            "arguments": { "variablesReference": reference },
        });
        let mut adapter = DapAdapter {
            reader: Cursor::new(Vec::<u8>::new()),
            writer: Vec::<u8>::new(),
            seq: 1,
            last_pause: Some(pause),
            pause_error: None,
        };
        adapter
            .send_variables_response(&request)
            .expect("variables response should be written");
        let mut reader = io::BufReader::new(Cursor::new(adapter.writer));
        read_dap_message(&mut reader)
            .expect("DAP response should parse")
            .expect("DAP response should be present")
    }

    fn debug_pause_with_task(frames: Vec<DebugTaskFrame>) -> DebugPause {
        DebugPause {
            reason: DebugPauseReason::Breakpoint,
            frame: "<main>".to_string(),
            source: "fixture.rco:18".to_string(),
            opcode: "PushString(\"task\")".to_string(),
            stack: Vec::new(),
            globals: Vec::new(),
            locals: Vec::new(),
            current_self: None,
            tasks: vec![DebugTask {
                id: 0,
                operation: "spawn".to_string(),
                status: "running".to_string(),
                pending: true,
                running: true,
                completed: false,
                failed: false,
                fault: None,
                frames,
            }],
        }
    }

    #[test]
    fn dap_task_variables_support_zero_frame_running_task() {
        let pause = debug_pause_with_task(Vec::new());
        let summary = dap_task_variable(&pause.tasks[0]);

        assert_eq!(summary["variablesReference"], DAP_TASK_REFERENCE_BASE);
        assert!(summary["value"]
            .as_str()
            .is_some_and(|value| value.contains("frames=0")));

        let details = dap_task_detail_variables(&pause, 0);
        let frame_count = details
            .iter()
            .find(|variable| variable["name"] == "frame_count")
            .expect("task details should include frame_count");
        assert_eq!(frame_count["value"], "0");
        assert!(!details.iter().any(|variable| variable["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("frame "))));
        assert!(dap_task_frame_variables(&pause, 0, 0).is_empty());
    }

    #[test]
    fn dap_task_variables_expand_published_worker_frame() {
        let pause = debug_pause_with_task(vec![DebugTaskFrame {
            frame: "<task>".to_string(),
            source: "fixture.rco:6".to_string(),
            opcode: "CallWord(\"sleep\")".to_string(),
            stack: vec![Value::Number(20)],
            locals: vec![("release_attempts".to_string(), Value::Number(0))],
            current_self: Some(Value::String("worker".to_string())),
        }]);

        let details = dap_task_detail_variables(&pause, 0);
        let frame = details
            .iter()
            .find(|variable| variable["name"] == "frame 0")
            .expect("task details should expose frame 0");
        assert_eq!(frame["variablesReference"], DAP_TASK_FRAME_REFERENCE_BASE);

        let variables = dap_task_frame_variables(&pause, 0, 0);
        assert!(variables
            .iter()
            .any(|variable| variable["name"] == "opcode"
                && variable["value"] == "CallWord(\"sleep\")"));
        assert!(variables
            .iter()
            .any(|variable| variable["name"] == "stack[0]" && variable["value"] == "Number(20)"));
        assert!(variables.iter().any(|variable| {
            variable["name"] == "local release_attempts" && variable["value"] == "Number(0)"
        }));
        assert!(variables.iter().any(
            |variable| variable["name"] == "self" && variable["value"] == "String(\"worker\")"
        ));
    }

    #[test]
    fn dap_variables_response_rejects_opaque_pause_and_task_scopes_without_metadata() {
        let sentinel = "synthetic-real-dap-secret-that-must-not-render";
        let nested = nested_deferred_credentials_for_dap(sentinel);

        let mut globals_pause = debug_pause_with_task(Vec::new());
        globals_pause
            .globals
            .push(("audited_global".to_string(), nested.clone()));

        let task_pause = debug_pause_with_task(vec![DebugTaskFrame {
            frame: "<task>".to_string(),
            source: "fixture.rco:6".to_string(),
            opcode: "PushValue".to_string(),
            stack: vec![nested],
            locals: Vec::new(),
            current_self: None,
        }]);

        for (response, forbidden_name) in [
            (dap_variables_response(globals_pause, 3), "audited_global"),
            (
                dap_variables_response(task_pause, DAP_TASK_FRAME_REFERENCE_BASE),
                "stack[0]",
            ),
        ] {
            assert_eq!(response["type"], "response");
            assert_eq!(response["command"], "variables");
            assert_eq!(response["success"], false);
            assert_eq!(
                response["message"],
                "debug adapter cannot serialize non-serializable value"
            );
            assert!(response.get("variables").is_none());
            assert!(response["body"].get("variables").is_none());
            let serialized = serde_json::to_string(&response).expect("response JSON");
            for forbidden in [
                sentinel,
                forbidden_name,
                "<http-credentials>",
                "deferred HTTP credentials",
                "literal",
            ] {
                assert!(!serialized.contains(forbidden));
            }
        }
    }

    #[test]
    fn diagnostics_include_macro_body_lints_for_top_level_macro_declarations() {
        let source = r#"
"unless" Macro
[
  name get
  http .request
]
end
"#;

        let diagnostics = source_lsp_diagnostics("test.rco", source);

        assert!(
            !diagnostics.iter().any(|diagnostic| diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("compile-time macros are not implemented yet"))),
            "top-level macro declarations should compile away without an unsupported macro diagnostic"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic["code"]
                .as_str()
                .is_some_and(|code| code == "prefer-dollar-reference")),
            "syntax lints inside macro bodies should be published with compile diagnostics"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic["code"]
                .as_str()
                .is_some_and(|code| code == "leading-dot-syntax")
                && diagnostic["data"]["replacement"] == "http_request"),
            "leading-dot lints inside macro bodies should include the usual replacement"
        );
    }

    #[test]
    fn diagnostics_keep_class_body_macro_rejection() {
        let source = r#"
User Model Subclass
  "displayName" Macro
  [
    "ok"
  ]
  end
end
"#;

        let diagnostics = source_lsp_diagnostics("test.rco", source);

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic["message"]
                .as_str()
                .is_some_and(|message| message
                    .contains("macro declarations are only supported at top level"))),
            "class-body macro declarations should still publish a clear unsupported diagnostic"
        );
    }

    #[test]
    fn doc_generation_omits_unsupported_macro_declarations() {
        let source = r#"
(( Internal macro docs. ))
"unless" Macro
[
  "ok"
]
end

(( User docs. ))
User Model Subclass
  (( Email docs. ))
  "email" Accessor
end
"#;
        let module = parse_module(source).expect("source should parse");
        let mut output = String::new();

        write_module_docs(&mut output, Path::new("test.rco"), &module).expect("docs should render");

        assert!(!output.contains("unless"));
        assert!(!output.contains("Internal macro docs"));
        assert!(output.contains("## Class `User`"));
        assert!(output.contains("- Accessor: `email`"));
    }

    #[test]
    fn expand_json_payload_includes_local_macro_table_trace_and_ast() {
        let source = r#"
(( Say ok. ))
"say_ok" Macro
[
  [ "ok" println ] quote_ast
]
end

"say_ok" macro_call
"#;

        let payload =
            expand_json_payload("src\\macro_test.rco", source).expect("expand JSON succeeds");

        assert_eq!(payload["schema_version"], EXPAND_JSON_SCHEMA_VERSION);
        assert_eq!(payload["module_id"], "src/macro_test.rco");
        assert!(payload["source_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:")));
        assert_eq!(
            payload["compiler_version"],
            ricochet_compiler::crate_version()
        );
        assert_eq!(
            payload["formatter_version"],
            ricochet_syntax::crate_version()
        );
        assert_eq!(
            payload["imports"].as_array().expect("imports array").len(),
            0
        );
        assert_eq!(
            payload["diagnostics"]
                .as_array()
                .expect("diagnostics array")
                .len(),
            0
        );
        assert!(payload["expanded_source"]
            .as_str()
            .is_some_and(|source| source.contains("\"ok\" println")));
        assert!(payload["output_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:")));

        let macro_table = &payload["macro_tables"][0];
        assert_eq!(macro_table["module_id"], "src/macro_test.rco");
        assert_eq!(macro_table["scope"], "local");
        assert_eq!(macro_table["macros"][0]["name"], "say_ok");
        assert_eq!(macro_table["macros"][0]["docs"][0], "Say ok.");

        assert_eq!(payload["expanded_ast"]["type"], "module");
        let item = &payload["expanded_ast"]["items"][0];
        assert_eq!(item["type"], "expr");
        assert_eq!(item["expr"]["kind"], "sequence");
        assert_eq!(item["expr"]["exprs"][0]["expr"]["kind"], "string");
        assert_eq!(item["expr"]["exprs"][0]["expr"]["value"], "ok");
        assert_eq!(item["expr"]["exprs"][1]["expr"]["kind"], "symbol");
        assert_eq!(item["expr"]["exprs"][1]["expr"]["name"], "println");

        let trace = &payload["trace"][0];
        assert_eq!(trace["macro_name"], "say_ok");
        assert_eq!(trace["module_id"], "src/macro_test.rco");
        assert_eq!(trace["depth"], 0);
        assert_eq!(trace["argument_count"], 0);
        assert_eq!(trace["output_node_count"], 2);
        let trace_id = trace["id"].as_str().expect("trace id");
        assert!(!trace_id.contains('\\'));
        assert!(!trace_id.contains('/'));
    }

    #[test]
    fn expand_json_payload_emits_stable_schema_sources_source_map_and_cache_fields() {
        let source = r#"
(( Say ok. ))
"say_ok" Macro
[
  [ "ok" println ] quote_ast
]
end

"say_ok" macro_call
"#;

        let payload =
            expand_json_payload("src\\macro_test.rco", source).expect("expand JSON succeeds");

        assert_eq!(payload["schema"], "ricochet.expand.v1");
        assert_eq!(payload["cache_hash"], payload["cache"]["key"]);
        assert_eq!(payload["cache"]["algorithm"], "sha256");
        assert_eq!(
            payload["source_map"]["root_source_id"],
            "src/macro_test.rco"
        );

        let sources = payload["sources"].as_array().expect("sources array");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["id"], "src/macro_test.rco");
        assert_eq!(sources[0]["module_id"], "src/macro_test.rco");
        assert_eq!(sources[0]["kind"], "local");
        assert_eq!(sources[0]["source_hash"], payload["source_hash"]);

        assert_eq!(
            payload["macro_tables"][0]["macros"][0]["span"]["source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["trace"][0]["invocation_span"]["source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["trace"][0]["name_span"]["source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["trace"][0]["definition_span"]["source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["source_map"]["macro_tables"][0]["source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["source_map"]["trace"][0]["invocation_source_id"],
            "src/macro_test.rco"
        );
        assert_eq!(
            payload["source_map"]["trace"][0]["definition_source_id"],
            "src/macro_test.rco"
        );
    }

    #[test]
    fn expand_module_id_for_external_absolute_paths_omits_machine_local_prefix() {
        let path = if cfg!(windows) {
            Path::new(r"C:\external\macro_test.rco")
        } else {
            Path::new("/tmp/macro_test.rco")
        };

        assert_eq!(module_id_for_path(path), "macro_test.rco");
    }

    #[test]
    fn packaged_sqlite_migrations_serialize_concurrent_first_launches() {
        let project = tempfile::tempdir().expect("temporary packaged MVC project");
        let migrations_dir = project.path().join("db/migrations");
        fs::create_dir_all(&migrations_dir).expect("migrations directory");
        fs::write(
            migrations_dir.join("0001_create_launches.sql"),
            "create table launches (id integer primary key, label text not null);\n",
        )
        .expect("migration fixture");
        let migrations = discover_migrations(project.path()).expect("migration discovery");
        let database = MigrationDatabase {
            adapter: "sqlite".to_string(),
            url: "db/development.sqlite3".to_string(),
        };
        let database_path = project.path().join("data/db/development.sqlite3");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let barrier = barrier.clone();
            let database_path = database_path.clone();
            let database = database.clone();
            let migrations = migrations.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                migrate_apply_packaged_sqlite_at_path(&database_path, &database, migrations)
            }));
        }
        for handle in handles {
            handle
                .join()
                .expect("migration thread should not panic")
                .expect("concurrent packaged migration should succeed");
        }

        let connection =
            rusqlite::Connection::open(&database_path).expect("persistent SQLite database");
        let applied: i64 = connection
            .query_row("select count(*) from schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count");
        let tables: i64 = connection
            .query_row(
                "select count(*) from sqlite_schema where type = 'table' and name = 'launches'",
                [],
                |row| row.get(0),
            )
            .expect("launches table count");
        assert_eq!(applied, 1);
        assert_eq!(tables, 1);
    }

    #[cfg(unix)]
    #[test]
    fn packaged_sqlite_rejects_data_root_symlink_escape_before_migrating() {
        let roots = tempfile::tempdir().expect("temporary packaged MVC roots");
        let project_root = roots.path().join("project");
        let data_root = roots.path().join("data");
        let outside = roots.path().join("outside");
        fs::create_dir_all(project_root.join("db/migrations")).expect("migrations directory");
        fs::create_dir_all(&data_root).expect("data root");
        fs::create_dir_all(&outside).expect("outside directory");
        fs::write(
            project_root.join("ricochet.toml"),
            r#"[database.default]
adapter = "sqlite"
url = "db/development.sqlite3"
"#,
        )
        .expect("SQLite manifest");
        fs::write(
            project_root.join("db/migrations/0001_schema.sql"),
            "create table entries (id integer primary key);\n",
        )
        .expect("migration");
        std::os::unix::fs::symlink(&outside, data_root.join("db"))
            .expect("database directory symlink");

        let error = prepare_packaged_mvc_sqlite(&project_root, &data_root)
            .expect_err("SQLite data-root symlink escape must be rejected");

        assert!(
            error.to_string().contains("resolves outside"),
            "unexpected containment error: {error:#}"
        );
        assert!(
            !outside.join("development.sqlite3").exists(),
            "containment must be checked before the database is created"
        );
    }
}
