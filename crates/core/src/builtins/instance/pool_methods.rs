// crates/core/src/builtins/instance/pool_methods.rs
//! Instance methods on `Value::Pool` — MEMORY_MODEL.md §11.
//!
//! Three operations, matching the resolved design exactly:
//!   - `acquire(value)` — store `value` into a free slot, return
//!     `Optional<Handle<T>>` (`null` if the pool is exhausted).
//!   - `release(handle)` — free the slot back to the free list if the
//!     handle's generation still matches; a stale/invalid handle is a
//!     silent no-op (fails safe, matching `Optional`'s "checked failure,
//!     not memory corruption" philosophy — not a panic).
//!   - `at(handle)` — read the slot's current value if the generation
//!     matches, else `Optional::None`. This is where staleness actually
//!     gets caught: an old handle held past release provably fails here
//!     instead of silently reading whatever's been written into a
//!     reused slot. Named `at` (not `get`) to match `Dictionary.at(key)`'s
//!     existing precedent for "keyed lookup" — `get`/`set` are reserved
//!     tokens in the lexer (`TokenType::Get`/`Set`), not usable as a
//!     method name after `.` without a separate parser change; `at`
//!     sidesteps that while staying consistent with the rest of the
//!     builtin vocabulary.

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, PoolData, Signal, Value};

pub const METHOD_NAMES: &[&str] = &["acquire", "release", "at"];

/// No `Pool` method is HIGH-only today. Real, consulted registry — not
/// a stub — see `instance::is_high_only`.
pub const HIGH_ONLY: &[&str] = &[];

/// `(return shape, arity)` for sema — see `instance::MethodReturn`.
pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "acquire" => (R::AcquireHandle, 1),
        "release" => (R::Void, 1),
        "at"      => (R::Elem, 1),
        _ => return None,
    })
}

type PoolInner = Rc<RefCell<PoolData>>;

pub fn acquire(p: &PoolInner, args: &[Value]) -> EvalResult {
    let value = args.first().cloned()
        .ok_or_else(|| Signal::Panic("acquire() needs 1 argument".into()))?;
    let mut pool = p.borrow_mut();
    match pool.free_list.pop() {
        Some(index) => {
            pool.slots[index] = Some(value);
            let generation = pool.generations[index];
            Ok(Value::Handle { index, generation })
        }
        None => Ok(Value::Null),
    }
}

pub fn release(p: &PoolInner, args: &[Value]) -> EvalResult {
    let (index, generation) = match args.first() {
        Some(Value::Handle { index, generation }) => (*index, *generation),
        _ => return Err(Signal::Panic("release() needs a Handle argument".into())),
    };
    let mut pool = p.borrow_mut();
    if index < pool.generations.len()
        && pool.generations[index] == generation
        && pool.slots[index].is_some()
    {
        pool.slots[index] = None;
        pool.generations[index] = pool.generations[index].wrapping_add(1);
        pool.free_list.push(index);
    }
    // Stale or out-of-range handle: silent no-op, not a panic.
    Ok(Value::Void)
}

pub fn at(p: &PoolInner, args: &[Value]) -> EvalResult {
    let (index, generation) = match args.first() {
        Some(Value::Handle { index, generation }) => (*index, *generation),
        _ => return Err(Signal::Panic("at() needs a Handle argument".into())),
    };
    let pool = p.borrow();
    if index < pool.generations.len() && pool.generations[index] == generation {
        Ok(pool.slots[index].clone().unwrap_or(Value::Null))
    } else {
        Ok(Value::Null)
    }
}
