// crates/core/src/builtins/instance/stack_methods.rs
//! Instance methods on `Value::Stack`. Operation set matches C#'s
//! `Stack<T>` (Push/Pop/Peek/Count/Contains/Clear).

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "is_empty", "push", "pop", "peek", "contains", "clear"];

/// No `Stack` method is HIGH-only today. Real, consulted registry — not
/// a stub — see `instance::is_high_only`.
pub const HIGH_ONLY: &[&str] = &[];

/// `(return shape, arity)` for sema — see `instance::MethodReturn`.
pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "len"      => (R::Int, 0),
        "is_empty" => (R::Bool, 0),
        "push"     => (R::Void, 1),
        "pop"      => (R::Elem, 0),
        "peek"     => (R::Elem, 0),
        "contains" => (R::Bool, 1),
        "clear"    => (R::Void, 0),
        _ => return None,
    })
}

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
