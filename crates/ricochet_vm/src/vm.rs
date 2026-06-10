use ricochet_bytecode::{Chunk, Op};
use thiserror::Error;

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
}

#[derive(Debug, Clone, Default)]
pub struct Vm {
    stack: Vec<Value>,
}

impl Vm {
    pub fn stack(&self) -> &[Value] {
        &self.stack
    }

    pub fn run_chunk(&mut self, chunk: &Chunk) -> Result<(), VmError> {
        for instruction in &chunk.instructions {
            match &instruction.op {
                Op::PushNil => self.stack.push(Value::Nil),
                Op::PushBool(value) => self.stack.push(Value::Bool(*value)),
                Op::PushNumber(value) => self.stack.push(Value::Number(*value)),
                Op::PushString(value) => self.stack.push(Value::String(value.clone())),
                Op::CallWord(word) => self.call_word(word)?,
                op => return Err(VmError::UnsupportedOpcode(format!("{op:?}"))),
            }
        }

        Ok(())
    }

    fn call_word(&mut self, word: &str) -> Result<(), VmError> {
        match word {
            "+" | "add" => self.call_add(word),
            "equals" | "=" => self.call_equals(word),
            "array" => {
                self.stack.push(Value::Array(Vec::new()));
                Ok(())
            }
            "!push" => self.call_push(word),
            predicate if predicate.ends_with('?') => self.call_predicate(predicate),
            _ => Err(VmError::UnknownWord(word.to_string())),
        }
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
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
        Value::Result(_) => "result",
    }
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
    use crate::value::Value;
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
}
