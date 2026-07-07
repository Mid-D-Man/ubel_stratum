// crates/core/src/builtins/instance/dict_methods.rs
//! Instance methods on `Value::Dict` — called as `myDict.method(...)`.

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] = &["len", "is_empty", "contains_key", "keys", "values"];

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
