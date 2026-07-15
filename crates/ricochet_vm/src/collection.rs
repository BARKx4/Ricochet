use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionEqualityError {
    actual: &'static str,
}

impl CollectionEqualityError {
    pub fn actual(&self) -> &'static str {
        self.actual
    }
}

impl fmt::Display for CollectionEqualityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "collection equality is unavailable for {}",
            self.actual
        )
    }
}

impl std::error::Error for CollectionEqualityError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionKind {
    Array,
    List,
    Map,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CollectionVisit {
    kind: CollectionKind,
    identity: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EqualityVisit {
    kind: CollectionKind,
    first: usize,
    second: usize,
}

thread_local! {
    static DEBUG_VISITS: RefCell<Vec<CollectionVisit>> = const { RefCell::new(Vec::new()) };
    static EQUALITY_VISITS: RefCell<Vec<EqualityVisit>> = const { RefCell::new(Vec::new()) };
}

struct DebugVisitGuard(CollectionVisit);

impl DebugVisitGuard {
    fn enter(kind: CollectionKind, identity: usize) -> Option<Self> {
        let visit = CollectionVisit { kind, identity };
        DEBUG_VISITS.with(|visits| {
            let mut visits = visits.borrow_mut();
            if visits.contains(&visit) {
                None
            } else {
                visits.push(visit);
                Some(Self(visit))
            }
        })
    }
}

impl Drop for DebugVisitGuard {
    fn drop(&mut self) {
        DEBUG_VISITS.with(|visits| {
            let popped = visits.borrow_mut().pop();
            debug_assert_eq!(popped, Some(self.0));
        });
    }
}

struct EqualityVisitGuard(EqualityVisit);

impl EqualityVisitGuard {
    fn enter(kind: CollectionKind, left: usize, right: usize) -> Option<Self> {
        let (first, second) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let visit = EqualityVisit {
            kind,
            first,
            second,
        };
        EQUALITY_VISITS.with(|visits| {
            let mut visits = visits.borrow_mut();
            if visits.contains(&visit) {
                None
            } else {
                visits.push(visit);
                Some(Self(visit))
            }
        })
    }
}

impl Drop for EqualityVisitGuard {
    fn drop(&mut self) {
        EQUALITY_VISITS.with(|visits| {
            let popped = visits.borrow_mut().pop();
            debug_assert_eq!(popped, Some(self.0));
        });
    }
}

fn set_values_equal(left: &[Value], right: &[Value]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let compatibility = left
        .iter()
        .map(|left| right.iter().map(|right| left == right).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut matched_right = vec![None; right.len()];

    for left_index in 0..left.len() {
        let mut visited_right = vec![false; right.len()];
        if !assign_set_match(
            left_index,
            &compatibility,
            &mut visited_right,
            &mut matched_right,
        ) {
            return false;
        }
    }
    true
}

fn assign_set_match(
    left_index: usize,
    compatibility: &[Vec<bool>],
    visited_right: &mut [bool],
    matched_right: &mut [Option<usize>],
) -> bool {
    for right_index in 0..matched_right.len() {
        if !compatibility[left_index][right_index] || visited_right[right_index] {
            continue;
        }
        visited_right[right_index] = true;
        if matched_right[right_index].is_none_or(|matched_left| {
            assign_set_match(matched_left, compatibility, visited_right, matched_right)
        }) {
            matched_right[right_index] = Some(left_index);
            return true;
        }
    }
    false
}

macro_rules! shared_sequence {
    ($name:ident, $kind:expr) => {
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
                let Some(_visit) = DebugVisitGuard::enter($kind, self.identity()) else {
                    return formatter.write_str("<cycle>");
                };
                self.snapshot().fmt(formatter)
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                if self.same_identity(other) {
                    return true;
                }
                let Some(_visit) =
                    EqualityVisitGuard::enter($kind, self.identity(), other.identity())
                else {
                    return true;
                };
                self.snapshot() == other.snapshot()
            }
        }
    };
}

shared_sequence!(ArrayValue, CollectionKind::Array);
shared_sequence!(ListValue, CollectionKind::List);

#[derive(Default)]
struct SetInner {
    values: RwLock<Vec<Value>>,
    mutation: Mutex<()>,
}

#[derive(Clone, Default)]
pub struct SetValue(Arc<SetInner>);

impl SetValue {
    pub fn new(values: Vec<Value>) -> Result<Self, CollectionEqualityError> {
        let set = Self::default();
        for value in values {
            set.insert(value)?;
        }
        Ok(set)
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.0
            .values
            .read()
            .expect("collection lock poisoned")
            .clone()
    }

