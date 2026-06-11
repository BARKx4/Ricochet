use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ricochet_compiler::{compile_source, CompileError};
use ricochet_syntax::{LexError, ParseError, TokenKind};
use ricochet_vm::{DebugAction, DebugEvent, DebugPauseReason, Vm};

const DEFAULT_BUILD_SOURCE: &str = "main.rco";
const BUILD_OUTPUT: &str = "build/app.rcob";

#[derive(Debug, Parser)]
#[command(name = "rco")]
#[command(about = "Ricochet language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    New { path: String },
    Check { path: Option<String> },
    Repl {
        #[arg(long)]
        debug: bool,
    },
    Run {
        #[arg(long)]
        debug: bool,
        #[arg(long)]
        step: bool,
        #[arg(long = "breakpoint", value_name = "LINE")]
        breakpoints: Vec<usize>,
        path: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Build { path: Option<String> },
    Serve { #[arg(long)] debug: bool, #[arg(long)] watch: bool },
    Test {
        #[arg(long)]
        debug: bool,
        path: Option<String>,
    },
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::New { path } => new_project(Path::new(&path))?,
        Command::Check { path } => check(path.as_deref().unwrap_or("."))?,
        Command::Repl { debug } => {
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            let stdout = io::stdout();
            run_repl(stdin.lock(), stdout.lock(), debug, interactive)?;
        }
        Command::Run {
            debug,
            step,
            breakpoints,
            path,
            args,
        } => run_file(&path, debug, step, &breakpoints, args)?,
        Command::Build { path } => build(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE))?,
        Command::Serve { debug, watch } => ricochet_web::serve_current_dir(debug, watch).await?,
        Command::Test { debug, path } => {
            run_tests(path.as_deref().unwrap_or("tests"), debug)?;
        }
    }
    Ok(())
}

