// src/error_management/error_types/mod.rs

pub mod lexical_error;
pub mod parse_error;
pub mod name_error;
pub mod type_error;

pub use lexical_error::{LexicalError, StringType};
pub use parse_error::{ParseContext, ParseError};
pub use name_error::NameError;
pub use type_error::TypeError;
