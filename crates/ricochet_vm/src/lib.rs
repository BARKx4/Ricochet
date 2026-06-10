pub mod debug;
pub mod result;
pub mod value;
pub mod vm;

pub use debug::DebugEvent;
pub use result::{RicochetError, RicochetResult};
pub use value::{TruthinessError, Value};
pub use vm::{Vm, VmError};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
