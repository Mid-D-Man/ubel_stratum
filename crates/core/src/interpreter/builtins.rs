// src/interpreter/builtins.rs
//! Native built-in function implementations.
//!
//! All builtins have signature `fn(&[Value]) -> EvalResult`.
//! They are registered into the interpreter's function table at startup
//! and exposed under their names in the global environment.

#![allow(dead_code)]

use std::rc::Rc;
use crate::interpreter::value::{EvalResult, Signal, Value};

/// Signature all built-in functions must satisfy.
pub type BuiltinFn = fn(&[Value]) -> EvalResult;

/// Returns every built-in as a (name, fn) pair for registration.
pub fn all_builtins() -> Vec<(&'static str, BuiltinFn)> {
    vec![
        ("println",   builtin_println),
        ("print",     builtin_print),
        ("log",       builtin_log),
        ("assert",    builtin_assert),
        ("panic",     builtin_panic),
        ("typeof",    builtin_typeof),
        ("len",       builtin_len),
        ("push",      builtin_push),
        ("pop",       builtin_pop),
        ("contains",  builtin_contains),
        ("to_string", builtin_to_string),
        ("to_int",    builtin_to_int),
        ("to_float",  builtin_to_float),
        ("to_double", builtin_to_double),
        ("sqrt",      builtin_sqrt),
        ("abs",       builtin_abs),
        ("min",       builtin_min),
        ("max",       builtin_max),
        ("floor",     builtin_floor),
        ("ceil",      builtin_ceil),
        ("range",     builtin_range),
    ]
}

// ── Output ────────────────────────────────────────────────────────

fn builtin_println(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    println!("{}", out.join(" "));
    Ok(Value::Void)
}

fn builtin_print(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    print!("{}", out.join(" "));
    Ok(Value::Void)
}

fn builtin_log(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    eprintln!("[log] {}", out.join(" "));
    Ok(Value::Void)
}

// ── Assertions ────────────────────────────────────────────────────

fn builtin_assert(args: &[Value]) -> EvalResult {
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

fn builtin_panic(args: &[Value]) -> EvalResult {
    let msg = args.first()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "explicit panic".into());
    Err(Signal::Panic(msg))
}

// ── Type inspection ───────────────────────────────────────────────

fn builtin_typeof(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("typeof() needs 1 argument".into()))?;
    Ok(Value::Str(Rc::new(val.type_name().into())))
}

// ── Collections ───────────────────────────────────────────────────

fn builtin_len(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("len() needs 1 argument".into()))?;
    let n: i64 = match val {
        Value::Str(s)   => s.len() as i64,
        Value::List(rc) => rc.borrow().len() as i64,
        Value::Dict(rc) => rc.borrow().len() as i64,
        Value::Tuple(v) => v.len() as i64,
        other => return Err(Signal::Panic(format!(
            "len() not supported on {}", other.type_name()
        ))),
    };
    Ok(Value::Int(n))
}

fn builtin_push(args: &[Value]) -> EvalResult {
    let (list, item) = two_args("push", args)?;
    match list {
        Value::List(rc) => { rc.borrow_mut().push(item.clone()); Ok(Value::Void) }
        other => Err(Signal::Panic(format!("push() not supported on {}", other.type_name()))),
    }
}

fn builtin_pop(args: &[Value]) -> EvalResult {
    let list = args.first()
        .ok_or_else(|| Signal::Panic("pop() needs 1 argument".into()))?;
    match list {
        Value::List(rc) => Ok(rc.borrow_mut().pop().unwrap_or(Value::Null)),
        other => Err(Signal::Panic(format!("pop() not supported on {}", other.type_name()))),
    }
}

fn builtin_contains(args: &[Value]) -> EvalResult {
    let (coll, item) = two_args("contains", args)?;
    let found = match coll {
        Value::List(rc) => rc.borrow().iter().any(|v| v.equals(item)),
        Value::Str(s)   => {
            if let Value::Str(sub) = item {
                s.contains(sub.as_str())
            } else { false }
        }
        other => return Err(Signal::Panic(format!(
            "contains() not supported on {}", other.type_name()
        ))),
    };
    Ok(Value::Bool(found))
}

/// Construct a `List` of integers `[start, start+1, …, end-1]`.
fn builtin_range(args: &[Value]) -> EvalResult {
    match args {
        [Value::Int(start), Value::Int(end)] => {
            let items: Vec<Value> = (*start..*end).map(Value::Int).collect();
            Ok(Value::List(Rc::new(std::cell::RefCell::new(items))))
        }
        _ => Err(Signal::Panic("range(start: int, end: int) needs 2 int arguments".into())),
    }
}

// ── Conversions ───────────────────────────────────────────────────

fn builtin_to_string(args: &[Value]) -> EvalResult {
    let val = args.first()
        .ok_or_else(|| Signal::Panic("to_string() needs 1 argument".into()))?;
    Ok(Value::Str(Rc::new(val.to_string())))
}

fn builtin_to_int(args: &[Value]) -> EvalResult {
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

fn builtin_to_float(args: &[Value]) -> EvalResult {
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

fn builtin_to_double(args: &[Value]) -> EvalResult {
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

// ── Math ──────────────────────────────────────────────────────────

fn builtin_sqrt(args: &[Value]) -> EvalResult {
    numeric_unary("sqrt", args, f64::sqrt)
}

fn builtin_floor(args: &[Value]) -> EvalResult {
    numeric_unary("floor", args, f64::floor)
}

fn builtin_ceil(args: &[Value]) -> EvalResult {
    numeric_unary("ceil", args, f64::ceil)
}

fn builtin_abs(args: &[Value]) -> EvalResult {
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

fn builtin_min(args: &[Value]) -> EvalResult {
    let (a, b) = two_args("min", args)?;
    numeric_compare("min", a, b, |x, y| if x <= y { a.clone() } else { b.clone() })
}

fn builtin_max(args: &[Value]) -> EvalResult {
    let (a, b) = two_args("max", args)?;
    numeric_compare("max", a, b, |x, y| if x >= y { a.clone() } else { b.clone() })
}

// ── Helpers ───────────────────────────────────────────────────────

/// Extract exactly two arguments or return a Panic.
fn two_args<'a>(name: &str, args: &'a [Value]) -> Result<(&'a Value, &'a Value), Signal> {
    if args.len() < 2 {
        return Err(Signal::Panic(format!("{}() needs 2 arguments", name)));
    }
    Ok((&args[0], &args[1]))
}

/// Apply a unary f64 function to any numeric value, returning the same type.
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
