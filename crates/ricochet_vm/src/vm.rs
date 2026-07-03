use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use ricochet_bytecode::{ArgsSpec, Chunk, Op, SourceSpan};
use thiserror::Error;

use crate::approval_runtime::ApprovalRegistry;
use crate::capability::Capability;
use crate::class::{BytecodeCallable, Class, NativeMethod};
use crate::collection::{ArrayValue, ListValue, MapValue, SetValue};
use crate::debug::{
    DebugAction, DebugEvent, DebugPause, DebugPauseReason, DebugTask, DebugTaskFrame,
};
use crate::http_stream_runtime::HttpStreamRegistry;
use crate::image::{
    class_to_image, value_from_image, value_to_image, ImageClass, ImageError, VmImage,
};
use crate::object::Instance;
use crate::process_runtime::ProcessRegistry;
use crate::pty_runtime::PtyRegistry;
use crate::result::RicochetResult;
use crate::socket_runtime::{
    TcpListenerRegistry, TcpSocketRegistry, WebSocketListenerRegistry, WebSocketRegistry,
};
use crate::upload_runtime::UploadStreamRegistry;
use crate::value::Value;

const DEFAULT_MAX_RUNNING_TASKS: usize = 64;
static GLOBAL_RUNNING_TASKS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    #[error("stack underflow in {word}: needed {needed}, available {available}")]
    StackUnderflow {
        word: String,
        needed: usize,
        available: usize,
    },
    #[error("unknown word: {0}")]
    UnknownWord(String),
    #[error("unsupported opcode: {0}")]
    UnsupportedOpcode(String),
    #[error("arithmetic overflow in {word}")]
    ArithmeticOverflow { word: String },
    #[error("division by zero in {word}")]
    DivisionByZero { word: String },
    #[error("index {index} is out of bounds in {word}; length is {length}")]
    IndexOutOfBounds {
        word: String,
        index: usize,
        length: usize,
    },
    #[error("invalid argument in {word}: {message}")]
    InvalidArgument { word: String, message: String },
    #[error("host operation failed in {word}: {message}")]
    HostError { word: String, message: String },
    #[error("process exit requested with status {code}")]
    ExitRequested { code: i32 },
    #[error("type error in {word}: expected {expected}, got {actual}")]
    TypeError {
        word: String,
        expected: String,
        actual: String,
    },
    #[error("no current class for {0}")]
    NoCurrentClass(String),
    #[error("unknown class: {0}")]
    UnknownClass(String),
    #[error("unknown method {method} on class {class_name}")]
    UnknownMethod { class_name: String, method: String },
    #[error("inheritance cycle involving class: {0}")]
    InheritanceCycle(String),
    #[error("invalid block index {index}: chunk has {available} blocks")]
    InvalidBlock { index: usize, available: usize },
    #[error("no current self for {0}")]
    NoCurrentSelf(String),
    #[error("unknown variable: {0}")]
    UnknownVariable(String),
    #[error("result values require ok? before they can be used as conditions")]
    UncheckedResultCondition,
    #[error("cannot use {word} on {actual} result; expected {expected} result")]
    ResultUnwrap {
        word: String,
        expected: String,
        actual: String,
    },
    #[error("invalid jump target {target}: chunk has {available} instructions")]
    InvalidJump { target: usize, available: usize },
    #[error("assert_equals failed: expected {expected}, got {actual}")]
    AssertionFailed { expected: String, actual: String },
    #[error("execution aborted in {frame} at {location}")]
    ExecutionAborted { frame: String, location: String },
    #[error("instruction limit exceeded after {limit} instructions")]
    InstructionLimitExceeded { limit: u64 },
    #[error("unknown task: {0}")]
    UnknownTask(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeErrorSite {
    pub frame: String,
    pub span: SourceSpan,
    pub opcode: String,
}

type DebugSink = Rc<RefCell<dyn FnMut(&DebugEvent)>>;
type DebugController = Rc<RefCell<dyn FnMut(&DebugPause) -> DebugAction>>;
type DebugControlController =
    Rc<RefCell<dyn for<'a> FnMut(&DebugPause, &mut DebugControl<'a>) -> DebugAction>>;
type InputReader = Rc<RefCell<dyn FnMut() -> Result<Option<String>, String>>>;
type DynamicModuleLoader = Arc<dyn Fn(&str) -> Result<DynamicModuleSource, String> + Send + Sync>;

pub struct DebugControl<'a> {
    vm: &'a mut Vm,
    default_file: String,
}

impl DebugControl<'_> {
    pub fn add_line_breakpoint(&mut self, file: Option<&str>, line: usize) {
        let file = file.unwrap_or(&self.default_file).to_string();
        self.vm.add_line_breakpoint(file, line);
    }

    pub fn remove_line_breakpoint(&mut self, file: Option<&str>, line: usize) -> bool {
        let file = file.unwrap_or(&self.default_file).to_string();
        self.vm.remove_line_breakpoint(file, line)
    }

    pub fn clear_line_breakpoints(&mut self, file: Option<&str>) -> usize {
        self.vm.clear_line_breakpoints(file)
    }

    pub fn line_breakpoints(&self) -> Vec<(String, usize)> {
        self.vm.line_breakpoints()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicModuleSource {
    pub specifier: String,
    pub module_id: String,
    pub path: Option<PathBuf>,
    pub chunk: Chunk,
}

impl DynamicModuleSource {
    pub fn new(
        specifier: impl Into<String>,
        module_id: impl Into<String>,
        path: Option<PathBuf>,
        chunk: Chunk,
    ) -> Self {
        Self {
            specifier: specifier.into(),
            module_id: module_id.into(),
            path,
            chunk,
        }
    }
}

#[derive(Clone, Default)]
struct LanguageState {
    stack: Vec<Value>,
    variables: BTreeMap<String, Value>,
    local_variables: Vec<BTreeMap<String, Value>>,
    last_call_variables: BTreeMap<String, Value>,
    functions: BTreeMap<String, BytecodeCallable>,
    classes: BTreeMap<String, Class>,
    current_class: Option<String>,
    self_stack: Vec<Value>,
}

#[derive(Clone)]
struct DynamicModuleState {
    specifier: String,
    module_id: String,
    path: Option<PathBuf>,
    language: LanguageState,
}

#[derive(Clone, Default)]
struct TaskDebugSnapshot {
    operation: String,
    fault: Option<String>,
    frames: Vec<DebugTaskFrame>,
}

#[derive(Clone)]
pub struct Vm {
    pub(super) stack: Vec<Value>,
    variables: BTreeMap<String, Value>,
    local_variables: Vec<BTreeMap<String, Value>>,
    last_call_variables: BTreeMap<String, Value>,
    functions: BTreeMap<String, BytecodeCallable>,
    classes: BTreeMap<String, Class>,
    current_class: Option<String>,
    self_stack: Vec<Value>,
    dynamic_module_loader: Option<DynamicModuleLoader>,
    dynamic_modules: BTreeMap<String, DynamicModuleState>,
    dynamic_module_aliases: BTreeMap<String, String>,
    dynamic_modules_loading: BTreeSet<String>,
    pub(super) output_lines: Vec<String>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) program_args: Vec<String>,
    pub(super) input_reader: Option<InputReader>,
    filesystem_enabled: bool,
    filesystem_root: Option<PathBuf>,
    filesystem_writes_enabled: bool,
    http_enabled: bool,
    http_allowed_hosts: Option<BTreeSet<String>>,
    http_stream_registry: HttpStreamRegistry,
    upload_stream_registry: UploadStreamRegistry,
    socket_enabled: bool,
    socket_allowed_hosts: Option<BTreeSet<String>>,
    tcp_socket_registry: TcpSocketRegistry,
    tcp_listener_registry: TcpListenerRegistry,
    websocket_registry: WebSocketRegistry,
    websocket_listener_registry: WebSocketListenerRegistry,
    process_enabled: bool,
    process_root: Option<PathBuf>,
    process_registry: ProcessRegistry,
    pty_enabled: bool,
    pty_registry: PtyRegistry,
    approval_registry: ApprovalRegistry,
    terminal_enabled: bool,
    webview_enabled: bool,
    pub(super) environment_enabled: bool,
    pub(super) environment_allowed_names: Option<BTreeSet<String>>,
    pub(super) sleep_enabled: bool,
    max_running_tasks: usize,
    debug_enabled: bool,
    debug_events: Vec<DebugEvent>,
    last_error_site: Option<RuntimeErrorSite>,
    debug_sink: Option<DebugSink>,
    debug_controller: Option<DebugController>,
    debug_control_controller: Option<DebugControlController>,
    step_mode: bool,
    step_over_depth: Option<usize>,
    step_out_depth: Option<usize>,
    debug_frame_depth: usize,
    task_debug_snapshot: Option<Arc<Mutex<TaskDebugSnapshot>>>,
    task_debug_frames: Vec<DebugTaskFrame>,
    breakpoints: BTreeSet<(String, usize)>,
    suppressed_breakpoint: Option<(String, String, usize)>,
    instruction_limit: Option<u64>,
    instructions_executed: u64,
    tasks: BTreeMap<u64, TaskState>,
    next_task_id: u64,
}

impl Default for Vm {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            variables: BTreeMap::new(),
            local_variables: Vec::new(),
            last_call_variables: BTreeMap::new(),
            functions: BTreeMap::new(),
            classes: BTreeMap::new(),
            current_class: None,
            self_stack: Vec::new(),
            dynamic_module_loader: None,
            dynamic_modules: BTreeMap::new(),
            dynamic_module_aliases: BTreeMap::new(),
            dynamic_modules_loading: BTreeSet::new(),
            output_lines: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            program_args: Vec::new(),
            input_reader: None,
            filesystem_enabled: false,
            filesystem_root: None,
            filesystem_writes_enabled: false,
            http_enabled: false,
            http_allowed_hosts: None,
            http_stream_registry: HttpStreamRegistry::default(),
            upload_stream_registry: UploadStreamRegistry::default(),
            socket_enabled: false,
            socket_allowed_hosts: None,
            tcp_socket_registry: TcpSocketRegistry::default(),
            tcp_listener_registry: TcpListenerRegistry::default(),
            websocket_registry: WebSocketRegistry::default(),
            websocket_listener_registry: WebSocketListenerRegistry::default(),
            process_enabled: false,
            process_root: None,
            process_registry: ProcessRegistry::default(),
            pty_enabled: false,
            pty_registry: PtyRegistry::default(),
            approval_registry: ApprovalRegistry::default(),
            terminal_enabled: false,
            webview_enabled: false,
            environment_enabled: false,
            environment_allowed_names: None,
            sleep_enabled: false,
            max_running_tasks: DEFAULT_MAX_RUNNING_TASKS,
            debug_enabled: false,
            debug_events: Vec::new(),
            last_error_site: None,
            debug_sink: None,
            debug_controller: None,
            debug_control_controller: None,
            step_mode: false,
            step_over_depth: None,
            step_out_depth: None,
            debug_frame_depth: 0,
            task_debug_snapshot: None,
            task_debug_frames: Vec::new(),
            breakpoints: BTreeSet::new(),
            suppressed_breakpoint: None,
            instruction_limit: None,
            instructions_executed: 0,
            tasks: BTreeMap::new(),
            next_task_id: 0,
        }
    }
}

#[derive(Clone)]
struct Task {
    block: Chunk,
    variables: BTreeMap<String, Value>,
    local_variables: Vec<BTreeMap<String, Value>>,
    functions: BTreeMap<String, BytecodeCallable>,
    classes: BTreeMap<String, Class>,
    current_class: Option<String>,
    self_stack: Vec<Value>,
    dynamic_module_loader: Option<DynamicModuleLoader>,
    dynamic_modules: BTreeMap<String, DynamicModuleState>,
    dynamic_module_aliases: BTreeMap<String, String>,
    dynamic_modules_loading: BTreeSet<String>,
    program_args: Vec<String>,
    filesystem_enabled: bool,
    filesystem_root: Option<PathBuf>,
    filesystem_writes_enabled: bool,
    http_enabled: bool,
    http_allowed_hosts: Option<BTreeSet<String>>,
    http_stream_registry: HttpStreamRegistry,
    upload_stream_registry: UploadStreamRegistry,
    socket_enabled: bool,
    socket_allowed_hosts: Option<BTreeSet<String>>,
    tcp_socket_registry: TcpSocketRegistry,
    tcp_listener_registry: TcpListenerRegistry,
    websocket_registry: WebSocketRegistry,
    websocket_listener_registry: WebSocketListenerRegistry,
    process_enabled: bool,
    process_root: Option<PathBuf>,
    process_registry: ProcessRegistry,
    pty_enabled: bool,
    pty_registry: PtyRegistry,
    approval_registry: ApprovalRegistry,
    terminal_enabled: bool,
    webview_enabled: bool,
    environment_enabled: bool,
    environment_allowed_names: Option<BTreeSet<String>>,
    sleep_enabled: bool,
    instruction_limit: Option<u64>,
}

#[derive(Clone)]
enum TaskState {
    Running(RunningTask),
    Finished(Box<TaskCompletion>),
}

#[derive(Clone)]
struct RunningTask {
    shared: Arc<RunningTaskShared>,
}

struct RunningTaskShared {
    completion: Mutex<Option<TaskCompletion>>,
    snapshot: Arc<Mutex<TaskDebugSnapshot>>,
    ready: Condvar,
}

struct TaskPermit;

impl Drop for TaskPermit {
    fn drop(&mut self) {
        GLOBAL_RUNNING_TASKS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn acquire_task_permit(word: &str, limit: usize) -> Result<TaskPermit, VmError> {
    let mut current = GLOBAL_RUNNING_TASKS.load(Ordering::Acquire);
    loop {
        if current >= limit {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: format!("task limit exceeded: at most {limit} tasks may run at once"),
            });
        }
        match GLOBAL_RUNNING_TASKS.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Ok(TaskPermit),
            Err(actual) => current = actual,
        }
    }
}

#[derive(Clone)]
struct TaskCompletion {
    result: Result<Value, VmError>,
    output: TaskOutput,
    output_consumed: bool,
    snapshot: TaskDebugSnapshot,
}

#[derive(Clone, Default)]
struct TaskOutput {
    output_lines: Vec<String>,
    stdout: String,
    stderr: String,
}

impl RunningTask {
    fn spawn(task: Task, permit: TaskPermit) -> Self {
        Self::spawn_completion("spawn".to_string(), permit, move |snapshot| {
            run_task_to_completion(task, Some(snapshot))
        })
    }

    fn spawn_value(
        word: String,
        permit: TaskPermit,
        operation: impl FnOnce() -> Value + Send + 'static,
    ) -> Self {
        Self::spawn_completion(word, permit, move |snapshot| TaskCompletion {
            result: Ok(operation()),
            output: TaskOutput::default(),
            output_consumed: false,
            snapshot: snapshot
                .lock()
                .expect("task snapshot lock poisoned")
                .clone(),
        })
    }

    fn spawn_completion(
        panic_word: String,
        permit: TaskPermit,
        operation: impl FnOnce(Arc<Mutex<TaskDebugSnapshot>>) -> TaskCompletion + Send + 'static,
    ) -> Self {
        let snapshot = Arc::new(Mutex::new(TaskDebugSnapshot {
            operation: panic_word.clone(),
            fault: None,
            frames: Vec::new(),
        }));
        let shared = Arc::new(RunningTaskShared {
            completion: Mutex::new(None),
            snapshot,
            ready: Condvar::new(),
        });
        let worker_shared = shared.clone();
        thread::spawn(move || {
            let _permit = permit;
            let worker_snapshot = worker_shared.snapshot.clone();
            let completion =
                match catch_unwind(AssertUnwindSafe(move || operation(worker_snapshot))) {
                    Ok(completion) => completion,
                    Err(_) => TaskCompletion {
                        result: Err(VmError::HostError {
                            word: panic_word,
                            message: "task worker thread panicked".to_string(),
                        }),
                        output: TaskOutput::default(),
                        output_consumed: false,
                        snapshot: worker_shared
                            .snapshot
                            .lock()
                            .expect("task snapshot lock poisoned")
                            .clone(),
                    },
                };
            let mut slot = worker_shared
                .completion
                .lock()
                .expect("task completion lock poisoned");
            *slot = Some(completion);
            worker_shared.ready.notify_all();
        });

        Self { shared }
    }

    fn status(&self) -> &'static str {
        let completion = self
            .shared
            .completion
            .lock()
            .expect("task completion lock poisoned");
        match completion.as_ref().map(|completion| &completion.result) {
            Some(Ok(_)) => "completed",
            Some(Err(_)) => "failed",
            None => "running",
        }
    }

    fn is_running(&self) -> bool {
        self.shared
            .completion
            .lock()
            .expect("task completion lock poisoned")
            .is_none()
    }

    fn is_completed(&self) -> bool {
        matches!(self.status(), "completed")
    }

    fn is_failed(&self) -> bool {
        matches!(self.status(), "failed")
    }

    fn debug_snapshot(&self) -> TaskDebugSnapshot {
        self.shared
            .snapshot
            .lock()
            .expect("task snapshot lock poisoned")
            .clone()
    }

    fn wait(&self) -> TaskCompletion {
        let mut completion = self
            .shared
            .completion
            .lock()
            .expect("task completion lock poisoned");
        loop {
            if let Some(completion) = completion.as_ref() {
                return completion.clone();
            }
            completion = self
                .shared
                .ready
                .wait(completion)
                .expect("task completion lock poisoned");
        }
    }
}

impl TaskCompletion {
    fn status(&self) -> &'static str {
        match &self.result {
            Ok(_) => "completed",
            Err(_) => "failed",
        }
    }
}

fn ensure_no_retained_resources(kind: &'static str, count: usize) -> Result<(), ImageError> {
    if count == 0 {
        Ok(())
    } else {
        Err(ImageError::RetainedResource { kind, count })
    }
}

fn image_restore_error(error: VmError) -> ImageError {
    ImageError::InvalidImage {
        message: error.to_string(),
    }
}

