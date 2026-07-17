// crates/core/src/builtins/constructors.rs
//! Static constructors for builtin collection types.
use crate::interpreter::value::{EvalResult, Value};

pub fn list_new(_args: &[Value]) -> EvalResult { Ok(Value::new_list()) }
pub fn dictionary_new(_args: &[Value]) -> EvalResult { Ok(Value::new_dict()) }
pub fn queue_new(_args: &[Value]) -> EvalResult { Ok(Value::new_queue()) }
pub fn stack_new(_args: &[Value]) -> EvalResult { Ok(Value::new_stack()) }
