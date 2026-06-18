use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use ricochet_vm::Value;
use ricochet_web::{
    ActionResult, ActiveRecordError, ControllerRegistry, DatabaseBackend, ModelMapping,
    MysqlDatabase, OrderPage, PostgresDatabase, RequestContext, SqliteDatabase, WatchTraceEvent,
    WatchTraceSink,
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

#[test]
fn active_record_mysql_url_smoke_allows_unset_or_mysql_url() {
    match std::env::var("MYSQL_URL") {
        Ok(url) => {
            assert!(
                url.starts_with("mysql://"),
                "MYSQL_URL must start with mysql://"
            );
        }
        Err(std::env::VarError::NotPresent) => {
            println!("MYSQL_URL unset; skipping MySQL Active Record smoke check");
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("MYSQL_URL must be valid Unicode");
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

#[tokio::test]
async fn active_record_pings_live_mysql_when_mysql_url_is_set() {
    let url = match std::env::var("MYSQL_URL") {
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => {
            println!("MYSQL_URL unset; skipping live MySQL ping");
            return;
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            panic!("MYSQL_URL must be valid Unicode");
        }
    };

    let database = MysqlDatabase::connect(&url)
        .await
        .expect("MySQL connection should succeed");
    database
        .ping()
        .await
        .expect("MySQL select 1 should succeed");
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
User Model Subclass
  "users" Table
  "id" Accessor
  "email" Accessor
end
"#,
    )
    .expect("model should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller Subclass
  [
    User all
    dup ok? if
      value json
    else
      error "message" at text
    end
  ] "index" Method
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

#[tokio::test]
async fn serves_active_record_results_from_sqlite_database() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("app/Models")).expect("model directory should be created");
    let database_path = project_root.join("development.sqlite3");
    let connection =
        rusqlite::Connection::open(&database_path).expect("sqlite database should open");
    connection
        .execute_batch(
            r#"
            create table users (
                id integer primary key,
                email text not null,
                name text not null
            );
            insert into users (email, name) values
                ('ada@example.com', 'Ada'),
                ('grace@example.com', 'Grace');
            "#,
        )
        .expect("sqlite schema should be created");
    drop(connection);

    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "sqlite_database_capability"

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
User Model Subclass
  "users" Table
  "id" Accessor
  "email" Accessor
  "name" Accessor
end
"#,
    )
    .expect("model should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller Subclass
  [
    "id" "asc" 10 0 User order-page
    dup ok? if
      value json
    else
      error "message" at text
    end
  ] "index" Method
end
"#,
    )
    .expect("controller should be written");

    let backend = SqliteDatabase::connect(database_path.to_str().expect("path is UTF-8"))
        .expect("sqlite backend connects");
    let app =
        ricochet_web::server::build_app_from_dir_with_database(&project_root, Arc::new(backend))
            .expect("build app with sqlite database capability");
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
    assert!(body.contains("ada@example.com"), "body was {body}");
    assert!(body.contains("grace@example.com"), "body was {body}");
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
HomeController Controller Subclass
  [
    title var
    ctx get "params" at "id" at title set
    ctx get
    "home/show" swap view
  ] "show" Method
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
HomeController Controller Subclass
  ( id ctx ) [
    nil title var
    ctx var
    id var
    id get title set
    ctx get
    "home/show" swap view
  ] "show" Method
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
PingController Controller Subclass
  [
    ctx get
    "pong" swap text
  ] "index" Method
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
ApiController Controller Subclass
  [
    map
    "name" "Ada" put!
    json
  ] "show" Method
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
LoginController Controller Subclass
  [
    "/dashboard" redirect
  ] "create" Method
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
PingController Controller Subclass
  [
    "pong" text
    201 status
    "x-ricochet" "yes" header
  ] "index" Method
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
SearchController Controller Subclass
  [
    ctx get "query" at "q" at text
  ] "index" Method
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
SearchController Controller Subclass
  ( q ) [
    q var
    q get text
  ] "index" Method
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
ContactController Controller Subclass
  [
    ctx get "form" at "email" at text
  ] "create" Method
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
UserController Controller Subclass
  [
    ctx get "params" at "id" at
    "deleted " swap concat text
  ] "destroy" Method
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
async fn serves_put_form_params_to_ricochet_controller() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "put_form_route"

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
        r#"PUT "/sessions/:id" SessionController "update" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/SessionController.rco"),
        r#"
SessionController Controller Subclass
  [
    map payload var
    payload get "id" ctx get "params" at "id" at put! drop
    payload get "title" ctx get "form" at "title" at put! drop
    payload get "metadata" ctx get "form" at "metadata" at put! drop
    payload get json
  ] "update" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/sessions/session-13")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "title=TDD+Session&metadata=%7B%22agent%22%3A%22codex%22%7D",
                ))
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");
    assert_eq!(body["id"], "session-13");
    assert_eq!(body["title"], "TDD Session");
    assert_eq!(body["metadata"], r#"{"agent":"codex"}"#);
}

#[tokio::test]
async fn put_form_fields_bind_to_declared_controller_args_like_post() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "put_declared_args"

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
        r#"
PUT "/probe/:id" ProbeController "update" route
POST "/probe/:id/update" ProbeController "update" route
"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/ProbeController.rco"),
        r#"
