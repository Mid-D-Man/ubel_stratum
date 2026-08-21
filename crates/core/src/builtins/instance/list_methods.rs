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
use crate::interpreter::eval::Interpreter;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub const METHOD_NAMES: &[&str] = &[
    "len", "push", "pop", "contains", "first", "last", "is_empty", "reverse",
    "get", "set", "find", "find_all", "query",
];

/// `query` is HIGH-only — the one real tier gate `Linqerizer<T>` has.
/// Every downstream `Linqerizer` method (`where`/`select`/`order_by`/
/// terminals) is deliberately NOT in `linqerizer_methods::HIGH_ONLY` —
/// this is the only checkpoint that matters, same principle as the old
/// LINQ grammar only gating the query *start*, not each clause.
pub const HIGH_ONLY: &[&str] = &["query"];

/// `(return shape, arity)` for sema — see `instance::MethodReturn`.
pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "len"      => (R::Int, 0),
        "push"     => (R::Void, 1),
        "pop"      => (R::Elem, 0),
        "contains" => (R::Bool, 1),
        "first"    => (R::Elem, 0),
        "last"     => (R::Elem, 0),
        "is_empty" => (R::Bool, 0),
        "reverse"  => (R::Void, 0),
        "get"      => (R::Elem, 1),
        "set"      => (R::Bool, 2),
        "find"     => (R::Elem, 1),
        "find_all" => (R::NewSelf, 1),
        "query"    => (R::NewLinqerizerOfElem, 0),
        _ => return None,
    })
}

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

/// Indexed read. Out-of-bounds (including negative) returns `Value::Null`
/// rather than panicking — the same convention `pop`/`first`/`last`
/// already established for "nothing there" outcomes.
pub fn get(list: &Rc<RefCell<Vec<Value>>>, args: &[Value]) -> EvalResult {
    let idx = match args.first() {
        Some(Value::Int(i)) => *i,
        _ => return Err(Signal::Panic("get() needs 1 integer argument".into())),
    };
    if idx < 0 {
        return Ok(Value::Null);
    }
    Ok(list.borrow().get(idx as usize).cloned().unwrap_or(Value::Null))
}

/// Indexed write. Out-of-bounds (including negative) returns `false`
/// rather than panicking or silently growing the list — the checked,
/// never-panics convention `InlineList.push()` already established for
/// a write that might legitimately fail based on runtime data.
pub fn set(list: &Rc<RefCell<Vec<Value>>>, args: &[Value]) -> EvalResult {
    let idx = match args.first() {
        Some(Value::Int(i)) => *i,
        _ => return Err(Signal::Panic("set() needs 2 arguments: index, value".into())),
    };
    let val = args.get(1).cloned()
        .ok_or_else(|| Signal::Panic("set() needs 2 arguments: index, value".into()))?;
    if idx < 0 {
        return Ok(Value::Bool(false));
    }
    let idx = idx as usize;
    let mut l = list.borrow_mut();
    if idx < l.len() {
        l[idx] = val;
        Ok(Value::Bool(true))
    } else {
        Ok(Value::Bool(false))
    }
}

/// Start a `Linqerizer<T>` pipeline. `HIGH_ONLY`-gated (see this
/// module's `HIGH_ONLY` const) — the snapshot is taken right here, once,
/// at call time; nothing about it is lazy. Laziness is about the *ops*
/// chained afterward not running until a terminal call, not about
/// deferring this snapshot — see `Value::Linqerizer`'s own doc comment.
pub fn query(list: &Rc<RefCell<Vec<Value>>>) -> Value {
    let source = Rc::new(list.borrow().clone());
    Value::Linqerizer(Rc::new(crate::interpreter::value::LinqPipeline {
        source,
        ops: Vec::new(),
    }))
}

/// First element for which `predicate(element)` is truthy, or `Null` if
/// none match. `predicate` must be a `Value::Function` (named fn or
/// lambda — both are `FunctionId` under the hood, see `Value::Function`'s
/// own doc comment).
///
/// Snapshots the list into a plain `Vec` before iterating, same as
/// `eval_linq` used to (and every other closure-invoking iteration here
/// will) — `interp.call_function` re-enters interpretation, and the
/// predicate body could in principle touch this same list, so no
/// `Ref`/`RefMut` borrow may be held live across that call.
pub fn find<'ast>(
    interp: &mut Interpreter<'ast>,
    list: &Rc<RefCell<Vec<Value>>>,
    args: &[Value],
) -> EvalResult {
    let pred_id = match args.first() {
        Some(Value::Function(id)) => *id,
        _ => return Err(Signal::Panic("find() needs 1 argument: a predicate function".into())),
    };
    let items = list.borrow().clone();
    for item in items {
        if interp.call_function(pred_id, &[item.clone()])?.is_truthy()? {
            return Ok(item);
        }
    }
    Ok(Value::Null)
}

/// Every element for which `predicate(element)` is truthy, as a new
/// `List<T>` (possibly empty). See `find` above for the predicate-call
/// and snapshot-before-iterating notes — identical here.
pub fn find_all<'ast>(
    interp: &mut Interpreter<'ast>,
    list: &Rc<RefCell<Vec<Value>>>,
    args: &[Value],
) -> EvalResult {
    let pred_id = match args.first() {
        Some(Value::Function(id)) => *id,
        _ => return Err(Signal::Panic("find_all() needs 1 argument: a predicate function".into())),
    };
    let items = list.borrow().clone();
    let mut matches = Vec::with_capacity(items.len());
    for item in items {
        if interp.call_function(pred_id, &[item.clone()])?.is_truthy()? {
            matches.push(item);
        }
    }
    Ok(Value::List(Rc::new(RefCell::new(matches))))
}
