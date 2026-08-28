// crates/core/src/builtins/constructors.rs
//! Static constructors for builtin collection types.
use crate::interpreter::value::{EvalResult, Value};

pub fn list_new(_args: &[Value]) -> EvalResult { Ok(Value::new_list()) }
pub fn dictionary_new(_args: &[Value]) -> EvalResult { Ok(Value::new_dict()) }
pub fn queue_new(_args: &[Value]) -> EvalResult { Ok(Value::new_queue()) }
pub fn stack_new(_args: &[Value]) -> EvalResult { Ok(Value::new_stack()) }

/// `InlineList.new(capacity)` — DATASTRUCTURES.md §5. Sema has already
/// validated `capacity` is a literal integer (`TypeError::
/// InlineListCapacityNotLiteral` otherwise), so this just reads it back;
/// no re-checking here, same "sema validated, interpreter trusts it"
/// convention as every other builtin constructor in this file.
pub fn inline_list_new(args: &[Value]) -> EvalResult {
    let capacity = match args.first() {
        Some(Value::Int(n)) if *n >= 0 => *n as usize,
        _ => return Err(crate::interpreter::value::Signal::Panic(
            "InlineList.new() requires a non-negative integer capacity".into(),
        )),
    };
    Ok(Value::new_inline_list(capacity))
}

// MEMORY_MODEL.md §9 — `Unique<T>`/`Shared<T>`/`SyncShared<T>` construction.
// Sema has already validated exactly one argument (`TypeError::
// ArgumentCountMismatch` otherwise) and the enclosing `@tier(low)`
// requirement (`TierError::OwnershipWrapperOutsideLowTier` otherwise), so
// this just wraps the already-evaluated argument — same "sema validated,
// interpreter trusts it" convention as every other constructor here.
pub fn unique_new(args: &[Value]) -> EvalResult {
    Ok(Value::Unique(Box::new(args[0].clone())))
}

/// `Rc<RefCell<Value>>` — genuinely shared, clone-aliasing backing, same
/// representation `List`/`Dict`/`Queue`/`Stack`/`Struct` already use.
pub fn shared_new(args: &[Value]) -> EvalResult {
    Ok(Value::Shared(std::rc::Rc::new(std::cell::RefCell::new(args[0].clone()))))
}

/// Same `Rc<RefCell<Value>>` backing as `Shared` for now — see
/// `Value::SyncShared`'s own doc comment for why this isn't `Arc<Mutex<_>>`
/// yet (`Value` overall isn't `Send` while `List`/`Dict`/etc. use `Rc`
/// internally, so an `Arc<Mutex<_>>` wrapper here would be a structurally
/// real but semantically hollow promise). A genuine thread-safety story
/// needs a `Value` representation that's actually `Send`, or waits for
/// native codegen where this tier's semantics can be enforced by the type
/// system instead of the tree-walking interpreter.
pub fn sync_shared_new(args: &[Value]) -> EvalResult {
    Ok(Value::SyncShared(std::rc::Rc::new(std::cell::RefCell::new(args[0].clone()))))
}
