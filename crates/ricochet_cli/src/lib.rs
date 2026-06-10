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
        } => run_file(&path, debug, step, &breakpoints)?,
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
    for line in &vm.output_lines()[*output_cursor..] {
        writeln!(output, "{line}")?;
    }
    *output_cursor = vm.output_lines().len();
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
    fs::create_dir_all(path.join("app").join("Views").join("home"))
        .with_context(|| format!("failed to create app/Views/home in {}", path.display()))?;
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
"#,
    )?;
    write_project_file(
        path.join("app").join("Controllers").join("HomeController.rco"),
        r#"HomeController Controller subclass
  "index" [
    title var
    "Hello Ricochet" title set
    ctx get
    "home/index" swap view
  ] !method
end
"#,
    )?;
    write_project_file(
        path.join("app").join("Views").join("home").join("index.html"),
        "<h1>{ title get }</h1>\n",
    )?;
    write_project_file(
        path.join("tests").join("HomeControllerTest.rco"),
        r#"HomeControllerTest TestCase subclass
  "testHomeTitle" [
    "Hello Ricochet"
    "Hello Ricochet" assert-equals
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

fn run_file(path: &str, debug: bool, step: bool, breakpoints: &[usize]) -> Result<()> {
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let mut vm = Vm::default();
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
    for line in vm.output_lines() {
        println!("{line}");
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
    let files = collect_test_files(Path::new(path))?;
    let mut total = 0usize;
    let mut failed = 0usize;

    for file in files {
        let (file_name, source) = read_source_path(&file)?;
        let chunk = compile_source(&file_name, &source)?;
        let mut vm = Vm::default();
        if debug {
            vm.enable_debug();
            vm.set_debug_sink(print_debug_event);
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
