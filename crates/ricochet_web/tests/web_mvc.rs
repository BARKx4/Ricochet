use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use ricochet_vm::Value;
use ricochet_web::{
    ActionResult, ActiveRecordError, ControllerRegistry, DatabaseBackend, ModelMapping, OrderPage,
    PostgresDatabase, RequestContext, WatchTraceEvent, WatchTraceSink,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

static NEXT_TEMP_PROJECT: AtomicU64 = AtomicU64::new(0);

#[test]
fn active_record_database_url_smoke_allows_unset_or_postgres_url() {
    match std::env::var("DATABASE_URL") {
        Ok(url) => {
            assert!(
                url.starts_with("postgres://") || url.starts_with("postgresql://"),
                "DATABASE_URL must start with postgres:// or postgresql://"
            );
        }
        Err(std::env::VarError::NotPresent) => {
            println!("DATABASE_URL unset; skipping PostgreSQL Active Record smoke check");
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("DATABASE_URL must be valid Unicode");
        }
    }
}

#[tokio::test]
async fn active_record_pings_live_postgres_when_database_url_is_set() {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => {
            println!("DATABASE_URL unset; skipping live PostgreSQL ping");
            return;
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("DATABASE_URL must be valid Unicode");
        }
    };

    let database = PostgresDatabase::connect(&url)
        .await
        .expect("PostgreSQL connection should succeed");
    database
        .ping()
        .await
        .expect("PostgreSQL select 1 should succeed");
}

struct FixtureDatabase;

impl DatabaseBackend for FixtureDatabase {
    fn find(
        &self,
        _mapping: &ModelMapping,
        _id: &Value,
    ) -> Result<Option<Value>, ActiveRecordError> {
        Ok(None)
    }

    fn all(&self, _mapping: &ModelMapping) -> Result<Vec<Value>, ActiveRecordError> {
        Ok(vec![Value::Map(
            BTreeMap::from([
                ("id".to_string(), Value::Number(1)),
                (
                    "email".to_string(),
                    Value::String("ada@example.com".to_string()),
                ),
            ])
            .into(),
        )])
    }

    fn count(&self, mapping: &ModelMapping) -> Result<i64, ActiveRecordError> {
        Ok(self.all(mapping)?.len() as i64)
    }

    fn first(&self, mapping: &ModelMapping) -> Result<Option<Value>, ActiveRecordError> {
        Ok(self.all(mapping)?.into_iter().next())
    }

    fn limit(&self, mapping: &ModelMapping, limit: i64) -> Result<Vec<Value>, ActiveRecordError> {
        let mut rows = self.all(mapping)?;
        rows.truncate(limit as usize);
        Ok(rows)
    }

