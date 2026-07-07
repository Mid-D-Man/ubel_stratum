// crates/core/src/builtins/global/diagnostics.rs
//! Global diagnostic functions: `assert`, `panic`, `typeof`.

use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub fn assert(args: &[Value]) -> EvalResult {
    let cond = args.first()
        .ok_or_else(|| Signal::Panic("assert() needs at least 1 argument".into()))?;
    let msg = args.get(1)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "assertion failed".into());
    match cond {
        Value::Bool(true)  => Ok(Value::Void),
        Value::Bool(false) => Err(Signal::Panic(msg)),
        other => Err(Signal::Panic(format!(
            "assert() condition must be bool, got {}", other.type_name()
        ))),
    }
}

pub fn panic(args: &[Value]) -> EvalResult {
    let msg = args.first()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "explicit panic".into());
    Err(Signal::Panic(msg))
}

pub fn type_of(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("typeof() needs 1 argument".into()))?;
    Ok(Value::Str(Rc::new(val.type_name().into())))
}
