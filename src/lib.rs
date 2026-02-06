//! Ubel Stratum Compiler Library

pub mod lexer;
pub mod error_management;

// TODO: Phase 2 - Implement these modules when ready for tree-walking interpreter
// pub mod parser;
// pub mod semantic;
// pub mod interpreter;
// pub mod tier_analysis;
// pub mod stdlib;

pub use lexer::{Token, TokenType, tokenize};
