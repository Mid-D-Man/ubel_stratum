// src/interpreter/eval/stmt.rs
//! Statement and block evaluation.
//! Full implementation — next response in this session.

#![allow(dead_code, unused_variables)]

use crate::ast::statements::Block;
use crate::interpreter::eval::Interpreter;
use crate::interpreter::value::{EvalResult, Value};

pub fn eval_block<'ast>(interp: &mut Interpreter<'ast>, block: &Block<'ast>) -> EvalResult {
    let mut last = Value::Void;
    for stmt in block.stmts {
        last = eval_stmt(interp, stmt)?;
    }
    Ok(last)
}

pub fn eval_stmt<'ast>(
    _interp: &mut Interpreter<'ast>,
    _stmt: &crate::ast::statements::Stmt<'ast>,
) -> EvalResult {
    // Full implementation next response.
    todo!("eval_stmt")
}
