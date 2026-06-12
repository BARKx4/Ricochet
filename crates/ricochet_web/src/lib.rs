pub mod active_record;
pub mod controller;
pub mod database_capability;
pub mod manifest;
pub mod revision;
pub mod router;
pub mod server;
pub mod template;

pub use active_record::{ActiveRecordError, ModelMapping, OrderPage, PostgresDatabase};
pub use controller::{ActionResult, ControllerRegistry, RequestContext};
pub use database_capability::{install_database_capability, DatabaseBackend};
pub use manifest::{Database, DatabaseDefault, Manifest, Package, Session, Views, Web};
pub use revision::{AppRevision, RevisionManager};
pub use router::{parse_routes, Route};
pub use server::{
    build_app_from_dir_with_database, build_test_app, build_watched_app_from_dir,
    build_watched_app_from_dir_with_database, build_watched_app_from_dir_with_database_and_trace,
    build_watched_app_from_dir_with_trace, routes_from_dir, serve_current_dir, ServeOptions,
    WatchTraceEvent, WatchTraceSink,
};
pub use template::{render_template, EscapeMode};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