ProbeController Controller Subclass
  ( id name status ) [
    status var
    name var
    id var

    map data var
    data get "id" id get put! drop
    data get "name" name get put! drop
    data get "status" status get put! drop

    map response var
    response get "ok" true put! drop
    response get "data" data get put! drop
    response get "error" nil put! drop
    response get json
  ] "update" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let body = "name=Renamed&status=paused";

    let post = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/probe/abc/update")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("POST response");
    let put = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/probe/abc")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("PUT response");

    assert_eq!(post.status(), StatusCode::OK);
    assert_eq!(put.status(), StatusCode::OK);

    let post_body = to_bytes(post.into_body(), usize::MAX)
        .await
        .expect("POST body bytes");
    let put_body = to_bytes(put.into_body(), usize::MAX)
        .await
        .expect("PUT body bytes");
    let post_body: serde_json::Value =
        serde_json::from_slice(&post_body).expect("POST response body should be JSON");
    let put_body: serde_json::Value =
        serde_json::from_slice(&put_body).expect("PUT response body should be JSON");

    assert_eq!(post_body, put_body);
    assert_eq!(put_body["ok"], true);
    assert_eq!(put_body["data"]["id"], "abc");
    assert_eq!(put_body["data"]["name"], "Renamed");
    assert_eq!(put_body["data"]["status"], "paused");
    assert!(put_body["error"].is_null());
}

#[tokio::test]
async fn json_request_body_fields_bind_to_declared_controller_args() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "json_body_args"

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
        r#"PUT "/json/:id" JsonController "update" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/JsonController.rco"),
        r#"
JsonController Controller Subclass
  ( id name enabled meta tags ctx ) [
    ctx var
    tags var
    meta var
    enabled var
    name var
    id var

    map response var
    response get "id" id get put! drop
    response get "name" name get put! drop
    response get "enabled" enabled get put! drop
    response get "meta_kind" meta get "kind" at put! drop
    response get "tags_count" tags get count put! drop
    response get "ctx_name" ctx get "request" at "json" at "name" at put! drop
    response get "body_kind" ctx get "request" at "body" at "meta" at "kind" at put! drop
    response get json
  ] "update" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/json/route-id")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"id":"body-id","name":"Ada","enabled":true,"meta":{"kind":"agent"},"tags":["code","review"]}"#,
                ))
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");

    assert_eq!(body["id"], "route-id");
    assert_eq!(body["name"], "Ada");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["meta_kind"], "agent");
    assert_eq!(body["tags_count"], 2);
    assert_eq!(body["ctx_name"], "Ada");
    assert_eq!(body["body_kind"], "agent");
}

#[tokio::test]
async fn invalid_json_request_body_returns_bad_request() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "invalid_json_body"

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
        r#"POST "/json" JsonController "create" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/JsonController.rco"),
        r#"
JsonController Controller Subclass
  [
    "unreachable" text
  ] "create" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Ada""#))
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert!(
        body.contains("invalid JSON request body"),
        "bad request should explain invalid JSON, got:\n{body}"
    );
}

#[tokio::test]
async fn multipart_fields_and_files_bind_to_form_uploads_and_declared_args() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "multipart_uploads"

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
        r#"POST "/upload/:id" UploadController "create" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/UploadController.rco"),
        r#"
