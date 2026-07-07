// crates/core/src/builtins/instance/list_methods.rs
//! Instance methods on `Value::List` — called as `myList.method(...)`.
//!
//! This is the ONLY place these operations are implemented. Previously
//! `len`/`push`/`pop`/`contains` existed twice: once as bare-call globals
//! in the old `interpreter/builtins.rs`, and again as inline match arms in
//! `eval_method_call`. Two hand-written copies of the same logic with no
//! shared source of truth — exactly the kind of drift risk this module
//! split exists to remove.

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "push", "pop", "contains", "first", "last", "is_empty", "reverse"];

pub fn len(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Int(list.borrow().len() as i64)
}

pub fn push(list: &Rc<RefCell<Vec<Value>>>, args: &[Value]) -> EvalResult {
    let val = args.first().cloned()
        .ok_or_else(|| Signal::Panic("push() needs 1 argument".into()))?;
    list.borrow_mut().push(val);
    Ok(Value::Void)
}

pub fn pop(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    list.borrow_mut().pop().unwrap_or(Value::Null)
}

pub fn contains(list: &Rc<RefCell<Vec<Value>>>, args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("contains() needs 1 argument".into()))?;
    Ok(Value::Bool(list.borrow().iter().any(|v| v.equals(val))))
}

pub fn first(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    list.borrow().first().cloned().unwrap_or(Value::Null)
}

pub fn last(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    list.borrow().last().cloned().unwrap_or(Value::Null)
}

pub fn is_empty(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Bool(list.borrow().is_empty())
}

pub fn reverse(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    list.borrow_mut().reverse();
    Value::Void
}