    fn page(
        &self,
        mapping: &ModelMapping,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        Ok(self
            .all(mapping)?
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    fn order_page(
        &self,
        mapping: &ModelMapping,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.page(mapping, order.limit, order.offset)
    }

    fn exists_by_id(&self, mapping: &ModelMapping, id: &Value) -> Result<bool, ActiveRecordError> {
        Ok(self.all(mapping)?.iter().any(|row| match row {
            Value::Map(row) => row.get("id").as_ref() == Some(id),
            _ => false,
        }))
    }

    fn where_eq(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        Ok(self
            .all(mapping)?
            .into_iter()
            .filter(|row| match row {
                Value::Map(row) => row.get(field).as_ref() == Some(value),
                _ => false,
            })
            .collect())
    }

    fn where_eq_limit(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        let mut rows = self.where_eq(mapping, field, value)?;
        rows.truncate(limit as usize);
        Ok(rows)
    }

    fn where_eq_page(
        &self,
        mapping: &ModelMapping,
        field: &str,
        value: &Value,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        Ok(self
            .where_eq(mapping, field, value)?
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    fn where_eq_order_page(
        &self,
        mapping: &ModelMapping,
        where_field: &str,
        value: &Value,
        order: OrderPage<'_>,
    ) -> Result<Vec<Value>, ActiveRecordError> {
        self.where_eq_page(mapping, where_field, value, order.limit, order.offset)
    }

    fn insert(
        &self,
        _mapping: &ModelMapping,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        Ok(Value::Map(attributes.clone().into()))
    }

    fn update_by_id(
        &self,
        _mapping: &ModelMapping,
        _id: Value,
        attributes: &BTreeMap<String, Value>,
    ) -> Result<Value, ActiveRecordError> {
        Ok(Value::Map(attributes.clone().into()))
    }
}

#[tokio::test]
async fn serves_database_capability_results_from_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Models")).expect("model directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "database_capability"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/users" UserController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Models/User.rco"),
        r#"
User Model subclass
  users table
  id field
  email field
end
"#,
    )
    .expect("model should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller subclass
  index method
    User .all
    dup ok? if
      value json
    else
      error .message get text
    end
  end
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir_with_database(
        &project_root,
        Arc::new(FixtureDatabase),
    )
    .expect("build app with database capability");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("application/json"))
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert!(body.contains("ada@example.com"));
}

#[test]
fn home_controller_index_sets_title_and_returns_view() {
    let mut controllers = ControllerRegistry::default();
    controllers.register_static("HomeController", "index", |ctx| {
        ctx.view_data.insert(
            "title".to_string(),
            Value::String("Hello Ricochet".to_string()),
        );
        Ok(ActionResult::View("home/index".to_string()))
    });

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("HomeController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::View("home/index".to_string()));
    assert_eq!(
        ctx.view_data.get("title"),
        Some(&Value::String("Hello Ricochet".to_string()))
    );
}

#[test]
fn ricochet_home_controller_source_sets_title_and_returns_view() {
    let mut controllers = ControllerRegistry::default();
    let source_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/web_minimal/app/Controllers/HomeController.rco");
    let source = std::fs::read_to_string(&source_path).expect("fixture controller should read");

    controllers
        .register_ricochet_source(
            "HomeController",
            "index",
            source_path.to_string_lossy().as_ref(),
            &source,
        )
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("HomeController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::View("home/index".to_string()));
    assert_eq!(
        ctx.view_data.get("title"),
        Some(&Value::String("Hello Ricochet".to_string()))
    );
}

#[test]
fn ricochet_controller_reads_request_params() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
HomeController Controller subclass
  "show" [
    title var
    ctx get .params get .id get title set
    ctx get
    "home/show" swap view
  ] !method
end
"#;

    controllers
        .register_ricochet_source("HomeController", "show", "HomeController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    ctx.params.insert("id".to_string(), "42".to_string());

    let result = controllers
        .call("HomeController", "show", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::View("home/show".to_string()));
    assert_eq!(
        ctx.view_data.get("title"),
        Some(&Value::String("42".to_string()))
    );
}

#[test]
fn ricochet_controller_receives_declared_request_args() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
HomeController Controller subclass
  ( id ctx ) "show" [
    nil title var
    ctx var
    id var
    id get title set
    ctx get
    "home/show" swap view
  ] !method
end
"#;

    controllers
        .register_ricochet_source("HomeController", "show", "HomeController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    ctx.params.insert("id".to_string(), "42".to_string());

    let result = controllers
        .call("HomeController", "show", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::View("home/show".to_string()));
    assert_eq!(
        ctx.view_data.get("title"),
        Some(&Value::String("42".to_string()))
    );
}

#[test]
fn ricochet_controller_returns_text_response() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
PingController Controller subclass
  "index" [
    ctx get
    "pong" swap text
  ] !method
end
"#;

    controllers
        .register_ricochet_source("PingController", "index", "PingController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("PingController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::Text("pong".to_string()));
}

#[test]
fn ricochet_controller_returns_json_response() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
ApiController Controller subclass
  "show" [
    map
    "name" "Ada" !put
    json
  ] !method
end
"#;

    controllers
        .register_ricochet_source("ApiController", "show", "ApiController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("ApiController", "show", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::Json(r#"{"name":"Ada"}"#.to_string()));
}

#[test]
fn ricochet_controller_returns_redirect_response() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
LoginController Controller subclass
  "create" [
    "/dashboard" redirect
  ] !method
end
"#;

    controllers
        .register_ricochet_source("LoginController", "create", "LoginController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("LoginController", "create", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(
        result,
        ActionResult::Redirect {
            location: "/dashboard".to_string(),
            status: None,
            headers: Default::default()
        }
    );
}

#[test]
fn ricochet_controller_returns_text_response_with_status_and_header() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
PingController Controller subclass
  "index" [
    "pong" text
    201 status
    "x-ricochet" "yes" header
  ] !method
end
"#;

