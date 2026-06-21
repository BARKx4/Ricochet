use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Step,
    StepOver,
    StepOut,
    Continue,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugPauseReason {
    Step,
    Breakpoint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugPause {
    pub reason: DebugPauseReason,
    pub frame: String,
    pub source: String,
    pub opcode: String,
    pub stack: Vec<Value>,
    pub globals: Vec<(String, Value)>,
    pub locals: Vec<(String, Value)>,
    pub current_self: Option<Value>,
    pub tasks: Vec<DebugTask>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugTask {
    pub id: u64,
    pub operation: String,
    pub status: String,
    pub pending: bool,
    pub running: bool,
    pub completed: bool,
    pub failed: bool,
    pub fault: Option<String>,
    pub frames: Vec<DebugTaskFrame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugTaskFrame {
    pub frame: String,
    pub source: String,
    pub opcode: String,
    pub stack: Vec<Value>,
    pub locals: Vec<(String, Value)>,
    pub current_self: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
    Paused(DebugPause),
    Instruction {
        frame: String,
        source: String,
        opcode: String,
        stack_before: Vec<Value>,
        stack_after: Vec<Value>,
    },
    Fault {
        frame: String,
        message: String,
        stack: Vec<Value>,
    },
}
