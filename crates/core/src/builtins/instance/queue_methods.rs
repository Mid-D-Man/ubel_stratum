// crates/core/src/builtins/instance/queue_methods.rs
//! Instance methods on `Value::Queue`. Operation set matches C#'s
//! `Queue<T>` (Enqueue/Dequeue/Peek/Count/Contains/Clear); naming matches
//! Ubel's snake_case convention.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "is_empty", "enqueue", "dequeue", "peek", "contains", "clear"];

type QueueInner = Rc<RefCell<VecDeque<Value>>>;

pub fn len(q: &QueueInner) -> Value { Value::Int(q.borrow().len() as i64) }
pub fn is_empty(q: &QueueInner) -> Value { Value::Bool(q.borrow().is_empty()) }

pub fn enqueue(q: &QueueInner, args: &[Value]) -> EvalResult {
    let val = args.first().cloned()
        .ok_or_else(|| Signal::Panic("enqueue() needs 1 argument".into()))?;
    q.borrow_mut().push_back(val);
    Ok(Value::Void)
}

pub fn dequeue(q: &QueueInner) -> Value {
    q.borrow_mut().pop_front().unwrap_or(Value::Null)
}

pub fn peek(q: &QueueInner) -> Value {
    q.borrow().front().cloned().unwrap_or(Value::Null)
}

pub fn contains(q: &QueueInner, args: &[Value]) -> EvalResult {
    let val = args.first().ok_or_else(|| Signal::Panic("contains() needs 1 argument".into()))?;
    Ok(Value::Bool(q.borrow().iter().any(|v| v.equals(val))))
}

pub fn clear(q: &QueueInner) -> Value {
    q.borrow_mut().clear();
    Value::Void
  }
