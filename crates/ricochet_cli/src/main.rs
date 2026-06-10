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
    Run { path: String },
    Build { path: Option<String> },
    Serve { #[arg(long)] debug: bool, #[arg(long)] watch: bool },
    Test { path: Option<String> },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => run(&path)?,
        Command::Build { path } => build(path.as_deref().unwrap_or(DEFAULT_BUILD_SOURCE))?,
        Command::Serve { debug, watch } => println!("serve debug={debug} watch={watch}"),
        Command::Test { path } => println!("test {}", path.unwrap_or_else(|| ".".to_string())),
    }
    Ok(())
}

fn run(path: &str) -> Result<()> {
    let (file, source) = read_source(path)?;
    let chunk = compile_source(&file, &source)?;
    let mut vm = Vm::default();
    vm.run_chunk(&chunk)?;

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
