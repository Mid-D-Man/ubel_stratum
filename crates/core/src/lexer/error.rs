//! Lexer error types

use std::fmt;
use crate::lexer::Span;

#[derive(Debug, Clone)]
pub enum LexError {
    UnexpectedChar { ch: char, span: Span },
    UnterminatedString { span: Span },
    UnterminatedBlockComment { span: Span },
    InvalidNumber { text: String, span: Span },
    InvalidEscape { ch: char, span: Span },
    InvalidInterpolation { message: String, span: Span },
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LexError::UnexpectedChar { ch, span } => {
                write!(f, "Unexpected character '{}' at line {}, column {}", 
                    ch, span.line, span.column)
            }
            LexError::UnterminatedString { span } => {
                write!(f, "Unterminated string starting at line {}, column {}", 
                    span.line, span.column)
            }
            LexError::UnterminatedBlockComment { span } => {
                write!(f, "Unterminated block comment starting at line {}, column {}", 
                    span.line, span.column)
            }
            LexError::InvalidNumber { text, span } => {
                write!(f, "Invalid number '{}' at line {}, column {}", 
                    text, span.line, span.column)
            }
            LexError::InvalidEscape { ch, span } => {
                write!(f, "Invalid escape sequence '\\{}' at line {}, column {}", 
                    ch, span.line, span.column)
            }
            LexError::InvalidInterpolation { message, span } => {
                write!(f, "Invalid string interpolation: {} at line {}, column {}", 
                    message, span.line, span.column)
            }
        }
    }
}

impl std::error::Error for LexError {}
