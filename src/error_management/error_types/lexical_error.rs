//! Lexical errors with fix suggestions

use crate::lexer::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub enum LexicalError {
    UnexpectedChar {
        ch: char,
        span: Span,
        suggestion: Option<String>,
    },
    UnterminatedString {
        span: Span,
        string_type: StringType,
    },
    UnterminatedBlockComment {
        span: Span,
        nesting_level: usize,
    },
    InvalidNumber {
        text: String,
        span: Span,
        reason: String,
    },
    InvalidEscape {
        sequence: String,
        span: Span,
        valid_escapes: Vec<String>,
    },
    InvalidInterpolation {
        message: String,
        span: Span,
        suggestion: Option<String>,
    },
    InvalidCharLiteral {
        content: String,
        span: Span,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StringType {
    Normal,
    Interpolated,
    Verbatim,
    InterpolatedVerbatim,
}

impl LexicalError {
    pub fn span(&self) -> Span {
        match self {
            LexicalError::UnexpectedChar { span, .. } => *span,
            LexicalError::UnterminatedString { span, .. } => *span,
            LexicalError::UnterminatedBlockComment { span, .. } => *span,
            LexicalError::InvalidNumber { span, .. } => *span,
            LexicalError::InvalidEscape { span, .. } => *span,
            LexicalError::InvalidInterpolation { span, .. } => *span,
            LexicalError::InvalidCharLiteral { span, .. } => *span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            LexicalError::UnexpectedChar { ch, .. } => {
                format!("Unexpected character '{}'", ch)
            }
            LexicalError::UnterminatedString { string_type, .. } => {
                match string_type {
                    StringType::Normal => "Unterminated string literal".to_string(),
                    StringType::Interpolated => "Unterminated interpolated string".to_string(),
                    StringType::Verbatim => "Unterminated verbatim string".to_string(),
                    StringType::InterpolatedVerbatim => "Unterminated interpolated verbatim string".to_string(),
                }
            }
            LexicalError::UnterminatedBlockComment { nesting_level, .. } => {
                format!("Unterminated block comment (nesting level: {})", nesting_level)
            }
            LexicalError::InvalidNumber { text, reason, .. } => {
                format!("Invalid number literal '{}': {}", text, reason)
            }
            LexicalError::InvalidEscape { sequence, .. } => {
                format!("Invalid escape sequence '{}'", sequence)
            }
            LexicalError::InvalidInterpolation { message, .. } => {
                message.clone()
            }
            LexicalError::InvalidCharLiteral { content, reason, .. } => {
                format!("Invalid character literal '{}': {}", content, reason)
            }
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            LexicalError::UnexpectedChar { suggestion, .. } => suggestion.clone(),
            LexicalError::UnterminatedString { string_type, .. } => {
                Some(match string_type {
                    StringType::Normal => "Add closing quote \"".to_string(),
                    StringType::Interpolated => "Add closing quote \" to interpolated string".to_string(),
                    StringType::Verbatim => "Add closing quote \" to verbatim string".to_string(),
                    StringType::InterpolatedVerbatim => "Add closing quote \" to interpolated verbatim string".to_string(),
                })
            }
            LexicalError::UnterminatedBlockComment { .. } => {
                Some("Add closing */".to_string())
            }
            LexicalError::InvalidNumber { text, .. } => {
                // Try to suggest fix based on common mistakes
                if text.contains("..") {
                    Some("Remove extra decimal point".to_string())
                } else if text.starts_with("0x") && text.len() == 2 {
                    Some("Add hex digits after 0x".to_string())
                } else if text.starts_with("0b") && text.len() == 2 {
                    Some("Add binary digits after 0b".to_string())
                } else {
                    None
                }
            }
            LexicalError::InvalidEscape { valid_escapes, .. } => {
                Some(format!("Valid escape sequences: {}", valid_escapes.join(", ")))
            }
            LexicalError::InvalidInterpolation { suggestion, .. } => {
                suggestion.clone()
            }
            LexicalError::InvalidCharLiteral { .. } => {
                Some("Character literals must contain exactly one character".to_string())
            }
        }
    }
}

impl fmt::Display for LexicalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for LexicalError {}