    controllers
        .register_ricochet_source("PingController", "index", "PingController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    let result = controllers
        .call("PingController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(
        result,
        ActionResult::TextResponse {
            body: "pong".to_string(),
            status: Some(201),
            headers: BTreeMap::from([("x-ricochet".to_string(), "yes".to_string())])
        }
    );
}

#[test]
fn ricochet_controller_reads_query_params() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
SearchController Controller subclass
  "index" [
    ctx get .query get .q get text
  ] !method
end
"#;

    controllers
        .register_ricochet_source("SearchController", "index", "SearchController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    ctx.query.insert("q".to_string(), "ricochet".to_string());

    let result = controllers
        .call("SearchController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::Text("ricochet".to_string()));
}

#[test]
fn ricochet_controller_receives_declared_query_args() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
SearchController Controller subclass
  ( q ) "index" [
    q var
    q get text
  ] !method
end
"#;

    controllers
        .register_ricochet_source("SearchController", "index", "SearchController.rco", source)
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    ctx.query.insert("q".to_string(), "ricochet".to_string());

    let result = controllers
        .call("SearchController", "index", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::Text("ricochet".to_string()));
}

#[test]
fn ricochet_controller_reads_form_params() {
    let mut controllers = ControllerRegistry::default();
    let source = r#"
ContactController Controller subclass
  "create" [
    ctx get .form get .email get text
  ] !method
end
"#;

    controllers
        .register_ricochet_source(
            "ContactController",
            "create",
            "ContactController.rco",
            source,
        )
        .expect("controller source should register");

    let mut ctx = RequestContext::default();
    ctx.form
        .insert("email".to_string(), "ada@example.com".to_string());

    let result = controllers
        .call("ContactController", "create", &mut ctx)
        .expect("action should dispatch");

    assert_eq!(result, ActionResult::Text("ada@example.com".to_string()));
}

#[tokio::test]
async fn serves_minimal_mvc_home_page() {
    let app = ricochet_web::server::build_test_app().expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert!(body.contains("Hello Ricochet"));
}

#[tokio::test]
async fn serves_minimal_mvc_home_page_from_project_files() {
    let project_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web_minimal");
    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(
        body.trim(),
        "<h1>Hello Ricochet</h1>\n<p>Ada &lt;Lovelace&gt;</p>\n<small>42</small>"
    );
}

#[tokio::test]
async fn serves_delete_route_from_project_files() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "delete_route"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"DELETE "/users/:id" UserController "destroy" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller subclass
  "destroy" [
    ctx get .params get .id get
    "deleted " .concat text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "deleted 42");
}

#[tokio::test]
async fn controller_execution_budget_returns_server_error_for_runaway_work() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "budgeted_controller"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/" HomeController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller subclass
  "index" [
    0 counter var
    counter get 10000 < while
      counter get 1 + counter set
    end
    "done" text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert!(
        body.contains("instruction limit exceeded"),
        "expected instruction budget error, got: {body}"
    );
}

