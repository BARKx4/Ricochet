#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    ricochet_cli::run_gui_launcher()
}
