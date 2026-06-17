use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ricochet_bytecode::Chunk;
use ricochet_compiler::{compile_file_with_imports, compile_source, CompileError};
use ricochet_syntax::{
    format_source, parse_module, utf16_range_for_span, ArgsDecl, Expr, Item as SyntaxItem,
    LexError, Module, ParseError, SourceDiagnostic, Span, SpannedExpr, TokenKind,
};
use ricochet_vm::{DebugAction, DebugEvent, DebugPauseReason, MapValue, RicochetResult, Value, Vm};
use ricochet_web::{MysqlDatabase, PostgresDatabase};
use semver::{Version, VersionReq};
use serde_json::json;
use sha2::{Digest, Sha256};
use toml_edit::{value, DocumentMut, Item, Table};

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
const DEFAULT_MVC_GUI_TITLE: &str = "Ricochet MVC App";
const DEFAULT_MVC_GUI_WIDTH: u32 = 1100;
const DEFAULT_MVC_GUI_HEIGHT: u32 = 760;

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
        #[command(flatten)]
        capabilities: CapabilityOptions,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    RunBytecode {
        #[arg(long)]
        debug: bool,
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
            long = "version",
            value_name = "REQ",
            help = "Require the dependency package version to satisfy REQ, for example ^0.1.0"
        )]
        version: Option<String>,
        #[arg(long)]
        no_fetch: bool,
    },
    Install,
    Verify {
        path: Option<String>,
    },
    Doctor {
        path: Option<String>,
        #[arg(
            long,
            help = "Print effective manifest capability declarations for MVC apps"
        )]
        capabilities: bool,
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
            help = "Enable MVC process environment reads for trusted local apps"
        )]
        allow_env: bool,
        #[arg(
            long = "no-env",
            help = "Keep MVC process environment reads disabled; conflicts with --allow-env"
        )]
        no_env: bool,
        #[arg(
            long = "env-allow",
            value_name = "NAME",
            help = "Allow MVC controllers to read only NAME from the process environment; repeat for multiple variables"
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebviewDocument {
    title: String,
    html: String,
    width: u32,
    height: u32,
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
        help = "Allow reading only NAME from the process environment; repeat for multiple variables"
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
                        args: std::env::args().skip(1).collect(),
                        capabilities: CapabilityOptions::default(),
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
            capabilities,
            path,
            args,
        } => run_file(&path, debug, step, &breakpoints, args, capabilities)?,
        Command::RunBytecode {
            debug,
            capabilities,
            path,
            args,
        } => run_bytecode(&path, debug, args, capabilities)?,
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
            version,
            no_fetch,
        } => add_dependency(&source, name.as_deref(), version.as_deref(), no_fetch)?,
        Command::Install => install_dependencies()?,
        Command::Verify { path } => verify_dependencies(path.as_deref())?,
        Command::Doctor { path, capabilities } => {
            doctor(path.as_deref().unwrap_or("."), capabilities)?
        }
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
    let diagnostics = match compile_source(&file, &source) {
        Ok(_) => Vec::new(),
        Err(error) => vec![compile_error_lsp_diagnostic(&file, &source, &error)],
    };
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
    ctx get
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
        "<link rel=\"stylesheet\" href=\"/assets/app.css\">\n<h1>{ title get }</h1>\n",
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
    users get swap push! drop
    users get count
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
        println!(
            "created {} with SQLite database at {}",
            path.display(),
            path.join("db").join("development.sqlite3").display()
        );
    } else {
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
    session get "last_page" "users" put! drop
    User default-page
    dup ok? if
      value users var
      users get count userCount var
      users get first firstUser var
      firstUser get "email" at firstEmail var
      "Users" title var
      ctx get
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
    users get swap push! drop
    users get count userCount var
    "Users" title var
    ctx get
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
    ctx get
    "auth/login" swap view
  ] "login" Method

  ( email session ) [
    session var
    email var
    email get nil? if
      "Email is required" text 400 status
    else
      email get blank? if
        "Email is required" text 400 status
      else
        session get "user_email" email get put! drop
        "/me" redirect
      end
    end
  ] "create" Method

  ( session ctx ) [
    ctx var
    session var
    session get "user_email" at nil? if
      "Not signed in" text
    else
      session get "user_email" at userEmail var
      "Signed in" title var
      ctx get
      "auth/show" swap view
    end
  ] "show" Method

  ( session ) [
    session var
    session get "user_email" remove! drop
    "/login" redirect
  ] "destroy" Method
end
"#
}

fn users_index_view_source(options: NewProjectOptions) -> &'static str {
    if options.with_sqlite {
        "<h1>{ title get }</h1>\n<p>{ userCount get } users ready.</p>\n<p>First user: { firstEmail get }</p>\n"
    } else {
        "<h1>{ title get }</h1>\n<p>{ userCount get } users ready.</p>\n"
    }
}