UploadController Controller Subclass
  ( id title file ctx ) [
    ctx var
    file var
    title var
    id var

    map response var
    response get "id" id get put! drop
    response get "title" title get put! drop
    response get "form_title" ctx get "request" at "form" at "title" at put! drop
    response get "filename" file get "filename" at put! drop
    response get "content_type" file get "content_type" at put! drop
    response get "size" file get "size" at put! drop
    response get "text" file get "text" at put! drop
    response get "data_base64" file get "data_base64" at put! drop
    response get "ctx_upload_text" ctx get "request" at "uploads" at "file" at "text" at put! drop
    response get "files_count" ctx get "request" at "files" at count put! drop
    response get json
  ] "create" Method
end
"#,
    )
    .expect("controller should be written");

    let boundary = "----ricochet-upload-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nAgent Harness\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"plan.txt\"\r\nContent-Type: text/plain\r\n\r\nHello uploads\r\n--{boundary}--\r\n"
    );

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/upload/run-1")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");

    assert_eq!(body["id"], "run-1");
    assert_eq!(body["title"], "Agent Harness");
    assert_eq!(body["form_title"], "Agent Harness");
    assert_eq!(body["filename"], "plan.txt");
    assert_eq!(body["content_type"], "text/plain");
    assert_eq!(body["size"], 13);
    assert_eq!(body["text"], "Hello uploads");
    assert_eq!(body["data_base64"], "SGVsbG8gdXBsb2Fkcw==");
    assert_eq!(body["ctx_upload_text"], "Hello uploads");
    assert_eq!(body["files_count"], 1);
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
HomeController Controller Subclass
  [
    0 counter var
    counter get 10000 < while
      counter get 1 + counter set
    end
    "done" text
  ] "index" Method
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
async fn controller_sleep_is_disabled_for_web_requests() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "sleep_disabled"

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
HomeController Controller Subclass
  [
    1000 sleep
    "done" text
  ] "index" Method
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
        body.contains("sleep capability is not enabled"),
        "expected sleep denial, got: {body}"
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
User Model Subclass
  "email" Accessor
  "name" Accessor

  [
    self name.get nil? if
      self email.get
    else
      self name.get
    end
  ] "displayName" Method
end
"#,
    )
    .expect("model should be written");
    fs::write(
        project_root.join("app/Controllers/UserController.rco"),
        r#"
UserController Controller Subclass
  [
    User new
    "ada@example.com" swap email.set
    displayName title var
    ctx get
    "users/index" swap view
  ] "index" Method
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
HomeController Controller Subclass
  [
    title var
    ctx get "params" at "id" at title set
    ctx get
    "home/show" swap view
  ] "show" Method
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
async fn rejects_request_selected_view_path_traversal() {
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
name = "view_traversal"

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
        r#"GET "/show" HomeController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        r#"
HomeController Controller Subclass
  ( template ctx ) [
    ctx var
    template var
    template get nil? if
      "home/safe" templateName var
    else
      template get templateName var
    end
    ctx get
    templateName get swap view
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");
    fs::write(project_root.join("app/Views/home/safe.html"), "safe")
        .expect("safe view should be written");
    fs::write(project_root.join("outside.html"), "outside marker")
        .expect("outside marker should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/show").body(Body::empty()).unwrap())
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(std::str::from_utf8(&body).unwrap(), "safe");

    for traversal in [
        "/show?template=../../outside",
        "/show?template=..%2F..%2Foutside",
        "/show?template=..%5C..%5Coutside",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(traversal)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let body = std::str::from_utf8(&body).expect("body should be UTF-8");
        assert!(
            body.contains("invalid view name") && !body.contains("outside marker"),
            "traversal {traversal} should fail closed, body was {body}"
        );
    }
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
HomeController Controller Subclass
  ( id ctx ) [
    nil title var
    ctx var
    id var
    id get title set
    ctx get
    "home/show" swap view
  ] "show" Method
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
PingController Controller Subclass
  [
    ctx get
    "pong" swap text
  ] "index" Method
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
async fn serves_static_assets_from_public_directory() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("public/styles"))
        .expect("public styles directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "static_assets"

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
HomeController Controller Subclass
  [
    "ok" text
  ] "index" Method
end
"#,
    )
    .expect("controller should be written");
    fs::write(
        project_root.join("public/styles/app.css"),
        "body { color: #312e81; }\n",
    )
    .expect("static asset should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/assets/styles/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("static asset response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/css; charset=utf-8")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "body { color: #312e81; }\n");

    let route_response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("route response");
    assert_eq!(route_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn static_assets_reject_encoded_traversal() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("public")).expect("public directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "static_traversal"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(project_root.join("config/routes.rco"), "").expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        "HomeController Controller Subclass\nend\n",
    )
    .expect("controller should be written");
    fs::write(project_root.join("public/app.css"), "safe").expect("static asset should be written");
    fs::write(project_root.join("secret.txt"), "secret marker").expect("secret should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/assets/%2e%2e/secret.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("traversal response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert!(
        !body.contains("secret marker"),
        "traversal response must not expose project files"
    );
}

#[tokio::test]
async fn serves_custom_static_asset_mount_and_rejects_invalid_static_dir() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("frontend/dist"))
        .expect("asset directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "custom_static"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.static]
dir = "frontend/dist"
mount = "/static"
"#,
    )
    .expect("manifest should be written");
    fs::write(project_root.join("config/routes.rco"), "").expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        "HomeController Controller Subclass\nend\n",
    )
    .expect("controller should be written");
    fs::write(
        project_root.join("frontend/dist/app.js"),
        "console.log('ricochet');",
    )
    .expect("asset should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("static asset response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "console.log('ricochet');");

    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "custom_static"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.static]