#[tokio::test]
async fn serves_mvc_controller_that_uses_project_model_without_database() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Models")).expect("model directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Views/users"))
        .expect("view directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "model_controller"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/users" UserController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Models/User.rco"),
        r#"
User Model subclass
  email field
  name field

  "displayName" [
    self .name get nil? if
      self .email get
    else
      self .name get
    end
  ] !method
end
"#,
    )
    .expect("model should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller subclass
  "index" [
    User new
    "ada@example.com" swap .email set
    .displayName title var
    ctx get
    "users/index" swap view
  ] !method
end
"#,
    )
    .expect("controller should be written");
    fs::write(
        project_root.join("app/Views/users/index.html"),
        "<h1>{ title get }</h1>",
    )
    .expect("view should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body.trim(), "<h1>ada@example.com</h1>");
}

#[tokio::test]
async fn serves_route_params_to_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Views/home"))
        .expect("view directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "route_params"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/users/:id" HomeController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller subclass
  "show" [
    title var
    ctx get .params get .id get title set
    ctx get
    "home/show" swap view
  ] !method
end
"#,
    )
    .expect("controller should be written");
    fs::write(
        project_root.join("app/Views/home/show.html"),
        "<h1>{ title get }</h1>",
    )
    .expect("view should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body.trim(), "<h1>42</h1>");
}

#[tokio::test]
async fn serves_declared_route_args_to_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Views/home"))
        .expect("view directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "route_arg_dispatch"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/users/:id" HomeController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller subclass
  ( id ctx ) "show" [
    nil title var
    ctx var
    id var
    id get title set
    ctx get
    "home/show" swap view
  ] !method
end
"#,
    )
    .expect("controller should be written");
    fs::write(
        project_root.join("app/Views/home/show.html"),
        "<h1>{ title get }</h1>",
    )
    .expect("view should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body.trim(), "<h1>42</h1>");
}

#[tokio::test]
async fn serves_ricochet_text_response_without_view_file() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "text_response"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/ping" PingController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/PingController.rco"),
        r#"
PingController Controller subclass
  "index" [
    ctx get
    "pong" swap text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "pong");
}

#[tokio::test]
async fn serves_ricochet_json_response() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "json_response"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/api/user" ApiController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/ApiController.rco"),
        r#"
ApiController Controller subclass
  "show" [
    map
    "name" "Ada" !put
    json
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, r#"{"name":"Ada"}"#);
}

#[tokio::test]
async fn serves_ricochet_redirect_response() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "redirect_response"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/login" LoginController "create" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/LoginController.rco"),
        r#"
LoginController Controller subclass
  "create" [
    "/dashboard" redirect
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/dashboard")
    );
}

#[tokio::test]
async fn serves_text_response_with_status_and_header() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "status_header_response"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/ping" PingController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/PingController.rco"),
        r#"
PingController Controller subclass
  "index" [
    "pong" text
    201 status
    "x-ricochet" "yes" header
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response
            .headers()
            .get("x-ricochet")
            .and_then(|value| value.to_str().ok()),
        Some("yes")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "pong");
}

#[tokio::test]
async fn serves_controller_with_static_string_import() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Services"))
        .expect("service directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "controller_import"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/" HomeController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Services/Greeting.rco"),
        r#"
"greeting" function
  "hello from import"
end
"#,
    )
    .expect("service should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
"../Services/Greeting" import

HomeController Controller subclass
  "index" [
    greeting text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "hello from import");
}

#[tokio::test]
async fn watched_app_reloads_routes_and_controller_sources_between_requests() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "watched_reload"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/" HomeController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller subclass
  "index" [
    "before" text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let trace_events = Arc::new(Mutex::new(Vec::new()));
    let trace_sink: WatchTraceSink = {
        let trace_events = Arc::clone(&trace_events);
        Arc::new(move |event| {
            trace_events
                .lock()
                .expect("trace events lock should not be poisoned")
                .push(event.clone());
        })
    };
    let app =
        ricochet_web::server::build_watched_app_from_dir_with_trace(&project_root, trace_sink)
            .expect("build watched app");

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "before");
    assert!(
        trace_events
            .lock()
            .expect("trace events lock should not be poisoned")
            .is_empty(),
        "initial watched request should not emit a reload trace"
    );

    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/now" HomeController "index" route"#,
    )
    .expect("routes should be rewritten");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller subclass
  "index" [
    "after" text
  ] !method
