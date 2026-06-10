pub mod manifest;
pub mod router;
pub mod server;
pub mod template;

pub use manifest::{Database, DatabaseDefault, Manifest, Package, Views, Web};
pub use router::{parse_routes, Route};
pub use server::serve_current_dir;
pub use template::{render_template, EscapeMode};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