dir = "../outside"
mount = "/static"
"#,
    )
    .expect("manifest should be rewritten");

    let error = ricochet_web::server::build_app_from_dir(&project_root)
        .expect_err("traversing static directory should be rejected");
    assert!(
        error.to_string().contains("web.static.dir"),
        "error should mention static dir validation, got: {error:#}"
    );
}

#[tokio::test]
async fn watched_static_assets_reflect_file_updates_without_restart() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(project_root.join("public")).expect("public directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "watched_static"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"
"#,
    )
    .expect("manifest should be written");
    fs::write(project_root.join("config/routes.rco"), "").expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/HomeController.rco"),
        "HomeController Controller Subclass\nend\n",
    )
    .expect("controller should be written");
    fs::write(project_root.join("public/app.css"), "before").expect("asset should be written");

    let app =
        ricochet_web::server::build_watched_app_from_dir(&project_root).expect("build watched app");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/assets/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("first static response");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(std::str::from_utf8(&body).unwrap(), "before");

    fs::write(project_root.join("public/app.css"), "after").expect("asset should be rewritten");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/assets/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("second static response");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(std::str::from_utf8(&body).unwrap(), "after");
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
ApiController Controller Subclass
  [
    map
    "name" "Ada" put!
    json
  ] "show" Method
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
async fn mvc_controller_mutates_json_decoded_nested_collections_and_writes_them_back() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "json_collection_mutation"

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
        r#"POST "/fork" ForkController "create" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("sessions.json"),
        r#"{"sessions":[{"id":"source","events":[{"role":"user","text":"hello"}]}]}"#,
    )
    .expect("seed JSON should be written");
    fs::write(
        project_root.join("app/Controllers/ForkController.rco"),
        r#"
ForkController Controller Subclass
  ( fs ) [
    fs var
    "sessions.json" fs_read_text value json-decode value state var
    state get "sessions" at sessions var
    sessions get 0 at source var
    source get "events" at sourceEvents var
    sourceEvents get 0 at sourceEvent var

    map fork var
    fork get "id" "fork" put! drop
    array forkEvents var
    fork get "events" forkEvents get put! drop
    forkEvents get sourceEvent get push! drop

    map forkEvent var
    forkEvent get "role" "system" put! drop
    forkEvent get "text" "forked" put! drop
    forkEvents get forkEvent get push! drop

    sessions get fork get push! drop
    state get "sessions" sessions get put! drop
    "sessions.json" state get json-encode fs_write_text value drop

    map
    "ok" true put!
    "session_count" sessions get count put!
    json
  ] "create" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            fs_root: Some(project_root.clone()),
            ..Default::default()
        },
    )
    .await
    .expect("build served app with bounded filesystem");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/fork")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("fork response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, r#"{"ok":true,"session_count":2}"#);

    let written = fs::read_to_string(project_root.join("sessions.json"))
        .expect("controller should write JSON file");
    assert!(
        !written.contains("Member(\"put!\")"),
        "internal member leaked into written JSON: {written}"
    );
    let written: serde_json::Value =
        serde_json::from_str(&written).expect("written file should be valid JSON");
    let sessions = written["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["id"], "source");
    assert_eq!(sessions[0]["events"].as_array().unwrap().len(), 1);
    assert_eq!(sessions[1]["id"], "fork");
    let fork_events = sessions[1]["events"]
        .as_array()
        .expect("fork events should be an array");
    assert_eq!(fork_events.len(), 2);
    assert_eq!(fork_events[0]["text"], "hello");
    assert_eq!(fork_events[1]["text"], "forked");

    let follow_up = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/fork")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("server should remain alive after mutation");
    assert_ne!(follow_up.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn served_mvc_can_read_environment_when_allowed() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "env_capability"

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
        r#"GET "/env" EnvController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/EnvController.rco"),
        r#"
EnvController Controller Subclass
  [
    "RICOCHET_MVC_ENV_TEST" env_get value text
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");
    std::env::set_var("RICOCHET_MVC_ENV_TEST", "visible-to-mvc");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            allow_env: true,
            ..Default::default()
        },
    )
    .await
    .expect("build served app with environment access");

    let response = app
        .oneshot(Request::builder().uri("/env").body(Body::empty()).unwrap())
        .await
        .expect("env response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "visible-to-mvc");
}

#[tokio::test]
async fn served_mvc_can_bound_environment_reads_to_allowlist() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "env_allowlist"

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
        r#"
GET "/allowed" EnvController "allowed" route
GET "/denied" EnvController "denied" route
GET "/caps" EnvController "caps" route
"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/EnvController.rco"),
        r#"
