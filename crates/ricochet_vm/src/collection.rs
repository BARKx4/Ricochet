use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use crate::value::Value;

macro_rules! shared_sequence {
    ($name:ident) => {
        #[derive(Clone, Default)]
        pub struct $name(Arc<RwLock<Vec<Value>>>);

        impl $name {
            pub fn new(values: Vec<Value>) -> Self {
                Self(Arc::new(RwLock::new(values)))
            }

            pub fn snapshot(&self) -> Vec<Value> {
                self.0.read().expect("collection lock poisoned").clone()
            }

            pub fn len(&self) -> usize {
                self.0.read().expect("collection lock poisoned").len()
            }

            pub fn is_empty(&self) -> bool {
                self.0.read().expect("collection lock poisoned").is_empty()
            }

            pub fn get(&self, index: usize) -> Option<Value> {
                self.0
                    .read()
                    .expect("collection lock poisoned")
                    .get(index)
                    .cloned()
            }

            pub fn push(&self, value: Value) {
                self.0
                    .write()
                    .expect("collection lock poisoned")
                    .push(value);
            }

            pub fn insert(&self, index: usize, value: Value) -> bool {
                let mut values = self.0.write().expect("collection lock poisoned");
                if index > values.len() {
                    return false;
                }
                values.insert(index, value);
                true
            }

            pub fn remove(&self, index: usize) -> Option<Value> {
                let mut values = self.0.write().expect("collection lock poisoned");
                (index < values.len()).then(|| values.remove(index))
            }

            pub fn clear(&self) {
                self.0.write().expect("collection lock poisoned").clear();
            }

            pub fn same_identity(&self, other: &Self) -> bool {
                Arc::ptr_eq(&self.0, &other.0)
            }

            pub fn identity(&self) -> usize {
                Arc::as_ptr(&self.0) as usize
            }
        }

        impl From<Vec<Value>> for $name {
            fn from(values: Vec<Value>) -> Self {
                Self::new(values)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0
                    .read()
                    .expect("collection lock poisoned")
                    .fmt(formatter)
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.same_identity(other) || self.snapshot() == other.snapshot()
            }
        }
    };
}

shared_sequence!(ArrayValue);
shared_sequence!(ListValue);

#[derive(Clone, Default)]
pub struct SetValue(Arc<RwLock<Vec<Value>>>);

impl SetValue {
    pub fn new(values: Vec<Value>) -> Self {
        let set = Self::default();
        for value in values {
            set.insert(value);
        }
        set
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.0.read().expect("collection lock poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.0.read().expect("collection lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.read().expect("collection lock poisoned").is_empty()
    }

    pub fn contains(&self, value: &Value) -> bool {
        self.0
            .read()
            .expect("collection lock poisoned")
            .contains(value)
    }

    pub fn insert(&self, value: Value) {
        let mut values = self.0.write().expect("collection lock poisoned");
        if !values.contains(&value) {
            values.push(value);
        }
    }

    pub fn remove(&self, value: &Value) -> bool {
        let mut values = self.0.write().expect("collection lock poisoned");
        let Some(index) = values.iter().position(|candidate| candidate == value) else {
            return false;
        };
        values.remove(index);
        true
    }

    pub fn clear(&self) {
        self.0.write().expect("collection lock poisoned").clear();
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

impl From<Vec<Value>> for SetValue {
    fn from(values: Vec<Value>) -> Self {
        Self::new(values)
    }
}

impl fmt::Debug for SetValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_set()
            .entries(self.0.read().expect("collection lock poisoned").iter())
            .finish()
    }
}

impl PartialEq for SetValue {
    fn eq(&self, other: &Self) -> bool {
        self.same_identity(other) || {
            let left = self.snapshot();
            let right = other.snapshot();
            left.len() == right.len() && left.iter().all(|value| right.contains(value))
        }
    }
}

#[derive(Clone, Default)]
pub struct MapValue(Arc<RwLock<BTreeMap<String, Value>>>);

impl MapValue {
    pub fn new(values: BTreeMap<String, Value>) -> Self {
        Self(Arc::new(RwLock::new(values)))
    }

    pub fn snapshot(&self) -> BTreeMap<String, Value> {
        self.0.read().expect("collection lock poisoned").clone()
    }

    pub fn len(&self) -> usize {
        self.0.read().expect("collection lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.read().expect("collection lock poisoned").is_empty()
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.0
            .read()
            .expect("collection lock poisoned")
            .get(key)
            .cloned()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0
            .read()
            .expect("collection lock poisoned")
            .contains_key(key)
    }

    pub fn insert(&self, key: String, value: Value) -> Option<Value> {
        self.0
            .write()
            .expect("collection lock poisoned")
            .insert(key, value)
    }

    pub fn remove(&self, key: &str) -> Option<Value> {
        self.0
            .write()
            .expect("collection lock poisoned")
            .remove(key)
    }

    pub fn clear(&self) {
        self.0.write().expect("collection lock poisoned").clear();
    }

    pub fn keys(&self) -> Vec<String> {
        self.0
            .read()
            .expect("collection lock poisoned")
            .keys()
            .cloned()
            .collect()
    }

    pub fn values(&self) -> Vec<Value> {
        self.0
            .read()
            .expect("collection lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn entries(&self) -> Vec<(String, Value)> {
        self.0
            .read()
            .expect("collection lock poisoned")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

impl From<BTreeMap<String, Value>> for MapValue {
    fn from(values: BTreeMap<String, Value>) -> Self {
        Self::new(values)
    }
}

impl fmt::Debug for MapValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .read()
            .expect("collection lock poisoned")
            .fmt(formatter)
    }
}

impl PartialEq for MapValue {
    fn eq(&self, other: &Self) -> bool {
        self.same_identity(other) || self.snapshot() == other.snapshot()
    }
}
