// crates/core/src/builtins/global/math.rs
//! Global math functions. Bare-call for now (`sqrt(x)`); each of these
//! maps to a native LLVM intrinsic later (`llvm.sqrt.f64`, `llvm.fabs.f64`,
//! `llvm.minnum.f64`, ...) rather than a linked runtime call — see
//! `Lowering` in `builtins/mod.rs`.

use std::rc::Rc;
use std::cell::RefCell;
use crate::interpreter::value::{EvalResult, Signal, Value};

pub fn sqrt(args: &[Value]) -> EvalResult { numeric_unary("sqrt", args, f64::sqrt) }
pub fn floor(args: &[Value]) -> EvalResult { numeric_unary("floor", args, f64::floor) }
pub fn ceil(args: &[Value]) -> EvalResult { numeric_unary("ceil", args, f64::ceil) }

pub fn abs(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("abs() needs 1 argument".into()))?;
    Ok(match val {
        Value::Int(n)    => Value::Int(n.abs()),
        Value::Float(f)  => Value::Float(f.abs()),
        Value::Double(d) => Value::Double(d.abs()),
        other => return Err(Signal::Panic(format!(
            "abs() not supported on {}", other.type_name()
        ))),
    })
}

pub fn min(args: &[Value]) -> EvalResult {
    let (a, b) = two_args("min", args)?;
    numeric_compare("min", a, b, |x, y| if x <= y { a.clone() } else { b.clone() })
}

pub fn max(args: &[Value]) -> EvalResult {
    let (a, b) = two_args("max", args)?;
    numeric_compare("max", a, b, |x, y| if x >= y { a.clone() } else { b.clone() })
}

/// Construct a `List` of integers `[start, start+1, …, end-1]`.
pub fn range(args: &[Value]) -> EvalResult {
    match args {
        [Value::Int(start), Value::Int(end)] => {
            let items: Vec<Value> = (*start..*end).map(Value::Int).collect();
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }
        _ => Err(Signal::Panic("range(start: int, end: int) needs 2 int arguments".into())),
    }
}

// ── Helpers ───────────────────────────────────────────────────────

fn two_args<'a>(name: &str, args: &'a [Value]) -> Result<(&'a Value, &'a Value), Signal> {
    if args.len() < 2 {
        return Err(Signal::Panic(format!("{}() needs 2 arguments", name)));
    }
    Ok((&args[0], &args[1]))
}

fn numeric_unary(name: &str, args: &[Value], f: fn(f64) -> f64) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic(format!("{}() needs 1 argument", name)))?;
    Ok(match val {
        Value::Double(d) => Value::Double(f(*d)),
        Value::Float(fv) => Value::Float(f(*fv as f64) as f32),
        Value::Int(n)    => Value::Double(f(*n as f64)),
        other => return Err(Signal::Panic(format!(
            "{}() not supported on {}", name, other.type_name()
        ))),
    })
}

fn numeric_compare(name: &str, a: &Value, b: &Value, pick: impl Fn(f64, f64) -> Value) -> EvalResult {
    let av = numeric_to_f64(a);
    let bv = numeric_to_f64(b);
    match (av, bv) {
        (Some(x), Some(y)) => Ok(pick(x, y)),
        _ => Err(Signal::Panic(format!(
            "{}() requires two numeric arguments, got {} and {}",
            name, a.type_name(), b.type_name()
        ))),
    }
}

fn numeric_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n)    => Some(*n as f64),
        Value::Float(f)  => Some(*f as f64),
        Value::Double(d) => Some(*d),
        _                => None,
    }
}
