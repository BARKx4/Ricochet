pub mod active_record;
pub mod ai_capability;
pub mod controller;
pub mod database_capability;
pub mod manifest;
pub mod revision;
pub mod router;
pub mod server;
pub mod template;

pub use active_record::{
    ActiveRecordError, ModelMapping, MysqlDatabase, OrderPage, PostgresDatabase, SqliteDatabase,
};
pub use ai_capability::{install_ai_capability, AiProvider, AiProviderConfig};
pub use controller::{ActionResult, ControllerRegistry, RequestContext};
pub use database_capability::{install_database_capability, DatabaseBackend};
pub use manifest::{
    Ai, AiDefault, Database, DatabaseDefault, Manifest, Package, Session, StaticFiles, Views, Web,
};
pub use revision::{AppRevision, RevisionManager};
pub use router::{parse_routes, Route};
pub use server::{
    build_app_from_dir_with_database, build_app_from_dir_with_options_and_request_fault_sink,
    build_served_app_from_dir, build_test_app, build_watched_app_from_dir,
    build_watched_app_from_dir_with_database, build_watched_app_from_dir_with_database_and_trace,
    build_watched_app_from_dir_with_options_and_request_fault_sink,
    build_watched_app_from_dir_with_trace, install_project_database_runtime, routes_from_dir,
    serve_app_on_listener, serve_current_dir, RequestFaultPause, RequestFaultSink,
    RequestFaultStage, ServeOptions, WatchTraceEvent, WatchTraceSink,
};
pub use template::{render_template, EscapeMode};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
