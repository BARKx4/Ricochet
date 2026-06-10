use std::collections::BTreeMap;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub class_name: String,
    pub fields: BTreeMap<String, Value>,
}

impl Instance {
    pub fn new(class_name: impl Into<String>, fields: BTreeMap<String, Value>) -> Self {
        Self {
            class_name: class_name.into(),
            fields,
        }
    }
}
