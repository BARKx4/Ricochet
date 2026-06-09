use clap::{Parser, Subcommand};

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
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { path } => println!("run {path}"),
        Command::Build { path } => println!("build {}", path.unwrap_or_else(|| ".".to_string())),
        Command::Serve { debug, watch } => println!("serve debug={debug} watch={watch}"),
        Command::Test { path } => println!("test {}", path.unwrap_or_else(|| ".".to_string())),
    }
    Ok(())
}
