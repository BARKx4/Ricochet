pub mod active_record;
pub mod controller;
pub mod manifest;
pub mod revision;
pub mod router;
pub mod server;
pub mod template;

pub use active_record::{ActiveRecordError, ModelMapping};
pub use controller::{ActionResult, ControllerRegistry, RequestContext};
pub use manifest::{Database, DatabaseDefault, Manifest, Package, Views, Web};
pub use revision::{AppRevision, RevisionManager};
pub use router::{parse_routes, Route};
pub use server::{build_test_app, serve_current_dir};
pub use template::{render_template, EscapeMode};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
