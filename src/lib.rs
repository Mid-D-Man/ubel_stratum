// src/lib.rs
#[macro_use]
extern crate lalrpop_util;

pub mod lexer;
pub mod error_management;
pub mod ast;
pub mod parser;
pub mod sema;

pub use lexer::{Token, TokenType, tokenize};
