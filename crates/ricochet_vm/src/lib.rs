pub mod approval_runtime;
mod builtins;
pub mod capability;
pub mod class;
pub mod collection;
pub mod debug;
pub mod http_stream_runtime;
pub mod image;
pub mod object;
pub mod process_runtime;
pub mod pty_runtime;
pub mod regex_value;
pub mod result;
mod runtime_state;
pub mod socket_runtime;
pub mod strictness;
pub mod upload_runtime;
pub mod value;
pub mod vm;
pub mod word_registry;

pub use approval_runtime::ApprovalRegistry;
pub use capability::Capability;
pub use class::{BytecodeCallable, Class, NativeMethod};
pub use collection::{ArrayValue, ListValue, MapValue, SetValue};
pub use debug::{DebugAction, DebugEvent, DebugPause, DebugPauseReason, DebugTask, DebugTaskFrame};
pub use http_stream_runtime::HttpStreamRegistry;
pub use image::{ImageError, ImageValue, VmImage, VM_IMAGE_FORMAT, VM_IMAGE_FORMAT_VERSION};
pub use object::Instance;
pub use process_runtime::ProcessRegistry;
pub use pty_runtime::PtyRegistry;
pub use regex_value::RegexValue;
pub use result::{RicochetError, RicochetResult};
pub use socket_runtime::{
    TcpListenerRegistry, TcpSocketRegistry, WebSocketListenerRegistry, WebSocketRegistry,
};
pub use strictness::{StrictnessConfig, StrictnessDiagnostic, StrictnessDiagnosticKind};
pub use upload_runtime::{UploadStreamMetadata, UploadStreamRegistry};
pub use value::{TruthinessError, Value};
pub use vm::DebugControl;
pub use vm::{DynamicModuleSource, Vm, VmError};
pub use word_registry::{
    builtin_word, builtin_words, CapabilityRequirement, WordCategory, WordMetadata,
};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
