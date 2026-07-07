// crates/core/src/builtins/instance/string_methods.rs
//! Instance methods on `Value::Str` — called as `"...".method(...)`.

use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] = &[
    "len", "is_empty", "to_upper", "to_lower", "trim", "trim_start", "trim_end",
    "chars", "contains", "starts_with", "ends_with", "split", "replace",
];

pub fn len(s: &Rc<String>) -> Value { Value::Int(s.len() as i64) }
pub fn is_empty(s: &Rc<String>) -> Value { Value::Bool(s.is_empty()) }
pub fn to_upper(s: &Rc<String>) -> Value { Value::str_from(s.to_uppercase()) }
pub fn to_lower(s: &Rc<String>) -> Value { Value::str_from(s.to_lowercase()) }
pub fn trim(s: &Rc<String>) -> Value { Value::str_from(s.trim()) }
pub fn trim_start(s: &Rc<String>) -> Value { Value::str_from(s.trim_start()) }
pub fn trim_end(s: &Rc<String>) -> Value { Value::str_from(s.trim_end()) }

pub fn chars(s: &Rc<String>) -> Value {
    let chars: Vec<Value> = s.chars().map(Value::Char).collect();
    Value::List(Rc::new(std::cell::RefCell::new(chars)))
}

pub fn contains(s: &Rc<String>, args: &[Value]) -> EvalResult {
    let sub = args.first().ok_or_else(|| Signal::Panic("contains() needs 1 arg".into()))?;
    match sub {
        Value::Str(sub_str) => Ok(Value::Bool(s.contains(sub_str.as_str()))),
        _ => Ok(Value::Bool(false)),
    }
}

pub fn starts_with(s: &Rc<String>, args: &[Value]) -> EvalResult {
    let sub = args.first().ok_or_else(|| Signal::Panic("starts_with() needs 1 arg".into()))?;
    match sub {
        Value::Str(sub_str) => Ok(Value::Bool(s.starts_with(sub_str.as_str()))),
        _ => Ok(Value::Bool(false)),
    }
}

pub fn ends_with(s: &Rc<String>, args: &[Value]) -> EvalResult {
    let sub = args.first().ok_or_else(|| Signal::Panic("ends_with() needs 1 arg".into()))?;
    match sub {
        Value::Str(sub_str) => Ok(Value::Bool(s.ends_with(sub_str.as_str()))),
        _ => Ok(Value::Bool(false)),
    }
}

pub fn split(s: &Rc<String>, args: &[Value]) -> EvalResult {
    let delim = args.first().ok_or_else(|| Signal::Panic("split() needs 1 arg".into()))?;
    match delim {
        Value::Str(d) => {
            let parts: Vec<Value> = s.split(d.as_str()).map(Value::str_from).collect();
            Ok(Value::List(Rc::new(std::cell::RefCell::new(parts))))
        }
        _ => Err(Signal::Panic("split() delimiter must be a string".into())),
    }
}

pub fn replace(s: &Rc<String>, args: &[Value]) -> EvalResult {
    if args.len() < 2 {
        return Err(Signal::Panic("replace() needs 2 arguments".into()));
    }
    match (&args[0], &args[1]) {
        (Value::Str(from), Value::Str(to)) => Ok(Value::str_from(s.replace(from.as_str(), to.as_str()))),
        _ => Err(Signal::Panic("replace() arguments must be strings".into())),
    }
}