    pub fn len(&self) -> usize {
        self.0
            .values
            .read()
            .expect("collection lock poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.0
            .values
            .read()
            .expect("collection lock poisoned")
            .is_empty()
    }

    pub fn contains(&self, value: &Value) -> Result<bool, CollectionEqualityError> {
        let snapshot = self.snapshot();
        reject_opaque_set_operation(&snapshot, value)?;
        Ok(snapshot.contains(value))
    }

    pub fn insert(&self, value: Value) -> Result<bool, CollectionEqualityError> {
        let _mutation = self
            .0
            .mutation
            .lock()
            .expect("collection mutation lock poisoned");
        let snapshot = self.snapshot();
        reject_opaque_set_operation(&snapshot, &value)?;
        if !snapshot.contains(&value) {
            self.0
                .values
                .write()
                .expect("collection lock poisoned")
                .push(value);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn remove(&self, value: &Value) -> Result<bool, CollectionEqualityError> {
        let _mutation = self
            .0
            .mutation
            .lock()
            .expect("collection mutation lock poisoned");
        let snapshot = self.snapshot();
        reject_opaque_set_operation(&snapshot, value)?;
        let Some(index) = snapshot.iter().position(|candidate| candidate == value) else {
            return Ok(false);
        };
        self.0
            .values
            .write()
            .expect("collection lock poisoned")
            .remove(index);
        Ok(true)
    }

    pub fn clear(&self) {
        let _mutation = self
            .0
            .mutation
            .lock()
            .expect("collection mutation lock poisoned");
        self.0
            .values
            .write()
            .expect("collection lock poisoned")
            .clear();
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}

impl TryFrom<Vec<Value>> for SetValue {
    type Error = CollectionEqualityError;

    fn try_from(values: Vec<Value>) -> Result<Self, Self::Error> {
        Self::new(values)
    }
}

fn reject_opaque_set_operation(
    stored: &[Value],
    candidate: &Value,
) -> Result<(), CollectionEqualityError> {
    let actual = stored
        .iter()
        .find_map(Value::opaque_value_kind)
        .or_else(|| candidate.opaque_value_kind());
    match actual {
        Some(actual) => Err(CollectionEqualityError { actual }),
        None => Ok(()),
    }
}

impl fmt::Debug for SetValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(_visit) = DebugVisitGuard::enter(CollectionKind::Set, self.identity()) else {
            return formatter.write_str("<cycle>");
        };
        let values = self.snapshot();
        formatter.debug_set().entries(values.iter()).finish()
    }
}

impl PartialEq for SetValue {
    fn eq(&self, other: &Self) -> bool {
        if self.same_identity(other) {
            return true;
        }
        let Some(_visit) =
            EqualityVisitGuard::enter(CollectionKind::Set, self.identity(), other.identity())
        else {
            return true;
        };
        set_values_equal(&self.snapshot(), &other.snapshot())
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
        let Some(_visit) = DebugVisitGuard::enter(CollectionKind::Map, self.identity()) else {
            return formatter.write_str("<cycle>");
        };
        self.snapshot().fmt(formatter)
    }
}

impl PartialEq for MapValue {
    fn eq(&self, other: &Self) -> bool {
        if self.same_identity(other) {
            return true;
        }
        let Some(_visit) =
            EqualityVisitGuard::enter(CollectionKind::Map, self.identity(), other.identity())
        else {
            return true;
        };
        self.snapshot() == other.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn debug_marks_self_cycles_for_every_collection_kind() {
        let array = ArrayValue::default();
        array.push(Value::Array(array.clone()));
        assert_eq!(
            format!("{:?}", Value::Array(array)),
            "Array([Array(<cycle>)])"
        );

        let list = ListValue::default();
        list.push(Value::List(list.clone()));
        assert_eq!(format!("{:?}", Value::List(list)), "List([List(<cycle>)])");

        let map = MapValue::default();
        map.insert("self".to_string(), Value::Map(map.clone()));
        assert_eq!(
            format!("{:?}", Value::Map(map)),
            "Map({\"self\": Map(<cycle>)})"
        );

        let set = SetValue::default();
        set.insert(Value::Set(set.clone()))
            .expect("cyclic ordinary set should remain supported");
        assert_eq!(format!("{:?}", Value::Set(set)), "Set({Set(<cycle>)})");
    }

    #[test]
    fn heterogeneous_cycles_compare_structurally_and_find_scalar_differences() {
        let left_array = ArrayValue::default();
        let left_map = MapValue::default();
        left_array.push(Value::Map(left_map.clone()));
        left_map.insert("array".to_string(), Value::Array(left_array.clone()));
        left_map.insert("marker".to_string(), Value::Number(1));

        let right_array = ArrayValue::default();
        let right_map = MapValue::default();
        right_array.push(Value::Map(right_map.clone()));
        right_map.insert("array".to_string(), Value::Array(right_array.clone()));
        right_map.insert("marker".to_string(), Value::Number(1));

        assert_eq!(left_array, right_array);

        right_map.insert("marker".to_string(), Value::Number(2));
        assert_ne!(left_array, right_array);
    }

    #[test]
    fn shared_acyclic_dags_render_fully_and_equal_duplicated_trees() {
        let shared = ArrayValue::from(vec![Value::Number(7)]);
        let dag = ArrayValue::from(vec![
            Value::Array(shared.clone()),
            Value::Array(shared.clone()),
        ]);
        let duplicated = ArrayValue::from(vec![
            Value::Array(ArrayValue::from(vec![Value::Number(7)])),
            Value::Array(ArrayValue::from(vec![Value::Number(7)])),
        ]);

        let rendered = format!("{:?}", Value::Array(dag.clone()));
        assert_eq!(rendered, "Array([Array([Number(7)]), Array([Number(7)])])");
        assert!(!rendered.contains("<cycle>"));
        assert_eq!(dag, duplicated);
    }

    #[test]
    fn cyclic_sets_compare_independent_of_insertion_order() {
        fn cyclic_array(marker: i64) -> Value {
            let array = ArrayValue::default();
            array.push(Value::Number(marker));
            array.push(Value::Array(array.clone()));
            Value::Array(array)
        }

        let left = SetValue::default();
        left.insert(cyclic_array(1)).expect("ordinary cyclic set");
        left.insert(cyclic_array(2)).expect("ordinary cyclic set");

        let right = SetValue::default();
        right.insert(cyclic_array(2)).expect("ordinary cyclic set");
        right.insert(cyclic_array(1)).expect("ordinary cyclic set");

        assert_eq!(left, right);
    }

    #[test]
    fn cyclic_set_membership_deduplication_and_removal_terminate() {
        let set = SetValue::default();
        let self_value = Value::Set(set.clone());

        set.insert(self_value.clone()).expect("ordinary cyclic set");
        set.insert(self_value.clone()).expect("ordinary cyclic set");

        assert_eq!(set.len(), 1);
        assert!(set.contains(&self_value).expect("ordinary cyclic set"));
        assert!(set.remove(&self_value).expect("ordinary cyclic set"));
        assert!(set.is_empty());
    }

    #[test]
    fn concurrent_equal_set_insertions_preserve_uniqueness() {
        let set = SetValue::default();
        let threads = (0..8)
            .map(|_| {
                let set = set.clone();
                thread::spawn(move || {
                    set.insert(Value::Array(ArrayValue::from(vec![Value::Number(7)])))
                        .expect("ordinary concurrent set value");
                })
            })
            .collect::<Vec<_>>();

        for thread in threads {
            thread.join().expect("set insertion thread should finish");
        }

        assert_eq!(set.len(), 1);
    }

    #[test]
    fn set_equality_operations_reject_nested_opaque_values_on_both_sides() {
        let source =
            ricochet_secrets::DeferredSecretSource::literal("synthetic-set-secret".to_string())
                .expect("synthetic literal");
        let opaque = Value::DeferredHttpCredentials(
            ricochet_secrets::DeferredHttpCredentials::bearer(source),
        );
        let nested_candidate = Value::Array(ArrayValue::from(vec![opaque.clone()]));

        let error = SetValue::try_from(vec![nested_candidate.clone()])
            .expect_err("set construction must reject nested opaque values");
        assert_eq!(error.actual(), "deferred HTTP credentials");

        let shared = ArrayValue::from(vec![Value::Number(1)]);
        let set = SetValue::try_from(vec![Value::Array(shared.clone())])
            .expect("ordinary shared collection should insert");
        shared.push(opaque);

        for error in [
            set.contains(&Value::Nil)
                .expect_err("stored opaque value must reject membership"),
            set.remove(&Value::Nil)
                .expect_err("stored opaque value must reject removal"),
            set.insert(Value::Nil)
                .expect_err("stored opaque value must reject deduplication"),
        ] {
            assert_eq!(error.actual(), "deferred HTTP credentials");
        }
        assert_eq!(set.len(), 1, "failed guards must not mutate the set");
    }
}