EnvController Controller Subclass
  [
    "RICOCHET_MVC_ALLOWED_ENV_TEST" env_get value text
  ] "allowed" Method

  [
    "RICOCHET_MVC_DENIED_ENV_TEST" env_get value text
  ] "denied" Method

  [
    runtime_capabilities "environment" at "allowlist" at count json
  ] "caps" Method
end
"#,
    )
    .expect("controller should be written");
    std::env::set_var("RICOCHET_MVC_ALLOWED_ENV_TEST", "allowlisted");
    std::env::set_var("RICOCHET_MVC_DENIED_ENV_TEST", "should-not-leak");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            env_allow: vec!["RICOCHET_MVC_ALLOWED_ENV_TEST".to_string()],
            ..Default::default()
        },
    )
    .await
    .expect("build served app with environment allowlist");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/allowed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("allowed env response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "allowlisted");

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "1");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/denied")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("denied env response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "body was {body}");
    assert!(
        body.contains("environment variable is not allowed: RICOCHET_MVC_DENIED_ENV_TEST"),
        "body should explain allowlist denial, got {body}"
    );
    assert!(
        !body.contains("should-not-leak"),
        "body should not include denied environment value, got {body}"
    );
}

#[tokio::test]
async fn served_mvc_applies_manifest_web_capabilities() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "manifest_capabilities"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.capabilities]
fs_root = "."
env_allow = ["RICOCHET_MVC_MANIFEST_ENV_TEST"]
allow_process = true
process_root = "."
allow_pty = true
http_allow_hosts = ["127.0.0.1"]
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/caps" CapabilityController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/CapabilityController.rco"),
        r#"
CapabilityController Controller Subclass
  [
    "RICOCHET_MVC_MANIFEST_ENV_TEST" env_get value envValue var
    runtime_capabilities caps var
    map data var
    data get "env" envValue get put! drop
    data get "fs_enabled" caps get "filesystem" at "enabled" at put! drop
    data get "process_enabled" caps get "process" at "enabled" at put! drop
    data get "pty_enabled" caps get "pty" at "enabled" at put! drop
    data get "http_enabled" caps get "http" at "enabled" at put! drop
    data get json
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");
    std::env::set_var("RICOCHET_MVC_MANIFEST_ENV_TEST", "manifest-visible");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions::default(),
    )
    .await
    .expect("build served app with manifest-declared capabilities");

    let response = app
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(
        status,
        StatusCode::OK,
        "manifest capabilities should allow the controller, body was {}",
        String::from_utf8_lossy(&body)
    );
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");

    assert_eq!(body["env"], "manifest-visible");
    assert_eq!(body["fs_enabled"], true);
    assert_eq!(body["process_enabled"], true);
    assert_eq!(body["pty_enabled"], true);
    assert_eq!(body["http_enabled"], true);
}

