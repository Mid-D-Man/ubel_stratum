// crates/core/src/builtins/instance/dict_methods.rs
//! Instance methods on `Value::Dict` — called as `myDict.method(...)`.

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "is_empty", "contains_key", "keys", "values", "insert", "at"];

type DictInner = Rc<RefCell<Vec<(Value, Value)>>>;

pub fn len(dict: &DictInner) -> Value {
    Value::Int(dict.borrow().len() as i64)
}

pub fn is_empty(dict: &DictInner) -> Value {
    Value::Bool(dict.borrow().is_empty())
}

pub fn contains_key(dict: &DictInner, args: &[Value]) -> EvalResult {
    let key = args.first().ok_or_else(|| Signal::Panic("contains_key() needs 1 arg".into()))?;
    Ok(Value::Bool(dict.borrow().iter().any(|(k, _)| k.equals(key))))
}

pub fn keys(dict: &DictInner) -> Value {
    let keys: Vec<Value> = dict.borrow().iter().map(|(k, _)| k.clone()).collect();
    Value::List(Rc::new(RefCell::new(keys)))
}

pub fn values(dict: &DictInner) -> Value {
    let vals: Vec<Value> = dict.borrow().iter().map(|(_, v)| v.clone()).collect();
    Value::List(Rc::new(RefCell::new(vals)))
}

/// Insert or update a key's value. `get`/`set` are reserved keywords in
/// Ubel (property-accessor syntax), so this pair is named `insert`/`at`
/// instead — `insert` matches Rust's HashMap::insert (insert-or-overwrite,
/// not insert-only) and Ubel's own List/Stack/Queue naming conventions.
pub fn insert(dict: &DictInner, args: &[Value]) -> EvalResult {
    let key = args.first().cloned()
        .ok_or_else(|| Signal::Panic("insert() needs 2 arguments (key, value)".into()))?;
    let val = args.get(1).cloned()
        .ok_or_else(|| Signal::Panic("insert() needs 2 arguments (key, value)".into()))?;

    let mut d = dict.borrow_mut();
    match d.iter_mut().find(|(k, _)| k.equals(&key)) {
        Some(entry) => entry.1 = val,
        None        => d.push((key, val)),
    }
    Ok(Value::Void)
}

/// Look up a key's value. Named `at` rather than `get` (reserved keyword)
/// — matches C++'s `std::map::at`. Returns `Value::Null` for a missing
/// key rather than panicking, consistent with Queue::dequeue/Stack::pop's
/// existing empty-collection convention (both `.unwrap_or(Value::Null)`).
/// Check `contains_key()` first if `Null` would otherwise be ambiguous
/// with a legitimately-stored null value.
pub fn at(dict: &DictInner, args: &[Value]) -> EvalResult {
    let key = args.first().ok_or_else(|| Signal::Panic("at() needs 1 argument".into()))?;
    let found = dict.borrow().iter()
        .find(|(k, _)| k.equals(key))
        .map(|(_, v)| v.clone());
    Ok(found.unwrap_or(Value::Null))
        }
