// crates/core/src/builtins/global/conversions.rs
//! Global type-conversion functions.

use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub fn to_string(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("to_string() needs 1 argument".into()))?;
    Ok(Value::Str(Rc::new(val.to_string())))
}

pub fn to_int(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("to_int() needs 1 argument".into()))?;
    let n = match val {
        Value::Int(n)    => *n,
        Value::Float(f)  => *f as i64,
        Value::Double(d) => *d as i64,
        Value::Bool(b)   => if *b { 1 } else { 0 },
        Value::Str(s)    => s.parse::<i64>().map_err(|_| {
            Signal::Fail(Value::str_from(format!("cannot parse '{}' as int", s)))
        })?,
        other => return Err(Signal::Panic(format!(
            "to_int() not supported on {}", other.type_name()
        ))),
    };
    Ok(Value::Int(n))
}

pub fn to_float(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("to_float() needs 1 argument".into()))?;
    let f = match val {
        Value::Float(f)  => *f,
        Value::Int(n)    => *n as f32,
        Value::Double(d) => *d as f32,
        Value::Str(s)    => s.parse::<f32>().map_err(|_| {
            Signal::Fail(Value::str_from(format!("cannot parse '{}' as float", s)))
        })?,
        other => return Err(Signal::Panic(format!(
            "to_float() not supported on {}", other.type_name()
        ))),
    };
    Ok(Value::Float(f))
}

pub fn to_double(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("to_double() needs 1 argument".into()))?;
    let d = match val {
        Value::Double(d) => *d,
        Value::Float(f)  => *f as f64,
        Value::Int(n)    => *n as f64,
        Value::Str(s)    => s.parse::<f64>().map_err(|_| {
            Signal::Fail(Value::str_from(format!("cannot parse '{}' as double", s)))
        })?,
        other => return Err(Signal::Panic(format!(
            "to_double() not supported on {}", other.type_name()
        ))),
    };
    Ok(Value::Double(d))
}
