// crates/core/src/builtins/instance/tuple_methods.rs
//! Instance methods on `Value::Tuple` — called as `myTuple.method(...)`.

use crate::interpreter::value::Value;

pub const METHOD_NAMES: &[&str] = &["len"];

/// No `Tuple` method is HIGH-only today. Real, consulted registry — not
/// a stub — see `instance::is_high_only`.
pub const HIGH_ONLY: &[&str] = &[];

/// `(return shape, arity)` for sema — see `instance::MethodReturn`.
pub fn signature(name: &str) -> Option<(crate::builtins::instance::MethodReturn, usize)> {
    use crate::builtins::instance::MethodReturn as R;
    Some(match name {
        "len" => (R::Int, 0),
        _ => return None,
    })
}

pub fn len(elems: &[Value]) -> Value {
    Value::Int(elems.len() as i64)
}
