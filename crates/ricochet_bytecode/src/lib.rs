pub mod chunk;
pub mod debug;
pub mod op;

pub use chunk::Chunk;
pub use debug::SourceSpan;
pub use op::Op;

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
