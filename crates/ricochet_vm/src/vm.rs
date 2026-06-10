use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ricochet_bytecode::{ArgsSpec, Chunk, Op, SourceSpan};
use thiserror::Error;

use crate::class::{BytecodeCallable, Class, NativeMethod};
use crate::debug::{DebugAction, DebugEvent, DebugPause, DebugPauseReason};
use crate::object::Instance;
use crate::result::RicochetResult;
use crate::value::Value;

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
    #[error("assert-equals failed: expected {expected}, got {actual}")]
    AssertionFailed { expected: String, actual: String },
    #[error("execution aborted in {frame} at {location}")]
    ExecutionAborted {
        frame: String,
        location: String,
    },
}

type DebugSink = Rc<RefCell<dyn FnMut(&DebugEvent)>>;
type DebugController = Rc<RefCell<dyn FnMut(&DebugPause) -> DebugAction>>;

#[derive(Clone, Default)]
pub struct Vm {
    stack: Vec<Value>,
    variables: BTreeMap<String, Value>,
    functions: BTreeMap<String, BytecodeCallable>,
    classes: BTreeMap<String, Class>,
    current_class: Option<String>,
    self_stack: Vec<Value>,
    output_lines: Vec<String>,
    debug_enabled: bool,
    debug_events: Vec<DebugEvent>,
    debug_sink: Option<DebugSink>,
    debug_controller: Option<DebugController>,
    step_mode: bool,
    breakpoints: BTreeSet<(String, usize)>,
    suppressed_breakpoint: Option<(String, String, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionSignal {
    Continue,
    Jump(usize),
    Return,
}

impl Vm {
    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn variables(&self) -> &BTreeMap<String, Value> {
        &self.variables
    }

    pub fn variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn output_lines(&self) -> &[String] {
        &self.output_lines
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
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
        self.functions.get(name).and_then(|function| function.args.as_ref())
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
    }

    pub fn debug_events(&self) -> &[DebugEvent] {
        &self.debug_events
    }

    pub fn clear_debug_events(&mut self) {
        self.debug_events.clear();
    }

    pub fn add_line_breakpoint(&mut self, file: impl Into<String>, line: usize) {
        self.debug_enabled = true;
        self.breakpoints.insert((file.into(), line));
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

    pub fn add_native_method<F>(
        &mut self,
        name: impl Into<String>,
        method: F,
    ) -> Result<(), VmError>
    where
        F: Fn(Vec<Value>) -> Result<Value, VmError> + 'static,
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
        F: Fn(Vec<Value>) -> Result<Value, VmError> + 'static,
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
        let class = self
            .classes
            .get(class_name)
            .ok_or_else(|| VmError::UnknownClass(class_name.to_string()))?;
        let fields = class
            .fields
            .iter()
            .map(|field| (field.clone(), Value::Nil))
            .collect();

        Ok(Value::Instance(Instance::new(class.name.clone(), fields)))
    }

    pub fn set_field(
        &self,
        instance: Value,
        field: &str,
        value: Value,
    ) -> Result<Value, VmError> {
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
            Value::Instance(instance) => Ok(instance
                .fields
                .get(field)
                .cloned()
                .unwrap_or(Value::Nil)),
            Value::Map(map) => Ok(map.get(field).cloned().unwrap_or(Value::Nil)),
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
        match receiver {
            Value::Class(class_name) => {
                let method = self
                    .classes
                    .get(&class_name)
                    .ok_or_else(|| VmError::UnknownClass(class_name.clone()))?
                    .native_methods
                    .get(method_name)
                    .cloned()
                    .ok_or_else(|| VmError::UnknownMethod {
                        class_name: class_name.clone(),
                        method: method_name.to_string(),
                    })?;
                let frame = format!("{class_name}.{method_name}");
                self.call_native_method(Value::Class(class_name), &frame, &method)
            }
            Value::Instance(instance) => {
                let class_name = instance.class_name.clone();
                let receiver = Value::Instance(instance);
                let native_method = self
                    .classes
                    .get(&class_name)
                    .ok_or_else(|| VmError::UnknownClass(class_name.clone()))?
                    .native_methods
                    .get(method_name)
                    .cloned();

                if let Some(method) = native_method {
                    let frame = format!("{class_name}.{method_name}");
                    return self.call_native_method(receiver, &frame, &method);
                }

                let bytecode_method = self
                    .classes
                    .get(&class_name)
                    .ok_or_else(|| VmError::UnknownClass(class_name.clone()))?
                    .bytecode_methods
                    .get(method_name)
                    .cloned();

                if let Some(method) = bytecode_method {
                    let frame = format!("{class_name}.{method_name}");
                    let input_count = method
                        .args
                        .as_ref()
                        .map(|args| args.inputs.len())
                        .unwrap_or(0);
                    return self.call_bytecode_method(receiver, &frame, &method.chunk, input_count);
                }

                Err(VmError::UnknownMethod {
                        class_name: class_name.clone(),
                        method: method_name.to_string(),
                })
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
        self.run_chunk_with_frame(chunk, "<main>", false).map(|_| ())
    }

    fn run_chunk_with_frame(
        &mut self,
        chunk: &Chunk,
        frame: &str,
        allow_return: bool,
    ) -> Result<ExecutionSignal, VmError> {
        let mut ip = 0;
        while ip < chunk.instructions.len() {
            let instruction = &chunk.instructions[ip];
            self.pause_before_instruction(frame, instruction)?;
            let stack_before = self.debug_enabled.then(|| self.stack.clone());
            let source = self
                .debug_enabled
                .then(|| source_label(&instruction.span));
            let opcode = self
                .debug_enabled
                .then(|| format!("{:?}", &instruction.op));

            let result = self.execute_instruction(&instruction.op, chunk, allow_return);

            if let (Some(stack_before), Some(source), Some(opcode)) =
                (stack_before, source, opcode)
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
                    if self.debug_enabled {
                        self.record_debug_event(DebugEvent::Fault {
                            frame: frame.to_string(),
                            message: error.to_string(),
                            stack: self.stack.clone(),
                        });
                    }
                    return Err(error);
                }
            }
        }

        Ok(ExecutionSignal::Continue)
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
        if !self.step_mode && !breakpoint_hit {
            return Ok(());
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
        };
        self.record_debug_event(DebugEvent::Paused(pause.clone()));

        let action = self
            .debug_controller
            .clone()
            .map(|controller| (controller.borrow_mut())(&pause))
            .unwrap_or(DebugAction::Continue);

        match action {
            DebugAction::Step => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = true;
                Ok(())
            }
            DebugAction::Continue => {
                if breakpoint_hit {
                    self.suppressed_breakpoint = Some(breakpoint_site);
                }
                self.step_mode = false;
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
                let function =
                    chunk
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
            "equals" | "=" => self.call_equals(word),
            "not-equals?" | "!=" => self.call_not_equals(word),
            "assert-equals" => self.call_assert_equals(word),
            "less-than?" | "<" => self.call_number_comparison(word, |left, right| left < right),
            "greater-than?" | ">" => self.call_number_comparison(word, |left, right| left > right),
            "less-or-equals?" | "<=" => {
                self.call_number_comparison(word, |left, right| left <= right)
            }
            "greater-or-equals?" | ">=" => {
                self.call_number_comparison(word, |left, right| left >= right)
            }
            "self" => self.call_self(word),
            "get" => self.call_get(word),
            "set" => self.call_set(word),
            "var" => self.call_var(word),
            "table" => self.call_table(word),
            "new" => self.call_new(word),
            "swap" => self.call_swap(word),
            "dup" => self.call_dup(word),
            "drop" => self.call_drop(word),
            "over" => self.call_over(word),
            "rot" => self.call_rot(word),
            "call" => self.call_block(word),
            "send" => self.call_send(word),
            "println" => self.call_println(word),
            "view" => self.call_view(word),
            "text" => self.call_text(word),
            "json" => self.call_json(word),
            "value" => self.call_result_value(word),
            "error" => self.call_result_error(word),
            "array" => {
                self.stack.push(Value::Array(Vec::new()));
                Ok(())
            }
            "map" => {
                self.stack.push(Value::Map(BTreeMap::new()));
                Ok(())
            }
            "!push" => self.call_push(word),
            "!put" => self.call_put(word),
            predicate if predicate.ends_with('?') => self.call_predicate(predicate),
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

        Err(VmError::UnknownWord(name.to_string()))
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
                self.stack.push(Value::Map(BTreeMap::from([
                    ("kind".to_string(), Value::String(error.kind)),
                    ("message".to_string(), Value::String(error.message)),
                ])));
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

    fn call_bytecode_function(
        &mut self,
        name: &str,
        function: &Chunk,
        input_count: usize,
    ) -> Result<Value, VmError> {
        self.ensure_stack(name, input_count)?;
        let base = self.stack.len() - input_count;
        let run_result = self.run_chunk_with_frame(function, name, true);

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

    fn call_bytecode_block(&mut self, frame: &str, block: &Chunk) -> Result<Value, VmError> {
        let base = self.stack.len();
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

    fn call_method_or_member(&mut self, name: &str) -> Result<(), VmError> {
        let should_dispatch = match self.stack.last() {
            Some(Value::Class(class_name)) => self
                .classes
                .get(class_name)
                .ok_or_else(|| VmError::UnknownClass(class_name.clone()))?
                .native_methods
                .contains_key(name),
            Some(Value::Instance(instance)) => {
                let class = self
                    .classes
                    .get(&instance.class_name)
                    .ok_or_else(|| VmError::UnknownClass(instance.class_name.clone()))?;
                class.native_methods.contains_key(name) || class.bytecode_methods.contains_key(name)
            }
            _ => false,
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

        let run_result = self.run_chunk_with_frame(method, frame, true);
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
            Value::String(name) => {
                match self.variables.get(&name).cloned() {
                    Some(value) => {
                        self.stack.push(value);
                        Ok(())
                    }
                    None => {
                        self.stack = stack_before;
                        Err(VmError::UnknownVariable(name))
                    }
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
                if !self.variables.contains_key(&name) {
                    self.stack = stack_before;
                    return Err(VmError::UnknownVariable(name));
                }
                let value = match self.pop(word) {
                    Ok(value) => value,
                    Err(error) => {
                        self.stack = stack_before;
                        return Err(error);
                    }
                };
                self.variables.insert(name, value);
                Ok(())
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
        self.variables.entry(name).or_insert(value);
        Ok(())
    }

    fn call_new(&mut self, word: &str) -> Result<(), VmError> {
        let stack_before = self.stack.clone();
        let class_name = match self.pop(word)? {
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
        self.output_lines.push(output_string(&value));
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
        self.stack.push(Value::Map(action));
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
        self.stack.push(Value::Map(action));
        Ok(())
    }

    fn call_json(&mut self, word: &str) -> Result<(), VmError> {
        let body = self.pop(word)?;
        let mut action = BTreeMap::new();
        action.insert("type".to_string(), Value::String("json".to_string()));
        action.insert("body".to_string(), body);
        self.stack.push(Value::Map(action));
        Ok(())
    }

    fn call_add(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let left = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        let value = match left.checked_add(right) {
            Some(value) => value,
            None => {
                self.stack = stack_before;
                return Err(VmError::ArithmeticOverflow {
                    word: word.to_string(),
                });
            }
        };

        self.stack.push(Value::Number(value));

        Ok(())
    }

    fn call_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let right = self.pop_unchecked();
        let left = self.pop_unchecked();
        self.stack.push(Value::Bool(left == right));

        Ok(())
    }

    fn call_not_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let right = self.pop_unchecked();
        let left = self.pop_unchecked();
        self.stack.push(Value::Bool(left != right));

        Ok(())
    }

    fn call_assert_equals(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let expected = self.pop_unchecked();
        let actual = self.pop_unchecked();

        if actual == expected {
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
        compare: impl FnOnce(i64, i64) -> bool,
    ) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let stack_before = self.stack.clone();
        let right = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };
        let left = match self.pop_number(word) {
            Ok(value) => value,
            Err(error) => {
                self.stack = stack_before;
                return Err(error);
            }
        };

        self.stack.push(Value::Bool(compare(left, right)));

        Ok(())
    }

    fn call_push(&mut self, word: &str) -> Result<(), VmError> {
        self.ensure_stack(word, 2)?;
        let array = self.stack[self.stack.len() - 2].clone();
        let value = self.stack[self.stack.len() - 1].clone();

        match array {
            Value::Array(mut values) => {
                values.push(value);
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(Value::Array(values));
                Ok(())
            }
            array => Err(VmError::TypeError {
                word: word.to_string(),
                expected: "array".to_string(),
                actual: value_kind(&array).to_string(),
            }),
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

        let mut map = map;
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

    fn ensure_stack(&self, word: &str, needed: usize) -> Result<(), VmError> {
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

    fn pop(&mut self, word: &str) -> Result<Value, VmError> {
        let available = self.stack.len();
        self.stack.pop().ok_or_else(|| VmError::StackUnderflow {
            word: word.to_string(),
            needed: 1,
            available,
        })
    }

    fn pop_number(&mut self, word: &str) -> Result<i64, VmError> {
        match self.pop(word)? {
            Value::Number(value) => Ok(value),
            value => Err(VmError::TypeError {
                word: word.to_string(),
                expected: "number".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    fn pop_unchecked(&mut self) -> Value {
        self.stack
            .pop()
            .expect("stack length checked before pop")
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
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::Member(_) => "member selector",
        Value::Block(_) => "block",
        Value::Result(_) => "result",
    }
}

fn output_string(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        value => format!("{value:?}"),
    }
}

fn source_label(span: &SourceSpan) -> String {
    format!("{}:{}", span.file, span.line)
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
        chunk.push(
            Op::PushNumber(2),
            SourceSpan {
                line: 1,
                ..span()
            },
        );
        chunk.push(
            Op::PushNumber(3),
            SourceSpan {
                line: 2,
                ..span()
            },
        );
        chunk.push(
            Op::CallWord("+".to_string()),
            SourceSpan {
                line: 3,
                ..span()
            },
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
        assert_eq!(err.truthy(), true);
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
            &[Value::Map(BTreeMap::from([
                (
                    "kind".to_string(),
                    Value::String("DatabaseError".to_string()),
                ),
                (
                    "message".to_string(),
                    Value::String("connection failed".to_string()),
                ),
            ]))]
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
        equals_vm
            .run_chunk(&equals_chunk)
            .expect("equals succeeds");
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
        chunk.push(Op::CallWord("assert-equals".to_string()), span());

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
            vm.call_word("assert-equals"),
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
            (2, 3, "less-than?", true),
            (2, 3, "<", true),
            (3, 2, "greater-than?", true),
            (3, 2, ">", true),
            (3, 3, "less-or-equals?", true),
            (3, 3, "<=", true),
            (3, 3, "greater-or-equals?", true),
            (3, 3, ">=", true),
            (3, 3, "not-equals?", false),
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
            vm.call_word("less-than?"),
            Err(VmError::TypeError {
                word: "less-than?".to_string(),
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
    fn executes_array_push_word() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("array".to_string()), span());
        chunk.push(Op::PushNumber(42), span());
        chunk.push(Op::CallWord("!push".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("array push succeeds");

        assert_eq!(vm.stack(), &[Value::Array(vec![Value::Number(42)])]);
    }

    #[test]
    fn executes_map_put_word() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(Op::CallWord("map".to_string()), span());
        chunk.push(Op::PushString("name".to_string()), span());
        chunk.push(Op::PushString("Ada".to_string()), span());
        chunk.push(Op::CallWord("!put".to_string()), span());
        chunk.push(Op::CallMethod("name".to_string()), span());
        chunk.push(Op::CallWord("get".to_string()), span());

        let mut vm = Vm::default();
        vm.run_chunk(&chunk).expect("map put succeeds");

        assert_eq!(vm.stack(), &[Value::String("Ada".to_string())]);
    }

    #[test]
    fn map_put_type_errors_preserve_stack() {
        let mut vm = Vm::default();
        vm.stack.push(Value::Array(Vec::new()));
        vm.stack.push(Value::String("name".to_string()));
        vm.stack.push(Value::String("Ada".to_string()));

        assert_eq!(
            vm.call_word("!put"),
            Err(VmError::TypeError {
                word: "!put".to_string(),
                expected: "map".to_string(),
                actual: "array".to_string(),
            })
        );
        assert_eq!(
            vm.stack(),
            &[
                Value::Array(Vec::new()),
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
        empty_vm
            .run_chunk(&empty_chunk)
            .expect("empty? succeeds");
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
        vm.add_native_method("label", |_| {
            Ok(Value::String("old label".to_string()))
        })
        .expect("method is declared");

        vm.define_class("Widget", "").expect("class reopens");
        vm.add_native_method("label", |_| {
            Ok(Value::String("new label".to_string()))
        })
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
    fn native_method_with_arity_consumes_stack_arguments_in_order() {
        let mut vm = Vm::default();
        vm.define_class("Calculator", "Object")
            .expect("class begins");
        vm.add_native_method_with_arity("sum", 2, |args| {
            assert_eq!(args.len(), 3);
            assert_eq!(args[0], Value::Number(2));
            assert_eq!(args[1], Value::Number(3));
            assert!(matches!(&args[2], Value::Instance(instance) if instance.class_name == "Calculator"));
            Ok(Value::Number(5))
        })
        .expect("native method added");
        vm.end_class();

        vm.stack.push(Value::Number(2));
        vm.stack.push(Value::Number(3));
        let calculator = vm
            .new_instance("Calculator")
            .expect("calculator instance");

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
        let calculator = vm
            .new_instance("Calculator")
            .expect("calculator instance");

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
            vm.get_field(&updated, "missing").expect("missing field is nil"),
            Value::Nil
        );
    }

    #[test]
    fn table_word_records_model_table_on_current_class() {
        let mut vm = Vm::default();
        vm.define_class("User", "Model").expect("class begins");
        vm.stack.push(Value::String("users".to_string()));

        vm.call_word("table").expect("table word succeeds");
        vm.end_class();

        assert_eq!(vm.class_table("User"), Some("users"));
    }

    #[test]
    fn map_member_get_reads_entries_and_missing_entries_are_nil() {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), Value::String("42".to_string()));

        let mut vm = Vm::default();
        vm.stack.push(Value::Map(params.clone()));
        vm.stack.push(Value::Member("id".to_string()));
        vm.call_word("get").expect("map member get succeeds");
        assert_eq!(vm.stack(), &[Value::String("42".to_string())]);

        vm.stack.clear();
        vm.stack.push(Value::Map(params));
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
            .set_field(
                user,
                "email",
                Value::String("ada@example.com".to_string()),
            )
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
            .set_field(
                user,
                "email",
                Value::String("ada@example.com".to_string()),
            )
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
            .set_field(
                user,
                "email",
                Value::String("ada@example.com".to_string()),
            )
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

        assert!(matches!(vm.stack(), [Value::Instance(_), Value::String(email)] if email == "ada@example.com"));
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
        assert_eq!(
            action.get("type"),
            Some(&Value::String("view".to_string()))
        );
        assert_eq!(
            action.get("name"),
            Some(&Value::String("home/index".to_string()))
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
        assert_eq!(
            action.get("type"),
            Some(&Value::String("text".to_string()))
        );
        assert_eq!(
            action.get("body"),
            Some(&Value::String("pong".to_string()))
        );
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
        assert_eq!(
            action.get("type"),
            Some(&Value::String("json".to_string()))
        );
        assert_eq!(action.get("body"), Some(&Value::String("ok".to_string())));
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

        assert_eq!(
            vm.run_chunk(&chunk),
            Err(VmError::UncheckedResultCondition)
        );
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
            .set_field(
                user,
                "email",
                Value::String("ada@example.com".to_string()),
            )
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
