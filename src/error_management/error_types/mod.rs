//! Error type definitions

pub mod lexical_error;
// TODO: Future error types
// pub mod parse_error;
// pub mod semantic_error;
// pub mod runtime_error;

pub use lexical_error::{LexicalError, StringType};