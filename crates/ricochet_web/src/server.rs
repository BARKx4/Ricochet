use anyhow::{bail, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::controller::{ActionResult, ControllerRegistry, RequestContext};
use crate::revision::{AppRevision, RevisionManager};
use crate::template::{render_template, EscapeMode};

#[derive(Clone)]
struct AppState {
    revisions: RevisionManager,
}

pub fn build_test_app() -> Result<Router> {
    Ok(Router::new().route("/", get(home)).with_state(AppState {
        revisions: RevisionManager::default(),
    }))
}

async fn home(State(state): State<AppState>) -> impl IntoResponse {
    let revision = state.revisions.current();

    match render_home(revision) {
        Ok(body) => Html(body).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ricochet MVC error: {err}"),
        )
            .into_response(),
    }
}

fn render_home(revision: AppRevision) -> Result<String> {
    let mut registry = ControllerRegistry::default();
    registry.register_static("HomeController", "index", |ctx| {
        ctx.view_data
            .insert("title".to_string(), "Hello Ricochet".to_string());
        Ok(ActionResult::View("home/index".to_string()))
    });

    let mut ctx = RequestContext::default();
    ctx.view_data
        .insert("revision".to_string(), revision.id.to_string());

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
        let _ = build_test_app().expect("server test app should build");
    }
}
