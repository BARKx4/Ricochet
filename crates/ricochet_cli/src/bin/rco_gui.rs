#![cfg_attr(windows, windows_subsystem = "windows")]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ricochet_cli::run_gui_launcher().await
}
