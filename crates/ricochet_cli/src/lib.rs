use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ricochet_compiler::compile_source;
use ricochet_vm::Vm;

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
    Test { path: Option<String> },
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
        Command::Test { path } => println!("test {}", path.unwrap_or_else(|| ".".to_string())),
    }
    Ok(())
}

fn run_file(path: &str, debug: bool) -> Result<()> {
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let mut vm = Vm::default();
    if debug {
        vm.enable_debug();
    }

    let result = vm.run_chunk(&chunk);
    if debug {
        for event in vm.debug_events() {
            println!("{event:?}");
        }
    }
    result?;

    println!("{:?}", vm.stack());

    Ok(())
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
    let source_path = Path::new(path);
    let source = fs::read_to_string(source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let file = source_path.to_string_lossy();

    Ok((file.into_owned(), source))
}
