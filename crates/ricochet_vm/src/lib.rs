pub mod class;
pub mod debug;
pub mod object;
pub mod result;
pub mod value;
pub mod vm;

pub use class::{BytecodeCallable, Class, NativeMethod};
pub use debug::DebugEvent;
pub use object::Instance;
pub use result::{RicochetError, RicochetResult};
pub use value::{TruthinessError, Value};
pub use vm::{Vm, VmError};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
