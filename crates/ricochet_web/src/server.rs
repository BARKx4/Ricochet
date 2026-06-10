use anyhow::{bail, Result};
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::template::{render_template, EscapeMode};

pub fn build_test_app() -> Result<Router> {
    Ok(Router::new().route("/", get(home)))
}

async fn home() -> impl IntoResponse {
    match render_home() {
        Ok(body) => Html(body).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ricochet MVC error: {err}"),
        )
            .into_response(),
    }
}

fn render_home() -> Result<String> {
    let mut registry = ControllerRegistry::default();
    registry.register_static("HomeController", "index", |ctx| {
        ctx.view_data
            .insert("title".to_string(), "Hello Ricochet".to_string());
        Ok(ActionResult::View("home/index".to_string()))
    });

    let mut ctx = RequestContext::default();
    match registry.call("HomeController", "index", &mut ctx)? {
        ActionResult::View(view) if view == "home/index" => {
            render_template("<h1>{ title get }</h1>", &ctx.view_data, EscapeMode::Html)
        }
        ActionResult::View(view) => bail!("no static test template registered for view {view}"),
        ActionResult::Text(text) => Ok(text),
    }
}

pub async fn serve_current_dir(debug: bool, watch: bool) -> Result<()> {
    let app = build_test_app()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    println!("Ricochet web server listening on http://127.0.0.1:3000 debug={debug} watch={watch}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_build_test_app_returns_ok() {
        build_test_app().expect("server test app should build");
    }
}
