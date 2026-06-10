use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArgsSpec {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    PushNil,
    PushBool(bool),
    PushNumber(i64),
    PushString(String),
    PushBlock(usize),
    CallWord(String),
    CallMethod(String),
    Send,
    GetVar(String),
    SetVar(String),
    DeclareVar(String),
    BeginClass { name: String, superclass: String },
    EndClass,
    AddField(String),
    AddMethod {
        name: String,
        block: usize,
        args: Option<ArgsSpec>,
    },
    AddFunction {
        name: String,
        block: usize,
        args: Option<ArgsSpec>,
    },
    Return,
    JumpIfFalse(usize),
    Jump(usize),
    Pop,
}
