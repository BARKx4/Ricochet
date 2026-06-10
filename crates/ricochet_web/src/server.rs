use anyhow::Result;

pub async fn serve_current_dir(debug: bool, watch: bool) -> Result<()> {
    println!("ricochet web server starting debug={debug} watch={watch}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_serve_current_dir_returns_ok() {
        serve_current_dir(true, false)
            .await
            .expect("server skeleton should return ok");
    }
}
