// src/lib.rs
// Gate lalrpop_util and the parser module behind the parser feature.
// When the feature is off, rustc never compiles grammar.rs (the huge
// generated file) even though lalrpop still generates it at build time.

#[cfg(feature = "parser")]
#[macro_use]
extern crate lalrpop_util;

pub mod lexer;
pub mod error_management;
pub mod ast;
pub mod sema;
pub mod interpreter;

#[cfg(feature = "parser")]
pub mod parser;

pub use lexer::{Token, TokenType, tokenize};