end
"#,
    )
    .expect("controller should be rewritten");

    let response = app
        .oneshot(Request::builder().uri("/now").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "after");
    let trace_events = trace_events
        .lock()
        .expect("trace events lock should not be poisoned");
    assert_eq!(trace_events.len(), 1);
    assert_eq!(
        trace_events[0],
        WatchTraceEvent::Reloaded {
            revision: 1,
            changed_files: vec![
                PathBuf::from("app")
                    .join("Controllers")
                    .join("HomeController.rco"),
                PathBuf::from("config").join("routes.rco"),
            ],
        }
    );
}

#[tokio::test]
async fn serves_query_params_to_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "query_params"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/search" SearchController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/SearchController.rco"),
        r#"
SearchController Controller subclass
  "index" [
    ctx get .query get .q get text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=ricochet")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "ricochet");
}

#[tokio::test]
async fn serves_post_form_args_to_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "post_form"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[database.default]
adapter = "postgres"
url = "${DATABASE_URL}"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"POST "/contact" ContactController "create" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/ContactController.rco"),
        r#"
ContactController Controller subclass
  ( email ) "create" [
    email var
    email get text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/contact")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("email=ada%40example.com"))
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert_eq!(body, "ada@example.com");
}

#[tokio::test]
async fn serves_request_cookies_and_config_to_ricochet_controller_args() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "request_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/context" ContextController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/ContextController.rco"),
        r#"