fn run_repl<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    debug: bool,
    interactive: bool,
) -> Result<()> {
    let mut vm = Vm::default();
    vm.enable_cli_capabilities();
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

fn write_repl_result<W: Write>(
    vm: &Vm,
    output: &mut W,
    output_cursor: &mut usize,
) -> Result<()> {
    write!(output, "{}", &vm.stdout()[*output_cursor..])?;
    *output_cursor = vm.stdout().len();
    writeln!(output, "{:?}", vm.stack())?;
    output.flush()?;
    Ok(())
}

fn is_incomplete_compile_error(error: &CompileError) -> bool {
    match error {
        CompileError::Parse(ParseError::Expected {
            found: TokenKind::Eof,
            ..
        })
        | CompileError::Parse(ParseError::Unexpected {
            found: TokenKind::Eof,
            ..
        })
        | CompileError::Parse(ParseError::Lex(
            LexError::UnterminatedString(_) | LexError::UnterminatedComment(_),
        )) => true,
        _ => false,
    }
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

    println!("checked {} Ricochet files in {}", files.len(), path.display());
    Ok(())
}

fn check_source_file(path: &Path) -> Result<()> {
    let (file, source) = read_source_path(path)?;
    compile_source(&file, &source)
        .with_context(|| format!("failed to compile {}", path.display()))?;
    Ok(())
}

fn new_project(path: &Path) -> Result<()> {
    ensure_project_path_is_ready(path)?;

    fs::create_dir_all(path.join("app").join("Controllers"))
        .with_context(|| format!("failed to create app/Controllers in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Models"))
        .with_context(|| format!("failed to create app/Models in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("home"))
        .with_context(|| format!("failed to create app/Views/home in {}", path.display()))?;
    fs::create_dir_all(path.join("app").join("Views").join("users"))
        .with_context(|| format!("failed to create app/Views/users in {}", path.display()))?;
    fs::create_dir_all(path.join("config"))
        .with_context(|| format!("failed to create config in {}", path.display()))?;
    fs::create_dir_all(path.join("tests"))
        .with_context(|| format!("failed to create tests in {}", path.display()))?;

    write_project_file(
        path.join("ricochet.toml"),
        &manifest_source(&project_name(path)),
    )?;
    write_project_file(
        path.join("config").join("routes.rco"),
        r#"GET "/" HomeController "index" route
GET "/users" UserController "index" route
"#,
    )?;
    write_project_file(
        path.join("app").join("Controllers").join("HomeController.rco"),
        r#"HomeController Controller subclass
  "index" [
    "Hello Ricochet" title var
    ctx get
    "home/index" swap view
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Models").join("User.rco"),
        r#"User Model subclass
  email field
  name field

  "displayName" [
    self .name get nil? if
      self .email get
    else
      self .name get
    end
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Controllers").join("UserController.rco"),
        r#"UserController Controller subclass
  "index" [
    users array
    User new
    "ada@example.com" swap .email set
    "Ada Lovelace" swap .name set
    users get .push! drop
    users get .count userCount var
    "Users" title var
    ctx get
    "users/index" swap view
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Views").join("home").join("index.html"),
        "<h1>{ title get }</h1>\n",
    )?;
    write_project_file(
        path.join("app").join("Views").join("users").join("index.html"),
        "<h1>{ title get }</h1>\n<p>{ userCount get } users ready.</p>\n",
    )?;
    write_project_file(
        path.join("tests").join("ApplicationSmokeTest.rco"),
        r#"ApplicationSmokeTest TestCase subclass
  "testUserDisplayNameFallsBackToEmail" [
    User new
    "ada@example.com" swap .email set
    .displayName
    "ada@example.com" assert-equals
  ] !method

  "testCollectionsCanHoldModels" [
    users array
    User new
    "grace@example.com" swap .email set
    users get .push! drop
    users get .count
    1 assert-equals
  ] !method
end
"#,
    )?;

    println!("created {}", path.display());
    Ok(())
}

fn ensure_project_path_is_ready(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!("project path already exists and is not a directory: {}", path.display());
        }

        if fs::read_dir(path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .next()
            .transpose()
            .with_context(|| format!("failed to read entry in {}", path.display()))?
            .is_some()
        {
            bail!("project path already exists and is not empty: {}", path.display());
        }
    }

    Ok(())
}

fn write_project_file(path: PathBuf, contents: &str) -> Result<()> {
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
}

fn manifest_source(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${{DATABASE_URL}}"
"#
    )
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

fn run_file(
    path: &str,
    debug: bool,
    step: bool,
    breakpoints: &[usize],
    args: Vec<String>,
) -> Result<()> {
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let mut vm = Vm::default();
    vm.enable_cli_capabilities();
    vm.set_program_args(args);
    vm.set_input_reader(|| {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|error| error.to_string())
            .map(|read| (read > 0).then_some(line))
    });
    let debugger_enabled = debug || step || !breakpoints.is_empty();
    if debugger_enabled {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
    }
    if step {
        vm.enable_step_debugging();
    }
    for &line in breakpoints {
        if line == 0 {
            bail!("breakpoint lines are 1-based");
        }
        vm.add_line_breakpoint(file.clone(), line);
    }
    if step || !breakpoints.is_empty() {
        vm.set_debug_controller(|_| read_terminal_debug_action());
    }

    let result = vm.run_chunk(&chunk);
    print!("{}", vm.stdout());
    eprint!("{}", vm.stderr());
    if let Err(ricochet_vm::VmError::ExitRequested { code }) = result {
        std::process::exit(code);
    }
    result?;

    println!("{:?}", vm.stack());

    Ok(())
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

fn run_tests(path: &str, debug: bool) -> Result<()> {
    let path = Path::new(path);
    let project_root = find_project_root_for_tests(path);
    let files = collect_test_files_for_path(path, project_root.as_deref())?;
    let mut total = 0usize;
    let mut failed = 0usize;

    for file in files {
        let (file_name, source) = read_source_path(&file)?;
        let chunk = compile_source(&file_name, &source)?;
        let mut vm = Vm::default();
        vm.enable_cli_capabilities();
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
            total += 1;
            let instance = vm
                .new_instance(&class_name)
                .with_context(|| format!("failed to instantiate test case {class_name}"))?;
            match vm.call_method_value(instance, &method_name) {
                Ok(_) => println!("PASS {class_name}.{method_name}"),
                Err(error) => {
                    failed += 1;
                    println!("FAIL {class_name}.{method_name}: {error}");
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
        let (file_name, source) = read_source_path(&file)?;
        let chunk = compile_source(&file_name, &source)
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
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let output = Path::new(BUILD_OUTPUT);
    let parent = output
        .parent()
        .expect("build output path should include parent directory");

    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(output, chunk.to_bytes()?)
        .with_context(|| format!("failed to write {}", output.display()))?;

    println!("built {}", output.display());

    Ok(())
}

fn read_source(path: &str) -> Result<(String, String)> {
    read_source_path(Path::new(path))
}

fn read_source_path(source_path: &Path) -> Result<(String, String)> {
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let file = source_path.to_string_lossy();

    Ok((file.into_owned(), source))
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
