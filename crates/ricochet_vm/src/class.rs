use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use ricochet_bytecode::{ArgsSpec, Chunk};

use crate::value::Value;
use crate::vm::VmError;

type NativeMethodFunction = Arc<dyn Fn(Vec<Value>) -> Result<Value, VmError> + Send + Sync>;

#[derive(Clone)]
pub struct NativeMethod {
    pub input_count: usize,
    function: NativeMethodFunction,
}

impl NativeMethod {
    pub fn new<F>(input_count: usize, function: F) -> Self
    where
        F: Fn(Vec<Value>) -> Result<Value, VmError> + Send + Sync + 'static,
    {
        Self {
            input_count,
            function: Arc::new(function),
        }
    }

    pub fn call(&self, arguments: Vec<Value>) -> Result<Value, VmError> {
        (self.function)(arguments)
    }
}

#[derive(Clone)]
pub struct BytecodeCallable {
    pub chunk: Chunk,
    pub args: Option<ArgsSpec>,
}

impl BytecodeCallable {
    pub fn new(chunk: Chunk, args: Option<ArgsSpec>) -> Self {
        Self { chunk, args }
    }
}

#[derive(Clone)]
pub struct Class {
    pub name: String,
    pub superclass: String,
    pub table_name: Option<String>,
    pub fields: Vec<String>,
    pub native_methods: BTreeMap<String, NativeMethod>,
    pub bytecode_methods: BTreeMap<String, BytecodeCallable>,
    pub revision: u64,
}

impl Class {
    pub fn new(name: impl Into<String>, superclass: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            superclass: superclass.into(),
            table_name: None,
            fields: Vec::new(),
            native_methods: BTreeMap::new(),
            bytecode_methods: BTreeMap::new(),
            revision: 0,
        }
    }

    pub fn add_field(&mut self, name: impl Into<String>) -> bool {
        let name = name.into();
        if self.fields.contains(&name) {
            return false;
        }

        self.fields.push(name);
        self.revision += 1;
        true
    }

    pub fn set_table(&mut self, name: impl Into<String>) {
        self.table_name = Some(name.into());
        self.revision += 1;
    }

    pub fn add_native_method(&mut self, name: impl Into<String>, method: NativeMethod) {
        let name = name.into();
        self.bytecode_methods.remove(&name);
        self.native_methods.insert(name, method);
        self.revision += 1;
    }

    pub fn add_bytecode_method(
        &mut self,
        name: impl Into<String>,
        method: Chunk,
        args: Option<ArgsSpec>,
    ) {
        let name = name.into();
        self.native_methods.remove(&name);
        self.bytecode_methods
            .insert(name, BytecodeCallable::new(method, args));
        self.revision += 1;
    }
}

impl fmt::Debug for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Class")
            .field("name", &self.name)
            .field("superclass", &self.superclass)
            .field("table_name", &self.table_name)
            .field("fields", &self.fields)
            .field("native_method_count", &self.native_methods.len())
            .field("bytecode_method_count", &self.bytecode_methods.len())
            .field("revision", &self.revision)
            .finish()
    }
}