#[tokio::test]
async fn watched_mvc_applies_serve_and_manifest_capabilities() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "watched_manifest_capabilities"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.capabilities]
env_allow = ["RICOCHET_MVC_WATCH_ENV_TEST"]
http_allow_hosts = ["127.0.0.1"]
"#,
    )
    .expect("manifest should be written");
    fs::write(
        project_root.join("config/routes.rco"),
        r#"GET "/caps" CapabilityController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/CapabilityController.rco"),
        r#"
CapabilityController Controller Subclass
  [
    "RICOCHET_MVC_WATCH_ENV_TEST" env_get value envValue var
    runtime_capabilities caps var
    map data var
    "RICOCHET_MVC_WATCH_ENV_EXTRA" env_get value extraEnvValue var
    data get "env" envValue get put! drop
    data get "extra_env" extraEnvValue get put! drop
    data get "fs_enabled" caps get "filesystem" at "enabled" at put! drop
    data get "process_enabled" caps get "process" at "enabled" at put! drop
    data get "pty_enabled" caps get "pty" at "enabled" at put! drop
    data get "http_enabled" caps get "http" at "enabled" at put! drop
    data get "revision" "before" put! drop
    data get json
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");
    std::env::set_var("RICOCHET_MVC_WATCH_ENV_TEST", "watch-visible");
    std::env::set_var("RICOCHET_MVC_WATCH_ENV_EXTRA", "watch-extra-visible");

    let options = ricochet_web::server::ServeOptions {
        watch: true,
        allow_env: true,
        allow_process: true,
        allow_pty: true,
        fs_root: Some(project_root.clone()),
        http_allow_hosts: vec!["localhost".to_string()],
        ..ricochet_web::server::ServeOptions::default()
    };

    let app = ricochet_web::server::build_served_app_from_dir(&project_root, true, true, &options)
        .await
        .expect("build watched app with serve and manifest capabilities");

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(
        status,
        StatusCode::OK,
        "watched capability response should succeed, body was {}",
        String::from_utf8_lossy(&body)
    );
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");
    assert_eq!(body["env"], "watch-visible");
    assert_eq!(body["extra_env"], "watch-extra-visible");
    assert_eq!(body["fs_enabled"], true);
    assert_eq!(body["process_enabled"], true);
    assert_eq!(body["pty_enabled"], true);
    assert_eq!(body["http_enabled"], true);
    assert_eq!(body["revision"], "before");

    fs::write(
        project_root.join("app/Controllers/CapabilityController.rco"),
        r#"
CapabilityController Controller Subclass
  [
    runtime_capabilities caps var
    map data var
    data get "process_enabled" caps get "process" at "enabled" at put! drop
    data get "http_enabled" caps get "http" at "enabled" at put! drop
    data get "revision" "after" put! drop
    data get json
  ] "show" Method
end
"#,
    )
    .expect("controller should be rewritten");

    let response = app
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("reloaded capability response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    assert_eq!(
        status,
        StatusCode::OK,
        "reloaded watched capability response should succeed, body was {}",
        String::from_utf8_lossy(&body)
    );
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");
    assert_eq!(body["process_enabled"], true);
    assert_eq!(body["http_enabled"], true);
    assert_eq!(body["revision"], "after");
}

#[tokio::test]
async fn served_mvc_keeps_environment_disabled_when_only_filesystem_is_allowed() {
    let project_root = temp_project_path();
    let fs_root = project_root.join("data");
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(&fs_root).expect("filesystem root should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "env_denied_with_fs"

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
        r#"GET "/env" EnvController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/EnvController.rco"),
        r#"
EnvController Controller Subclass
  [
    "RICOCHET_MVC_ENV_DENIED_TEST" env_get value text
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");
    std::env::set_var("RICOCHET_MVC_ENV_DENIED_TEST", "should-not-leak");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            fs_root: Some(fs_root),
            ..Default::default()
        },
    )
    .await
    .expect("build served app with bounded filesystem only");

    let response = app
        .oneshot(Request::builder().uri("/env").body(Body::empty()).unwrap())
        .await
        .expect("env denial response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "body was {body}");
    assert!(
        body.contains("environment capability is not enabled"),
        "body should explain env denial, got {body}"
    );
    assert!(
        !body.contains("should-not-leak"),
        "body should not include the environment value, got {body}"
    );
}

#[tokio::test]
async fn served_mvc_reports_process_capability_when_allowed() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "process_capability"

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
        r#"GET "/caps" CapabilityController "show" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/CapabilityController.rco"),
        r#"
CapabilityController Controller Subclass
  [
    runtime_capabilities "process" at "enabled" at json
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            allow_process: true,
            ..Default::default()
        },
    )
    .await
    .expect("build served app with process capability");

    let response = app
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "true");
}

