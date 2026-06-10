pub mod compiler;

pub use compiler::{compile_source, CompileError};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
