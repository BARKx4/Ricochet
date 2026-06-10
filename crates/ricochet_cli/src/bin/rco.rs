#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ricochet_cli::run_cli().await
}
