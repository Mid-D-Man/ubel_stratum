// crates/core/src/builtins/instance/pool_methods.rs
//! Instance methods on `Value::Pool` — MEMORY_MODEL.md §11,
//! DATASTRUCTURES.md §1 (`.growable()`/`.iter()` — Pool absorbs Hive
//! rather than a separate type).
//!
//!   - `acquire(value)` — store `value` into a free slot, return
//!     `Optional<Handle<T>>` (`null` if the pool is exhausted and not
//!     growable, or growable but somehow still exhausted after a grow
//!     attempt — shouldn't happen, defensive).
//!   - `release(handle)` — free the slot back to the free list if the
//!     handle's generation still matches; a stale/invalid handle is a
//!     silent no-op (fails safe, matching `Optional`'s "checked failure,
//!     not memory corruption" philosophy — not a panic).
//!   - `get(handle)` — read the slot's current value if the generation
//!     matches, else `Optional::None`. This is where staleness actually
//!     gets caught: an old handle held past release provably fails here
//!     instead of silently reading whatever's been written into a
//!     reused slot. Was named `at` (matching `Dictionary.at(key)`) back
//!     when `get`/`set` were still reserved for the never-built
//!     property-accessor feature; renamed once that reservation was
//!     freed up (see `docs/NAMING_CONVENTIONS.md` §12).
//!   - `growable()` — opt in to block-chained growth on exhaustion
//!     instead of `acquire()` returning `null`. Void, no args, mutates
//!     the pool's own flag in place.
//!   - `fifo()` — opt in to oldest-freed-first reuse instead of the LIFO
//!     default. Void, no args, mutates the pool's own flag in place.
//!
//! `for x in pool { }` (skipfield-style iteration, holes skipped) is
//! handled separately in `interpreter::eval::stmt::value_to_iter_vec` and
//! `sema::type_infer::element_type_of` — not a method call, matching
//! every other collection's direct-iterability (no `.iter()` precedent
//! exists anywhere in this file's siblings either).

use std::cell::RefCell;
use std::rc::Rc;
use crate::interpreter::value::{EvalResult, PoolData, Signal, Value};

pub const METHOD_NAMES: &[&str] = &["acquire", "release", "get", "growable", "fifo"];

/// No `Pool` method is HIGH-only today. Real, consulted registry — not
/// a stub — see `instance::is_high_only`.
pub const HIGH_ONLY: &[&str] = &[];

/// `(return shape, arity)` for sema — see `instance::MethodReturn`.
pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "acquire"  => (R::AcquireHandle, 1),
        "release"  => (R::Void, 1),
        "get"      => (R::Elem, 1),
        "growable" => (R::Void, 0),
        "fifo"     => (R::Void, 0),
        _ => return None,
    })
}

type PoolInner = Rc<RefCell<PoolData>>;

