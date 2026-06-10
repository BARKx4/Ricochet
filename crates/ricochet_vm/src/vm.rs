use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ricochet_bytecode::{Chunk, Op, SourceSpan};
use thiserror::Error;

use crate::class::{Class, NativeMethod};
use crate::debug::DebugEvent;
use crate::object::Instance;
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
    #[error("invalid jump target {target}: chunk has {available} instructions")]
    InvalidJump { target: usize, available: usize },
}

#[derive(Debug, Clone, Default)]
pub struct Vm {
    stack: Vec<Value>,
    variables: BTreeMap<String, Value>,
    classes: BTreeMap<String, Class>,
    current_class: Option<String>,
    self_stack: Vec<Value>,
    debug_enabled: bool,
    debug_events: Vec<DebugEvent>,
    breakpoints: BTreeSet<(String, usize)>,
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

    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    pub fn enable_debug(&mut self) {
        self.debug_enabled = true;
    }

    pub fn debug_events(&self) -> &[DebugEvent] {
        &self.debug_events
    }

    pub fn clear_debug_events(&mut self) {
        self.debug_events.clear();
    }

    pub fn add_line_breakpoint(&mut self, file: impl Into<String>, line: usize) {
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
        let method: NativeMethod = Rc::new(method);
        self.current_class_mut("add_native_method")?
            .add_native_method(name, method);
        Ok(())
    }

    pub fn add_bytecode_method(
        &mut self,
        name: impl Into<String>,
        method: Chunk,
    ) -> Result<(), VmError> {
        self.current_class_mut("add_bytecode_method")?
            .add_bytecode_method(name, method);
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
            value => Err(VmError::TypeError {
                word: format!("get_field {field}"),
                expected: "instance".to_string(),
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
                    return method(vec![receiver]);
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
                    return self.call_bytecode_method(receiver, &frame, &method);
                }

                Err(VmError::UnknownMethod {
                        class_name: class_name.clone(),
                        method: method_name.to_string(),
                })
            }
            value => Err(VmError::TypeError {
                word: format!("call_method {method_name}"),
                expected: "instance".to_string(),
                actual: value_kind(&value).to_string(),
            }),
        }
    }

    pub fn run_chunk(&mut self, chunk: &Chunk) -> Result<(), VmError> {
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
                self.debug_events.push(DebugEvent::Instruction {
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
                        self.debug_events.push(DebugEvent::Fault {
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
            Op::CallMethod(name) => self.call_method_or_member(name)?,
            Op::CallWord(word) => self.call_word(word)?,
            Op::BeginClass { name, superclass } => {
                self.define_class(name.clone(), superclass.clone())?
            }
            Op::EndClass => self.end_class(),
            Op::AddField(name) => self.add_field(name.clone())?,
            Op::AddMethod { name, block } => {
                let method = chunk
                    .blocks
                    .get(*block)
                    .cloned()
                    .ok_or(VmError::InvalidBlock {
                        index: *block,
                        available: chunk.blocks.len(),
                    })?;
                self.add_bytecode_method(name.clone(), method)?;
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
            "self" => self.call_self(word),
            "get" => self.call_get(word),
            "set" => self.call_set(word),
            "var" => self.call_var(word),
            "new" => self.call_new(word),
            "swap" => self.call_swap(word),
            "dup" => self.call_dup(word),
            "view" => self.call_view(word),
            "array" => {
                self.stack.push(Value::Array(Vec::new()));
                Ok(())
            }
            "!push" => self.call_push(word),
            predicate if predicate.ends_with('?') => self.call_predicate(predicate),
            _ => Err(VmError::UnknownWord(word.to_string())),
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

    fn call_method_or_member(&mut self, name: &str) -> Result<(), VmError> {
        let should_dispatch = match self.stack.last() {
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
    ) -> Result<Value, VmError> {
        let base = self.stack.len();
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
        self.variables.entry(name).or_insert(Value::Nil);
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
        Value::Instance(_) => "instance",
        Value::Member(_) => "member selector",
        Value::Result(_) => "result",
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
    use super::*;
    use crate::{debug::DebugEvent, value::Value};
    use ricochet_bytecode::{Chunk, Op, SourceSpan};

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
    fn class_add_method_reports_invalid_block_index() {
        let mut chunk = Chunk::new("test.rco");
        chunk.push(
            Op::AddMethod {
                name: "render".to_string(),
                block: 0,
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
