use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
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