ContextController Controller subclass
  ( request cookies config ) "show" [
    config var
    cookies var
    request var
    map
    "method" request get .method get !put
    "path" request get .path get !put
    "theme" cookies get .theme get !put
    "session" cookies get .session get !put
    "package" config get .package get .name get !put
    json
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("cookie", "theme=dark; session=abc%20123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert!(body.contains(r#""method":"GET""#), "body was {body}");
    assert!(body.contains(r#""path":"/context""#), "body was {body}");
    assert!(body.contains(r#""theme":"dark""#), "body was {body}");
    assert!(body.contains(r#""session":"abc 123""#), "body was {body}");
    assert!(
        body.contains(r#""package":"request_context""#),
        "body was {body}"
    );
}

#[tokio::test]
async fn serves_session_map_from_cookie_and_sets_cookie_after_mutation() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "session_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/session" SessionController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/SessionController.rco"),
        r#"
SessionController Controller subclass
  ( session ) "show" [
    session var
    session get .user get nil? if
      session get "user" "Ada" !put drop
    end
    session get .user get text
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("session mutation should set a cookie")
        .to_str()
        .expect("set-cookie should be UTF-8")
        .to_string();
    assert!(
        set_cookie.starts_with("ricochet_session="),
        "set-cookie was {set_cookie}"
    );
    assert!(
        set_cookie.contains("HttpOnly"),
        "set-cookie was {set_cookie}"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "Ada");

    let cookie = set_cookie
        .split(';')
        .next()
        .expect("set-cookie should include cookie pair");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/session")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "unchanged session should not rewrite the cookie"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "Ada");
}

#[tokio::test]
async fn serves_signed_session_cookie_when_secret_env_is_configured() {
    let project_root = temp_project_path();
    let secret_env = "RICOCHET_TEST_SIGNED_SESSION_SECRET";
    std::env::set_var(secret_env, "test-session-secret");
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        format!(
            r#"
[package]
name = "signed_session_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.session]
signing_secret_env = "{secret_env}"
"#
        ),
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/session" SessionController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/SessionController.rco"),
        r#"
SessionController Controller subclass
  ( session ) "show" [
    session var
    session get .user get nil? if
      session get "user" "Ada" !put drop
      "new" text
    else
      session get .user get text
    end
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("session mutation should set a cookie")
        .to_str()
        .expect("set-cookie should be UTF-8")
        .to_string();
    assert!(
        set_cookie.starts_with("ricochet_session=v1%3A"),
        "set-cookie was {set_cookie}"
    );
    assert!(
        !set_cookie.contains("Ada") && !set_cookie.contains("%7B"),
        "signed cookie should not expose raw JSON, got {set_cookie}"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "new");

    let cookie = set_cookie
        .split(';')
        .next()
        .expect("set-cookie should include cookie pair");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/session")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get(header::SET_COOKIE).is_none(),
        "unchanged signed session should not rewrite the cookie"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "Ada");

    let tampered_cookie = format!("{cookie}00");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/session")
                .header("cookie", tampered_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get(header::SET_COOKIE).is_some(),
        "tampered signed session should be replaced"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "new");
}

#[tokio::test]
async fn serves_ai_capability_to_ricochet_controllers() {
    let project_root = temp_project_path();
    let (base_url, ai_server) = spawn_openai_compatible_server("hello from ai");
    std::env::set_var("RICOCHET_TEST_AI_HTTP_KEY", "test-ai-key");
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        format!(
            r#"
[package]
name = "ai_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[ai.default]
provider = "openai"
model = "test-model"
api_key = "${{RICOCHET_TEST_AI_HTTP_KEY}}"
base_url = "{base_url}"
"#
        ),
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/ai" AiController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/AiController.rco"),
        r#"
AiController Controller subclass
  ( ai ) "index" [
    ai var
    "Say hello" ai get .chat result var
    result get ok? if
      result get value .text get text
    else
      result get error .message get text
    end
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(Request::builder().uri("/ai").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "hello from ai");

    let request = ai_server.join().expect("AI server thread should finish");
    let request_lower = request.to_ascii_lowercase();
    assert!(
        request.starts_with("POST /v1/chat/completions "),
        "request was {request}"
    );
    assert!(
        request_lower.contains("authorization: bearer test-ai-key"),
        "request was {request}"
    );
    assert!(
        request.contains(r#""model":"test-model""#),
        "request was {request}"
    );
    assert!(
        request.contains(r#""content":"Say hello""#),
        "request was {request}"
    );
}

#[tokio::test]
async fn serves_logger_capability_to_ricochet_controllers() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "logger_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/logs" LogController "index" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/LogController.rco"),
        r#"
LogController Controller subclass
  ( logger ) "index" [
    logger var
    "loaded" logger get .info drop
    "careful" logger get .warn drop
    logger get .entries json
  ] !method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(Request::builder().uri("/logs").body(Body::empty()).unwrap())
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("application/json"))
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");

    assert!(body.contains(r#""level":"info""#), "body was {body}");
    assert!(body.contains(r#""message":"loaded""#), "body was {body}");
    assert!(body.contains(r#""level":"warn""#), "body was {body}");
    assert!(body.contains(r#""message":"careful""#), "body was {body}");
}

fn temp_project_path() -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let sequence = NEXT_TEMP_PROJECT.fetch_add(1, Ordering::Relaxed);

    base.join("web-mvc")
        .join(format!("project-{}-{nanos}-{sequence}", std::process::id()))
}

fn spawn_openai_compatible_server(response_text: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("AI fixture should bind");
    let base_url = format!(
        "http://{}/v1",
        listener
            .local_addr()
            .expect("AI fixture should have address")
    );
    let response_text = response_text.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("AI fixture should accept one request");
        let request = read_http_request(&mut stream);
        let body = serde_json::json!({
            "id": "chatcmpl-test",
            "model": "test-model",
            "choices": [
                {
                    "message": {
                        "content": response_text,
                    }
                }
            ],
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("AI fixture should write response");
        request
    });

    (base_url, handle)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("AI fixture should set read timeout");
    let mut buffer = Vec::new();
    loop {
        let mut chunk = [0_u8; 1024];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if http_request_complete(&buffer) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("AI fixture failed to read request: {error}"),
        }
    }

    String::from_utf8(buffer).expect("AI fixture request should be UTF-8")
}

fn http_request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    buffer.len() >= header_end + 4 + content_length
}