pub fn acquire(p: &PoolInner, args: &[Value]) -> EvalResult {
    let value = args.first().cloned()
        .ok_or_else(|| Signal::Panic("acquire() needs 1 argument".into()))?;
    let mut pool = p.borrow_mut();
    // Growable pools get exactly one grow attempt per exhausted acquire —
    // a single new block always has room for at least this one value,
    // so there's no retry loop needed.
    if pool.free_list.is_empty() {
        pool.try_grow();
    }
    match pool.free_pop() {
        Some(index) => {
            let generation = pool.generation(index).unwrap_or(0);
            *pool.slot_mut(index).expect("free_list index always in range") = Some(value);
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
    let matches = pool.generation(index) == Some(generation)
        && pool.slot(index).is_some();
    if matches {
        if let Some(slot) = pool.slot_mut(index) { *slot = None; }
        if let Some(gen) = pool.generation_mut(index) { *gen = gen.wrapping_add(1); }
        pool.free_push(index);
    }
    // Stale or out-of-range handle: silent no-op, not a panic.
    Ok(Value::Void)
}

pub fn get(p: &PoolInner, args: &[Value]) -> EvalResult {
    let (index, generation) = match args.first() {
        Some(Value::Handle { index, generation }) => (*index, *generation),
        _ => return Err(Signal::Panic("get() needs a Handle argument".into())),
    };
    let pool = p.borrow();
    if pool.generation(index) == Some(generation) {
        Ok(pool.slot(index).cloned().unwrap_or(Value::Null))
    } else {
        Ok(Value::Null)
    }
}

pub fn growable(p: &PoolInner, _args: &[Value]) -> EvalResult {
    p.borrow_mut().growable = true;
    Ok(Value::Void)
}

pub fn fifo(p: &PoolInner, _args: &[Value]) -> EvalResult {
    p.borrow_mut().fifo = true;
    Ok(Value::Void)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::value::Value;

    // `Handle` is deliberately opaque at the Ubel-language level (no
    // accessor for its raw index — see the type's own doc comment in
    // value.rs), so the exact LIFO-vs-FIFO reuse *order* isn't provable
    // from a `.ubl` fixture at all; these test `PoolData` directly
    // instead. Fixture-level coverage (`ok_pool_growable.ubl`,
    // `ok_pool_iterate.ubl`, `ok_pool_fifo.ubl`) proves what a real Ubel
    // program can actually observe (growth doesn't fail, holes get
    // skipped during iteration, `.fifo()` doesn't break basic
    // acquire/release/at) — not the raw index-reuse order.

    fn pool(capacity: usize) -> PoolInner {
        Rc::new(RefCell::new(PoolData::with_capacity(capacity)))
    }

    #[test]
    fn lifo_is_the_default_most_recently_freed_reused_first() {
        let p = pool(3);
        let a = acquire(&p, &[Value::Int(1)]).unwrap();
        let b = acquire(&p, &[Value::Int(2)]).unwrap();
        let _c = acquire(&p, &[Value::Int(3)]).unwrap();
        release(&p, &[a.clone()]).unwrap();
        release(&p, &[b.clone()]).unwrap();
        // Released b last -> LIFO gives b's slot back first.
        let d = acquire(&p, &[Value::Int(4)]).unwrap();
        let Value::Handle { index: b_idx, .. } = b else { panic!() };
        let Value::Handle { index: d_idx, .. } = d else { panic!() };
        assert_eq!(b_idx, d_idx, "LIFO should reuse the most-recently-released slot first");
    }

    #[test]
    fn fifo_opt_in_reuses_oldest_freed_first() {
        let p = pool(3);
        let a = acquire(&p, &[Value::Int(1)]).unwrap();
        let b = acquire(&p, &[Value::Int(2)]).unwrap();
        let _c = acquire(&p, &[Value::Int(3)]).unwrap();
        fifo(&p, &[]).unwrap();
        release(&p, &[a.clone()]).unwrap();
        release(&p, &[b.clone()]).unwrap();
        // Released a first -> FIFO gives a's slot back first (opposite of LIFO).
        let d = acquire(&p, &[Value::Int(4)]).unwrap();
        let Value::Handle { index: a_idx, .. } = a else { panic!() };
        let Value::Handle { index: d_idx, .. } = d else { panic!() };
        assert_eq!(a_idx, d_idx, "FIFO should reuse the oldest-released slot first");
    }

    #[test]
    fn growable_appends_a_block_without_touching_existing_ones() {
        let p = pool(2);
        let a = acquire(&p, &[Value::Int(1)]).unwrap();
        let _b = acquire(&p, &[Value::Int(2)]).unwrap();
        // Pool full, not growable -> null.
        assert_eq!(acquire(&p, &[Value::Int(3)]).unwrap(), Value::Null);
        growable(&p, &[]).unwrap();
        let c = acquire(&p, &[Value::Int(3)]).unwrap();
        assert!(!matches!(c, Value::Null), "growable pool should not return null once grown");
        // The original handle must still read correctly -- proves the
        // grow didn't reallocate/move the first block's existing data.
        assert_eq!(get(&p, &[a]).unwrap(), Value::Int(1));
        assert_eq!(p.borrow().blocks.len(), 2, "should have appended exactly one new block");
        assert_eq!(p.borrow().total_capacity(), 4);
    }

    #[test]
    fn iter_occupied_skips_released_holes_in_index_order() {
        // Deliberately doesn't assume which physical slot a given
        // acquire() lands in (LIFO-from-an-ascending-initialized free
        // list actually fills fresh pools 3,2,1,0 -- not 0,1,2,3 --
        // matching the pre-existing Vec::pop()-based behavior this
        // replaced; an incidental detail, not a property to assert on).
        // What's actually being proven: the released value is gone, the
        // other three are still there, and nothing panics walking past
        // a hole -- the actual skipfield property, order-independent.
        let p = pool(4);
        let _a = acquire(&p, &[Value::Int(10)]).unwrap();
        let b  = acquire(&p, &[Value::Int(20)]).unwrap();
        let _c = acquire(&p, &[Value::Int(30)]).unwrap();
        let _d = acquire(&p, &[Value::Int(40)]).unwrap();
        release(&p, &[b]).unwrap();
        let mut seen: Vec<Value> = p.borrow().iter_occupied().cloned().collect();
        seen.sort_by_key(|v| match v { Value::Int(n) => *n, _ => 0 });
        assert_eq!(seen, vec![Value::Int(10), Value::Int(30), Value::Int(40)],
            "released value (20) should be gone; the other three should all still be present");
    }
}
