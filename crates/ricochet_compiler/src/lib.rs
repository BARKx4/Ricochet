pub mod compiler;
pub mod imports;

pub use compiler::{compile_source, CompileError};
pub use imports::compile_file_with_imports;

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
