use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ricochet_compiler::compile_source;
use ricochet_vm::{DebugEvent, Vm};

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
    Run {
        #[arg(long)]
        debug: bool,
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
        Command::Run { debug, path } => run_file(&path, debug)?,
        Command::Build { path } => build(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE))?,
        Command::Serve { debug, watch } => ricochet_web::serve_current_dir(debug, watch).await?,
        Command::Test { debug, path } => {
            run_tests(path.as_deref().unwrap_or("tests"), debug)?;
        }
    }
    Ok(())
}

fn run_file(path: &str, debug: bool) -> Result<()> {
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let mut vm = Vm::default();
    if debug {
        vm.enable_debug();
        vm.set_debug_sink(print_debug_event);
    }

    let result = vm.run_chunk(&chunk);
    for line in vm.output_lines() {
        println!("{line}");
    }
    result?;

    println!("{:?}", vm.stack());

    Ok(())
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
