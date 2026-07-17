// crates/core/src/builtins/instance/stack_methods.rs
//! Instance methods on `Value::Stack`. Operation set matches C#'s
//! `Stack<T>` (Push/Pop/Peek/Count/Contains/Clear).

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "is_empty", "push", "pop", "peek", "contains", "clear"];

type StackInner = Rc<RefCell<Vec<Value>>>;

pub fn len(s: &StackInner) -> Value { Value::Int(s.borrow().len() as i64) }
pub fn is_empty(s: &StackInner) -> Value { Value::Bool(s.borrow().is_empty()) }

pub fn push(s: &StackInner, args: &[Value]) -> EvalResult {
    let val = args.first().cloned()
        .ok_or_else(|| Signal::Panic("push() needs 1 argument".into()))?;
    s.borrow_mut().push(val);
    Ok(Value::Void)
}

pub fn pop(s: &StackInner) -> Value {
    s.borrow_mut().pop().unwrap_or(Value::Null)
}

pub fn peek(s: &StackInner) -> Value {
    s.borrow().last().cloned().unwrap_or(Value::Null)
}

pub fn contains(s: &StackInner, args: &[Value]) -> EvalResult {
    let val = args.first().ok_or_else(|| Signal::Panic("contains() needs 1 argument".into()))?;
    Ok(Value::Bool(s.borrow().iter().any(|v| v.equals(val))))
}

pub fn clear(s: &StackInner) -> Value {
    s.borrow_mut().clear();
    Value::Void
}
