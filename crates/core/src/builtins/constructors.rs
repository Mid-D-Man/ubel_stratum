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
