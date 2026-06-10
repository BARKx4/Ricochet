use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::value::Value;
use crate::vm::VmError;

pub type NativeMethod = Rc<dyn Fn(Vec<Value>) -> Result<Value, VmError>>;

#[derive(Clone)]
pub struct Class {
    pub name: String,
    pub superclass: String,
    pub fields: Vec<String>,
    pub native_methods: BTreeMap<String, NativeMethod>,
    pub revision: u64,
}

impl Class {
    pub fn new(name: impl Into<String>, superclass: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            superclass: superclass.into(),
            fields: Vec::new(),
            native_methods: BTreeMap::new(),
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

    pub fn add_native_method(&mut self, name: impl Into<String>, method: NativeMethod) {
        self.native_methods.insert(name.into(), method);
        self.revision += 1;
    }
}

impl fmt::Debug for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Class")
            .field("name", &self.name)
            .field("superclass", &self.superclass)
            .field("fields", &self.fields)
            .field("native_method_count", &self.native_methods.len())
            .field("revision", &self.revision)
            .finish()
    }
}