fn auth_login_view_source() -> &'static str {
    "<h1>{ title get }</h1>\n<form method=\"post\" action=\"/login\">\n  <label>Email <input name=\"email\" type=\"email\" value=\"ada@example.com\"></label>\n  <button type=\"submit\">Sign in</button>\n</form>\n"
}

fn auth_show_view_source() -> &'static str {
    "<h1>{ title get }</h1>\n<p>Signed in as { userEmail get }</p>\n<form method=\"post\" action=\"/logout\">\n  <button type=\"submit\">Sign out</button>\n</form>\n"
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
    path: String,
    source: String,
    git: Option<String>,
    rev: Option<String>,
    commit: Option<String>,
    version_req: Option<String>,
    package_version: Option<String>,
    integrity: Option<String>,
    display_source: String,
}

#[derive(Debug)]
struct LockedPackage {
    source: String,
    path: String,
    git: Option<String>,
    rev: Option<String>,
    commit: Option<String>,
    version_req: Option<String>,
    package_version: Option<String>,
    integrity: Option<String>,
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
}

fn add_dependency(
    source: &str,
    name: Option<&str>,
    version_req: Option<&str>,
    no_fetch: bool,
) -> Result<()> {
    let manifest_path = find_project_manifest_for_current_dir("add")?;
    let project_root = manifest_path
        .parent()
        .expect("project manifest should have a parent");
    let dependency_source = parse_dependency_source(source)?;
    let mut spec = dependency_spec(project_root, source, dependency_source, name, version_req)?;

    if spec.git.is_some() && !no_fetch {
        spec.commit = Some(fetch_git_dependency(project_root, &spec)?);
    }
    if !(spec.git.is_some() && no_fetch) {
        spec.package_version = package_version_for_spec(project_root, &spec)?;
        spec.integrity = Some(package_integrity(project_root, &spec)?);
    }

    write_dependency_manifest(&manifest_path, &spec)?;
    if !(spec.git.is_some() && no_fetch) {
        write_lockfile(&project_root.join("ricochet.lock"), &spec)?;
    }

    if spec.git.is_some() && no_fetch {
        println!(
            "added {} from {} (fetch skipped)",
            spec.name, spec.display_source
        );
    } else {
        println!("added {} from {}", spec.name, spec.display_source);
    }
    Ok(())
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
        let path = table
            .get("path")
            .and_then(Item::as_str)
            .with_context(|| {
                format!(
                    "dependency {name} in {} must include a string path",
                    manifest_path.display()
                )
            })?
            .to_string();
        let git = table.get("git").and_then(Item::as_str).map(str::to_string);
        let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
        let version_req = table
            .get("version")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(version_req) = version_req.as_deref() {
            validate_version_req(version_req)?;
        }
        let commit = git
            .as_ref()
            .map(|_| locked_git_commit(&lock_path, name))
            .transpose()?
            .flatten();
        let display_source = git.clone().unwrap_or_else(|| path.clone());
        let mut spec = DependencySpec {
            name: name.to_string(),
            source: git
                .as_ref()
                .map(|git| format!("git+{git}"))
                .unwrap_or_else(|| format!("path+{path}")),
            path: path.clone(),
            git,
            rev,
            commit,
            version_req,
            package_version: None,
            integrity: None,
            display_source,
        };

        if spec.git.is_some() {
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
        let table = item.as_table().with_context(|| {
            format!(
                "dependency {name} in {} must be a table",
                manifest_path.display()
            )
        })?;
        let path = table
            .get("path")
            .and_then(Item::as_str)
            .with_context(|| {
                format!(
                    "dependency {name} in {} must include a string path",
                    manifest_path.display()
                )
            })?
            .to_string();
        let git = table.get("git").and_then(Item::as_str).map(str::to_string);
        let rev = table.get("rev").and_then(Item::as_str).map(str::to_string);
        let version_req = table
            .get("version")
            .and_then(Item::as_str)
            .map(str::to_string);
        if let Some(version_req) = version_req.as_deref() {
            validate_version_req(version_req)?;
        }
        let spec = DependencySpec {
            name: name.to_string(),
            source: git
                .as_ref()
                .map(|git| format!("git+{git}"))
                .unwrap_or_else(|| format!("path+{path}")),
            path,
            git,
            rev,
            commit: None,
            version_req,
            package_version: None,
            integrity: None,
            display_source: String::new(),
        };

        verify_dependency(project_root, &lock_path, lock_doc.as_ref(), &spec)?;
        if verbose {
            println!("verified {}", spec.name);
        }
        verified += 1;
    }

    verify_no_stale_lock_packages(&lock_path, lock_doc.as_ref(), &declared)?;
    Ok(verified)
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
    if lock.version_req != spec.version_req {
        bail!(
            "lock entry for {} has version requirement {:?}, expected {:?}",
            spec.name,
            lock.version_req,
            spec.version_req
        );
    }

    if let Some(git) = spec.git.as_deref() {
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
        if lock.git.is_some() || lock.commit.is_some() {
            bail!(
                "lock entry for {} is git-shaped, but manifest uses a local path",
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
    let Some(packages) = lock_doc
        .and_then(|doc| doc.get("package"))
        .and_then(Item::as_table)
    else {
        return Ok(());
    };

    for (name, _) in packages.iter() {
        if !declared.contains(name) {
            bail!(
                "{} contains package {name:?}, but ricochet.toml does not declare it",
                lock_path.display()
            );
        }
    }

    Ok(())
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

    Ok(Some(LockedPackage {
        source,
        path,
        git,
        rev,
        commit,
        version_req,
        package_version,
        integrity,
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

fn parse_dependency_source(source: &str) -> Result<DependencySource> {
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
                path: path.clone(),
                source: format!("path+{path}"),
                git: None,
                rev: None,
                commit: None,
                version_req,
                package_version: metadata.version,
                integrity: Some(package_tree_integrity(&absolute_path)?),
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
                path,
                source: format!("git+{git}"),
                git: Some(git),
                rev,
                commit: None,
                version_req,
                package_version: None,
                integrity: None,
                display_source: original_source.to_string(),
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

fn package_integrity(project_root: &Path, spec: &DependencySpec) -> Result<String> {
    let package_dir = if spec.git.is_some() {
        project_dependency_path(project_root, &spec.path, "git package cache")?
    } else {
        resolve_local_dependency_dir(project_root, &spec.path)?
    };
    package_tree_integrity(&package_dir)
}

fn package_version_for_spec(project_root: &Path, spec: &DependencySpec) -> Result<Option<String>> {
    let package_dir = if spec.git.is_some() {
        project_dependency_path(project_root, &spec.path, "git package cache")?
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
    dependency["path"] = value(spec.path.clone());
    if let Some(git) = &spec.git {
        dependency["git"] = value(git.clone());
    }
    if let Some(rev) = &spec.rev {
        dependency["rev"] = value(rev.clone());
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
    packages.insert(&spec.name, Item::Table(package));

    fs::write(lock_path, doc.to_string())
        .with_context(|| format!("failed to write {}", lock_path.display()))
}

fn locked_git_commit(lock_path: &Path, name: &str) -> Result<Option<String>> {
    if !lock_path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(lock_path)
        .with_context(|| format!("failed to read {}", lock_path.display()))?;
    let doc = source
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", lock_path.display()))?;
    let commit = doc
        .get("package")
        .and_then(Item::as_table)
        .and_then(|packages| packages.get(name))
        .and_then(Item::as_table)
        .and_then(|package| package.get("commit"))
        .and_then(Item::as_str)
        .map(str::to_string);
    if let Some(commit) = commit.as_deref() {
        validate_git_commit(commit)?;
    }
    Ok(commit)
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
            args,
            capabilities,
            print_final_stack: true,
        },
    )
}

fn run_bytecode(
    path: &str,
    debug: bool,
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
            args,
            capabilities,
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
            args,
            capabilities,
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
            args,
            capabilities: CapabilityOptions::default(),
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
    webview_document_from_vm(&vm)
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
        "GUI apps must leave a `webview .window` result on the stack or store it in a variable named `document`"
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
    }))
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

struct RunChunkCliOptions<'a> {
    debug: bool,
    step: bool,
    breakpoints: &'a [usize],
    breakpoint_file: Option<&'a str>,
    args: Vec<String>,
    capabilities: CapabilityOptions,
    print_final_stack: bool,
}

fn run_chunk_cli(chunk: &Chunk, options: RunChunkCliOptions<'_>) -> Result<()> {
    let mut vm = cli_vm(options.args, &options.capabilities)?;
    let debugger_enabled = options.debug || options.step || !options.breakpoints.is_empty();
    if debugger_enabled {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
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
    if options.step || !options.breakpoints.is_empty() {
        vm.set_debug_controller(|_| read_terminal_debug_action());
    }

    let result = vm.run_chunk(chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
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

fn read_terminal_debug_action() -> DebugAction {
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
                "c" | "continue" => return DebugAction::Continue,
                "a" | "abort" | "q" | "quit" => return DebugAction::Abort,
                _ => println!("commands: step, continue, abort"),
            },
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
