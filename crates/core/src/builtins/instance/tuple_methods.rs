// crates/core/src/builtins/instance/tuple_methods.rs
//! Instance methods on `Value::Tuple` — called as `myTuple.method(...)`.

use crate::interpreter::value::Value;

pub const METHOD_NAMES: &[&str] = &["len"];

pub fn len(elems: &[Value]) -> Value {
    Value::Int(elems.len() as i64)
}
