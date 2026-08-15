// crates/core/src/builtins/instance/inline_list_methods.rs
//! Instance methods on `Value::InlineList` — DATASTRUCTURES.md §5.
//!
//! Deliberately mirrors `list_methods.rs`'s own method surface and
//! runtime conventions closely — `InlineList<T>` is List-like by design,
//! and `pop()`/`first()`/`last()` return `Value::Null` on empty here for
//! the exact same reason `List<T>`'s own do: established, existing
//! convention (checked failure via a sentinel value, not a panic for a
//! completely ordinary outcome), not something new introduced for this
//! type. `push()` follows the same "fail safely" philosophy — returns
//! `bool` (`false` when full) rather than panicking or silently
//! discarding the value, so overflow can never happen invisibly.

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, InlineListData, Signal, Value};

pub const METHOD_NAMES: &[&str] =
    &["len", "push", "pop", "contains", "first", "last", "is_empty", "reverse", "capacity"];

/// No `InlineList` method is HIGH-only today. Real, consulted registry —
/// not a stub — see `instance::is_high_only`.
pub const HIGH_ONLY: &[&str] = &[];

pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "len"      => (R::Int, 0),
        "push"     => (R::Bool, 1),
        "pop"      => (R::Elem, 0),
        "contains" => (R::Bool, 1),
        "first"    => (R::Elem, 0),
        "last"     => (R::Elem, 0),
        "is_empty" => (R::Bool, 0),
        "reverse"  => (R::Void, 0),
        "capacity" => (R::Int, 0),
        _ => return None,
    })
}

type InlineListInner = Rc<RefCell<InlineListData>>;

pub fn len(v: &InlineListInner) -> Value {
    Value::Int(v.borrow().items.len() as i64)
}

pub fn capacity(v: &InlineListInner) -> Value {
    Value::Int(v.borrow().capacity as i64)
}

/// `false` (not a panic, not silent truncation) when already at
/// capacity — the actual point of this type over a plain `List<T>`.
pub fn push(v: &InlineListInner, args: &[Value]) -> EvalResult {
    let val = args.first().cloned()
        .ok_or_else(|| Signal::Panic("push() needs 1 argument".into()))?;
    let mut data = v.borrow_mut();
    if data.items.len() >= data.capacity {
        return Ok(Value::Bool(false));
    }
    data.items.push(val);
    Ok(Value::Bool(true))
}

pub fn pop(v: &InlineListInner) -> Value {
    v.borrow_mut().items.pop().unwrap_or(Value::Null)
}

pub fn contains(v: &InlineListInner, args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("contains() needs 1 argument".into()))?;
    Ok(Value::Bool(v.borrow().items.iter().any(|x| x.equals(val))))
}

pub fn first(v: &InlineListInner) -> Value {
    v.borrow().items.first().cloned().unwrap_or(Value::Null)
}

pub fn last(v: &InlineListInner) -> Value {
    v.borrow().items.last().cloned().unwrap_or(Value::Null)
}

pub fn is_empty(v: &InlineListInner) -> Value {
    Value::Bool(v.borrow().items.is_empty())
}

pub fn reverse(v: &InlineListInner) -> Value {
    v.borrow_mut().items.reverse();
    Value::Void
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline_list(capacity: usize) -> InlineListInner {
        Rc::new(RefCell::new(InlineListData { items: Vec::with_capacity(capacity), capacity }))
    }

    #[test]
    fn push_fails_cleanly_at_capacity_no_panic_no_truncation() {
        let v = inline_list(2);
        assert_eq!(push(&v, &[Value::Int(1)]).unwrap(), Value::Bool(true));
        assert_eq!(push(&v, &[Value::Int(2)]).unwrap(), Value::Bool(true));
        assert_eq!(push(&v, &[Value::Int(3)]).unwrap(), Value::Bool(false));
        // The rejected value must not have been silently added anyway.
        assert_eq!(v.borrow().items.len(), 2);
        assert_eq!(v.borrow().items, vec![Value::Int(1), Value::Int(2)]);
    }

    #[test]
    fn pop_on_empty_returns_null_not_panic() {
        let v = inline_list(4);
        assert_eq!(pop(&v), Value::Null);
    }

    #[test]
    fn capacity_is_independent_of_current_length() {
        let v = inline_list(10);
        push(&v, &[Value::Int(1)]).unwrap();
        assert_eq!(len(&v), Value::Int(1));
        assert_eq!(capacity(&v), Value::Int(10));
    }
}
