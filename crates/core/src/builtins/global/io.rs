// crates/core/src/builtins/global/io.rs
//! Global output functions: `println`, `print`, `log`.

use crate::interpreter::value::{EvalResult, Value};

pub fn println(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    println!("{}", out.join(" "));
    Ok(Value::Void)
}

pub fn print(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    print!("{}", out.join(" "));
    Ok(Value::Void)
}

pub fn log(args: &[Value]) -> EvalResult {
    let out: Vec<String> = args.iter().map(|v| v.to_string()).collect();
    eprintln!("[log] {}", out.join(" "));
    Ok(Value::Void)
}