#[tokio::test]
async fn served_mvc_reports_configured_process_root() {
    let project_root = temp_project_path();
    let process_root = project_root.join("process-root");
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::create_dir_all(&process_root).expect("process root should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "process_root"

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
        r#"GET "/root" CapabilityController "root" route"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/CapabilityController.rco"),
        r#"
CapabilityController Controller Subclass
  [
    runtime_capabilities "process" at "root" at json
  ] "root" Method
end
"#,
    )
    .expect("controller should be written");

    let expected = normalized_test_path(&process_root);
    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            allow_process: true,
            process_root: Some(process_root),
            ..Default::default()
        },
    )
    .await
    .expect("build served app with process root");

    let response = app
        .oneshot(Request::builder().uri("/root").body(Body::empty()).unwrap())
        .await
        .expect("capability response");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body.as_str(), Some(expected.as_str()));
}

#[tokio::test]
async fn served_mvc_persists_process_jobs_across_requests_when_allowed() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "process_jobs"

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
        r#"
GET "/start" ProcessController "start" route
GET "/jobs" ProcessController "jobs" route
GET "/caps" ProcessController "caps" route
"#,
    )
    .expect("routes should be written");

    #[cfg(windows)]
    let (command, args): (&str, &[&str]) = ("cmd", &["/C", "echo", "hello"]);
    #[cfg(not(windows))]
    let (command, args): (&str, &[&str]) = ("printf", &["hello"]);

    let mut arg_lines = String::new();
    for arg in args {
        arg_lines.push_str(&format!(
            "    args get \"{}\" push! drop\n",
            escape_ricochet_string_for_test(arg)
        ));
    }
    let command = escape_ricochet_string_for_test(command);
    fs::write(
        project_root.join("app/Controllers/ProcessController.rco"),
        format!(
            r#"
ProcessController Controller Subclass
  [
    args array
{arg_lines}    options map
    options get "timeout_ms" 10000 put! drop
    "{command}" args get options get process_start value "id" at json
  ] "start" Method

  [
    process_jobs count json
  ] "jobs" Method

  [
    runtime_capabilities "process" at "jobs" at json
  ] "caps" Method
end
"#
        ),
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions {
            allow_process: true,
            ..Default::default()
        },
    )
    .await
    .expect("build served app with process capability");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("start response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "0");

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/jobs").body(Body::empty()).unwrap())
        .await
        .expect("jobs response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "1");

    let response = app
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "1");
}