fn run_task_to_completion(
    task: Task,
    task_debug_snapshot: Option<Arc<Mutex<TaskDebugSnapshot>>>,
) -> TaskCompletion {
    let mut task_vm = Vm {
        variables: task.variables,
        local_variables: task.local_variables,
        last_call_variables: BTreeMap::new(),
        functions: task.functions,
        classes: task.classes,
        current_class: task.current_class,
        self_stack: task.self_stack,
        dynamic_module_loader: task.dynamic_module_loader,
        dynamic_modules: task.dynamic_modules,
        dynamic_module_aliases: task.dynamic_module_aliases,
        dynamic_modules_loading: task.dynamic_modules_loading,
        program_args: task.program_args,
        filesystem_enabled: task.filesystem_enabled,
        filesystem_root: task.filesystem_root,
        filesystem_writes_enabled: task.filesystem_writes_enabled,
        http_enabled: task.http_enabled,
        http_allowed_hosts: task.http_allowed_hosts,
        http_stream_registry: task.http_stream_registry,
        upload_stream_registry: task.upload_stream_registry,
        socket_enabled: task.socket_enabled,
        socket_allowed_hosts: task.socket_allowed_hosts,
        tcp_socket_registry: task.tcp_socket_registry,
        tcp_listener_registry: task.tcp_listener_registry,
        websocket_registry: task.websocket_registry,
        websocket_listener_registry: task.websocket_listener_registry,
        process_enabled: task.process_enabled,
        process_root: task.process_root,
        process_registry: task.process_registry,
        pty_enabled: task.pty_enabled,
        pty_registry: task.pty_registry,
        approval_registry: task.approval_registry,
        terminal_enabled: task.terminal_enabled,
        webview_enabled: task.webview_enabled,
        environment_enabled: task.environment_enabled,
        environment_allowed_names: task.environment_allowed_names,
        sleep_enabled: task.sleep_enabled,
        instruction_limit: task.instruction_limit,
        task_debug_snapshot,
        ..Vm::default()
    };

    let result = task_vm.call_bytecode_block("<task>", &task.block);
    let snapshot = task_vm.task_debug_snapshot();
    TaskCompletion {
        result,
        output: TaskOutput {
            output_lines: task_vm.output_lines,
            stdout: task_vm.stdout,
            stderr: task_vm.stderr,
        },
        output_consumed: false,
        snapshot,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionSignal {
    Continue,
    Jump(usize),
    Return,
}

#[derive(Clone)]
pub(super) enum ResolvedMethod {
    Native {
        owner: String,
        method: NativeMethod,
    },
    Bytecode {
        owner: String,
        method: BytecodeCallable,
    },
}

impl Vm {
    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn push_value(&mut self, value: Value) {
        self.stack.push(value);
    }

    pub fn variables(&self) -> &BTreeMap<String, Value> {
        &self.variables
    }

    pub fn to_image(&self) -> Result<VmImage, ImageError> {
        self.ensure_image_has_no_retained_resources()?;

        Ok(VmImage {
            format: crate::image::VM_IMAGE_FORMAT.to_string(),
            format_version: crate::image::VM_IMAGE_FORMAT_VERSION,
            ricochet_version: crate::crate_version().to_string(),
            stack: self
                .stack
                .iter()
                .enumerate()
                .map(|(index, value)| value_to_image(value, &format!("stack[{index}]")))
                .collect::<Result<Vec<_>, _>>()?,
            variables: self
                .variables
                .iter()
                .map(|(name, value)| {
                    value_to_image(value, &format!("variable {name}"))
                        .map(|value| (name.clone(), value))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            functions: self
                .functions
                .iter()
                .map(|(name, callable)| (name.clone(), callable.into()))
                .collect(),
            classes: self
                .classes
                .iter()
                .map(|(name, class)| class_to_image(class).map(|class| (name.clone(), class)))
                .collect::<Result<BTreeMap<_, _>, _>>()?,
        })
    }

    pub fn restore_image(&mut self, image: VmImage) -> Result<(), ImageError> {
        image.validate_format()?;
        self.ensure_image_has_no_retained_resources()?;

        let stack = image
            .stack
            .into_iter()
            .map(value_from_image)
            .collect::<Result<Vec<_>, _>>()?;
        let variables = image
            .variables
            .into_iter()
            .map(|(name, value)| value_from_image(value).map(|value| (name, value)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let functions = image
            .functions
            .into_iter()
            .map(|(name, callable)| (name, callable.into()))
            .collect::<BTreeMap<_, _>>();
        let classes = image.classes.into_values().collect::<Vec<_>>();

        self.stack = stack;
        self.variables = variables;
        self.local_variables.clear();
        self.last_call_variables.clear();
        self.functions = functions;
        self.classes.clear();
        self.current_class = None;
        self.self_stack.clear();
        self.dynamic_modules.clear();
        self.dynamic_module_aliases.clear();
        self.dynamic_modules_loading.clear();

        for class in classes {
            self.restore_image_class(class)?;
        }
        self.current_class = None;

        Ok(())
    }

    fn restore_image_class(&mut self, class: ImageClass) -> Result<(), ImageError> {
        let accessors = class.accessors.into_iter().collect::<BTreeSet<_>>();
        self.define_class(class.name.clone(), class.superclass)
            .map_err(image_restore_error)?;
        for field in class.fields {
            if accessors.contains(&field) {
                self.add_accessor(field).map_err(image_restore_error)?;
            } else {
                self.add_field(field).map_err(image_restore_error)?;
            }
        }
        if let Some(table_name) = class.table_name {
            self.current_class_mut("restore_image")
                .map_err(image_restore_error)?
                .set_table(table_name);
        }
        for (method_name, method) in class.bytecode_methods {
            self.add_bytecode_method(method_name, method.chunk, method.args)
                .map_err(image_restore_error)?;
        }
        self.end_class();
        Ok(())
    }

    fn ensure_image_has_no_retained_resources(&self) -> Result<(), ImageError> {
        ensure_no_retained_resources("task", self.tasks.len())?;
        ensure_no_retained_resources("HTTP stream", self.http_stream_registry.len())?;
        ensure_no_retained_resources("upload stream", self.upload_stream_registry.len())?;
        ensure_no_retained_resources("TCP socket", self.tcp_socket_registry.len())?;
        ensure_no_retained_resources("TCP listener", self.tcp_listener_registry.len())?;
        ensure_no_retained_resources("WebSocket", self.websocket_registry.len())?;
        ensure_no_retained_resources("WebSocket listener", self.websocket_listener_registry.len())?;
        ensure_no_retained_resources("process", self.process_registry.len())?;
        ensure_no_retained_resources("PTY", self.pty_registry.len())?;
        ensure_no_retained_resources("approval", self.approval_registry.len())?;
        ensure_no_retained_resources("dynamic module", self.dynamic_modules.len())?;
        Ok(())
    }

    pub fn last_call_variables(&self) -> &BTreeMap<String, Value> {
        &self.last_call_variables
    }

    pub fn variable(&self, name: &str) -> Option<&Value> {
        self.lookup_variable(name)
    }

    pub fn output_lines(&self) -> &[String] {
        &self.output_lines
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn set_program_args(&mut self, args: impl IntoIterator<Item = String>) {
        self.program_args = args.into_iter().collect();
    }

    pub fn set_input_reader<F>(&mut self, reader: F)
    where
        F: FnMut() -> Result<Option<String>, String> + 'static,
    {
        self.input_reader = Some(Rc::new(RefCell::new(reader)));
    }

    pub fn set_dynamic_module_loader<F>(&mut self, loader: F)
    where
        F: Fn(&str) -> Result<DynamicModuleSource, String> + Send + Sync + 'static,
    {
        self.dynamic_module_loader = Some(Arc::new(loader));
    }

    pub fn clear_dynamic_module_loader(&mut self) {
        self.dynamic_module_loader = None;
    }

    pub fn enable_cli_capabilities(&mut self) {
        self.filesystem_enabled = true;
        self.filesystem_writes_enabled = true;
        self.http_enabled = true;
        self.socket_enabled = true;
        self.process_enabled = true;
        self.pty_enabled = true;
        self.terminal_enabled = true;
        self.webview_enabled = true;
        self.environment_enabled = true;
        self.sleep_enabled = true;
    }

    pub fn set_host_capabilities(&mut self, filesystem_enabled: bool, http_enabled: bool) {
        self.filesystem_enabled = filesystem_enabled;
        self.filesystem_writes_enabled = filesystem_enabled;
        self.http_enabled = http_enabled;
    }

    pub fn set_http_stream_registry(&mut self, registry: HttpStreamRegistry) {
        self.http_stream_registry = registry;
    }

    pub fn set_upload_stream_registry(&mut self, registry: UploadStreamRegistry) {
        self.upload_stream_registry = registry;
    }

    pub fn set_socket_enabled(&mut self, enabled: bool) {
        self.socket_enabled = enabled;
    }

    pub fn set_socket_allowed_hosts(&mut self, hosts: impl IntoIterator<Item = String>) {
        self.socket_allowed_hosts = Some(
            hosts
                .into_iter()
                .map(|host| host.to_ascii_lowercase())
                .collect(),
        );
    }

    pub fn clear_socket_allowed_hosts(&mut self) {
        self.socket_allowed_hosts = None;
    }

    pub fn set_tcp_socket_registry(&mut self, registry: TcpSocketRegistry) {
        self.tcp_socket_registry = registry;
    }

    pub fn set_tcp_listener_registry(&mut self, registry: TcpListenerRegistry) {
        self.tcp_listener_registry = registry;
    }

    pub fn set_websocket_registry(&mut self, registry: WebSocketRegistry) {
        self.websocket_registry = registry;
    }

    pub fn set_websocket_listener_registry(&mut self, registry: WebSocketListenerRegistry) {
        self.websocket_listener_registry = registry;
    }

    pub fn set_process_enabled(&mut self, enabled: bool) {
        self.process_enabled = enabled;
    }

    pub fn set_process_root(&mut self, root: impl Into<PathBuf>) {
        self.process_root = Some(normalize_path(&strip_verbatim_prefix(root.into())));
    }

    pub fn set_process_registry(&mut self, registry: ProcessRegistry) {
        self.process_registry = registry;
    }

    pub fn set_pty_enabled(&mut self, enabled: bool) {
        self.pty_enabled = enabled;
    }

    pub fn set_pty_registry(&mut self, registry: PtyRegistry) {
        self.pty_registry = registry;
    }

    pub fn set_approval_registry(&mut self, registry: ApprovalRegistry) {
        self.approval_registry = registry;
    }

    pub fn set_terminal_enabled(&mut self, enabled: bool) {
        self.terminal_enabled = enabled;
    }

    pub fn set_environment_enabled(&mut self, enabled: bool) {
        self.environment_enabled = enabled;
    }

    pub fn set_environment_allowed_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.environment_allowed_names = Some(names.into_iter().collect());
    }

    pub fn clear_environment_allowed_names(&mut self) {
        self.environment_allowed_names = None;
    }

    pub fn set_sleep_enabled(&mut self, enabled: bool) {
        self.sleep_enabled = enabled;
    }

    pub fn set_max_running_tasks(&mut self, limit: usize) {
        self.max_running_tasks = limit;
    }

    pub fn set_filesystem_root(&mut self, root: impl Into<PathBuf>) {
        self.filesystem_root = Some(normalize_path(&strip_verbatim_prefix(root.into())));
    }

    pub fn set_filesystem_writes_enabled(&mut self, enabled: bool) {
        self.filesystem_writes_enabled = enabled;
    }

    pub fn set_http_allowed_hosts(&mut self, hosts: impl IntoIterator<Item = String>) {
        self.http_allowed_hosts = Some(
            hosts
                .into_iter()
                .map(|host| host.to_ascii_lowercase())
                .collect(),
        );
    }

    pub fn set_webview_enabled(&mut self, enabled: bool) {
        self.webview_enabled = enabled;
    }

    pub fn set_instruction_limit(&mut self, limit: u64) {
        self.instruction_limit = Some(limit);
        self.instructions_executed = 0;
    }

    pub fn clear_instruction_limit(&mut self) {
        self.instruction_limit = None;
        self.instructions_executed = 0;
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    fn lookup_variable(&self, name: &str) -> Option<&Value> {
        for frame in self.local_variables.iter().rev() {
            if let Some(value) = frame.get(name) {
                return Some(value);
            }
        }
        self.variables.get(name)
    }

    fn declare_variable(&mut self, name: String, value: Value) {
        if let Some(frame) = self.local_variables.last_mut() {
            frame.insert(name, value);
        } else {
            self.variables.entry(name).or_insert(value);
        }
    }

    fn set_existing_variable(&mut self, name: &str, value: Value) -> bool {
        for frame in self.local_variables.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return true;
            }
        }

        if self.variables.contains_key(name) {
            self.variables.insert(name.to_string(), value);
            return true;
        }

        false
    }

    fn push_variable_frame(&mut self) {
        self.local_variables.push(BTreeMap::new());
    }

    fn pop_variable_frame(&mut self) -> BTreeMap<String, Value> {
        self.local_variables
            .pop()
            .expect("variable frame stack should not underflow")
    }

    pub fn add_function(
        &mut self,
        name: impl Into<String>,
        function: Chunk,
        args: Option<ArgsSpec>,
    ) {
        self.functions
            .insert(name.into(), BytecodeCallable::new(function, args));
    }

    pub fn function_args(&self, name: &str) -> Option<&ArgsSpec> {
        self.functions
            .get(name)
            .and_then(|function| function.args.as_ref())
    }

    pub fn method_args(&self, class_name: &str, method_name: &str) -> Option<&ArgsSpec> {
        self.classes
            .get(class_name)
            .and_then(|class| class.bytecode_methods.get(method_name))
            .and_then(|method| method.args.as_ref())
    }

    pub fn class_table(&self, class_name: &str) -> Option<&str> {
        self.classes
            .get(class_name)
            .and_then(|class| class.table_name.as_deref())
    }

    pub fn class_fields(&self, class_name: &str) -> Option<&[String]> {
        self.classes
            .get(class_name)
            .map(|class| class.fields.as_slice())
    }

    pub fn test_methods(&self) -> Vec<(String, String)> {
        let mut tests = Vec::new();
        for class in self.classes.values() {
            if class.superclass != "TestCase" {
                continue;
            }

            for method in class.bytecode_methods.keys() {
                if method.starts_with("test") {
                    tests.push((class.name.clone(), method.clone()));
                }
            }
        }
        tests
    }

    pub fn enable_debug(&mut self) {
        self.debug_enabled = true;
    }

    pub fn enable_step_debugging(&mut self) {
        self.debug_enabled = true;
        self.step_mode = true;
    }

    pub fn set_debug_sink<F>(&mut self, sink: F)
    where
        F: FnMut(&DebugEvent) + 'static,
    {
        self.debug_sink = Some(Rc::new(RefCell::new(sink)));
    }

    pub fn set_debug_controller<F>(&mut self, controller: F)
    where
        F: FnMut(&DebugPause) -> DebugAction + 'static,
    {
        self.debug_controller = Some(Rc::new(RefCell::new(controller)));
        self.debug_control_controller = None;
    }

    pub fn set_debug_controller_with_control<F>(&mut self, controller: F)
    where
        F: for<'a> FnMut(&DebugPause, &mut DebugControl<'a>) -> DebugAction + 'static,
    {
        self.debug_controller = None;
        self.debug_control_controller = Some(Rc::new(RefCell::new(controller)));
    }

    pub fn debug_events(&self) -> &[DebugEvent] {
        &self.debug_events
    }

    pub fn last_error_site(&self) -> Option<&RuntimeErrorSite> {
        self.last_error_site.as_ref()
    }

    pub fn clear_debug_events(&mut self) {
        self.debug_events.clear();
    }

    pub fn add_line_breakpoint(&mut self, file: impl Into<String>, line: usize) {
        self.debug_enabled = true;
        self.breakpoints.insert((file.into(), line));
    }

    pub fn remove_line_breakpoint(&mut self, file: impl Into<String>, line: usize) -> bool {
        let file = file.into();
        self.breakpoints.remove(&(file, line))
    }

    pub fn clear_line_breakpoints(&mut self, file: Option<&str>) -> usize {
        let before = self.breakpoints.len();
        if let Some(file) = file {
            self.breakpoints
                .retain(|(breakpoint_file, _)| breakpoint_file != file);
        } else {
            self.breakpoints.clear();
        }
        before - self.breakpoints.len()
    }

    pub fn line_breakpoints(&self) -> Vec<(String, usize)> {
        self.breakpoints.iter().cloned().collect()
    }

    pub fn define_class(
        &mut self,
        name: impl Into<String>,
        superclass: impl Into<String>,
    ) -> Result<(), VmError> {
        let name = name.into();
        let superclass = superclass.into();

        if let Some(class) = self.classes.get_mut(&name) {
            class.superclass = superclass;
        } else {
            self.classes
                .insert(name.clone(), Class::new(name.clone(), superclass));
        }
        self.current_class = Some(name);

        Ok(())
    }

    pub fn end_class(&mut self) {
        self.current_class = None;
    }

    pub fn add_field(&mut self, name: impl Into<String>) -> Result<(), VmError> {
        self.current_class_mut("add_field")?.add_field(name);
        Ok(())
    }

    pub fn add_accessor(&mut self, name: impl Into<String>) -> Result<(), VmError> {
        let field = name.into();
        self.add_field(field.clone())?;

        let getter_name = format!("{field}.get");
        let getter_field = field.clone();
        self.add_native_method(getter_name, move |arguments| {
            let receiver = arguments.last().ok_or_else(|| VmError::StackUnderflow {
                word: format!("{}.get", getter_field),
                needed: 1,
                available: 0,
            })?;
            accessor_get(&getter_field, receiver)
        })?;

        let setter_name = format!("{field}.set");
        let setter_field = field;
        self.add_native_method_with_arity(setter_name, 1, move |arguments| {
            let value = arguments
                .first()
                .cloned()
                .ok_or_else(|| VmError::StackUnderflow {
                    word: format!("{}.set", setter_field),
                    needed: 2,
                    available: arguments.len(),
                })?;
            let receiver = arguments
                .get(1)
                .cloned()
                .ok_or_else(|| VmError::StackUnderflow {
                    word: format!("{}.set", setter_field),
                    needed: 2,
                    available: arguments.len(),
                })?;
            accessor_set(&setter_field, receiver, value)
        })?;

        Ok(())
    }

    pub fn add_native_method<F>(
        &mut self,
        name: impl Into<String>,
        method: F,
    ) -> Result<(), VmError>
    where
        F: Fn(Vec<Value>) -> Result<Value, VmError> + Send + Sync + 'static,
    {
        self.add_native_method_with_arity(name, 0, method)
    }

    pub fn add_native_method_with_arity<F>(
        &mut self,
        name: impl Into<String>,
        input_count: usize,
        method: F,
    ) -> Result<(), VmError>
    where
        F: Fn(Vec<Value>) -> Result<Value, VmError> + Send + Sync + 'static,
    {
        let method = NativeMethod::new(input_count, method);
        self.current_class_mut("add_native_method")?
            .add_native_method(name, method);
        Ok(())
    }

    pub fn add_bytecode_method(
        &mut self,
        name: impl Into<String>,
        method: Chunk,
        args: Option<ArgsSpec>,
    ) -> Result<(), VmError> {
        self.current_class_mut("add_bytecode_method")?
            .add_bytecode_method(name, method, args);
        Ok(())
    }

    pub fn new_instance(&self, class_name: &str) -> Result<Value, VmError> {
        let classes = self.inheritance_chain(class_name)?;
        let mut fields = BTreeMap::new();
        for class in classes.iter().rev() {
            for field in &class.fields {
                fields.insert(field.clone(), Value::Nil);
            }
        }

        Ok(Value::Instance(Instance::new(class_name, fields)))
    }

    pub fn set_field(&self, instance: Value, field: &str, value: Value) -> Result<Value, VmError> {
        match instance {
            Value::Instance(mut instance) => {
                instance.fields.insert(field.to_string(), value);
                Ok(Value::Instance(instance))
            }
            value => Err(VmError::TypeError {
                word: format!("set_field {field}"),
                expected: "instance".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    pub fn get_field(&self, instance: &Value, field: &str) -> Result<Value, VmError> {
        match instance {
            Value::Instance(instance) => {
                Ok(instance.fields.get(field).cloned().unwrap_or(Value::Nil))
            }
            Value::Map(map) => Ok(map.get(field).unwrap_or(Value::Nil)),
            value => Err(VmError::TypeError {
                word: format!("get_field {field}"),
                expected: "instance or map".to_string(),
                actual: value_kind(value).to_string(),
            }),
        }
    }

    pub fn call_method_value(
        &mut self,
        receiver: Value,
        method_name: &str,
    ) -> Result<Value, VmError> {
        if self.builtin_method_exists(&receiver, method_name) {
            return self.call_builtin_method(receiver, method_name);
        }

        match receiver {
            Value::Class(class_name) => {
                let (owner, method) = self
                    .resolve_native_method(&class_name, method_name)?
                    .ok_or_else(|| VmError::UnknownMethod {
                        class_name: class_name.clone(),
                        method: method_name.to_string(),
                    })?;
                let frame = format!("{owner}.{method_name}");
                self.call_native_method(Value::Class(class_name), &frame, &method)
            }
            Value::Instance(instance) => {
                let class_name = instance.class_name.clone();
                let receiver = Value::Instance(instance);
                match self.resolve_instance_method(&class_name, method_name)? {
                    Some(ResolvedMethod::Native { owner, method }) => {
                        let frame = format!("{owner}.{method_name}");
                        self.call_native_method(receiver, &frame, &method)
                    }
                    Some(ResolvedMethod::Bytecode { owner, method }) => {
                        let frame = format!("{owner}.{method_name}");
                        let input_count = method
                            .args
                            .as_ref()
                            .map(|args| args.inputs.len())
                            .unwrap_or(0);
                        self.call_bytecode_method(receiver, &frame, &method.chunk, input_count)
                    }
                    None => Err(VmError::UnknownMethod {
                        class_name,
                        method: method_name.to_string(),
                    }),
                }
            }
            value => Err(VmError::TypeError {
                word: format!("call_method {method_name}"),
                expected: "class or instance".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    pub fn call_method_value_with_args(
        &mut self,
        receiver: Value,
        method_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        let stack_before = self.stack.clone();
        self.stack.extend(args);

        match self.call_method_value(receiver, method_name) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    pub fn run_chunk(&mut self, chunk: &Chunk) -> Result<(), VmError> {
        self.suppressed_breakpoint = None;
        self.last_error_site = None;
        self.instructions_executed = 0;
        self.run_chunk_with_frame(chunk, "<main>", false)
            .map(|_| ())
    }

    fn run_chunk_with_frame(
        &mut self,
        chunk: &Chunk,
        frame: &str,
        allow_return: bool,
    ) -> Result<ExecutionSignal, VmError> {
        let previous_depth = self.debug_frame_depth;
        let task_frame_index = self.task_debug_frames.len();
        if self.task_debug_snapshot.is_some() {
            self.task_debug_frames.push(DebugTaskFrame {
                frame: frame.to_string(),
                source: "<pending>".to_string(),
                opcode: "<pending>".to_string(),
                stack: self.stack.clone(),
                locals: self
                    .local_variables
                    .last()
                    .map(debug_variables)
                    .unwrap_or_default(),
                current_self: self.self_stack.last().cloned(),
            });
            self.publish_task_debug_snapshot();
        }
        self.debug_frame_depth = previous_depth + 1;
        let result = self.run_chunk_with_frame_inner(chunk, frame, allow_return);
        self.debug_frame_depth = previous_depth;
        if self.task_debug_snapshot.is_some() && result.is_ok() {
            self.task_debug_frames.truncate(task_frame_index);
            self.publish_task_debug_snapshot();
        }
        result
    }

    fn run_chunk_with_frame_inner(
        &mut self,
        chunk: &Chunk,
        frame: &str,
        allow_return: bool,
    ) -> Result<ExecutionSignal, VmError> {
        let mut ip = 0;
        while ip < chunk.instructions.len() {
            self.consume_instruction_budget()?;
            let instruction = &chunk.instructions[ip];
            self.update_task_debug_frame(frame, instruction);
            self.pause_before_instruction(frame, instruction)?;
            let stack_before = self.debug_enabled.then(|| self.stack.clone());
            let source = self.debug_enabled.then(|| source_label(&instruction.span));
            let opcode = self.debug_enabled.then(|| format!("{:?}", &instruction.op));

            let result = self.execute_instruction(&instruction.op, chunk, allow_return);

            if let (Some(stack_before), Some(source), Some(opcode)) = (stack_before, source, opcode)
            {
                self.record_debug_event(DebugEvent::Instruction {
                    frame: frame.to_string(),
                    source,
                    opcode,
                    stack_before,
                    stack_after: self.stack.clone(),
                });
            }

            match result {
                Ok(ExecutionSignal::Continue) => ip += 1,
                Ok(ExecutionSignal::Jump(target)) => ip = target,
                Ok(ExecutionSignal::Return) => return Ok(ExecutionSignal::Return),
                Err(error) => {
                    if self.last_error_site.is_none() {
                        self.last_error_site = Some(RuntimeErrorSite {
                            frame: frame.to_string(),
                            span: instruction.span.clone(),
                            opcode: format!("{:?}", &instruction.op),
                        });
                    }
                    if self.debug_enabled {
                        self.record_debug_event(DebugEvent::Fault {
                            frame: frame.to_string(),
                            message: error.to_string(),
                            stack: self.stack.clone(),
                        });
                    }
                    self.record_task_debug_fault(&error);
                    return Err(error);
                }
            }
        }

        Ok(ExecutionSignal::Continue)
    }

    fn consume_instruction_budget(&mut self) -> Result<(), VmError> {
        if let Some(limit) = self.instruction_limit {
            if self.instructions_executed >= limit {
                return Err(VmError::InstructionLimitExceeded { limit });
            }
        }
        self.instructions_executed += 1;
        Ok(())
    }

    fn pause_before_instruction(
        &mut self,
        frame: &str,
        instruction: &ricochet_bytecode::Instruction,
    ) -> Result<(), VmError> {
        let breakpoint_site = (
            frame.to_string(),
            instruction.span.file.clone(),
            instruction.span.line,
        );
        if self
            .suppressed_breakpoint
            .as_ref()
            .is_some_and(|suppressed| suppressed != &breakpoint_site)
        {
            self.suppressed_breakpoint = None;
        }
        let breakpoint_hit = self
            .breakpoints
            .contains(&(instruction.span.file.clone(), instruction.span.line))
            && self.suppressed_breakpoint.as_ref() != Some(&breakpoint_site);
        let step_over_hit = self
            .step_over_depth
            .is_some_and(|target| self.debug_frame_depth <= target);
        let step_out_hit = self
            .step_out_depth
            .is_some_and(|target| self.debug_frame_depth < target);
        let step_hit = self.step_mode || step_over_hit || step_out_hit;
        if !step_hit && !breakpoint_hit {
            return Ok(());
        }
        if step_over_hit {
            self.step_over_depth = None;
        }
        if step_out_hit {
            self.step_out_depth = None;
        }

        let pause = DebugPause {
            reason: if breakpoint_hit {
                DebugPauseReason::Breakpoint
            } else {
                DebugPauseReason::Step
            },
            frame: frame.to_string(),
            source: source_label(&instruction.span),
            opcode: format!("{:?}", instruction.op),
            stack: self.stack.clone(),
            globals: debug_variables(&self.variables),
            locals: self
                .local_variables
                .last()
                .map(debug_variables)
                .unwrap_or_default(),
            current_self: self.self_stack.last().cloned(),
            tasks: self.debug_tasks(),
        };
        self.record_debug_event(DebugEvent::Paused(pause.clone()));

        let action = if let Some(controller) = self.debug_control_controller.clone() {
            let mut control = DebugControl {
                vm: self,
                default_file: instruction.span.file.clone(),
            };
            (controller.borrow_mut())(&pause, &mut control)
        } else {
            self.debug_controller
                .clone()
                .map(|controller| (controller.borrow_mut())(&pause))
                .unwrap_or(DebugAction::Continue)
        };

        match action {
            DebugAction::Step => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = true;
                self.step_over_depth = None;
                self.step_out_depth = None;
                Ok(())
            }
            DebugAction::StepOver => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = false;
                self.step_over_depth = Some(self.debug_frame_depth);
                self.step_out_depth = None;
                Ok(())
            }
            DebugAction::StepOut => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = false;
                self.step_over_depth = None;
                self.step_out_depth = Some(self.debug_frame_depth);
                Ok(())
            }
            DebugAction::Continue => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = false;
                self.step_over_depth = None;
                self.step_out_depth = None;
                Ok(())
            }
            DebugAction::Abort => Err(VmError::ExecutionAborted {
                frame: pause.frame,
                location: pause.source,
            }),
        }
    }

    fn record_debug_event(&mut self, event: DebugEvent) {
        if let Some(sink) = self.debug_sink.clone() {
            (sink.borrow_mut())(&event);
        }
        self.debug_events.push(event);
    }

    fn task_debug_snapshot(&self) -> TaskDebugSnapshot {
        self.task_debug_snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .lock()
                    .expect("task snapshot lock poisoned")
                    .clone()
            })
            .unwrap_or_default()
    }

    fn update_task_debug_frame(
        &mut self,
        frame: &str,
        instruction: &ricochet_bytecode::Instruction,
    ) {
        if self.task_debug_snapshot.is_none() {
            return;
        }

        let task_frame = DebugTaskFrame {
            frame: frame.to_string(),
            source: source_label(&instruction.span),
            opcode: format!("{:?}", &instruction.op),
            stack: self.stack.clone(),
            locals: self
                .local_variables
                .last()
                .map(debug_variables)
                .unwrap_or_default(),
            current_self: self.self_stack.last().cloned(),
        };
        if let Some(current_frame) = self.task_debug_frames.last_mut() {
            *current_frame = task_frame;
        } else {
            self.task_debug_frames.push(task_frame);
        }
        self.publish_task_debug_snapshot();
    }

    fn record_task_debug_fault(&mut self, error: &VmError) {
        let Some(snapshot) = self.task_debug_snapshot.as_ref() else {
            return;
        };
        let mut snapshot = snapshot.lock().expect("task snapshot lock poisoned");
        snapshot.frames = self.task_debug_frames.clone();
        snapshot.fault = Some(error.to_string());
    }

    fn publish_task_debug_snapshot(&self) {
        let Some(snapshot) = self.task_debug_snapshot.as_ref() else {
            return;
        };
        let mut snapshot = snapshot.lock().expect("task snapshot lock poisoned");
        snapshot.frames = self.task_debug_frames.clone();
    }

    fn debug_tasks(&self) -> Vec<DebugTask> {
        self.tasks
            .iter()
            .map(|(task_id, task)| {
                let snapshot = match task {
                    TaskState::Running(task) => task.debug_snapshot(),
                    TaskState::Finished(completion) => completion.snapshot.clone(),
                };
                DebugTask {
                    id: *task_id,
                    operation: snapshot.operation,
                    status: self.task_status(*task_id).to_string(),
                    pending: self.task_pending(*task_id),
                    running: self.task_running(*task_id),
                    completed: self.task_completed(*task_id),
                    failed: self.task_failed(*task_id),
                    fault: snapshot.fault,
                    frames: snapshot.frames,
                }
            })
            .collect()
    }

    fn execute_instruction(
        &mut self,
        op: &Op,
        chunk: &Chunk,
        allow_return: bool,
    ) -> Result<ExecutionSignal, VmError> {
        match op {
            Op::PushNil => self.stack.push(Value::Nil),
            Op::PushBool(value) => self.stack.push(Value::Bool(*value)),
            Op::PushNumber(value) => self.stack.push(Value::Number(*value)),
            Op::PushFloat(value) => self.stack.push(Value::Float(*value)),
            Op::PushString(value) => self.stack.push(Value::String(value.clone())),
            Op::PushBlock(block) => {
                let block = chunk
                    .blocks
                    .get(*block)
                    .cloned()
                    .ok_or(VmError::InvalidBlock {
                        index: *block,
                        available: chunk.blocks.len(),
                    })?;
                self.stack.push(Value::Block(block));
            }
            Op::CallMethod(name) => self.call_method_or_member(name)?,
            Op::CallWord(word) => self.call_word(word)?,
            Op::BeginClass { name, superclass } => {
                self.define_class(name.clone(), superclass.clone())?
            }
            Op::EndClass => self.end_class(),
            Op::AddField(name) => self.add_field(name.clone())?,
            Op::AddAccessor(name) => self.add_accessor(name.clone())?,
            Op::AddMethod { name, block, args } => {
                let method = chunk
                    .blocks
                    .get(*block)
                    .cloned()
                    .ok_or(VmError::InvalidBlock {
                        index: *block,
                        available: chunk.blocks.len(),
                    })?;
                self.add_bytecode_method(name.clone(), method, args.clone())?;
            }
            Op::AddFunction { name, block, args } => {
                let function = chunk
                    .blocks
                    .get(*block)
                    .cloned()
                    .ok_or(VmError::InvalidBlock {
                        index: *block,
                        available: chunk.blocks.len(),
                    })?;
                self.add_function(name.clone(), function, args.clone());
            }
            Op::Return if allow_return => return Ok(ExecutionSignal::Return),
            Op::JumpIfFalse(target) => {
                self.validate_jump(*target, chunk)?;
                let stack_before = self.stack.clone();
                let condition = self.pop("if")?;
                match condition.truthy_for_condition() {
                    Ok(false) => return Ok(ExecutionSignal::Jump(*target)),
                    Ok(true) => {}
                    Err(_) => {
                        self.stack = stack_before;
                        return Err(VmError::UncheckedResultCondition);
                    }
                }
            }
            Op::Jump(target) => {
                self.validate_jump(*target, chunk)?;
                return Ok(ExecutionSignal::Jump(*target));
            }
            Op::Pop => {
                self.pop("pop")?;
            }
            op => return Err(VmError::UnsupportedOpcode(format!("{op:?}"))),
        }

        Ok(ExecutionSignal::Continue)
    }

    fn call_word(&mut self, word: &str) -> Result<(), VmError> {
        match word {
            "+" | "add" => self.call_add(word),
            "-" | "subtract" => self.call_subtract(word),
            "*" | "multiply" => self.call_multiply(word),
            "/" | "divide" => self.call_divide(word),
            "%" | "modulo" => self.call_modulo(word),
            "negate" => self.call_negate(word),
            "abs" => self.call_abs(word),
            "min" => self.call_min(word),
            "max" => self.call_max(word),
            "clamp" => self.call_clamp(word),
            "not" => self.call_not(word),
            "and" => self.call_boolean_binary(word, |left, right| left && right),
            "or" => self.call_boolean_binary(word, |left, right| left || right),
            "equals" | "=" => self.call_equals(word),
            "not_equals?" | "!=" => self.call_not_equals(word),
            "assert" => self.call_assert(word),
            "assert_true" => self.call_assert_true(word),
            "assert_false" => self.call_assert_false(word),
            "assert_ok" => self.call_assert_ok(word),
            "assert_error" => self.call_assert_error(word),
            "assert_equals" => self.call_assert_equals(word),
            "less_than?" | "<" => self.call_number_comparison(word, |left, right| left < right),
            "greater_than?" | ">" => self.call_number_comparison(word, |left, right| left > right),
            "less_or_equals?" | "<=" => {
                self.call_number_comparison(word, |left, right| left <= right)
            }
            "greater_or_equals?" | ">=" => {
                self.call_number_comparison(word, |left, right| left >= right)
            }
            "self" => self.call_self(word),
            "get" => self.call_get(word),
            "set" => self.call_set(word),
            "var" => self.call_var(word),
            "Field" => self.call_field_declaration(word),
            "Accessor" => self.call_accessor_declaration(word),
            "Table" => self.call_table(word),
            "Subclass" => self.call_subclass(word),
            "Method" => self.call_install_current_class_method(word),
            "new" => self.call_new(word),
            "swap" => self.call_swap(word),
            "dup" => self.call_dup(word),
            "drop" => self.call_drop(word),
            "over" => self.call_over(word),
            "rot" => self.call_rot(word),
            "nip" => self.call_nip(word),
            "tuck" => self.call_tuck(word),
            "pick" => self.call_pick(word),
            "roll" => self.call_roll(word),
            "depth" => {
                self.stack.push(Value::Number(self.stack.len() as i64));
                Ok(())
            }
            "clear" => {
                self.stack.clear();
                Ok(())
            }
            "call" => self.call_block(word),
            "spawn" => self.call_spawn(word),
            "await" => self.call_await(word),
            "await_all" => self.call_await_all(word),
            "release_task" => self.call_release_task(word),
            "tasks" => self.call_tasks(word),
            "import_dynamic" => self.call_import_dynamic(word),
            "module_call" => self.call_module_call(word),
            "module_get" => self.call_module_get(word),
            "send" => self.call_send(word),
            "at" => self.call_receiver_argument_method(word, "at"),
            "has?" | "contains?" => self.call_receiver_argument_method(word, word),
            "take" | "skip" | "repeat" | "split" | "join" | "concat" | "index_of"
            | "last_index_of" | "starts_with?" | "ends_with?" | "remove" | "remove_at" => {
                self.call_receiver_argument_method(word, word)
            }
            "slice" | "replace" | "regex_replace" | "insert_at" => {
                self.call_receiver_two_argument_method(word, word)
            }
            "push" => self.call_push(word),
            "put" => self.call_put(word),
            "fs_read_text" => {
                self.call_capability_method_word(word, Capability::FileSystem, "read_text")
            }
            "fs_write_text" => {
                self.call_capability_method_word(word, Capability::FileSystem, "write_text")
            }
            "fs_exists?" => {
                self.call_capability_method_word(word, Capability::FileSystem, "exists?")
            }
            "fs_list" => self.call_capability_method_word(word, Capability::FileSystem, "list"),
            "fs_create_dir" => {
                self.call_capability_method_word(word, Capability::FileSystem, "create_dir")
            }
            "fs_delete" => self.call_capability_method_word(word, Capability::FileSystem, "delete"),
            "workspace_resolve" => self.call_workspace_resolve(word),
            "workspace_contains?" => self.call_workspace_contains(word),
            "workspace_metadata" => self.call_workspace_metadata(word),
            "workspace_list" => self.call_workspace_list(word),
            "workspace_read_text" => self.call_workspace_read_text(word),
            "workspace_write_text" => self.call_workspace_write_text(word),
            "workspace_mkdir" => self.call_workspace_mkdir(word),
            "workspace_delete" => self.call_workspace_delete(word),
            "workspace_copy" => self.call_workspace_copy(word),
            "workspace_move" => self.call_workspace_move(word),
            "config_get" => self.call_config_get(word),
            "secret_env" => self.call_secret_env(word),
            "secret_literal" => self.call_secret_literal(word),
            "secret_resolve" => self.call_secret_resolve(word),
            "password_hash" => self.call_password_hash(word),
            "password_verify" => self.call_password_verify(word),
            "http_request_new" => self.call_http_request_new(word),
            "http_header_put" => self.call_http_header_put(word),
            "http_bearer_auth" => self.call_http_bearer_auth(word),
            "http_json_body" => self.call_http_json_body(word),
            "http_timeout" => self.call_http_timeout(word),
            "http_get" => self.call_capability_method_word(word, Capability::Http, "get"),
            "http_post_json" => {
                self.call_capability_method_word(word, Capability::Http, "post_json")
            }
            "http_request" => self.call_capability_method_word(word, Capability::Http, "request"),
            "http_get_task" => self.call_capability_method_word(word, Capability::Http, "get_task"),
            "http_post_json_task" => {
                self.call_capability_method_word(word, Capability::Http, "post_json_task")
            }
            "http_request_task" => {
                self.call_capability_method_word(word, Capability::Http, "request_task")
            }
            "http_stream_start" => self.call_http_stream_start(word),
            "http_streams" => self.call_http_streams(),
            "http_stream" => self.call_http_stream(word),
            "http_stream_read" => self.call_http_stream_read(word),
            "http_stream_cancel" => self.call_http_stream_cancel(word),
            "http_stream_release" => self.call_http_stream_release(word),
            "upload_streams" => self.call_upload_streams(),
            "upload_stream" => self.call_upload_stream(word),
            "upload_read" => self.call_upload_read(word),
            "upload_release" => self.call_upload_release(word),
            "tcp_listen" => self.call_tcp_listen(word),
            "tcp_listeners" => self.call_tcp_listeners(word),
            "tcp_listener" => self.call_tcp_listener(word),
            "tcp_accept" => self.call_tcp_accept(word),
            "tcp_listener_close" => self.call_tcp_listener_close(word),
            "tcp_listener_release" => self.call_tcp_listener_release(word),
            "tcp_connect" => self.call_tcp_connect(word),
            "tcp_connections" => self.call_tcp_connections(word),
            "tcp_connection" => self.call_tcp_connection(word),
            "tcp_write" => self.call_tcp_write(word),
            "tcp_read" => self.call_tcp_read(word),
            "tcp_close" => self.call_tcp_close(word),
            "tcp_release" => self.call_tcp_release(word),
            "ws_listen" => self.call_ws_listen(word),
            "ws_listeners" => self.call_ws_listeners(word),
            "ws_listener" => self.call_ws_listener(word),
            "ws_accept" => self.call_ws_accept(word),
            "ws_listener_close" => self.call_ws_listener_close(word),
            "ws_listener_release" => self.call_ws_listener_release(word),
            "ws_connect" => self.call_ws_connect(word),
            "ws_connections" => self.call_ws_connections(word),
            "ws_connection" => self.call_ws_connection(word),
            "ws_send" => self.call_ws_send(word),
            "ws_read" => self.call_ws_read(word),
            "ws_close" => self.call_ws_close(word),
            "ws_release" => self.call_ws_release(word),
            "process_env_put" => self.call_process_env_put(word),
            "process_spawn" => self.call_process_spawn(word),
            "process_spawn_task" => self.call_process_spawn_task(word),
            "process_start" => self.call_process_start(word),
            "process_jobs" => self.call_process_jobs(),
            "process_job" => self.call_process_job(word),
            "process_cancel" => self.call_process_cancel(word),
            "process_release" => self.call_process_release(word),
            "process_write" => self.call_process_write(word),
            "process_read" => self.call_process_read(word),
            "pty_start" => self.call_pty_start(word),
            "pty_write" => self.call_pty_write(word),
            "pty_read" => self.call_pty_read(word),
            "pty_resize" => self.call_pty_resize(word),
            "pty_stop" => self.call_pty_stop(word),
            "pty_release" => self.call_pty_release(word),
            "pty_list" => self.call_pty_list(),
            "pty_detail" => self.call_pty_detail(word),
            "approval_create" => self.call_approval_create(word),
            "approval_claim" => self.call_approval_claim(word),
            "approval_complete" => self.call_approval_complete(word),
            "approval_reject" => self.call_approval_reject(word),
            "approval_detail" => self.call_approval_detail(word),
            "approval_release" => self.call_approval_release(word),
            "tui_enter" => self.call_capability_method_word(word, Capability::Terminal, "enter"),
            "tui_leave" => self.call_capability_method_word(word, Capability::Terminal, "leave"),
            "tui_clear" => self.call_capability_method_word(word, Capability::Terminal, "clear"),
            "tui_move_to" => {
                self.call_capability_method_word(word, Capability::Terminal, "move_to")
            }
            "tui_write" => self.call_capability_method_word(word, Capability::Terminal, "write"),
            "tui_flush" => self.call_capability_method_word(word, Capability::Terminal, "flush"),
            "tui_size" => self.call_capability_method_word(word, Capability::Terminal, "size"),
            "tui_poll_key" => {
                self.call_capability_method_word(word, Capability::Terminal, "poll_key")
            }
            "tui_read_key" => {
                self.call_capability_method_word(word, Capability::Terminal, "read_key")
            }
            "webview_text" => self.call_capability_method_word(word, Capability::Webview, "text"),
            "webview_heading" => {
                self.call_capability_method_word(word, Capability::Webview, "heading")
            }
            "webview_button" => {
                self.call_capability_method_word(word, Capability::Webview, "button")
            }
            "webview_action" => {
                self.call_capability_method_word(word, Capability::Webview, "action")
            }
            "webview_input" => self.call_capability_method_word(word, Capability::Webview, "input"),
            "webview_link" => self.call_capability_method_word(word, Capability::Webview, "link"),
            "webview_container" => {
                self.call_capability_method_word(word, Capability::Webview, "container")
            }
            "webview_window_state" => {
                self.call_capability_method_word(word, Capability::Webview, "window_state")
            }
            "webview_window" | "webview_document" => {
                self.call_capability_method_word(word, Capability::Webview, "window")
            }
            "println" => self.call_println(word),
            "inspect" => self.call_inspect(word),
            "debug" => self.call_debug(word),
            "print" => self.call_print(word),
            "eprint" => self.call_eprint(word),
            "read_line" => self.call_read_line(word),
            "args" => self.call_args(),
            "env" | "env_get" => self.call_env(word),
            "env_set" => self.call_env_set(word),
            "cwd" => self.call_cwd(),
            "runtime_capabilities" => self.call_runtime_capabilities(),
            "now" => self.call_now(word),
            "timestamp_now" => self.call_timestamp_now(word),
            "timestamp_parse" => self.call_timestamp_parse(word),
            "timestamp_format" => self.call_timestamp_format(word),
            "timestamp_format_pattern" => self.call_timestamp_format_pattern(word),
            "timestamp_parts" => self.call_timestamp_parts(word),
            "timestamp_from_parts" => self.call_timestamp_from_parts(word),
            "timestamp_add" => self.call_timestamp_add(word),
            "timestamp_diff" => self.call_timestamp_diff(word),
            "date_from_timestamp" => self.call_date_from_timestamp(word),
            "date_to_timestamp" => self.call_date_to_timestamp(word),
            "date_parse" => self.call_date_parse(word),
            "date_format" => self.call_date_format(word),
            "date_add_days" => self.call_date_add_days(word),
            "date_diff_days" => self.call_date_diff_days(word),
            "duration_millis" => self.call_duration_unit(word, 1),
            "duration_seconds" => self.call_duration_unit(word, 1_000),
            "duration_minutes" => self.call_duration_unit(word, 60_000),
            "duration_hours" => self.call_duration_unit(word, 3_600_000),
            "duration_days" => self.call_duration_unit(word, 86_400_000),
            "duration_weeks" => self.call_duration_unit(word, 604_800_000),
            "duration_parts" => self.call_duration_parts(word),
            "sleep" => self.call_sleep(word),
            "random" => self.call_random(word),
            "exit" => self.call_exit(word),
            "fs" => {
                if self.filesystem_enabled {
                    self.stack.push(Value::Capability(Capability::FileSystem));
                    Ok(())
                } else {
                    Err(VmError::HostError {
                        word: word.to_string(),
                        message: "filesystem capability is not enabled".to_string(),
                    })
                }
            }
            "http" => {
                if self.http_enabled {
                    self.stack.push(Value::Capability(Capability::Http));
                    Ok(())
                } else {
                    Err(VmError::HostError {
                        word: word.to_string(),
                        message: "HTTP capability is not enabled".to_string(),
                    })
                }
            }
            "tui" => {
                if self.terminal_enabled {
                    self.stack.push(Value::Capability(Capability::Terminal));
                    Ok(())
                } else {
                    Err(VmError::HostError {
                        word: word.to_string(),
                        message: "terminal UI capability is not enabled".to_string(),
                    })
                }
            }
            "webview" => {
                if self.webview_enabled {
                    self.stack.push(Value::Capability(Capability::Webview));
                    Ok(())
                } else {
                    Err(VmError::HostError {
                        word: word.to_string(),
                        message: "webview capability is not enabled".to_string(),
                    })
                }
            }
            "view" => self.call_view(word),
            "text" => self.call_text(word),
            "json" => self.call_json(word),
            "redirect" => self.call_redirect(word),
            "info" if self.receiver_method_exists(word)? => self.call_top_receiver_method(word),
            "task_status" if self.receiver_method_exists(word)? => {
                self.call_top_receiver_method(word)
            }
            "status" => self.call_status(word),
            "header" => self.call_header(word),
            "value" => self.call_result_value(word),
            "error" => self.call_result_error(word),
            "ok" => self.call_ok(word),
            "fail" => self.call_fail(word),
            "result_envelope" => self.call_result_envelope(word),
            "range" => self.call_range(word),
            "regex" => self.call_regex(word),
            "to_string" => self.call_to_string(word),
            "to_number"
            | "to_integer"
            | "to_bigint"
            | "to_int"
            | "to_mediumint"
            | "to_smallint"
            | "to_tinyint"
            | "to_bit"
            | "to_unsigned_int"
            | "to_unsigned_mediumint"
            | "to_unsigned_smallint"
            | "to_unsigned_tinyint"
            | "to_unsigned_bigint"
            | "to_float"
            | "to_float32"
            | "to_float64"
            | "to_double"
            | "to_real" => self.call_numeric_conversion(word),
            "json_encode" => self.call_json_encode(word),
            "json_decode" => self.call_json_decode(word),
            "type" => self.call_type(word),
            "class_of" => self.call_class_of(word),
            "instance_of?" => self.call_instance_of(word),
            "responds_to?" => self.call_responds_to(word),
            "fields" => self.call_fields(word),
            "methods" => self.call_methods(word),
            "callable?" => self.call_callable(word),
            "Array" | "List" | "Map" | "Set" => self.call_collection_class_or_declaration(word),
            "array" => {
                self.call_collection_declaration_or_constructor(Value::Array(ArrayValue::default()))
            }
            "list" => {
                self.call_collection_declaration_or_constructor(Value::List(ListValue::default()))
            }
            "map" => {
                self.call_collection_declaration_or_constructor(Value::Map(MapValue::default()))
            }
            predicate if predicate.ends_with('?') => self.call_question_word(predicate),
            _ => self.call_function(word),
        }
    }

    fn validate_jump(&self, target: usize, chunk: &Chunk) -> Result<(), VmError> {
        if target > chunk.instructions.len() {
            return Err(VmError::InvalidJump {
                target,
                available: chunk.instructions.len(),
            });
        }
        Ok(())
    }

    fn call_function(&mut self, name: &str) -> Result<(), VmError> {
        if let Some(function) = self.functions.get(name).cloned() {
            let input_count = function
                .args
                .as_ref()
                .map(|args| args.inputs.len())
                .unwrap_or(0);
            let result = self.call_bytecode_function(name, &function.chunk, input_count)?;
            self.stack.push(result);
            return Ok(());
        }

        if self.classes.contains_key(name) {
            self.stack.push(Value::Class(name.to_string()));
            return Ok(());
        }

        if self.receiver_method_exists(name)? {
            return self.call_top_receiver_method(name);
        }

        Err(VmError::UnknownWord(name.to_string()))
    }

    fn call_question_word(&mut self, word: &str) -> Result<(), VmError> {
        if is_known_predicate(word) {
            if self.receiver_method_exists(word)? {
                return self.call_top_receiver_method(word);
            }
            return self.call_predicate(word);
        }

        match self.call_function(word) {
            Err(VmError::UnknownWord(_)) => self.call_predicate(word),
            result => result,
        }
    }

    fn call_result_value(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let result = self.pop(word)?;
        match result {
            Value::Result(RicochetResult::Ok(value)) => {
                self.stack.push(*value);
                Ok(())
            }
            Value::Result(RicochetResult::Err(_)) => {
                self.stack = stack_before;
                Err(VmError::ResultUnwrap {
                    word: word.to_string(),
                    expected: "ok".to_string(),
                    actual: "error".to_string(),
                })
            }
            value => {
                self.stack = stack_before;
                Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "result".to_string(),
                    actual: value_kind(&value).to_string(),
                })
            }
        }
    }

    fn call_result_error(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let result = self.pop(word)?;
        match result {
            Value::Result(RicochetResult::Err(error)) => {
                self.stack.push(Value::Map(MapValue::from(BTreeMap::from([
                    ("kind".to_string(), Value::String(error.kind)),
                    ("message".to_string(), Value::String(error.message)),
                ]))));
                Ok(())
            }
            Value::Result(RicochetResult::Ok(_)) => {
                self.stack = stack_before;
                Err(VmError::ResultUnwrap {
                    word: word.to_string(),
                    expected: "error".to_string(),
                    actual: "ok".to_string(),
                })
            }
            value => {
                self.stack = stack_before;
                Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "result".to_string(),
                    actual: value_kind(&value).to_string(),
                })
            }
        }
    }

    fn call_table(&mut self, word: &str) -> Result<(), VmError> {
        if self.current_class.is_none() {
            return self.call_targeted_table(word);
        }

        let stack_before = self.stack.clone();
        let table_name = match self.pop(word)? {
            Value::String(table_name) => table_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "table name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.current_class_mut(word) {
            Ok(class) => {
                class.set_table(table_name);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_targeted_table(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let table_name = match self.pop_unchecked() {
            Value::String(table_name) => table_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "table name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let class_name = match self.pop_unchecked() {
            Value::Class(class_name) | Value::String(class_name) => class_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class or class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.classes.get_mut(&class_name) {
            Some(class) => {
                class.set_table(table_name);
                Ok(())
            }
            None => {
                self.stack = stack_before;
                Err(VmError::UnknownClass(class_name))
            }
        }
    }

    fn call_field_declaration(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let field_name = match self.pop_unchecked() {
            Value::String(field_name) => field_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "field name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let class_name = match self.pop_unchecked() {
            Value::Class(class_name) | Value::String(class_name) => class_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class or class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.classes.get_mut(&class_name) {
            Some(class) => {
                class.add_field(field_name);
                Ok(())
            }
            None => {
                self.stack = stack_before;
                Err(VmError::UnknownClass(class_name))
            }
        }
    }

    fn call_accessor_declaration(&mut self, word: &str) -> Result<(), VmError> {
        if self.current_class.is_none() {
            return self.call_targeted_accessor(word);
        }

        let stack_before = self.stack.clone();
        let field_name = match self.pop(word)? {
            Value::String(field_name) => field_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "field name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.add_accessor(field_name) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_targeted_accessor(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let field_name = match self.pop_unchecked() {
            Value::String(field_name) => field_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "field name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let class_name = match self.pop_unchecked() {
            Value::Class(class_name) | Value::String(class_name) => class_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class or class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let previous_class = self.current_class.clone();
        self.current_class = Some(class_name);
        let result = self.add_accessor(field_name);
        self.current_class = previous_class;
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_subclass(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let superclass = match self.pop_unchecked() {
            Value::String(superclass) | Value::Class(superclass) => superclass,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class or class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let class_name = match self.pop_unchecked() {
            Value::String(class_name) => class_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.define_class(class_name, superclass) {
            Ok(()) => {
                self.end_class();
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_bytecode_function(
        &mut self,
        name: &str,
        function: &Chunk,
        input_count: usize,
    ) -> Result<Value, VmError> {
        self.ensure_stack(name, input_count)?;
        let base = self.stack.len() - input_count;
        self.push_variable_frame();
        let run_result = self.run_chunk_with_frame(function, name, true);
        self.last_call_variables = self.pop_variable_frame();

        match run_result {
            Ok(ExecutionSignal::Continue | ExecutionSignal::Return) => {
                let result = if self.stack.len() > base {
                    self.pop_unchecked()
                } else {
                    Value::Nil
                };
                self.stack.truncate(base);
                Ok(result)
            }
            Ok(ExecutionSignal::Jump(target)) => Err(VmError::InvalidJump {
                target,
                available: function.instructions.len(),
            }),
            Err(error) => {
                self.stack.truncate(base);
                Err(error)
            }
        }
    }

    fn call_block(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let block = match self.pop(word)? {
            Value::Block(block) => block,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "block".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.call_bytecode_block("<block>", &block) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_spawn(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let block = match self.pop(word)? {
            Value::Block(block) => block,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "block".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let (task_id, permit) = match self.reserve_task_slot(word) {
            Ok(slot) => slot,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let task = Task {
            block,
            variables: self.variables.clone(),
            local_variables: self.local_variables.clone(),
            functions: self.functions.clone(),
            classes: self.classes.clone(),
            current_class: self.current_class.clone(),
            self_stack: self.self_stack.clone(),
            dynamic_module_loader: self.dynamic_module_loader.clone(),
            dynamic_modules: self.dynamic_modules.clone(),
            dynamic_module_aliases: self.dynamic_module_aliases.clone(),
            dynamic_modules_loading: self.dynamic_modules_loading.clone(),
            program_args: self.program_args.clone(),
            filesystem_enabled: self.filesystem_enabled,
            filesystem_root: self.filesystem_root.clone(),
            filesystem_writes_enabled: self.filesystem_writes_enabled,
            http_enabled: self.http_enabled,
            http_allowed_hosts: self.http_allowed_hosts.clone(),
            http_stream_registry: self.http_stream_registry.clone(),
            upload_stream_registry: self.upload_stream_registry.clone(),
            socket_enabled: self.socket_enabled,
            socket_allowed_hosts: self.socket_allowed_hosts.clone(),
            tcp_socket_registry: self.tcp_socket_registry.clone(),
            tcp_listener_registry: self.tcp_listener_registry.clone(),
            websocket_registry: self.websocket_registry.clone(),
            websocket_listener_registry: self.websocket_listener_registry.clone(),
            process_enabled: self.process_enabled,
            process_root: self.process_root.clone(),
            process_registry: self.process_registry.clone(),
            pty_enabled: self.pty_enabled,
            pty_registry: self.pty_registry.clone(),
            approval_registry: self.approval_registry.clone(),
            terminal_enabled: self.terminal_enabled,
            webview_enabled: self.webview_enabled,
            environment_enabled: self.environment_enabled,
            environment_allowed_names: self.environment_allowed_names.clone(),
            sleep_enabled: self.sleep_enabled,
            instruction_limit: self.instruction_limit,
        };
        self.tasks.insert(
            task_id,
            TaskState::Running(RunningTask::spawn(task, permit)),
        );
        self.stack.push(Value::Task(task_id));
        Ok(())
    }

    pub(super) fn spawn_value_task(
        &mut self,
        word: &str,
        operation: impl FnOnce() -> Value + Send + 'static,
    ) -> Result<Value, VmError> {
        let (task_id, permit) = self.reserve_task_slot(word)?;
        self.tasks.insert(
            task_id,
            TaskState::Running(RunningTask::spawn_value(
                word.to_string(),
                permit,
                operation,
            )),
        );
        Ok(Value::Task(task_id))
    }

    fn reserve_task_slot(&mut self, word: &str) -> Result<(u64, TaskPermit), VmError> {
        let permit = acquire_task_permit(word, self.max_running_tasks)?;
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        Ok((task_id, permit))
    }

    fn call_await(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let task_id = match self.pop(word)? {
            Value::Task(task_id) => task_id,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "task".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        match self.resolve_task(task_id) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_await_all(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let values = match self.pop(word)? {
            Value::Array(values) => values.snapshot(),
            Value::List(values) => values.snapshot(),
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "array or list of tasks".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let mut task_ids = Vec::with_capacity(values.len());
        for value in values {
            match value {
                Value::Task(task_id) => task_ids.push(task_id),
                value => {
                    self.stack = stack_before;
                    return Err(VmError::TypeError {
                        word: word.to_string(),
                        expected: "task".to_string(),
                        actual: value_kind(&value).to_string(),
                    });
                }
            }
        }

        let mut results = Vec::with_capacity(task_ids.len());
        for task_id in task_ids {
            match self.resolve_task(task_id) {
                Ok(value) => results.push(value),
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            }
        }

        self.stack.push(Value::Array(results.into()));
        Ok(())
    }

    fn call_release_task(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let task_id = match self.pop(word)? {
            Value::Task(task_id) => task_id,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "task".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let released = match self.tasks.get(&task_id) {
            Some(TaskState::Finished(_)) => {
                self.tasks.remove(&task_id);
                true
            }
            Some(TaskState::Running(_)) => {
                self.stack = stack_before;
                return Err(VmError::HostError {
                    word: word.to_string(),
                    message: "task must be awaited before release_task".to_string(),
                });
            }
            None => false,
        };

        self.stack.push(Value::Bool(released));
        Ok(())
    }

    fn resolve_task(&mut self, task_id: u64) -> Result<Value, VmError> {
        let Some(task) = self.tasks.remove(&task_id) else {
            return Err(VmError::UnknownTask(task_id));
        };

        let mut completion = match task {
            TaskState::Running(task) => task.wait(),
            TaskState::Finished(completion) => *completion,
        };
        self.merge_task_output(&mut completion);
        let result = completion.result.clone();
        self.tasks
            .insert(task_id, TaskState::Finished(Box::new(completion)));
        result
    }

    fn merge_task_output(&mut self, completion: &mut TaskCompletion) {
        if completion.output_consumed {
            return;
        }
        self.output_lines
            .extend(completion.output.output_lines.clone());
        self.stdout.push_str(&completion.output.stdout);
        self.stderr.push_str(&completion.output.stderr);
        completion.output_consumed = true;
    }

    pub(super) fn task_status(&self, task_id: u64) -> &'static str {
        match self.tasks.get(&task_id) {
            Some(TaskState::Running(task)) => task.status(),
            Some(TaskState::Finished(completion)) => completion.status(),
            None => "consumed",
        }
    }

    pub(super) fn task_pending(&self, task_id: u64) -> bool {
        matches!(self.tasks.get(&task_id), Some(TaskState::Running(task)) if task.is_running())
    }

    pub(super) fn task_running(&self, task_id: u64) -> bool {
        self.task_pending(task_id)
    }

    pub(super) fn task_completed(&self, task_id: u64) -> bool {
        match self.tasks.get(&task_id) {
            Some(TaskState::Running(task)) => task.is_completed(),
            Some(TaskState::Finished(completion)) => completion.result.is_ok(),
            None => false,
        }
    }

    pub(super) fn task_failed(&self, task_id: u64) -> bool {
        match self.tasks.get(&task_id) {
            Some(TaskState::Running(task)) => task.is_failed(),
            Some(TaskState::Finished(completion)) => completion.result.is_err(),
            None => false,
        }
    }

    pub(super) fn pending_task_ids(&self) -> Vec<u64> {
        self.tasks
            .iter()
            .filter_map(|(task_id, task)| match task {
                TaskState::Running(task) if task.is_running() => Some(*task_id),
                TaskState::Running(_) | TaskState::Finished(_) => None,
            })
            .collect()
    }

    pub(super) fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub(super) fn running_task_count(&self) -> usize {
        self.pending_task_ids().len()
    }

    pub(super) fn filesystem_writes_enabled(&self) -> bool {
        self.filesystem_enabled && self.filesystem_writes_enabled
    }

    pub(super) fn filesystem_enabled(&self) -> bool {
        self.filesystem_enabled
    }

    pub(super) fn filesystem_root_path(&self) -> Option<&Path> {
        self.filesystem_root.as_deref()
    }

    pub(super) fn process_enabled(&self) -> bool {
        self.process_enabled
    }

    pub(super) fn http_enabled(&self) -> bool {
        self.http_enabled
    }

    pub(super) fn http_host_policy_enabled(&self) -> bool {
        self.http_allowed_hosts.is_some()
    }

    pub(super) fn socket_enabled(&self) -> bool {
        self.socket_enabled
    }

    pub(super) fn socket_host_policy_enabled(&self) -> bool {
        self.socket_allowed_hosts.is_some()
    }

    pub(super) fn process_registry(&self) -> ProcessRegistry {
        self.process_registry.clone()
    }

    pub(super) fn http_stream_registry(&self) -> HttpStreamRegistry {
        self.http_stream_registry.clone()
    }

    pub(super) fn upload_stream_registry(&self) -> UploadStreamRegistry {
        self.upload_stream_registry.clone()
    }

    pub(super) fn tcp_socket_registry(&self) -> TcpSocketRegistry {
        self.tcp_socket_registry.clone()
    }

    pub(super) fn tcp_listener_registry(&self) -> TcpListenerRegistry {
        self.tcp_listener_registry.clone()
    }

    pub(super) fn websocket_registry(&self) -> WebSocketRegistry {
        self.websocket_registry.clone()
    }

    pub(super) fn websocket_listener_registry(&self) -> WebSocketListenerRegistry {
        self.websocket_listener_registry.clone()
    }

    pub(super) fn pty_enabled(&self) -> bool {
        self.pty_enabled
    }

    pub(super) fn pty_registry(&self) -> PtyRegistry {
        self.pty_registry.clone()
    }

    pub(super) fn approval_registry(&self) -> ApprovalRegistry {
        self.approval_registry.clone()
    }

    pub(super) fn runtime_capabilities_value(&self) -> Value {
        let filesystem_root = self
            .filesystem_root
            .as_ref()
            .map(|path| Value::String(path.to_string_lossy().into_owned()))
            .unwrap_or(Value::Nil);
        let process_root = self
            .process_root
            .as_ref()
            .or(self.filesystem_root.as_ref())
            .map(|path| Value::String(path.to_string_lossy().into_owned()))
            .unwrap_or(Value::Nil);
        let http_allowed_hosts = self
            .http_allowed_hosts
            .as_ref()
            .map(|hosts| {
                Value::Array(
                    hosts
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                )
            })
            .unwrap_or(Value::Nil);
        let socket_allowed_hosts = self
            .socket_allowed_hosts
            .as_ref()
            .map(|hosts| {
                Value::Array(
                    hosts
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                )
            })
            .unwrap_or(Value::Nil);
        let environment_allowlist = self
            .environment_allowed_names
            .as_ref()
            .map(|names| {
                Value::Array(
                    names
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                )
            })
            .unwrap_or(Value::Nil);
        let task_count = i64::try_from(self.task_count()).unwrap_or(i64::MAX);
        let running_task_count = i64::try_from(self.running_task_count()).unwrap_or(i64::MAX);
        let max_running_tasks = i64::try_from(self.max_running_tasks).unwrap_or(i64::MAX);
        let global_running_tasks =
            i64::try_from(GLOBAL_RUNNING_TASKS.load(Ordering::Acquire)).unwrap_or(i64::MAX);

        Value::Map(
            BTreeMap::from([
                (
                    "tasks".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("known".to_string(), Value::Number(task_count)),
                            ("running".to_string(), Value::Number(running_task_count)),
                            ("max_running".to_string(), Value::Number(max_running_tasks)),
                            (
                                "global_running".to_string(),
                                Value::Number(global_running_tasks),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "filesystem".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.filesystem_enabled)),
                            (
                                "writes_enabled".to_string(),
                                Value::Bool(self.filesystem_writes_enabled()),
                            ),
                            ("root".to_string(), filesystem_root.clone()),
                        ])
                        .into(),
                    ),
                ),
                (
                    "http".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.http_enabled)),
                            ("allowed_hosts".to_string(), http_allowed_hosts),
                            (
                                "streams".to_string(),
                                Value::Number(self.http_stream_registry.len() as i64),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "uploads".to_string(),
                    Value::Map(
                        BTreeMap::from([(
                            "streams".to_string(),
                            Value::Number(self.upload_stream_registry.len() as i64),
                        )])
                        .into(),
                    ),
                ),
                (
                    "sockets".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.socket_enabled)),
                            ("allowed_hosts".to_string(), socket_allowed_hosts),
                            (
                                "tcp_connections".to_string(),
                                Value::Number(self.tcp_socket_registry.len() as i64),
                            ),
                            (
                                "tcp_listeners".to_string(),
                                Value::Number(self.tcp_listener_registry.len() as i64),
                            ),
                            (
                                "websocket_connections".to_string(),
                                Value::Number(self.websocket_registry.len() as i64),
                            ),
                            (
                                "websocket_listeners".to_string(),
                                Value::Number(self.websocket_listener_registry.len() as i64),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "workspace".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.filesystem_enabled)),
                            (
                                "writes_enabled".to_string(),
                                Value::Bool(self.filesystem_writes_enabled()),
                            ),
                            ("root".to_string(), filesystem_root.clone()),
                        ])
                        .into(),
                    ),
                ),
                (
                    "process".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.process_enabled)),
                            ("root".to_string(), process_root),
                            (
                                "jobs".to_string(),
                                Value::Number(self.process_registry.len() as i64),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "terminal".to_string(),
                    Value::Map(
                        BTreeMap::from([(
                            "enabled".to_string(),
                            Value::Bool(self.terminal_enabled),
                        )])
                        .into(),
                    ),
                ),
                (
                    "pty".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.pty_enabled)),
                            (
                                "sessions".to_string(),
                                Value::Number(self.pty_registry.len() as i64),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "approval".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(true)),
                            (
                                "records".to_string(),
                                Value::Number(self.approval_registry.len() as i64),
                            ),
                        ])
                        .into(),
                    ),
                ),
                (
                    "webview".to_string(),
                    Value::Map(
                        BTreeMap::from([(
                            "enabled".to_string(),
                            Value::Bool(self.webview_enabled),
                        )])
                        .into(),
                    ),
                ),
                (
                    "environment".to_string(),
                    Value::Map(
                        BTreeMap::from([
                            ("enabled".to_string(), Value::Bool(self.environment_enabled)),
                            ("allowlist".to_string(), environment_allowlist),
                        ])
                        .into(),
                    ),
                ),
                (
                    "sleep".to_string(),
                    Value::Map(
                        BTreeMap::from([("enabled".to_string(), Value::Bool(self.sleep_enabled))])
                            .into(),
                    ),
                ),
            ])
            .into(),
        )
    }

    pub(super) fn check_http_url_allowed(&self, word: &str, url: &str) -> Result<(), VmError> {
        let Some(allowed_hosts) = &self.http_allowed_hosts else {
            return Ok(());
        };

        let parsed = reqwest::Url::parse(url).map_err(|error| VmError::InvalidArgument {
            word: word.to_string(),
            message: format!("invalid HTTP URL {url:?}: {error}"),
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: format!("unsupported HTTP URL scheme: {}", parsed.scheme()),
            });
        }
        let host = parsed.host_str().ok_or_else(|| VmError::InvalidArgument {
            word: word.to_string(),
            message: format!("HTTP URL has no host: {url:?}"),
        })?;
        let host = host.to_ascii_lowercase();

        if allowed_hosts.contains(&host) {
            Ok(())
        } else {
            Err(VmError::HostError {
                word: word.to_string(),
                message: format!("HTTP host is not allowed: {host}"),
            })
        }
    }

    pub(super) fn check_socket_host_allowed(&self, word: &str, host: &str) -> Result<(), VmError> {
        let Some(allowed_hosts) = &self.socket_allowed_hosts else {
            return Ok(());
        };

        let host = host.to_ascii_lowercase();
        if allowed_hosts.contains(&host) {
            Ok(())
        } else {
            Err(VmError::HostError {
                word: word.to_string(),
                message: format!("socket host is not allowed: {host}"),
            })
        }
    }

    pub(super) fn resolve_filesystem_path(
        &self,
        word: &str,
        source: &str,
    ) -> Result<PathBuf, VmError> {
        let Some(root) = &self.filesystem_root else {
            return Ok(PathBuf::from(source));
        };

        resolve_bounded_path(word, "filesystem", root, source)
    }

    pub(super) fn resolve_process_path(
        &self,
        word: &str,
        source: &str,
    ) -> Result<PathBuf, VmError> {
        match &self.process_root {
            Some(root) => resolve_bounded_path(word, "process", root, source),
            None => self.resolve_filesystem_path(word, source),
        }
    }

    pub(super) fn resolve_process_command(
        &self,
        word: &str,
        source: &str,
    ) -> Result<String, VmError> {
        let Some(root) = &self.process_root else {
            return Ok(source.to_string());
        };
        let path = resolve_bounded_path(word, "process executable", root, source)?;
        if !path.is_file() {
            return Err(VmError::HostError {
                word: word.to_string(),
                message: format!("process executable is not a file inside process root: {source}"),
            });
        }
        Ok(path.to_string_lossy().into_owned())
    }

    fn call_bytecode_block(&mut self, frame: &str, block: &Chunk) -> Result<Value, VmError> {
        self.call_bytecode_block_with_args(frame, block, Vec::new())
    }

    pub(super) fn call_bytecode_block_with_args(
        &mut self,
        frame: &str,
        block: &Chunk,
        arguments: Vec<Value>,
    ) -> Result<Value, VmError> {
        let base = self.stack.len();
        self.stack.extend(arguments);
        let run_result = self.run_chunk_with_frame(block, frame, true);

        match run_result {
            Ok(ExecutionSignal::Continue | ExecutionSignal::Return) => {
                let result = if self.stack.len() > base {
                    self.pop_unchecked()
                } else {
                    Value::Nil
                };
                self.stack.truncate(base);
                Ok(result)
            }
            Ok(ExecutionSignal::Jump(target)) => Err(VmError::InvalidJump {
                target,
                available: block.instructions.len(),
            }),
            Err(error) => {
                self.stack.truncate(base);
                Err(error)
            }
        }
    }

    fn call_send(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let method_name = match self.pop(word)? {
            Value::String(method_name) => method_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "method name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let receiver = match self.pop(word) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        match self.call_method_value(receiver, &method_name) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_receiver_argument_method(&mut self, word: &str, method: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let argument = self.pop_unchecked();
        let receiver = self.pop_unchecked();
        self.stack.push(argument);
        match self.call_builtin_method(receiver, method) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_receiver_two_argument_method(
        &mut self,
        word: &str,
        method: &str,
    ) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let stack_before = self.stack.clone();
        let second = self.pop_unchecked();
        let first = self.pop_unchecked();
        let receiver = self.pop_unchecked();
        self.stack.push(first);
        self.stack.push(second);
        match self.call_builtin_method(receiver, method) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_capability_method_word(
        &mut self,
        word: &str,
        capability: Capability,
        method: &str,
    ) -> Result<(), VmError> {
        let capability = self.enabled_capability(word, capability)?;
        match self.call_builtin_method(capability, method) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn enabled_capability(&self, word: &str, capability: Capability) -> Result<Value, VmError> {
        let enabled = match capability {
            Capability::FileSystem => self.filesystem_enabled,
            Capability::Http => self.http_enabled,
            Capability::Process => self.process_enabled,
            Capability::Terminal => self.terminal_enabled,
            Capability::Webview => self.webview_enabled,
        };

        if enabled {
            Ok(Value::Capability(capability))
        } else {
            Err(VmError::HostError {
                word: word.to_string(),
                message: format!("{} capability is not enabled", capability_name(capability)),
            })
        }
    }

    fn call_method_or_member(&mut self, name: &str) -> Result<(), VmError> {
        let should_dispatch = match self.stack.last() {
            Some(Value::Class(class_name)) => {
                self.resolve_native_method(class_name, name)?.is_some()
            }
            Some(Value::Instance(instance)) => self
                .resolve_instance_method(&instance.class_name, name)?
                .is_some(),
            Some(value) => self.builtin_method_exists(value, name),
            None => false,
        };

        if should_dispatch {
            let stack_before = self.stack.clone();
            let receiver = self.pop_unchecked();
            match self.call_method_value(receiver, name) {
                Ok(value) => {
                    self.stack.push(value);
                    Ok(())
                }
                Err(error) => {
                    self.stack = stack_before;
                    Err(error)
                }
            }
        } else {
            self.stack.push(Value::Member(name.to_string()));
            Ok(())
        }
    }

    fn receiver_method_exists(&self, name: &str) -> Result<bool, VmError> {
        match self.stack.last() {
            Some(Value::Class(class_name)) => {
                Ok(self.resolve_native_method(class_name, name)?.is_some())
            }
            Some(Value::Instance(instance)) => Ok(self
                .resolve_instance_method(&instance.class_name, name)?
                .is_some()),
            Some(value) => Ok(self.builtin_method_exists(value, name)),
            None => Ok(false),
        }
    }

    fn call_import_dynamic(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let specifier = match self.pop(word)? {
            Value::String(specifier) => specifier,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "module specifier string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let result = match self.import_dynamic_module(&specifier) {
            Ok(module) => Value::result_ok(module),
            Err(message) => Value::result_err("ImportError", message),
        };
        self.stack.push(result);
        Ok(())
    }

    fn call_module_get(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        self.ensure_stack(word, 2)?;
        let name = match self.pop_unchecked() {
            Value::String(name) => name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "module binding name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let module_id = match self.pop_unchecked() {
            Value::Map(module) => match module_id_from_map(word, &module) {
                Ok(module_id) => module_id,
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            },
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "module map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let result = match self.dynamic_modules.get(&module_id) {
            Some(module) => module_get_value(module, &name),
            None => Value::result_err(
                "ModuleError",
                format!("dynamic module {module_id:?} is not loaded"),
            ),
        };
        self.stack.push(result);
        Ok(())
    }

    fn call_module_call(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        self.ensure_stack(word, 3)?;
        let args = match self.pop_unchecked() {
            Value::Array(values) => values.snapshot(),
            Value::List(values) => values.snapshot(),
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "array or list of function arguments".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let name = match self.pop_unchecked() {
            Value::String(name) => name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "module function name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let module_id = match self.pop_unchecked() {
            Value::Map(module) => match module_id_from_map(word, &module) {
                Ok(module_id) => module_id,
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            },
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "module map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let result = match self.call_dynamic_module_function(&module_id, &name, args) {
            Ok(value) => Value::result_ok(value),
            Err(message) => Value::result_err("ModuleError", message),
        };
        self.stack.push(result);
        Ok(())
    }

    fn import_dynamic_module(&mut self, specifier: &str) -> Result<Value, String> {
        if let Some(module_id) = self.dynamic_module_aliases.get(specifier) {
            if let Some(module) = self.dynamic_modules.get(module_id) {
                return Ok(self.dynamic_module_descriptor(module));
            }
        }
        if !self.dynamic_modules_loading.insert(specifier.to_string()) {
            return Err(format!("cyclic dynamic import involving {specifier:?}"));
        }

        let result = self.import_dynamic_module_inner(specifier);
        self.dynamic_modules_loading.remove(specifier);
        result
    }

    fn import_dynamic_module_inner(&mut self, specifier: &str) -> Result<Value, String> {
        let loader = self
            .dynamic_module_loader
            .clone()
            .ok_or_else(|| "dynamic module loading is not configured for this VM".to_string())?;
        let source = loader(specifier)?;
        if source.module_id.trim().is_empty() {
            return Err(format!(
                "dynamic module loader returned an empty module id for {specifier:?}"
            ));
        }
        if let Some(module) = self.dynamic_modules.get(&source.module_id) {
            self.dynamic_module_aliases
                .insert(specifier.to_string(), source.module_id.clone());
            return Ok(self.dynamic_module_descriptor(module));
        }
        if !self
            .dynamic_modules_loading
            .insert(source.module_id.clone())
        {
            return Err(format!(
                "cyclic dynamic import involving module {:?}",
                source.module_id
            ));
        }

        let previous_error_site = self.last_error_site.clone();
        let caller_language = self.take_language_state();
        let frame = format!("<module:{}>", source.module_id);
        let run_result = self.run_chunk_with_frame(&source.chunk, &frame, false);
        let module_language = self.take_language_state();
        self.restore_language_state(caller_language);
        self.last_error_site = previous_error_site;
        self.dynamic_modules_loading.remove(&source.module_id);

        if let Err(error) = run_result {
            return Err(error.to_string());
        }

        let module_id = source.module_id.clone();
        let module = DynamicModuleState {
            specifier: source.specifier,
            module_id: source.module_id,
            path: source.path,
            language: module_language,
        };
        self.dynamic_modules.insert(module_id.clone(), module);
        self.dynamic_module_aliases
            .insert(specifier.to_string(), module_id.clone());
        let module = self
            .dynamic_modules
            .get(&module_id)
            .expect("dynamic module was inserted before descriptor");
        Ok(self.dynamic_module_descriptor(module))
    }

    fn call_dynamic_module_function(
        &mut self,
        module_id: &str,
        name: &str,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        let Some(mut module) = self.dynamic_modules.remove(module_id) else {
            return Err(format!("dynamic module {module_id:?} is not loaded"));
        };
        if !module.language.functions.contains_key(name) {
            let functions = module_function_names(&module).join(", ");
            self.dynamic_modules.insert(module_id.to_string(), module);
            return Err(if functions.is_empty() {
                format!("dynamic module {module_id:?} has no function {name:?}")
            } else {
                format!(
                    "dynamic module {module_id:?} has no function {name:?}; available: {functions}"
                )
            });
        }

        let previous_error_site = self.last_error_site.clone();
        let caller_language = self.take_language_state();
        self.restore_language_state(module.language);
        self.stack.extend(args);
        let call_result = self.call_function(name);
        let value = call_result.and_then(|()| {
            self.pop(name).or_else(|error| match error {
                VmError::StackUnderflow { .. } => Ok(Value::Nil),
                error => Err(error),
            })
        });
        let module_language = self.take_language_state();
        self.restore_language_state(caller_language);
        self.last_error_site = previous_error_site;
        module.language = module_language;
        self.dynamic_modules.insert(module_id.to_string(), module);

        value.map_err(|error| error.to_string())
    }

    fn take_language_state(&mut self) -> LanguageState {
        LanguageState {
            stack: mem::take(&mut self.stack),
            variables: mem::take(&mut self.variables),
            local_variables: mem::take(&mut self.local_variables),
            last_call_variables: mem::take(&mut self.last_call_variables),
            functions: mem::take(&mut self.functions),
            classes: mem::take(&mut self.classes),
            current_class: self.current_class.take(),
            self_stack: mem::take(&mut self.self_stack),
        }
    }

    fn restore_language_state(&mut self, state: LanguageState) {
        self.stack = state.stack;
        self.variables = state.variables;
        self.local_variables = state.local_variables;
        self.last_call_variables = state.last_call_variables;
        self.functions = state.functions;
        self.classes = state.classes;
        self.current_class = state.current_class;
        self.self_stack = state.self_stack;
    }

    fn dynamic_module_descriptor(&self, module: &DynamicModuleState) -> Value {
        Value::Map(
            BTreeMap::from([
                ("type".to_string(), Value::String("module".to_string())),
                ("id".to_string(), Value::String(module.module_id.clone())),
                (
                    "specifier".to_string(),
                    Value::String(module.specifier.clone()),
                ),
                (
                    "path".to_string(),
                    module
                        .path
                        .as_ref()
                        .map(|path| Value::String(path.to_string_lossy().replace('\\', "/")))
                        .unwrap_or(Value::Nil),
                ),
                (
                    "variables".to_string(),
                    string_array(module.language.variables.keys().cloned()),
                ),
                (
                    "functions".to_string(),
                    string_array(module.language.functions.keys().cloned()),
                ),
                (
                    "classes".to_string(),
                    string_array(module.language.classes.keys().cloned()),
                ),
            ])
            .into(),
        )
    }

    fn call_top_receiver_method(&mut self, name: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let receiver = self.pop_unchecked();
        match self.call_method_value(receiver, name) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_bytecode_method(
        &mut self,
        receiver: Value,
        frame: &str,
        method: &Chunk,
        input_count: usize,
    ) -> Result<Value, VmError> {
        self.ensure_stack(frame, input_count)?;
        let base = self.stack.len() - input_count;
        self.self_stack.push(receiver);
        self.push_variable_frame();

        let run_result = self.run_chunk_with_frame(method, frame, true);
        self.last_call_variables = self.pop_variable_frame();
        self.self_stack
            .pop()
            .expect("method call pushed self before running");

        match run_result {
            Ok(ExecutionSignal::Continue | ExecutionSignal::Return) => {
                let result = if self.stack.len() > base {
                    self.pop_unchecked()
                } else {
                    Value::Nil
                };
                self.stack.truncate(base);
                Ok(result)
            }
            Ok(ExecutionSignal::Jump(target)) => Err(VmError::InvalidJump {
                target,
                available: method.instructions.len(),
            }),
            Err(error) => {
                self.stack.truncate(base);
                Err(error)
            }
        }
    }

    fn call_native_method(
        &mut self,
        receiver: Value,
        frame: &str,
        method: &NativeMethod,
    ) -> Result<Value, VmError> {
        self.ensure_stack(frame, method.input_count)?;
        let base = self.stack.len() - method.input_count;
        let stack_arguments = self.stack.split_off(base);
        let mut arguments = stack_arguments.clone();
        arguments.push(receiver);

        match method.call(arguments) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.stack.extend(stack_arguments);
                Err(error)
            }
        }
    }

    fn call_self(&mut self, word: &str) -> Result<(), VmError> {
        let value = self
            .self_stack
            .last()
            .cloned()
            .ok_or_else(|| VmError::NoCurrentSelf(word.to_string()))?;
        self.stack.push(value);
        Ok(())
    }

    fn call_get(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let selector = self.pop(word)?;
        match selector {
            Value::Member(field) => self.call_field_get(word, stack_before, field),
            Value::String(name) => match self.lookup_variable(&name).cloned() {
                Some(value) => {
                    self.stack.push(value);
                    Ok(())
                }
                None => {
                    self.stack = stack_before;
                    Err(VmError::UnknownVariable(name))
                }
            },
            value => {
                self.stack = stack_before;
                Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "member selector or variable name string".to_string(),
                    actual: value_kind(&value).to_string(),
                })
            }
        }
    }

    fn call_field_get(
        &mut self,
        word: &str,
        stack_before: Vec<Value>,
        field: String,
    ) -> Result<(), VmError> {
        let receiver = match self.pop(word) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        match self.get_field(&receiver, &field) {
            Ok(value) => {
                self.stack.push(value);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_set(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let selector = self.pop(word)?;
        match selector {
            Value::Member(field) => self.call_field_set(word, stack_before, field),
            Value::String(name) => {
                let value = match self.pop(word) {
                    Ok(value) => value,
                    Err(error) => {
                        self.stack = stack_before;
                        return Err(error);
                    }
                };
                if self.set_existing_variable(&name, value) {
                    Ok(())
                } else {
                    self.stack = stack_before;
                    Err(VmError::UnknownVariable(name))
                }
            }
            value => {
                self.stack = stack_before;
                Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "member selector or variable name string".to_string(),
                    actual: value_kind(&value).to_string(),
                })
            }
        }
    }

    fn call_field_set(
        &mut self,
        word: &str,
        stack_before: Vec<Value>,
        field: String,
    ) -> Result<(), VmError> {
        let receiver = match self.pop(word) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let value = match self.pop(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        match self.set_field(receiver, &field, value) {
            Ok(updated) => {
                self.stack.push(updated);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_var(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let name = match self.pop(word)? {
            Value::String(name) => name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "variable name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let value = self.stack.pop().unwrap_or(Value::Nil);
        self.declare_variable(name, value);
        Ok(())
    }

    fn call_collection_declaration_or_constructor(&mut self, value: Value) -> Result<(), VmError> {
        if let Some(Value::String(name)) = self.stack.last().cloned() {
            self.stack.pop();
            self.declare_variable(name, value);
        } else {
            self.stack.push(value);
        }
        Ok(())
    }

    fn call_collection_class_or_declaration(&mut self, class_name: &str) -> Result<(), VmError> {
        let collection = match class_name {
            "Array" => Value::Array(ArrayValue::default()),
            "List" => Value::List(ListValue::default()),
            "Map" => Value::Map(MapValue::default()),
            "Set" => Value::Set(SetValue::default()),
            _ => unreachable!("collection class caller restricts names"),
        };

        if let Some(Value::String(name)) = self.stack.last().cloned() {
            self.stack.pop();
            self.declare_variable(name, collection);
        } else {
            self.stack.push(Value::Class(class_name.to_string()));
        }
        Ok(())
    }

    fn call_new(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let class_name = match self.pop(word)? {
            Value::String(class_name) | Value::Class(class_name) => class_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "class or class name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        let built_in = match class_name.as_str() {
            "Array" => Some(Value::Array(ArrayValue::default())),
            "List" => Some(Value::List(ListValue::default())),
            "Map" => Some(Value::Map(MapValue::default())),
            "Set" => Some(Value::Set(SetValue::default())),
            _ => None,
        };
        if let Some(value) = built_in {
            self.stack.push(value);
            return Ok(());
        }

        match self.new_instance(&class_name) {
            Ok(instance) => {
                self.stack.push(instance);
                Ok(())
            }
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_install_current_class_method(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let method_name = match self.pop_unchecked() {
            Value::String(method_name) => method_name,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "method name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };
        let method = match self.pop_unchecked() {
            Value::Block(method) => method,
            value => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "method block".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
        };

        match self.add_bytecode_method(method_name, method, None) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.stack = stack_before;
                Err(error)
            }
        }
    }

    fn call_swap(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let top = self.stack.len() - 1;
        self.stack.swap(top, top - 1);
        Ok(())
    }

    fn call_dup(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 1)?;
        let value = self
            .stack
            .last()
            .expect("stack length checked before dup")
            .clone();
        self.stack.push(value);
        Ok(())
    }

    fn call_drop(&mut self, word: &str) -> Result<(), VmError> {
        self.pop(word)?;
        Ok(())
    }

    fn call_over(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let value = self.stack[self.stack.len() - 2].clone();
        self.stack.push(value);
        Ok(())
    }

    fn call_rot(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let third = self.stack.len() - 3;
        let value = self.stack.remove(third);
        self.stack.push(value);
        Ok(())
    }

    fn call_println(&mut self, word: &str) -> Result<(), VmError> {
        let value = self.pop(word)?;
        let output = output_string(&value);
        self.output_lines.push(output.clone());
        self.stdout.push_str(&output);
        self.stdout.push('\n');
        Ok(())
    }

    fn call_view(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let top = self.pop(word)?;
        let view_name = match top {
            Value::String(view_name) => view_name,
            _context => match self.pop(word) {
                Ok(Value::String(view_name)) => view_name,
                Ok(value) => {
                    self.stack = stack_before;
                    return Err(VmError::TypeError {
                        word: word.to_string(),
                        expected: "view name string".to_string(),
                        actual: value_kind(&value).to_string(),
                    });
                }
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            },
        };

        let mut action = BTreeMap::new();
        action.insert("type".to_string(), Value::String("view".to_string()));
        action.insert("name".to_string(), Value::String(view_name));
        self.stack.push(Value::Map(action.into()));
        Ok(())
    }

    fn call_text(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let top = self.pop(word)?;
        let body = match top {
            Value::String(body) => body,
            _context => match self.pop(word) {
                Ok(Value::String(body)) => body,
                Ok(value) => {
                    self.stack = stack_before;
                    return Err(VmError::TypeError {
                        word: word.to_string(),
                        expected: "text body string".to_string(),
                        actual: value_kind(&value).to_string(),
                    });
                }
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            },
        };

        let mut action = BTreeMap::new();
        action.insert("type".to_string(), Value::String("text".to_string()));
        action.insert("body".to_string(), Value::String(body));
        self.stack.push(Value::Map(action.into()));
        Ok(())
    }

    fn call_json(&mut self, word: &str) -> Result<(), VmError> {
        let body = self.pop(word)?;
        let mut action = BTreeMap::new();
        action.insert("type".to_string(), Value::String("json".to_string()));
        action.insert("body".to_string(), body);
        self.stack.push(Value::Map(action.into()));
        Ok(())
    }

    fn call_redirect(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let top = self.pop(word)?;
        let location = match top {
            Value::String(location) => location,
            _context => match self.pop(word) {
                Ok(Value::String(location)) => location,
                Ok(value) => {
                    self.stack = stack_before;
                    return Err(VmError::TypeError {
                        word: word.to_string(),
                        expected: "redirect location string".to_string(),
                        actual: value_kind(&value).to_string(),
                    });
                }
                Err(error) => {
                    self.stack = stack_before;
                    return Err(error);
                }
            },
        };

        let mut action = BTreeMap::new();
        action.insert("type".to_string(), Value::String("redirect".to_string()));
        action.insert("location".to_string(), Value::String(location));
        self.stack.push(Value::Map(action.into()));
        Ok(())
    }

    fn call_status(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let status = match self.pop_number(word) {
            Ok(status) if (100..=599).contains(&status) => status,
            Ok(status) => {
                self.stack = stack_before;
                return Err(VmError::HostError {
                    word: word.to_string(),
                    message: format!("HTTP status must be between 100 and 599, got {status}"),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let action = match self.pop(word) {
            Ok(Value::Map(action)) => action,
            Ok(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "action result map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        action.insert("status".to_string(), Value::Number(status));
        self.stack.push(Value::Map(action));
        Ok(())
    }

    fn call_header(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let stack_before = self.stack.clone();
        let value = match self.pop(word) {
            Ok(Value::String(value)) => value,
            Ok(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "header value string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let name = match self.pop(word) {
            Ok(Value::String(name)) => name,
            Ok(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "header name string".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let action = match self.pop(word) {
            Ok(Value::Map(action)) => action,
            Ok(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "action result map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        let headers = match action.remove("headers") {
            Some(Value::Map(headers)) => headers,
            Some(value) => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "headers map".to_string(),
                    actual: value_kind(&value).to_string(),
                });
            }
            None => MapValue::default(),
        };
        headers.insert(name, Value::String(value));
        action.insert("headers".to_string(), Value::Map(headers));
        self.stack.push(Value::Map(action));
        Ok(())
    }

    fn call_add(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_add(word, left, right) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }

        Ok(())
    }

    fn call_subtract(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;
        match numeric_subtract(word, left, right) {
            Ok(value) => self.stack.push(value),
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        }
        Ok(())
    }

    fn call_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let right = self.pop_unchecked();
        let left = self.pop_unchecked();
        let equal = numeric_values_equal(&left, &right).unwrap_or(left == right);
        self.stack.push(Value::Bool(equal));

        Ok(())
    }

    fn call_not_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let right = self.pop_unchecked();
        let left = self.pop_unchecked();
        let equal = numeric_values_equal(&left, &right).unwrap_or(left == right);
        self.stack.push(Value::Bool(!equal));

        Ok(())
    }

    fn call_assert_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let expected = self.pop_unchecked();
        let actual = self.pop_unchecked();

        if numeric_values_equal(&actual, &expected).unwrap_or(actual == expected) {
            return Ok(());
        }

        self.stack = stack_before;
        Err(VmError::AssertionFailed {
            expected: format!("{expected:?}"),
            actual: format!("{actual:?}"),
        })
    }

    fn call_number_comparison(
        &mut self,
        word: &str,
        compare: impl FnOnce(f64, f64) -> bool,
    ) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = self.pop_numeric_or_restore(word, &stack_before)?;
        let left = self.pop_numeric_or_restore(word, &stack_before)?;

        self.stack
            .push(Value::Bool(compare(left.as_f64(), right.as_f64())));

        Ok(())
    }

    fn call_push(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let value = self.pop_unchecked();
        let collection = self.pop_unchecked();

        match collection {
            Value::Array(values) => {
                values.push(value);
                self.stack.push(Value::Array(values));
                Ok(())
            }
            Value::List(values) => {
                values.push(value);
                self.stack.push(Value::List(values));
                Ok(())
            }
            Value::Set(values) => {
                values.insert(value);
                self.stack.push(Value::Set(values));
                Ok(())
            }
            value => {
                self.stack = stack_before;
                Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "array, list, or set".to_string(),
                    actual: value_kind(&value).to_string(),
                })
            }
        }
    }

    fn call_put(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 3)?;
        let stack_before = self.stack.clone();
        let value = self.pop_unchecked();
        let key = match self.pop_unchecked() {
            Value::String(key) => key,
            key => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "map key string".to_string(),
                    actual: value_kind(&key).to_string(),
                });
            }
        };
        let map = match self.pop_unchecked() {
            Value::Map(map) => map,
            map => {
                self.stack = stack_before;
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: "map".to_string(),
                    actual: value_kind(&map).to_string(),
                });
            }
        };

        map.insert(key, value);
        self.stack.push(Value::Map(map));
        Ok(())
    }

    fn call_predicate(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 1)?;
        let value = self
            .stack
            .last()
            .expect("stack length checked before predicate");

        let result = match value.call_predicate(word) {
            Some(result) => result,
            None if is_known_predicate(word) => {
                return Err(VmError::TypeError {
                    word: word.to_string(),
                    expected: predicate_expected_receiver(word).to_string(),
                    actual: value_kind(value).to_string(),
                });
            }
            None => return Err(VmError::UnknownWord(word.to_string())),
        };

        self.pop_unchecked();
        self.stack.push(result);

        Ok(())
    }

    pub(super) fn ensure_stack(&self, word: &str, needed: usize) -> Result<(), VmError> {
        let available = self.stack.len();
        if available < needed {
            return Err(VmError::StackUnderflow {
                word: word.to_string(),
                needed,
                available,
            });
        }

        Ok(())
    }

    pub(super) fn pop(&mut self, word: &str) -> Result<Value, VmError> {
        let available = self.stack.len();
        self.stack.pop().ok_or_else(|| VmError::StackUnderflow {
            word: word.to_string(),
            needed: 1,
            available,
        })
    }

    pub(super) fn pop_number(&mut self, word: &str) -> Result<i64, VmError> {
        match self.pop(word)? {
            Value::Number(value) => Ok(value),
            value => Err(VmError::TypeError {
                word: word.to_string(),
                expected: "number".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    pub(super) fn pop_numeric_or_restore(
        &mut self,
        word: &str,
        stack_before: &[Value],
    ) -> Result<NumericValue, VmError> {
        match self.pop_numeric(word) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.stack = stack_before.to_vec();
                Err(error)
            }
        }
    }

    pub(super) fn pop_numeric(&mut self, word: &str) -> Result<NumericValue, VmError> {
        match self.pop(word)? {
            Value::Number(value) => Ok(NumericValue::Integer(value)),
            Value::Float(value) => Ok(NumericValue::Float(value)),
            value => Err(VmError::TypeError {
                word: word.to_string(),
                expected: "number".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    pub(super) fn pop_unchecked(&mut self) -> Value {
        self.stack.pop().expect("stack length checked before pop")
    }

    fn current_class_mut(&mut self, word: &str) -> Result<&mut Class, VmError> {
        let class_name = self
            .current_class
            .clone()
            .ok_or_else(|| VmError::NoCurrentClass(word.to_string()))?;

        self.classes
            .get_mut(&class_name)
            .ok_or(VmError::UnknownClass(class_name))
    }

    pub(super) fn inheritance_chain(&self, class_name: &str) -> Result<Vec<&Class>, VmError> {
        let mut chain = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current_name = class_name;

        loop {
            if !visited.insert(current_name.to_string()) {
                return Err(VmError::InheritanceCycle(current_name.to_string()));
            }

            let Some(class) = self.classes.get(current_name) else {
                if chain.is_empty() {
                    return Err(VmError::UnknownClass(class_name.to_string()));
                }
                break;
            };
            chain.push(class);

            if class.superclass.is_empty() {
                break;
            }
            current_name = &class.superclass;
        }

        Ok(chain)
    }

    pub(super) fn resolve_native_method(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<(String, NativeMethod)>, VmError> {
        for class in self.inheritance_chain(class_name)? {
            if let Some(method) = class.native_methods.get(method_name) {
                return Ok(Some((class.name.clone(), method.clone())));
            }
        }
        Ok(None)
    }

    pub(super) fn resolve_instance_method(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Result<Option<ResolvedMethod>, VmError> {
        for class in self.inheritance_chain(class_name)? {
            if let Some(method) = class.native_methods.get(method_name) {
                return Ok(Some(ResolvedMethod::Native {
                    owner: class.name.clone(),
                    method: method.clone(),
                }));
            }
            if let Some(method) = class.bytecode_methods.get(method_name) {
                return Ok(Some(ResolvedMethod::Bytecode {
                    owner: class.name.clone(),
                    method: method.clone(),
                }));
            }
        }
        Ok(None)
    }
}

fn module_id_from_map(word: &str, module: &MapValue) -> Result<String, VmError> {
    match module.get("type") {
        Some(Value::String(kind)) if kind == "module" => {}
        Some(value) => {
            return Err(VmError::TypeError {
                word: word.to_string(),
                expected: "module map".to_string(),
                actual: value_kind(&value).to_string(),
            });
        }
        None => {
            return Err(VmError::InvalidArgument {
                word: word.to_string(),
                message: "module map is missing type".to_string(),
            });
        }
    }

    match module.get("id") {
        Some(Value::String(id)) if !id.is_empty() => Ok(id),
        Some(value) => Err(VmError::TypeError {
            word: word.to_string(),
            expected: "module id string".to_string(),
            actual: value_kind(&value).to_string(),
        }),
        None => Err(VmError::InvalidArgument {
            word: word.to_string(),
            message: "module map is missing id".to_string(),
        }),
    }
}

fn module_get_value(module: &DynamicModuleState, name: &str) -> Value {
    if let Some(value) = module.language.variables.get(name) {
        return Value::result_ok(value.clone());
    }
    if module.language.classes.contains_key(name) {
        return Value::result_ok(Value::Class(name.to_string()));
    }
    if module.language.functions.contains_key(name) {
        return Value::result_err(
            "ModuleError",
            format!("module binding {name:?} is a function; use module_call"),
        );
    }
    Value::result_err(
        "ModuleError",
        format!(
            "dynamic module {:?} has no binding {name:?}",
            module.module_id
        ),
    )
}

fn module_function_names(module: &DynamicModuleState) -> Vec<String> {
    module.language.functions.keys().cloned().collect()
}

fn string_array(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(
        values
            .into_iter()
            .map(Value::String)
            .collect::<Vec<_>>()
            .into(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum NumericValue {
    Integer(i64),
    Float(f64),
}

impl NumericValue {
    pub(super) fn as_f64(self) -> f64 {
        match self {
            NumericValue::Integer(value) => value as f64,
            NumericValue::Float(value) => value,
        }
    }
}

pub(super) fn numeric_value(value: &Value) -> Option<NumericValue> {
    match value {
        Value::Number(value) => Some(NumericValue::Integer(*value)),
        Value::Float(value) => Some(NumericValue::Float(*value)),
        _ => None,
    }
}

pub(super) fn numeric_values_equal(left: &Value, right: &Value) -> Option<bool> {
    Some(numeric_value(left)?.as_f64() == numeric_value(right)?.as_f64())
}

pub(super) fn numeric_add(
    word: &str,
    left: NumericValue,
    right: NumericValue,
) -> Result<Value, VmError> {
    match (left, right) {
        (NumericValue::Integer(left), NumericValue::Integer(right)) => left
            .checked_add(right)
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        _ => finite_float_result(word, left.as_f64() + right.as_f64()),
    }
}

pub(super) fn numeric_subtract(
    word: &str,
    left: NumericValue,
    right: NumericValue,
) -> Result<Value, VmError> {
    match (left, right) {
        (NumericValue::Integer(left), NumericValue::Integer(right)) => left
            .checked_sub(right)
            .map(Value::Number)
            .ok_or_else(|| arithmetic_overflow(word)),
        _ => finite_float_result(word, left.as_f64() - right.as_f64()),
    }
}

pub(super) fn finite_float_result(word: &str, value: f64) -> Result<Value, VmError> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(arithmetic_overflow(word))
    }
}

pub(super) fn arithmetic_overflow(word: &str) -> VmError {
    VmError::ArithmeticOverflow {
        word: word.to_string(),
    }
}

pub(super) fn display_float(value: f64) -> String {
    let formatted = value.to_string();
    if formatted.contains('.') || formatted.contains('e') || formatted.contains('E') {
        formatted
    } else {
        format!("{formatted}.0")
    }
}

pub(super) fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::Member(_) => "member selector",
        Value::Block(_) => "block",
        Value::Task(_) => "task",
        Value::Result(_) => "result",
        Value::Regex(_) => "regex",
        Value::Capability(_) => "capability",
    }
}

fn accessor_get(field: &str, receiver: &Value) -> Result<Value, VmError> {
    match receiver {
        Value::Instance(instance) => Ok(instance.fields.get(field).cloned().unwrap_or(Value::Nil)),
        value => Err(VmError::TypeError {
            word: format!("{field}.get"),
            expected: "instance".to_string(),
            actual: value_kind(value).to_string(),
        }),
    }
}

fn accessor_set(field: &str, receiver: Value, value: Value) -> Result<Value, VmError> {
    match receiver {
        Value::Instance(mut instance) => {
            instance.fields.insert(field.to_string(), value);
            Ok(Value::Instance(instance))
        }
        value => Err(VmError::TypeError {
            word: format!("{field}.set"),
            expected: "instance".to_string(),
            actual: value_kind(&value).to_string(),
        }),
    }
}

fn capability_name(capability: Capability) -> &'static str {
    match capability {
        Capability::FileSystem => "filesystem",
        Capability::Http => "HTTP",
        Capability::Process => "process",
        Capability::Terminal => "terminal UI",
        Capability::Webview => "webview",
    }
}

fn resolve_bounded_path(
    word: &str,
    label: &str,
    root: &Path,
    source: &str,
) -> Result<PathBuf, VmError> {
    let source_path = Path::new(source);
    let candidate = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        root.join(source_path)
    };
    let normalized = normalize_path(&candidate);

    if !normalized.starts_with(root) {
        return Err(VmError::HostError {
            word: word.to_string(),
            message: format!("{label} path is outside root: {source}"),
        });
    }

    let existing = nearest_existing_ancestor(&normalized);
    let canonical_existing = existing
        .canonicalize()
        .map_err(|error| VmError::HostError {
            word: word.to_string(),
            message: format!("failed to resolve {label} path {}: {error}", source),
        })?;
    let canonical_existing = normalize_path(&strip_verbatim_prefix(canonical_existing));
    if !canonical_existing.starts_with(root) {
        return Err(VmError::HostError {
            word: word.to_string(),
            message: format!("{label} path is outside root: {source}"),
        });
    }

    Ok(normalized)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        if !current.pop() {
            return path.to_path_buf();
        }
    }
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{stripped}"))
    } else if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

fn output_string(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Float(value) => display_float(*value),
        Value::String(value) => value.clone(),
        value => format!("{value:?}"),
    }
}

fn source_label(span: &SourceSpan) -> String {
    format!("{}:{}", span.file, span.line)
}

fn debug_variables(variables: &BTreeMap<String, Value>) -> Vec<(String, Value)> {
    variables
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn is_known_predicate(name: &str) -> bool {
    matches!(name, "ok?" | "nil?" | "empty?")
}

fn predicate_expected_receiver(name: &str) -> &'static str {
    match name {
        "ok?" => "result",
        "empty?" => "string, array, or map",
        "nil?" => "any value",
        _ => "value",
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::{debug::DebugEvent, value::Value};
    use ricochet_bytecode::{ArgsSpec, Chunk, Op, SourceSpan};

    fn span() -> SourceSpan {
        SourceSpan {
            file: "test.rco".to_string(),
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn executes_basic_stack_words() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("vm succeeds");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
    }

    #[test]
    fn numeric_words_support_float_promotion_and_type() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushFloat(1.5), span());
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::CallWord("+".to_string()), span());
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::PushFloat(1.0), span());
        chunk.push(Op::CallWord("=".to_string()), span());
        chunk.push(Op::PushFloat(2.0), span());
        chunk.push(Op::CallWord("type".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("float words run");

        assert_eq!(
            vm.stack(),
            &[
                Value::Float(3.5),
                Value::Bool(true),
                Value::String("float".to_string())
            ]
        );
    }

    #[test]
    fn executes_core_stack_manipulation_words() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::CallWord("over".to_string()), span());
        chunk.push(Op::CallWord("drop".to_string()), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("rot".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("stack words run");

        assert_eq!(
            vm.stack(),
            &[Value::Number(2), Value::Number(3), Value::Number(1)]
        );
    }

    #[test]
    fn stack_manipulation_words_report_underflow() {
        let mut vm = Vm::default();
        vm.stack.push(Value::Number(1));

        assert_eq!(
            vm.call_word("rot"),
            Err(VmError::StackUnderflow {
                word: "rot".to_string(),
                needed: 3,
                available: 1,
            })
        );
        assert_eq!(vm.stack(), &[Value::Number(1)]);
    }

    #[test]
    fn number_words_preserve_stack_on_type_errors() {
        let cases = [
            (
                "*",
                vec![Value::Number(4), Value::String("oops".to_string())],
            ),
            (
                "/",
                vec![Value::Number(4), Value::String("oops".to_string())],
            ),
            (
                "%",
                vec![Value::Number(4), Value::String("oops".to_string())],
            ),
            (
                "min",
                vec![Value::Number(4), Value::String("oops".to_string())],
            ),
            ("negate", vec![Value::String("oops".to_string())]),
            (
                "clamp",
                vec![
                    Value::Number(4),
                    Value::String("oops".to_string()),
                    Value::Number(10),
                ],
            ),
        ];

        for (word, stack) in cases {
            let mut vm = Vm {
                stack: stack.clone(),
                ..Vm::default()
            };

            assert!(
                matches!(vm.call_word(word), Err(VmError::TypeError { .. })),
                "{word} should reject non-number operands"
            );
            assert_eq!(
                vm.stack(),
                stack.as_slice(),
                "{word} should preserve operands for debug inspection"
            );
        }
    }

    #[test]
    fn division_and_modulo_preserve_stack_on_zero() {
        for word in ["/", "%"] {
            let mut vm = Vm {
                stack: vec![Value::Number(8), Value::Number(0)],
                ..Vm::default()
            };

            assert_eq!(
                vm.call_word(word),
                Err(VmError::DivisionByZero {
                    word: word.to_string(),
                })
            );
            assert_eq!(vm.stack(), &[Value::Number(8), Value::Number(0)]);
        }
    }

    #[test]
    fn disabled_capability_words_preserve_stack() {
        for word in ["fs", "http"] {
            let mut vm = Vm::default();
            vm.stack.push(Value::String("sentinel".to_string()));

            assert!(
                matches!(vm.call_word(word), Err(VmError::HostError { .. })),
                "{word} should require explicit host enablement"
            );
            assert_eq!(vm.stack(), &[Value::String("sentinel".to_string())]);
        }
    }

    #[test]
    fn debug_mode_records_instruction_events_with_stack_before_and_after() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();
        vm.enable_debug();
        vm.run_chunk(&chunk).expect("vm succeeds");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert_eq!(vm.debug_events().len(), 3);
        assert_eq!(
            vm.debug_events()[2],
            DebugEvent::Instruction {
                frame: "<main>".to_string(),
                source: "test.rco:1".to_string(),
                opcode: "CallWord(\"+\")".to_string(),
                stack_before: vec![Value::Number(2), Value::Number(3)],
                stack_after: vec![Value::Number(5)],
            }
        );
    }

    #[test]
    fn debug_sink_receives_events_as_vm_runs() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink_seen = Rc::clone(&seen);

        let mut vm = Vm::default();
        vm.enable_debug();
        vm.set_debug_sink(move |event| sink_seen.borrow_mut().push(event.clone()));
        vm.run_chunk(&chunk).expect("vm succeeds");

        let seen = seen.borrow();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen.as_slice(), vm.debug_events());
        assert!(matches!(
            seen.last(),
            Some(DebugEvent::Instruction {
                opcode,
                stack_after,
                ..
            }) if opcode == "CallWord(\"+\")" && stack_after == &[Value::Number(5)]
        ));
    }

    #[test]
    fn step_debugger_can_abort_before_first_instruction() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let pauses = Rc::new(RefCell::new(Vec::new()));
        let seen = pauses.clone();
        let mut vm = Vm::default();
        vm.enable_step_debugging();
        vm.set_debug_controller(move |pause| {
            seen.borrow_mut().push(pause.clone());
            DebugAction::Abort
        });

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::ExecutionAborted {
                frame: "<main>".to_string(),
                location: "test.rco:1".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[]);
        assert_eq!(pauses.borrow().len(), 1);
        assert_eq!(pauses.borrow()[0].reason, DebugPauseReason::Step);
    }

    #[test]
    fn line_breakpoint_pauses_before_matching_instruction() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), SourceSpan { line: 1, ..span() });
        chunk.push(Op::PushNumber(3), SourceSpan { line: 2, ..span() });
        chunk.push(
            Op::CallWord("+".to_string()),
            SourceSpan { line: 3, ..span() },
        );

        let pauses = Rc::new(RefCell::new(Vec::new()));
        let seen = pauses.clone();
        let mut vm = Vm::default();
        vm.enable_debug();
        vm.add_line_breakpoint("test.rco", 2);
        vm.set_debug_controller(move |pause| {
            seen.borrow_mut().push(pause.clone());
            DebugAction::Continue
        });

        vm.run_chunk(&chunk).expect("breakpoint continues");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert_eq!(pauses.borrow().len(), 1);
        assert_eq!(pauses.borrow()[0].reason, DebugPauseReason::Breakpoint);
        assert_eq!(pauses.borrow()[0].source, "test.rco:2");
        assert_eq!(pauses.borrow()[0].stack, vec![Value::Number(2)]);
    }

    #[test]
    fn debug_control_can_add_line_breakpoint_while_paused() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), SourceSpan { line: 1, ..span() });
        chunk.push(Op::PushNumber(3), SourceSpan { line: 2, ..span() });
        chunk.push(
            Op::CallWord("+".to_string()),
            SourceSpan { line: 3, ..span() },
        );

        let pauses = Rc::new(RefCell::new(Vec::new()));
        let seen = pauses.clone();
        let mut vm = Vm::default();
        vm.enable_step_debugging();
        vm.set_debug_controller_with_control(move |pause, control| {
            seen.borrow_mut().push(pause.clone());
            if seen.borrow().len() == 1 {
                control.add_line_breakpoint(None, 3);
            }
            DebugAction::Continue
        });

        vm.run_chunk(&chunk).expect("runtime breakpoint continues");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert_eq!(pauses.borrow().len(), 2);
        assert_eq!(pauses.borrow()[0].reason, DebugPauseReason::Step);
        assert_eq!(pauses.borrow()[0].source, "test.rco:1");
        assert_eq!(pauses.borrow()[1].reason, DebugPauseReason::Breakpoint);
        assert_eq!(pauses.borrow()[1].source, "test.rco:3");
        assert_eq!(
            pauses.borrow()[1].stack,
            vec![Value::Number(2), Value::Number(3)]
        );
    }

    #[test]
    fn continuing_from_line_breakpoint_skips_remaining_opcodes_on_same_line() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let pause_count = Rc::new(RefCell::new(0usize));
        let seen = pause_count.clone();
        let mut vm = Vm::default();
        vm.add_line_breakpoint("test.rco", 1);
        vm.set_debug_controller(move |_| {
            *seen.borrow_mut() += 1;
            DebugAction::Continue
        });

        vm.run_chunk(&chunk).expect("breakpoint continues");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert_eq!(*pause_count.borrow(), 1);
    }

    #[test]
    fn line_breakpoint_can_pause_again_on_a_later_top_level_run() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());

        let pause_count = Rc::new(RefCell::new(0usize));
        let seen = pause_count.clone();
        let mut vm = Vm::default();
        vm.add_line_breakpoint("test.rco", 1);
        vm.set_debug_controller(move |_| {
            *seen.borrow_mut() += 1;
            DebugAction::Continue
        });

        vm.run_chunk(&chunk).expect("first run continues");
        vm.run_chunk(&chunk).expect("second run continues");

        assert_eq!(*pause_count.borrow(), 2);
    }

    #[test]
    fn step_action_pauses_before_each_instruction() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let pause_count = Rc::new(RefCell::new(0usize));
        let seen = pause_count.clone();
        let mut vm = Vm::default();
        vm.enable_step_debugging();
        vm.set_debug_controller(move |_| {
            *seen.borrow_mut() += 1;
            DebugAction::Step
        });

        vm.run_chunk(&chunk).expect("step debugger continues");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert_eq!(*pause_count.borrow(), 3);
    }

    #[test]
    fn step_over_skips_nested_function_frame() {
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushNumber(2), SourceSpan { line: 2, ..span() });
        function.push(Op::PushNumber(3), SourceSpan { line: 3, ..span() });
        function.push(
            Op::CallWord("+".to_string()),
            SourceSpan { line: 4, ..span() },
        );
        function.push(Op::Return, SourceSpan { line: 4, ..span() });

        let mut chunk = Chunk::new("test.rco");
        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "work".to_string(),
                block,
                args: None,
            },
            SourceSpan { line: 1, ..span() },
        );
        chunk.push(
            Op::CallWord("work".to_string()),
            SourceSpan { line: 5, ..span() },
        );
        chunk.push(Op::PushNumber(10), SourceSpan { line: 6, ..span() });

        let pauses = Rc::new(RefCell::new(Vec::new()));
        let seen = pauses.clone();
        let mut vm = Vm::default();
        vm.add_line_breakpoint("test.rco", 5);
        vm.set_debug_controller(move |pause| {
            seen.borrow_mut().push(pause.clone());
            if seen.borrow().len() == 1 {
                DebugAction::StepOver
            } else {
                DebugAction::Continue
            }
        });

        vm.run_chunk(&chunk).expect("step over continues");

        let pauses = pauses.borrow();
        assert_eq!(pauses.len(), 2);
        assert_eq!(pauses[0].source, "test.rco:5");
        assert_eq!(pauses[1].source, "test.rco:6");
        assert_eq!(vm.stack(), &[Value::Number(5), Value::Number(10)]);
    }

    #[test]
    fn step_out_pauses_in_caller_after_function_returns() {
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushNumber(2), SourceSpan { line: 2, ..span() });
        function.push(Op::PushNumber(3), SourceSpan { line: 3, ..span() });
        function.push(
            Op::CallWord("+".to_string()),
            SourceSpan { line: 4, ..span() },
        );
        function.push(Op::Return, SourceSpan { line: 4, ..span() });

        let mut chunk = Chunk::new("test.rco");
        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "work".to_string(),
                block,
                args: None,
            },
            SourceSpan { line: 1, ..span() },
        );
        chunk.push(
            Op::CallWord("work".to_string()),
            SourceSpan { line: 5, ..span() },
        );
        chunk.push(Op::PushNumber(10), SourceSpan { line: 6, ..span() });

        let pauses = Rc::new(RefCell::new(Vec::new()));
        let seen = pauses.clone();
        let mut vm = Vm::default();
        vm.add_line_breakpoint("test.rco", 2);
        vm.set_debug_controller(move |pause| {
            seen.borrow_mut().push(pause.clone());
            if seen.borrow().len() == 1 {
                DebugAction::StepOut
            } else {
                DebugAction::Continue
            }
        });

        vm.run_chunk(&chunk).expect("step out continues");

        let pauses = pauses.borrow();
        assert_eq!(pauses.len(), 2);
        assert_eq!(pauses[0].source, "test.rco:2");
        assert_eq!(pauses[1].source, "test.rco:6");
        assert_eq!(vm.stack(), &[Value::Number(5), Value::Number(10)]);
    }

    #[test]
    fn debug_mode_records_fault_event_and_still_returns_error() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::CallWord("missing".to_string()), span());

        let mut vm = Vm::default();
        vm.enable_debug();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnknownWord("missing".to_string()))
        );
        assert_eq!(vm.stack(), &[Value::Number(1)]);
        assert_eq!(vm.debug_events().len(), 3);
        assert_eq!(
            vm.debug_events()[1],
            DebugEvent::Instruction {
                frame: "<main>".to_string(),
                source: "test.rco:1".to_string(),
                opcode: "CallWord(\"missing\")".to_string(),
                stack_before: vec![Value::Number(1)],
                stack_after: vec![Value::Number(1)],
            }
        );
        assert_eq!(
            vm.debug_events().last(),
            Some(&DebugEvent::Fault {
                frame: "<main>".to_string(),
                message: "unknown word: missing".to_string(),
                stack: vec![Value::Number(1)],
            })
        );
    }

    #[test]
    fn debug_disabled_does_not_record_events() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("vm succeeds");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
        assert!(vm.debug_events().is_empty());
    }

    #[test]
    fn result_values_require_explicit_ok_check() {
        let ok = Value::result_ok(Value::String("saved".to_string()));
        let err = Value::result_err("ValidationError", "email required");

        assert_eq!(ok.call_predicate("ok?"), Some(Value::Bool(true)));
        assert_eq!(err.call_predicate("ok?"), Some(Value::Bool(false)));
        assert!(err.truthy());
    }

    #[test]
    fn value_word_unwraps_successful_result() {
        let mut vm = Vm::default();
        vm.stack
            .push(Value::result_ok(Value::String("saved".to_string())));

        vm.call_word("value").expect("value unwraps ok result");

        assert_eq!(vm.stack(), &[Value::String("saved".to_string())]);
    }

    #[test]
    fn error_word_unwraps_failed_result_as_map() {
        let mut vm = Vm::default();
        vm.stack
            .push(Value::result_err("DatabaseError", "connection failed"));

        vm.call_word("error").expect("error unwraps failed result");

        assert_eq!(
            vm.stack(),
            &[Value::Map(
                BTreeMap::from([
                    (
                        "kind".to_string(),
                        Value::String("DatabaseError".to_string()),
                    ),
                    (
                        "message".to_string(),
                        Value::String("connection failed".to_string()),
                    ),
                ])
                .into()
            )]
        );
    }

    #[test]
    fn value_word_preserves_failed_result_when_unwrap_is_invalid() {
        let result = Value::result_err("DatabaseError", "connection failed");
        let mut vm = Vm::default();
        vm.stack.push(result.clone());

        assert_eq!(
            vm.call_word("value"),
            Err(VmError::ResultUnwrap {
                word: "value".to_string(),
                expected: "ok".to_string(),
                actual: "error".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[result]);
    }

    #[test]
    fn index_of_names_the_argument_below_the_receiver_on_type_errors() {
        let result = Value::result_ok(Value::String("haystack".to_string()));
        let mut vm = Vm::default();
        vm.stack.push(result.clone());

        assert_eq!(
            vm.call_method_value(Value::String("needle".to_string()), "index_of"),
            Err(VmError::TypeError {
                word: "index_of".to_string(),
                expected: "needle string below receiver".to_string(),
                actual: "result".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[result]);
    }

    #[test]
    fn index_of_names_the_receiver_on_receiver_type_errors() {
        let result = Value::result_ok(Value::String("haystack".to_string()));
        let mut vm = Vm::default();
        vm.stack.push(result.clone());
        vm.stack.push(Value::String("needle".to_string()));

        assert_eq!(
            vm.call_word("index_of"),
            Err(VmError::TypeError {
                word: "index_of".to_string(),
                expected: "receiver string".to_string(),
                actual: "result".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[result, Value::String("needle".to_string())]);
    }

    #[test]
    fn json_encode_rejects_cyclic_collections_without_overflowing() {
        let map = MapValue::default();
        map.insert("self".to_string(), Value::Map(map.clone()));
        let mut vm = Vm::default();
        vm.stack.push(Value::Map(map));

        let error = vm
            .call_word("json_encode")
            .expect_err("cyclic map should fail loudly");

        assert_eq!(
            error,
            VmError::InvalidArgument {
                word: "json_encode".to_string(),
                message: "cannot encode cyclic collection as JSON at $.self".to_string(),
            }
        );
    }

    #[test]
    fn pop_reports_stack_underflow() {
        let mut vm = Vm::default();

        assert_eq!(
            vm.pop("test"),
            Err(VmError::StackUnderflow {
                word: "test".to_string(),
                needed: 1,
                available: 0,
            })
        );
    }

    #[test]
    fn pop_number_rejects_non_numbers() {
        let mut vm = Vm::default();
        vm.stack.push(Value::String("nope".to_string()));

        assert_eq!(
            vm.pop_number("add"),
            Err(VmError::TypeError {
                word: "add".to_string(),
                expected: "number".to_string(),
                actual: "string".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[]);
    }

    #[test]
    fn executes_equals_words() {
        let mut equals_chunk = Chunk::new("test.rco");
        equals_chunk.push(Op::PushNumber(7), span());
        equals_chunk.push(Op::PushNumber(7), span());
        equals_chunk.push(Op::CallWord("equals".to_string()), span());

        let mut equals_vm = Vm::default();
        equals_vm.run_chunk(&equals_chunk).expect("equals succeeds");
        assert_eq!(equals_vm.stack(), &[Value::Bool(true)]);

        let mut symbol_chunk = Chunk::new("test.rco");
        symbol_chunk.push(Op::PushNumber(7), span());
        symbol_chunk.push(Op::PushNumber(8), span());
        symbol_chunk.push(Op::CallWord("=".to_string()), span());

        let mut symbol_vm = Vm::default();
        symbol_vm.run_chunk(&symbol_chunk).expect("= succeeds");
        assert_eq!(symbol_vm.stack(), &[Value::Bool(false)]);
    }

    #[test]
    fn assert_equals_consumes_matching_values() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::CallWord("assert_equals".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("matching assertion succeeds");

        assert_eq!(vm.stack(), &[]);
    }

    #[test]
    fn assert_equals_reports_mismatch_and_preserves_stack() {
        let mut vm = Vm::default();
        vm.stack.push(Value::String("Ada".to_string()));
        vm.stack.push(Value::String("Grace".to_string()));

        assert_eq!(
            vm.call_word("assert_equals"),
            Err(VmError::AssertionFailed {
                expected: "String(\"Grace\")".to_string(),
                actual: "String(\"Ada\")".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[
                Value::String("Ada".to_string()),
                Value::String("Grace".to_string())
            ]
        );
    }

    #[test]
    fn executes_comparison_words() {
        let cases = [
            (2, 3, "less_than?", true),
            (2, 3, "<", true),
            (3, 2, "greater_than?", true),
            (3, 2, ">", true),
            (3, 3, "less_or_equals?", true),
            (3, 3, "<=", true),
            (3, 3, "greater_or_equals?", true),
            (3, 3, ">=", true),
            (3, 3, "not_equals?", false),
            (3, 4, "!=", true),
        ];

        for (left, right, word, expected) in cases {
            let mut chunk = Chunk::new("test.rco");
            chunk.push(Op::PushNumber(left), span());
            chunk.push(Op::PushNumber(right), span());
            chunk.push(Op::CallWord(word.to_string()), span());

            let mut vm = Vm::default();
            vm.run_chunk(&chunk)
                .unwrap_or_else(|err| panic!("{word} should succeed: {err}"));
            assert_eq!(
                vm.stack(),
                &[Value::Bool(expected)],
                "{left} {right} {word} should be {expected}"
            );
        }
    }

    #[test]
    fn comparison_type_errors_preserve_stack() {
        let mut vm = Vm::default();
        vm.stack.push(Value::String("Ada".to_string()));
        vm.stack.push(Value::Number(3));

        assert_eq!(
            vm.call_word("less_than?"),
            Err(VmError::TypeError {
                word: "less_than?".to_string(),
                expected: "number".to_string(),
                actual: "string".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[Value::String("Ada".to_string()), Value::Number(3)]
        );
    }

    #[test]
    fn run_chunk_records_runtime_error_site() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(
            Op::CallWord("less_than?".to_string()),
            SourceSpan {
                start: 8,
                end: 18,
                line: 1,
                column: 9,
                ..span()
            },
        );

        let mut vm = Vm::default();
        let error = vm.run_chunk(&chunk).expect_err("comparison should fail");

        assert_eq!(
            error,
            VmError::TypeError {
                word: "less_than?".to_string(),
                expected: "number".to_string(),
                actual: "string".to_string(),
            }
        );
        assert_eq!(
            vm.last_error_site(),
            Some(&RuntimeErrorSite {
                frame: "<main>".to_string(),
                span: SourceSpan {
                    file: "test.rco".to_string(),
                    start: 8,
                    end: 18,
                    line: 1,
                    column: 9,
                },
                opcode: "CallWord(\"less_than?\")".to_string(),
            })
        );
    }

    #[test]
    fn executes_array_push_word() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("array".to_string()), span());
        chunk.push(Op::PushNumber(42), span());
        chunk.push(Op::CallWord("push".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("array push succeeds");

        assert_eq!(vm.stack(), &[Value::Array(vec![Value::Number(42)].into())]);
    }

    #[test]
    fn executes_map_put_word() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("map".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::CallWord("put".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::CallWord("at".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("map put succeeds");

        assert_eq!(vm.stack(), &[Value::String("Ada".to_string())]);
    }

    #[test]
    fn map_put_type_errors_preserve_stack() {
        let mut vm = Vm::default();
        vm.stack.push(Value::Array(Vec::new().into()));
        vm.stack.push(Value::String("name".to_string()));
        vm.stack.push(Value::String("Ada".to_string()));

        assert_eq!(
            vm.call_word("put"),
            Err(VmError::TypeError {
                word: "put".to_string(),
                expected: "map".to_string(),
                actual: "array".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[
                Value::Array(Vec::new().into()),
                Value::String("name".to_string()),
                Value::String("Ada".to_string())
            ]
        );
    }

    #[test]
    fn executes_predicate_words() {
        let mut nil_chunk = Chunk::new("test.rco");
        nil_chunk.push(Op::PushNil, span());
        nil_chunk.push(Op::CallWord("nil?".to_string()), span());

        let mut nil_vm = Vm::default();
        nil_vm.run_chunk(&nil_chunk).expect("nil? succeeds");
        assert_eq!(nil_vm.stack(), &[Value::Bool(true)]);

        let mut empty_chunk = Chunk::new("test.rco");
        empty_chunk.push(Op::PushString(String::new()), span());
        empty_chunk.push(Op::CallWord("empty?".to_string()), span());

        let mut empty_vm = Vm::default();
        empty_vm.run_chunk(&empty_chunk).expect("empty? succeeds");
        assert_eq!(empty_vm.stack(), &[Value::Bool(true)]);

        let mut ok_chunk = Chunk::new("test.rco");
        ok_chunk.push(Op::CallWord("ok?".to_string()), span());

        let mut ok_vm = Vm::default();
        ok_vm
            .stack
            .push(Value::result_ok(Value::String("saved".to_string())));
        ok_vm.run_chunk(&ok_chunk).expect("ok? succeeds");
        assert_eq!(ok_vm.stack(), &[Value::Bool(true)]);
    }

    #[test]
    fn unsupported_opcode_reports_unsupported_opcode() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::Return, span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnsupportedOpcode("Return".to_string()))
        );
    }

    #[test]
    fn addition_overflow_reports_overflow() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(i64::MAX), span());
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::ArithmeticOverflow {
                word: "+".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[Value::Number(i64::MAX), Value::Number(1)]);
    }

    #[test]
    fn executes_subtraction_words_and_preserves_stack_on_overflow() {
        for word in ["subtract", "-"] {
            let mut chunk = Chunk::new("test.rco");
            chunk.push(Op::PushNumber(10), span());
            chunk.push(Op::PushNumber(3), span());
            chunk.push(Op::CallWord(word.to_string()), span());

            let mut vm = Vm::default();
            vm.run_chunk(&chunk).expect("subtraction succeeds");
            assert_eq!(vm.stack(), &[Value::Number(7)]);
        }

        let mut overflow = Chunk::new("test.rco");
        overflow.push(Op::PushNumber(i64::MIN), span());
        overflow.push(Op::PushNumber(1), span());
        overflow.push(Op::CallWord("-".to_string()), span());

        let mut vm = Vm::default();
        assert_eq!(
            vm.run_chunk(&overflow),
            Err(VmError::ArithmeticOverflow {
                word: "-".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[Value::Number(i64::MIN), Value::Number(1)]);
    }

    #[test]
    fn addition_type_errors_preserve_stack() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("left".to_string()), span());
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::CallWord("+".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::TypeError {
                word: "+".to_string(),
                expected: "number".to_string(),
                actual: "string".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[Value::String("left".to_string()), Value::Number(1)]
        );
    }

    #[test]
    fn known_predicate_on_wrong_type_reports_type_error() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::CallWord("empty?".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::TypeError {
                word: "empty?".to_string(),
                expected: "string, array, or map".to_string(),
                actual: "number".to_string(),
            })
        );
        assert_eq!(vm.stack(), &[Value::Number(1)]);

        let mut ok_chunk = Chunk::new("test.rco");
        ok_chunk.push(Op::PushNumber(1), span());
        ok_chunk.push(Op::CallWord("ok?".to_string()), span());

        let mut ok_vm = Vm::default();

        assert_eq!(
            ok_vm.run_chunk(&ok_chunk),
            Err(VmError::TypeError {
                word: "ok?".to_string(),
                expected: "result".to_string(),
                actual: "number".to_string(),
            })
        );
        assert_eq!(ok_vm.stack(), &[Value::Number(1)]);
    }

    #[test]
    fn unknown_predicate_reports_unknown_word() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(1), span());
        chunk.push(Op::CallWord("ready?".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnknownWord("ready?".to_string()))
        );
        assert_eq!(vm.stack(), &[Value::Number(1)]);
    }

    #[test]
    fn open_class_replaces_method() {
        let mut vm = Vm::default();

        vm.define_class("Widget", "").expect("class opens");
        vm.add_field("name").expect("field is declared");
        vm.add_native_method("label", |_| Ok(Value::String("old label".to_string())))
            .expect("method is declared");

        vm.define_class("Widget", "").expect("class reopens");
        vm.add_native_method("label", |_| Ok(Value::String("new label".to_string())))
            .expect("method is replaced");
        vm.end_class();

        let instance = vm.new_instance("Widget").expect("instance is created");

        assert_eq!(
            vm.get_field(&instance, "name").expect("field exists"),
            Value::Nil
        );
        assert_eq!(
            vm.call_method_value(instance, "label")
                .expect("native method is called"),
            Value::String("new label".to_string())
        );
    }

    #[test]
    fn class_value_constructs_an_instance_with_new() {
        let mut vm = Vm::default();
        vm.define_class("Widget", "Object").expect("class opens");
        vm.end_class();
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("Widget".to_string()), span());
        chunk.push(Op::CallWord("new".to_string()), span());

        vm.run_chunk(&chunk)
            .expect("class value constructs instance");

        assert!(matches!(
            vm.stack(),
            [Value::Instance(instance)] if instance.class_name == "Widget"
        ));
    }

    #[test]
    fn method_word_installs_a_runtime_bytecode_method() {
        let mut method = Chunk::new("test.rco");
        method.push(Op::PushString("dynamic".to_string()), span());
        method.push(Op::Return, span());
        let mut chunk = Chunk::new("test.rco");
        let method_block = chunk.push_block(method);
        chunk.push(
            Op::BeginClass {
                name: "Widget".to_string(),
                superclass: "Object".to_string(),
            },
            span(),
        );
        chunk.push(Op::PushBlock(method_block), span());
        chunk.push(Op::PushString("label".to_string()), span());
        chunk.push(Op::CallWord("Method".to_string()), span());
        chunk.push(Op::EndClass, span());
        chunk.push(Op::CallWord("Widget".to_string()), span());
        chunk.push(Op::CallWord("new".to_string()), span());
        chunk.push(Op::CallWord("label".to_string()), span());
        let mut vm = Vm::default();

        vm.run_chunk(&chunk)
            .expect("runtime method installs and runs");

        assert_eq!(vm.stack(), &[Value::String("dynamic".to_string())]);
    }

    #[test]
    fn method_word_preserves_the_stack_without_current_class() {
        let method = Chunk::new("test.rco");
        let mut chunk = Chunk::new("test.rco");
        let method_block = chunk.push_block(method);
        chunk.push(Op::PushBlock(method_block), span());
        chunk.push(Op::PushString("label".to_string()), span());
        chunk.push(Op::CallWord("Method".to_string()), span());
        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::NoCurrentClass("add_bytecode_method".to_string()))
        );
        assert!(matches!(
            vm.stack(),
            [Value::Block(_), Value::String(method_name)] if method_name == "label"
        ));
    }

    #[test]
    fn subclass_word_creates_a_class_from_runtime_names() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Widget".to_string()), span());
        chunk.push(Op::PushString("Object".to_string()), span());
        chunk.push(Op::CallWord("Subclass".to_string()), span());
        chunk.push(Op::PushString("Widget".to_string()), span());
        chunk.push(Op::CallWord("new".to_string()), span());
        let mut vm = Vm::default();

        vm.run_chunk(&chunk).expect("runtime class is created");

        assert!(matches!(
            vm.stack(),
            [Value::Instance(instance)] if instance.class_name == "Widget"
        ));
    }

    #[test]
    fn subclass_word_preserves_the_stack_on_type_error() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushNumber(7), span());
        chunk.push(Op::PushString("Object".to_string()), span());
        chunk.push(Op::CallWord("Subclass".to_string()), span());
        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::TypeError {
                word: "Subclass".to_string(),
                expected: "class name string".to_string(),
                actual: "number".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[Value::Number(7), Value::String("Object".to_string())]
        );
    }

    #[test]
    fn native_method_with_arity_consumes_stack_arguments_in_order() {
        let mut vm = Vm::default();
        vm.define_class("Calculator", "Object")
            .expect("class begins");
        vm.add_native_method_with_arity("sum", 2, |args| {
            assert_eq!(args.len(), 3);
            assert_eq!(args[0], Value::Number(2));
            assert_eq!(args[1], Value::Number(3));
            assert!(
                matches!(&args[2], Value::Instance(instance) if instance.class_name == "Calculator")
            );
            Ok(Value::Number(5))
        })
        .expect("native method added");
        vm.end_class();

        vm.stack.push(Value::Number(2));
        vm.stack.push(Value::Number(3));
        let calculator = vm.new_instance("Calculator").expect("calculator instance");

        let result = vm
            .call_method_value(calculator, "sum")
            .expect("native method succeeds");

        assert_eq!(result, Value::Number(5));
        assert!(vm.stack().is_empty());
    }

    #[test]
    fn native_method_with_arity_preserves_stack_on_failure() {
        let mut vm = Vm::default();
        vm.define_class("Calculator", "Object")
            .expect("class begins");
        vm.add_native_method_with_arity("fail", 1, |_| {
            Err(VmError::UnknownWord("native failure".to_string()))
        })
        .expect("native method added");
        vm.end_class();

        vm.stack.push(Value::Number(7));
        let calculator = vm.new_instance("Calculator").expect("calculator instance");

        assert_eq!(
            vm.call_method_value(calculator, "fail"),
            Err(VmError::UnknownWord("native failure".to_string()))
        );
        assert_eq!(vm.stack(), &[Value::Number(7)]);
    }

    #[test]
    fn class_name_word_pushes_class_value_for_static_native_method() {
        let mut vm = Vm::default();
        vm.define_class("Clock", "Object").expect("class begins");
        vm.add_native_method("name", |arguments| {
            assert!(matches!(
                arguments.as_slice(),
                [Value::Class(class_name)] if class_name == "Clock"
            ));
            Ok(Value::String("Clock".to_string()))
        })
        .expect("native method added");
        vm.end_class();
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("Clock".to_string()), span());
        chunk.push(Op::CallMethod("name".to_string()), span());

        vm.run_chunk(&chunk).expect("class method runs");

        assert_eq!(vm.stack(), &[Value::String("Clock".to_string())]);
    }

    #[test]
    fn subclass_instances_include_inherited_fields() {
        let mut vm = Vm::default();
        vm.define_class("Record", "Object")
            .expect("base class begins");
        vm.add_field("id").expect("base field added");
        vm.end_class();
        vm.define_class("User", "Record").expect("subclass begins");
        vm.add_field("email").expect("child field added");
        vm.end_class();

        let user = vm.new_instance("User").expect("subclass instance created");

        assert_eq!(
            vm.get_field(&user, "id").expect("base field reads"),
            Value::Nil
        );
        assert_eq!(
            vm.get_field(&user, "email").expect("child field reads"),
            Value::Nil
        );
    }

    #[test]
    fn subclass_uses_nearest_native_method_override() {
        let mut vm = Vm::default();
        vm.define_class("Record", "Object")
            .expect("base class begins");
        vm.add_native_method("kind", |_| Ok(Value::String("record".to_string())))
            .expect("base method added");
        vm.end_class();
        vm.define_class("User", "Record").expect("subclass begins");
        vm.add_native_method("kind", |_| Ok(Value::String("user".to_string())))
            .expect("override added");
        vm.end_class();
        vm.define_class("Admin", "User").expect("leaf class begins");
        vm.end_class();

        let user = vm.new_instance("User").expect("user instance created");
        let admin = vm.new_instance("Admin").expect("admin instance created");

        assert_eq!(
            vm.call_method_value(user, "kind").expect("override runs"),
            Value::String("user".to_string())
        );
        assert_eq!(
            vm.call_method_value(admin, "kind")
                .expect("inherited override runs"),
            Value::String("user".to_string())
        );
    }

    #[test]
    fn subclass_inherits_bytecode_methods() {
        let mut method = Chunk::new("test.rco");
        method.push(Op::PushString("record".to_string()), span());
        method.push(Op::Return, span());
        let mut vm = Vm::default();
        vm.define_class("Record", "Object")
            .expect("base class begins");
        vm.add_bytecode_method("kind", method, None)
            .expect("base method added");
        vm.end_class();
        vm.define_class("User", "Record").expect("subclass begins");
        vm.end_class();
        let user = vm.new_instance("User").expect("user instance created");

        assert_eq!(
            vm.call_method_value(user, "kind")
                .expect("inherited bytecode method runs"),
            Value::String("record".to_string())
        );
    }

    #[test]
    fn child_bytecode_method_overrides_parent_native_method() {
        let mut child_method = Chunk::new("test.rco");
        child_method.push(Op::PushString("user".to_string()), span());
        child_method.push(Op::Return, span());
        let mut vm = Vm::default();
        vm.define_class("Record", "Object")
            .expect("base class begins");
        vm.add_native_method("kind", |_| Ok(Value::String("record".to_string())))
            .expect("base method added");
        vm.end_class();
        vm.define_class("User", "Record").expect("subclass begins");
        vm.add_bytecode_method("kind", child_method, None)
            .expect("override added");
        vm.end_class();
        let user = vm.new_instance("User").expect("user instance created");

        assert_eq!(
            vm.call_method_value(user, "kind").expect("override runs"),
            Value::String("user".to_string())
        );
    }

    #[test]
    fn class_values_inherit_native_methods_with_the_child_receiver() {
        let mut vm = Vm::default();
        vm.define_class("Record", "Object")
            .expect("base class begins");
        vm.add_native_method("className", |arguments| {
            Ok(arguments
                .last()
                .cloned()
                .expect("native method receives class receiver"))
        })
        .expect("base class method added");
        vm.end_class();
        vm.define_class("User", "Record").expect("subclass begins");
        vm.end_class();

        assert_eq!(
            vm.call_method_value(Value::Class("User".to_string()), "className")
                .expect("inherited class method runs"),
            Value::Class("User".to_string())
        );
    }

    #[test]
    fn inheritance_cycles_fail_loudly() {
        let mut vm = Vm::default();
        vm.define_class("First", "Second")
            .expect("first class begins");
        vm.end_class();
        vm.define_class("Second", "First")
            .expect("second class begins");
        vm.end_class();

        assert_eq!(
            vm.new_instance("First"),
            Err(VmError::InheritanceCycle("First".to_string()))
        );
    }

    #[test]
    fn class_field_get_and_set_are_postfix_words_api() {
        let mut vm = Vm::default();
        vm.define_class("Article", "").expect("class opens");
        vm.add_field("title").expect("field is declared");
        vm.end_class();

        let instance = vm.new_instance("Article").expect("instance is created");
        assert_eq!(
            vm.get_field(&instance, "title").expect("field reads"),
            Value::Nil
        );

        let updated = vm
            .set_field(instance, "title", Value::String("Launch".to_string()))
            .expect("field writes");

        assert_eq!(
            vm.get_field(&updated, "title").expect("field reads"),
            Value::String("Launch".to_string())
        );
        assert_eq!(
            vm.get_field(&updated, "missing")
                .expect("missing field is nil"),
            Value::Nil
        );
    }

    #[test]
    fn table_word_records_model_table_on_current_class() {
        let mut vm = Vm::default();
        vm.define_class("User", "Model").expect("class begins");
        vm.stack.push(Value::String("users".to_string()));

        vm.call_word("Table").expect("table word succeeds");
        vm.end_class();

        assert_eq!(vm.class_table("User"), Some("users"));
    }

    #[test]
    fn targeted_table_and_field_words_mutate_a_runtime_class() {
        let mut vm = Vm::default();
        vm.define_class("User", "Model").expect("class begins");
        vm.end_class();
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("User".to_string()), span());
        chunk.push(Op::PushString("users".to_string()), span());
        chunk.push(Op::CallWord("Table".to_string()), span());
        chunk.push(Op::PushString("User".to_string()), span());
        chunk.push(Op::PushString("email".to_string()), span());
        chunk.push(Op::CallWord("Field".to_string()), span());

        vm.run_chunk(&chunk)
            .expect("targeted declarations mutate class");

        assert_eq!(vm.class_table("User"), Some("users"));
        assert_eq!(
            vm.class_fields("User"),
            Some(["email".to_string()].as_slice())
        );
        let user = vm.new_instance("User").expect("instance created");
        assert_eq!(
            vm.get_field(&user, "email").expect("field reads"),
            Value::Nil
        );
    }

    #[test]
    fn targeted_field_preserves_the_stack_for_an_unknown_class() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Missing".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::CallWord("Field".to_string()), span());
        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnknownClass("Missing".to_string()))
        );
        assert_eq!(
            vm.stack(),
            &[
                Value::String("Missing".to_string()),
                Value::String("name".to_string())
            ]
        );
    }

    #[test]
    fn map_member_get_reads_entries_and_missing_entries_are_nil() {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), Value::String("42".to_string()));

        let mut vm = Vm::default();
        vm.stack.push(Value::Map(params.clone().into()));
        vm.stack.push(Value::Member("id".to_string()));
        vm.call_word("get").expect("map member get succeeds");
        assert_eq!(vm.stack(), &[Value::String("42".to_string())]);

        vm.stack.clear();
        vm.stack.push(Value::Map(params.into()));
        vm.stack.push(Value::Member("missing".to_string()));
        vm.call_word("get")
            .expect("missing map member get succeeds");
        assert_eq!(vm.stack(), &[Value::Nil]);
    }

    #[test]
    fn run_chunk_handles_class_field_declarations() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(
            Op::BeginClass {
                name: "Post".to_string(),
                superclass: "".to_string(),
            },
            span(),
        );
        chunk.push(Op::AddField("title".to_string()), span());
        chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("class opcodes run");

        let instance = vm.new_instance("Post").expect("instance is created");
        assert_eq!(
            vm.get_field(&instance, "title").expect("field reads"),
            Value::Nil
        );
    }

    #[test]
    fn bytecode_method_reads_field_through_self_and_get() {
        let mut chunk = Chunk::new("test.rco");
        let mut display_name = Chunk::new("test.rco");
        display_name.push(Op::CallWord("self".to_string()), span());
        display_name.push(Op::CallMethod("email".to_string()), span());
        display_name.push(Op::CallWord("get".to_string()), span());
        display_name.push(Op::Return, span());

        let display_name_block = chunk.push_block(display_name);
        chunk.push(
            Op::BeginClass {
                name: "User".to_string(),
                superclass: "Model".to_string(),
            },
            span(),
        );
        chunk.push(Op::AddField("email".to_string()), span());
        chunk.push(
            Op::AddMethod {
                name: "displayName".to_string(),
                block: display_name_block,
                args: None,
            },
            span(),
        );
        chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("class opcodes run");

        let user = vm.new_instance("User").expect("instance is created");
        let user = vm
            .set_field(user, "email", Value::String("ada@example.com".to_string()))
            .expect("field writes");

        assert_eq!(
            vm.call_method_value(user, "displayName")
                .expect("bytecode method is called"),
            Value::String("ada@example.com".to_string())
        );
    }

    #[test]
    fn call_method_opcode_dispatches_bytecode_method_from_stack() {
        let mut class_chunk = Chunk::new("test.rco");
        let mut display_name = Chunk::new("test.rco");
        display_name.push(Op::CallWord("self".to_string()), span());
        display_name.push(Op::CallMethod("email".to_string()), span());
        display_name.push(Op::CallWord("get".to_string()), span());
        display_name.push(Op::Return, span());

        let display_name_block = class_chunk.push_block(display_name);
        class_chunk.push(
            Op::BeginClass {
                name: "User".to_string(),
                superclass: "Model".to_string(),
            },
            span(),
        );
        class_chunk.push(Op::AddField("email".to_string()), span());
        class_chunk.push(
            Op::AddMethod {
                name: "displayName".to_string(),
                block: display_name_block,
                args: None,
            },
            span(),
        );
        class_chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&class_chunk).expect("class opcodes run");
        let user = vm.new_instance("User").expect("instance is created");
        let user = vm
            .set_field(user, "email", Value::String("ada@example.com".to_string()))
            .expect("field writes");

        let mut call_chunk = Chunk::new("test.rco");
        call_chunk.push(Op::CallMethod("displayName".to_string()), span());
        vm.stack.push(user);
        vm.run_chunk(&call_chunk).expect("method call opcode runs");

        assert_eq!(vm.stack(), &[Value::String("ada@example.com".to_string())]);
    }

    #[test]
    fn debug_trace_records_bytecode_method_frame_events() {
        let mut class_chunk = Chunk::new("test.rco");
        let mut display_name = Chunk::new("test.rco");
        display_name.push(Op::CallWord("self".to_string()), span());
        display_name.push(Op::CallMethod("email".to_string()), span());
        display_name.push(Op::CallWord("get".to_string()), span());
        display_name.push(Op::Return, span());

        let display_name_block = class_chunk.push_block(display_name);
        class_chunk.push(
            Op::BeginClass {
                name: "User".to_string(),
                superclass: "Model".to_string(),
            },
            span(),
        );
        class_chunk.push(Op::AddField("email".to_string()), span());
        class_chunk.push(
            Op::AddMethod {
                name: "displayName".to_string(),
                block: display_name_block,
                args: None,
            },
            span(),
        );
        class_chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.enable_debug();
        vm.run_chunk(&class_chunk).expect("class opcodes run");
        vm.clear_debug_events();

        let user = vm.new_instance("User").expect("instance is created");
        let user = vm
            .set_field(user, "email", Value::String("ada@example.com".to_string()))
            .expect("field writes");

        let mut call_chunk = Chunk::new("test.rco");
        call_chunk.push(Op::CallMethod("displayName".to_string()), span());
        vm.stack.push(user);
        vm.run_chunk(&call_chunk).expect("method call opcode runs");

        assert!(vm.debug_events().iter().any(|event| {
            matches!(
                event,
                DebugEvent::Instruction { frame, opcode, .. }
                    if frame == "User.displayName" && opcode == "CallWord(\"self\")"
            )
        }));
    }

    #[test]
    fn new_get_and_set_are_postfix_words_for_instances() {
        let mut class_chunk = Chunk::new("test.rco");
        class_chunk.push(
            Op::BeginClass {
                name: "User".to_string(),
                superclass: "Model".to_string(),
            },
            span(),
        );
        class_chunk.push(Op::AddField("email".to_string()), span());
        class_chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&class_chunk).expect("class opcodes run");

        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("User".to_string()), span());
        chunk.push(Op::CallWord("new".to_string()), span());
        chunk.push(Op::PushString("ada@example.com".to_string()), span());
        chunk.push(Op::CallWord("swap".to_string()), span());
        chunk.push(Op::CallMethod("email".to_string()), span());
        chunk.push(Op::CallWord("set".to_string()), span());
        chunk.push(Op::CallWord("dup".to_string()), span());
        chunk.push(Op::CallMethod("email".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());

        vm.run_chunk(&chunk).expect("object field words run");

        assert!(
            matches!(vm.stack(), [Value::Instance(_), Value::String(email)] if email == "ada@example.com")
        );
    }

    #[test]
    fn variable_words_declare_set_and_get_named_values() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("amount".to_string()), span());
        chunk.push(Op::CallWord("var".to_string()), span());
        chunk.push(Op::PushNumber(100), span());
        chunk.push(Op::PushString("amount".to_string()), span());
        chunk.push(Op::CallWord("set".to_string()), span());
        chunk.push(Op::PushString("amount".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("variable words run");

        assert_eq!(vm.stack(), &[Value::Number(100)]);
        assert_eq!(vm.variable("amount"), Some(&Value::Number(100)));
    }

    #[test]
    fn var_word_captures_top_stack_value_when_available() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::CallWord("var".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("var captures stack value");

        assert_eq!(vm.stack(), &[Value::String("Ada".to_string())]);
        assert_eq!(vm.variable("name"), Some(&Value::String("Ada".to_string())));
    }

    #[test]
    fn bytecode_function_local_declarations_refresh_within_call_frame() {
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushString("first".to_string()), span());
        function.push(Op::PushString("local".to_string()), span());
        function.push(Op::CallWord("var".to_string()), span());
        function.push(Op::PushString("second".to_string()), span());
        function.push(Op::PushString("local".to_string()), span());
        function.push(Op::CallWord("var".to_string()), span());
        function.push(Op::PushString("local".to_string()), span());
        function.push(Op::CallWord("get".to_string()), span());
        function.push(Op::Return, span());

        let mut chunk = Chunk::new("test.rco");
        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "capture".to_string(),
                block,
                args: None,
            },
            span(),
        );
        chunk.push(Op::CallWord("capture".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function call runs");

        assert_eq!(vm.stack(), &[Value::String("second".to_string())]);
        assert_eq!(vm.variable("local"), None);
    }

    #[test]
    fn bytecode_function_locals_do_not_leak_between_calls() {
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushString("local".to_string()), span());
        function.push(Op::CallWord("var".to_string()), span());
        function.push(Op::PushString("local".to_string()), span());
        function.push(Op::CallWord("get".to_string()), span());
        function.push(Op::Return, span());

        let mut chunk = Chunk::new("test.rco");
        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "capture".to_string(),
                block,
                args: Some(ArgsSpec {
                    inputs: vec!["value".to_string()],
                    outputs: Vec::new(),
                }),
            },
            span(),
        );
        chunk.push(Op::PushString("first".to_string()), span());
        chunk.push(Op::CallWord("capture".to_string()), span());
        chunk.push(Op::PushString("second".to_string()), span());
        chunk.push(Op::CallWord("capture".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function calls run");

        assert_eq!(
            vm.stack(),
            &[
                Value::String("first".to_string()),
                Value::String("second".to_string())
            ]
        );
        assert_eq!(vm.variable("local"), None);
    }

    #[test]
    fn bytecode_method_locals_do_not_leak_between_calls() {
        let mut method = Chunk::new("test.rco");
        method.push(Op::PushString("local".to_string()), span());
        method.push(Op::CallWord("var".to_string()), span());
        method.push(Op::PushString("local".to_string()), span());
        method.push(Op::CallWord("get".to_string()), span());
        method.push(Op::Return, span());

        let mut vm = Vm::default();
        vm.define_class("Probe", "Object")
            .expect("class should be defined");
        vm.add_bytecode_method(
            "capture",
            method,
            Some(ArgsSpec {
                inputs: vec!["value".to_string()],
                outputs: Vec::new(),
            }),
        )
        .expect("method should be added");
        vm.end_class();

        let first = vm.new_instance("Probe").expect("instance");
        vm.stack.push(Value::String("first".to_string()));
        let first_result = vm
            .call_method_value(first, "capture")
            .expect("first method call runs");

        let second = vm.new_instance("Probe").expect("instance");
        vm.stack.push(Value::String("second".to_string()));
        let second_result = vm
            .call_method_value(second, "capture")
            .expect("second method call runs");

        assert_eq!(first_result, Value::String("first".to_string()));
        assert_eq!(second_result, Value::String("second".to_string()));
        assert_eq!(vm.stack(), &[]);
        assert_eq!(vm.variable("local"), None);
    }

    #[test]
    fn println_word_records_output_and_consumes_value() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("Hello Ricochet".to_string()), span());
        chunk.push(Op::CallWord("println".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("println runs");

        assert_eq!(vm.stack(), &[]);
        assert_eq!(vm.output_lines(), &["Hello Ricochet".to_string()]);
    }

    #[test]
    fn view_word_returns_view_action_map() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("title".to_string()), span());
        chunk.push(Op::CallWord("var".to_string()), span());
        chunk.push(Op::PushString("Hello Ricochet".to_string()), span());
        chunk.push(Op::PushString("title".to_string()), span());
        chunk.push(Op::CallWord("set".to_string()), span());
        chunk.push(Op::PushString("ctx".to_string()), span());
        chunk.push(Op::CallWord("var".to_string()), span());
        chunk.push(Op::PushString("ctx".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());
        chunk.push(Op::PushString("home/index".to_string()), span());
        chunk.push(Op::CallWord("swap".to_string()), span());
        chunk.push(Op::CallWord("view".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("view word runs");

        let [Value::Map(action)] = vm.stack() else {
            panic!("expected one action map on stack, got {:?}", vm.stack());
        };
        assert_eq!(
            vm.variable("title"),
            Some(&Value::String("Hello Ricochet".to_string()))
        );
        assert_eq!(action.get("type"), Some(Value::String("view".to_string())));
        assert_eq!(
            action.get("name"),
            Some(Value::String("home/index".to_string()))
        );
    }

    #[test]
    fn text_word_returns_text_action_map() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("ctx".to_string()), span());
        chunk.push(Op::CallWord("var".to_string()), span());
        chunk.push(Op::PushString("ctx".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());
        chunk.push(Op::PushString("pong".to_string()), span());
        chunk.push(Op::CallWord("swap".to_string()), span());
        chunk.push(Op::CallWord("text".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("text word runs");

        let [Value::Map(action)] = vm.stack() else {
            panic!("expected one action map on stack, got {:?}", vm.stack());
        };
        assert_eq!(action.get("type"), Some(Value::String("text".to_string())));
        assert_eq!(action.get("body"), Some(Value::String("pong".to_string())));
    }

    #[test]
    fn json_word_returns_json_action_map() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("ok".to_string()), span());
        chunk.push(Op::CallWord("json".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("json word runs");

        let [Value::Map(action)] = vm.stack() else {
            panic!("expected one action map on stack, got {:?}", vm.stack());
        };
        assert_eq!(action.get("type"), Some(Value::String("json".to_string())));
        assert_eq!(action.get("body"), Some(Value::String("ok".to_string())));
    }

    #[test]
    fn webview_word_requires_enabled_capability() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("webview".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::HostError {
                word: "webview".to_string(),
                message: "webview capability is not enabled".to_string(),
            })
        );
    }

    #[test]
    fn webview_words_build_escaped_document_map() {
        let mut button = Chunk::new("test.rco");
        button.push(Op::PushString("Save <Now>".to_string()), span());
        button.push(Op::PushString("save\"now".to_string()), span());
        button.push(Op::CallWord("webview".to_string()), span());
        button.push(Op::CallMethod("button".to_string()), span());

        let mut vm = Vm::default();
        vm.set_webview_enabled(true);
        vm.run_chunk(&button).expect("button builds");

        let button_html = vm.pop("test").expect("button html is on the stack");
        assert_eq!(
            button_html,
            Value::String(
                r#"<button type="button" data-rco-action="save&quot;now">Save &lt;Now&gt;</button>"#
                    .to_string()
            )
        );

        let mut window = Chunk::new("test.rco");
        window.push(Op::PushString("Counter & Tools".to_string()), span());
        window.push(Op::PushString("<h1>Ready</h1>".to_string()), span());
        window.push(Op::CallWord("webview".to_string()), span());
        window.push(Op::CallMethod("window".to_string()), span());

        vm.run_chunk(&window).expect("window builds");

        let [Value::Result(RicochetResult::Ok(document))] = vm.stack() else {
            panic!(
                "expected one document result on stack, got {:?}",
                vm.stack()
            );
        };
        let Value::Map(document) = document.as_ref() else {
            panic!("expected webview document map, got {document:?}");
        };
        assert_eq!(
            document.get("type"),
            Some(Value::String("webview".to_string()))
        );
        assert_eq!(
            document.get("title"),
            Some(Value::String("Counter & Tools".to_string()))
        );
        assert_eq!(document.get("width"), Some(Value::Number(800)));
        assert_eq!(document.get("height"), Some(Value::Number(600)));
        let Some(Value::Map(state)) = document.get("state") else {
            panic!("expected default state map, got {document:?}");
        };
        assert!(state.is_empty());
        let Some(Value::Array(actions)) = document.get("actions") else {
            panic!("expected default actions array, got {document:?}");
        };
        assert!(actions.is_empty());

        let Some(Value::String(html)) = document.get("html") else {
            panic!("expected document html, got {document:?}");
        };
        assert!(html.contains("<title>Counter &amp; Tools</title>"));
        assert!(html.contains("<h1>Ready</h1>"));
        assert!(html.contains("window.__RICOCHET_STATE__"));
    }

    #[test]
    fn get_fails_loudly_for_unknown_variables() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("typo".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnknownVariable("typo".to_string()))
        );
        assert_eq!(vm.stack(), &[Value::String("typo".to_string())]);
    }

    #[test]
    fn set_fails_loudly_for_unknown_variables_and_preserves_stack() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushString("value".to_string()), span());
        chunk.push(Op::PushString("typo".to_string()), span());
        chunk.push(Op::CallWord("set".to_string()), span());

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UnknownVariable("typo".to_string()))
        );
        assert_eq!(
            vm.stack(),
            &[
                Value::String("value".to_string()),
                Value::String("typo".to_string())
            ]
        );
    }

    #[test]
    fn jump_if_false_executes_then_branch_for_truthy_condition() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushBool(true), span());
        chunk.push(Op::JumpIfFalse(4), span());
        chunk.push(Op::PushString("yes".to_string()), span());
        chunk.push(Op::Jump(5), span());
        chunk.push(Op::PushString("no".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("if runs");

        assert_eq!(vm.stack(), &[Value::String("yes".to_string())]);
    }

    #[test]
    fn jump_if_false_executes_else_branch_for_falsey_condition() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushBool(false), span());
        chunk.push(Op::JumpIfFalse(4), span());
        chunk.push(Op::PushString("yes".to_string()), span());
        chunk.push(Op::Jump(5), span());
        chunk.push(Op::PushString("no".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("if runs");

        assert_eq!(vm.stack(), &[Value::String("no".to_string())]);
    }

    #[test]
    fn result_values_cannot_be_used_as_conditions() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::JumpIfFalse(1), span());

        let mut vm = Vm::default();
        vm.stack
            .push(Value::result_ok(Value::String("ok".to_string())));

        assert_eq!(vm.run_chunk(&chunk), Err(VmError::UncheckedResultCondition));
    }

    #[test]
    fn instruction_limit_faults_runaway_loop() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::PushBool(true), span());
        chunk.push(Op::JumpIfFalse(3), span());
        chunk.push(Op::Jump(0), span());
        let mut vm = Vm::default();
        vm.set_instruction_limit(16);

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::InstructionLimitExceeded { limit: 16 })
        );
    }

    #[test]
    fn await_retains_failed_task_status_for_inspection() {
        let mut task = Chunk::new("test.rco");
        task.push(Op::PushBool(true), span());
        task.push(Op::JumpIfFalse(3), span());
        task.push(Op::Jump(0), span());

        let mut chunk = Chunk::new("test.rco");
        let task_block = chunk.push_block(task);
        chunk.push(Op::PushBlock(task_block), span());
        chunk.push(Op::CallWord("spawn".to_string()), span());

        let mut vm = Vm::default();
        vm.set_instruction_limit(16);
        vm.run_chunk(&chunk).expect("spawn succeeds");

        assert_eq!(vm.stack(), &[Value::Task(0)]);

        let expected = Err(VmError::InstructionLimitExceeded { limit: 16 });
        assert_eq!(vm.call_word("await"), expected);
        assert_eq!(vm.stack(), &[Value::Task(0)]);
        assert_eq!(vm.task_status(0), "failed");
        assert!(!vm.task_pending(0));
        assert!(!vm.task_completed(0));
        assert!(vm.task_failed(0));

        assert_eq!(vm.call_word("await"), expected);
        assert_eq!(vm.stack(), &[Value::Task(0)]);
    }

    #[test]
    fn await_all_resolves_tasks_in_order_and_retains_completed_status() {
        let mut first = Chunk::new("test.rco");
        first.push(Op::PushNumber(50), span());
        first.push(Op::CallWord("sleep".to_string()), span());
        first.push(Op::PushNumber(1), span());
        let mut second = Chunk::new("test.rco");
        second.push(Op::PushNumber(50), span());
        second.push(Op::CallWord("sleep".to_string()), span());
        second.push(Op::PushNumber(2), span());

        let mut chunk = Chunk::new("test.rco");
        let first_block = chunk.push_block(first);
        let second_block = chunk.push_block(second);
        chunk.push(Op::PushBlock(first_block), span());
        chunk.push(Op::CallWord("spawn".to_string()), span());
        chunk.push(Op::PushBlock(second_block), span());
        chunk.push(Op::CallWord("spawn".to_string()), span());

        let mut vm = Vm::default();
        vm.set_sleep_enabled(true);
        vm.run_chunk(&chunk).expect("spawn succeeds");

        assert_eq!(vm.stack(), &[Value::Task(0), Value::Task(1)]);
        assert_eq!(vm.pending_task_ids(), vec![0, 1]);
        assert_eq!(vm.task_status(0), "running");
        assert!(vm.task_running(0));

        vm.stack.clear();
        vm.stack
            .push(Value::Array(vec![Value::Task(0), Value::Task(1)].into()));
        vm.call_word("await_all").expect("await_all resolves tasks");

        assert_eq!(
            vm.stack(),
            &[Value::Array(
                vec![Value::Number(1), Value::Number(2)].into()
            )]
        );
        assert_eq!(vm.task_status(0), "completed");
        assert_eq!(vm.task_status(1), "completed");
        assert!(vm.pending_task_ids().is_empty());

        vm.stack.clear();
        vm.stack
            .push(Value::Array(vec![Value::Task(0), Value::Task(1)].into()));
        vm.call_word("await_all")
            .expect("await_all reuses completed task results");
        assert_eq!(
            vm.stack(),
            &[Value::Array(
                vec![Value::Number(1), Value::Number(2)].into()
            )]
        );
    }

    #[test]
    fn await_all_retains_failed_task_status_and_preserves_stack_on_error() {
        let mut first = Chunk::new("test.rco");
        first.push(Op::PushNumber(1), span());
        let mut second = Chunk::new("test.rco");
        second.push(Op::PushBool(true), span());
        second.push(Op::JumpIfFalse(3), span());
        second.push(Op::Jump(0), span());

        let mut chunk = Chunk::new("test.rco");
        let first_block = chunk.push_block(first);
        let second_block = chunk.push_block(second);
        chunk.push(Op::PushBlock(first_block), span());
        chunk.push(Op::CallWord("spawn".to_string()), span());
        chunk.push(Op::PushBlock(second_block), span());
        chunk.push(Op::CallWord("spawn".to_string()), span());

        let mut vm = Vm::default();
        vm.set_instruction_limit(16);
        vm.run_chunk(&chunk).expect("spawn succeeds");
        vm.stack.clear();
        vm.stack
            .push(Value::Array(vec![Value::Task(0), Value::Task(1)].into()));

        assert_eq!(
            vm.call_word("await_all"),
            Err(VmError::InstructionLimitExceeded { limit: 16 })
        );
        assert_eq!(
            vm.stack(),
            &[Value::Array(vec![Value::Task(0), Value::Task(1)].into())]
        );
        assert_eq!(vm.task_status(0), "completed");
        assert_eq!(vm.task_status(1), "failed");
    }

    #[test]
    fn call_word_dispatches_bytecode_function() {
        let mut chunk = Chunk::new("test.rco");
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushString("hi".to_string()), span());
        function.push(Op::Return, span());

        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "hello".to_string(),
                block,
                args: None,
            },
            span(),
        );
        chunk.push(Op::CallWord("hello".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function call runs");

        assert_eq!(vm.stack(), &[Value::String("hi".to_string())]);
    }

    #[test]
    fn call_word_dispatches_bytecode_function_with_question_mark_suffix() {
        let mut chunk = Chunk::new("test.rco");
        let mut function = Chunk::new("test.rco");
        function.push(Op::PushBool(true), span());
        function.push(Op::Return, span());

        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "ready?".to_string(),
                block,
                args: None,
            },
            span(),
        );
        chunk.push(Op::CallWord("ready?".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function call runs");

        assert_eq!(vm.stack(), &[Value::Bool(true)]);
    }

    #[test]
    fn known_predicate_word_still_dispatches_receiver_method() {
        let mut chunk = Chunk::new("test.rco");
        let mut method = Chunk::new("test.rco");
        method.push(Op::PushString("method".to_string()), span());
        method.push(Op::Return, span());

        let block = chunk.push_block(method);
        chunk.push(
            Op::BeginClass {
                name: "Widget".to_string(),
                superclass: "Object".to_string(),
            },
            span(),
        );
        chunk.push(
            Op::AddMethod {
                name: "empty?".to_string(),
                block,
                args: None,
            },
            span(),
        );
        chunk.push(Op::EndClass, span());
        chunk.push(Op::CallWord("Widget".to_string()), span());
        chunk.push(Op::CallWord("new".to_string()), span());
        chunk.push(Op::CallWord("empty?".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("receiver method call runs");

        assert_eq!(vm.stack(), &[Value::String("method".to_string())]);
    }

    #[test]
    fn run_chunk_preserves_function_args_metadata() {
        let mut chunk = Chunk::new("test.rco");
        let mut function = Chunk::new("test.rco");
        function.push(Op::Return, span());

        let args = ArgsSpec {
            inputs: vec!["user".to_string(), "request".to_string()],
            outputs: vec!["response".to_string()],
        };
        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "render".to_string(),
                block,
                args: Some(args.clone()),
            },
            span(),
        );

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function definition loads");

        assert_eq!(vm.function_args("render"), Some(&args));
    }

    #[test]
    fn declared_arg_function_uses_args_as_call_frame_inputs() {
        let mut chunk = Chunk::new("test.rco");
        let mut function = Chunk::new("test.rco");
        function.push(Op::CallWord("+".to_string()), span());
        function.push(Op::Return, span());

        let block = chunk.push_block(function);
        chunk.push(
            Op::AddFunction {
                name: "sum".to_string(),
                block,
                args: Some(ArgsSpec {
                    inputs: vec!["left".to_string(), "right".to_string()],
                    outputs: vec!["Number".to_string()],
                }),
            },
            span(),
        );
        chunk.push(Op::PushNumber(2), span());
        chunk.push(Op::PushNumber(3), span());
        chunk.push(Op::CallWord("sum".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("function call runs");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
    }

    #[test]
    fn run_chunk_preserves_method_args_metadata() {
        let mut chunk = Chunk::new("test.rco");
        let mut method = Chunk::new("test.rco");
        method.push(Op::Return, span());

        let args = ArgsSpec {
            inputs: vec!["ctx".to_string()],
            outputs: vec!["response".to_string()],
        };
        let block = chunk.push_block(method);
        chunk.push(
            Op::BeginClass {
                name: "HomeController".to_string(),
                superclass: "Controller".to_string(),
            },
            span(),
        );
        chunk.push(
            Op::AddMethod {
                name: "index".to_string(),
                block,
                args: Some(args.clone()),
            },
            span(),
        );
        chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("class definition loads");

        assert_eq!(vm.method_args("HomeController", "index"), Some(&args));
    }

    #[test]
    fn declared_arg_method_uses_args_as_call_frame_inputs() {
        let mut chunk = Chunk::new("test.rco");
        let mut method = Chunk::new("test.rco");
        method.push(Op::CallWord("+".to_string()), span());
        method.push(Op::Return, span());

        let block = chunk.push_block(method);
        chunk.push(
            Op::BeginClass {
                name: "Calculator".to_string(),
                superclass: "Object".to_string(),
            },
            span(),
        );
        chunk.push(
            Op::AddMethod {
                name: "sum".to_string(),
                block,
                args: Some(ArgsSpec {
                    inputs: vec!["left".to_string(), "right".to_string()],
                    outputs: vec!["Number".to_string()],
                }),
            },
            span(),
        );
        chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("class definition loads");
        let calculator = vm.new_instance("Calculator").expect("instance is created");

        let mut call_chunk = Chunk::new("test.rco");
        call_chunk.push(Op::CallMethod("sum".to_string()), span());
        vm.stack.push(Value::Number(2));
        vm.stack.push(Value::Number(3));
        vm.stack.push(calculator);
        vm.run_chunk(&call_chunk).expect("method call runs");

        assert_eq!(vm.stack(), &[Value::Number(5)]);
    }

    #[test]
    fn push_block_pushes_first_class_block_value() {
        let mut chunk = Chunk::new("test.rco");
        let mut block = Chunk::new("test.rco");
        block.push(Op::PushString("ok".to_string()), span());
        block.push(Op::Return, span());

        let block_index = chunk.push_block(block.clone());
        chunk.push(Op::PushBlock(block_index), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("block literal runs");

        assert_eq!(vm.stack(), &[Value::Block(block)]);
    }

    #[test]
    fn call_word_executes_first_class_block() {
        let mut chunk = Chunk::new("test.rco");
        let mut block = Chunk::new("test.rco");
        block.push(Op::PushString("ok".to_string()), span());
        block.push(Op::Return, span());

        let block_index = chunk.push_block(block);
        chunk.push(Op::PushBlock(block_index), span());
        chunk.push(Op::CallWord("call".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("block call runs");

        assert_eq!(vm.stack(), &[Value::String("ok".to_string())]);
    }

    #[test]
    fn send_word_dispatches_dynamic_method_name() {
        let mut class_chunk = Chunk::new("test.rco");
        let mut display_name = Chunk::new("test.rco");
        display_name.push(Op::CallWord("self".to_string()), span());
        display_name.push(Op::CallMethod("email".to_string()), span());
        display_name.push(Op::CallWord("get".to_string()), span());
        display_name.push(Op::Return, span());

        let display_name_block = class_chunk.push_block(display_name);
        class_chunk.push(
            Op::BeginClass {
                name: "User".to_string(),
                superclass: "Model".to_string(),
            },
            span(),
        );
        class_chunk.push(Op::AddField("email".to_string()), span());
        class_chunk.push(
            Op::AddMethod {
                name: "displayName".to_string(),
                block: display_name_block,
                args: None,
            },
            span(),
        );
        class_chunk.push(Op::EndClass, span());

        let mut vm = Vm::default();
        vm.run_chunk(&class_chunk).expect("class opcodes run");
        let user = vm.new_instance("User").expect("instance is created");
        let user = vm
            .set_field(user, "email", Value::String("ada@example.com".to_string()))
            .expect("field writes");

        let mut call_chunk = Chunk::new("test.rco");
        call_chunk.push(Op::PushString("displayName".to_string()), span());
        call_chunk.push(Op::CallWord("send".to_string()), span());
        vm.stack.push(user);
        vm.run_chunk(&call_chunk).expect("send runs");

        assert_eq!(vm.stack(), &[Value::String("ada@example.com".to_string())]);
    }

    #[test]
    fn class_add_method_reports_invalid_block_index() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(
            Op::AddMethod {
                name: "render".to_string(),
                block: 0,
                args: None,
            },
            span(),
        );

        let mut vm = Vm::default();

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::InvalidBlock {
                index: 0,
                available: 0,
            })
        );
    }
}