#[tokio::test]
async fn served_mvc_shares_approval_records_across_requests() {
    let project_root = temp_project_path();
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        r#"
[package]
name = "approval_registry"

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
        r#"
GET "/create" ApprovalController "create" route
GET "/claim" ApprovalController "claim" route
GET "/second" ApprovalController "secondClaim" route
GET "/caps" ApprovalController "caps" route
"#,
    )
    .expect("routes should be written");
    fs::write(
        project_root.join("app/Controllers/ApprovalController.rco"),
        r#"
ApprovalController Controller Subclass
  [
    operation map
    operation get "capability" "workspace.write" put! drop
    options map
    options get "id" "mvc-approval" put! drop
    options get "token" "secret-token" put! drop
    operation get options get approval_create value "pending" at json
  ] "create" Method

  [
    "mvc-approval" "secret-token" approval_claim value "claimed" at json
  ] "claim" Method

  [
    "mvc-approval" "secret-token" approval_claim error "kind" at json
  ] "secondClaim" Method

  [
    runtime_capabilities "approval" at "records" at json
  ] "caps" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_served_app_from_dir(
        &project_root,
        false,
        false,
        &ricochet_web::server::ServeOptions::default(),
    )
    .await
    .expect("build served app with approval registry");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/create")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("create response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "true");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/claim")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("claim response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "true");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/second")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("second claim response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, r#""ApprovalAlreadyClaimed""#);

    let response = app
        .oneshot(Request::builder().uri("/caps").body(Body::empty()).unwrap())
        .await
        .expect("capability response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(status, StatusCode::OK, "body was {body}");
    assert_eq!(body, "1");
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
LoginController Controller Subclass
  [
    "/dashboard" redirect
  ] "create" Method
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
PingController Controller Subclass
  [
    "pong" text
    201 status
    "x-ricochet" "yes" header
  ] "index" Method
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

HomeController Controller Subclass
  [
    greeting text
  ] "index" Method
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
HomeController Controller Subclass
  [
    "before" text
  ] "index" Method
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
HomeController Controller Subclass
  [
    "after" text
  ] "index" Method
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
SearchController Controller Subclass
  [
    ctx get "query" at "q" at text
  ] "index" Method
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
ContactController Controller Subclass
  ( email ) [
    email var
    email get text
  ] "create" Method
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
ContextController Controller Subclass
  ( request cookies config ) [
    config var
    cookies var
    request var
    map
    "method" request get "method" at put!
    "path" request get "path" at put!
    "theme" cookies get "theme" at put!
    "session" cookies get "session" at put!
    "package" config get "package" at "name" at put!
    json
  ] "show" Method
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
SessionController Controller Subclass
  ( session ) [
    session var
    session get "user" at nil? if
      session get "user" "Ada" put! drop
    end
    session get "user" at text
  ] "show" Method
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
        set_cookie.starts_with("ricochet_session=v1%3A"),
        "default session cookie should be signed, got {set_cookie}"
    );
    assert!(
        set_cookie.contains("HttpOnly"),
        "set-cookie was {set_cookie}"
    );
    assert!(set_cookie.contains("Secure"), "set-cookie was {set_cookie}");
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
async fn default_session_rejects_forged_raw_json_cookie() {
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
SessionController Controller Subclass
  ( session ) [
    session var
    session get "user" at nil? if
      session get "user" "Ada" put! drop
      "new" text
    else
      session get "user" at text
    end
  ] "show" Method
end
"#,
    )
    .expect("controller should be written");

    let app = ricochet_web::server::build_app_from_dir(&project_root).expect("build app");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/session")
                .header(
                    "cookie",
                    "ricochet_session=%7B%22user%22%3A%22Mallory%22%7D",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("forged raw cookie should be replaced")
        .to_str()
        .expect("set-cookie should be UTF-8");
    assert!(
        set_cookie.starts_with("ricochet_session=v1%3A"),
        "replacement cookie should be signed, got {set_cookie}"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let body = std::str::from_utf8(&body).expect("body should be UTF-8");
    assert_eq!(body, "new");
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
SessionController Controller Subclass
  ( session ) [
    session var
    session get "user" at nil? if
      session get "user" "Ada" put! drop
      "new" text
    else
      session get "user" at text
    end
  ] "show" Method
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
    assert!(set_cookie.contains("Secure"), "set-cookie was {set_cookie}");
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
async fn serves_encrypted_session_cookie_when_secret_env_is_configured() {
    let project_root = temp_project_path();
    let secret_env = "RICOCHET_TEST_ENCRYPTED_SESSION_SECRET";
    std::env::set_var(secret_env, "test-session-encryption-secret");
    fs::create_dir_all(project_root.join("config")).expect("config directory should be created");
    fs::create_dir_all(project_root.join("app/Controllers"))
        .expect("controller directory should be created");
    fs::write(
        project_root.join("ricochet.toml"),
        format!(
            r#"
[package]
name = "encrypted_session_context"

[web]
mode = "mvc"
routes = "config/routes.rco"

[web.views]
escape = "html"

[web.session]
encryption_secret_env = "{secret_env}"
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
SessionController Controller Subclass
  ( session ) [
    session var
    session get "user" at nil? if
      session get "user" "Ada" put! drop
      "new" text
    else
      session get "user" at text
    end
  ] "show" Method
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
        set_cookie.starts_with("ricochet_session=v2%3A"),
        "set-cookie was {set_cookie}"
    );
    assert!(set_cookie.contains("Secure"), "set-cookie was {set_cookie}");
    let readable_cookie = set_cookie.replace("%3A", ":");
    assert!(
        !readable_cookie.contains("Ada") && !readable_cookie.contains("%7B"),
        "encrypted cookie should not expose raw JSON, got {set_cookie}"
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
        "unchanged encrypted session should not rewrite the cookie"
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
        "tampered encrypted session should be replaced"
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
AiController Controller Subclass
  ( ai ) [
    ai var
    "Say hello" ai get chat result var
    result get ok? if
      result get value "text" at text
    else
      result get error "message" at text
    end
  ] "index" Method
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
LogController Controller Subclass
  ( logger ) [
    logger var
    "loaded" logger get info drop
    "careful" logger get warn drop
    logger get entries json
  ] "index" Method
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

fn normalized_test_path(path: &std::path::Path) -> String {
    let path = fs::canonicalize(path).expect("test path should canonicalize");
    strip_verbatim_prefix_for_test(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn strip_verbatim_prefix_for_test(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{stripped}"))
    } else if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix_for_test(path: PathBuf) -> PathBuf {
    path
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

fn escape_ricochet_string_for_test(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
