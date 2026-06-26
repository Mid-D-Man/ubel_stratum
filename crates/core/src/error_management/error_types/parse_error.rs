// src/error_management/error_types/parse_error.rs

use crate::lexer::{Span, TokenType};
use std::fmt;

#[derive(Debug, Clone)]
pub enum ParseError {
    /// Got a token we didn't expect at all
    UnexpectedToken {
        found:    TokenType,
        expected: Vec<String>,   // human-readable, e.g. ["'fn'", "identifier"]
        span:     Span,
        context:  ParseContext,  // where in the grammar we were
    },

    /// Hit EOF mid-parse
    UnexpectedEof {
        expected: Vec<String>,
        span:     Span,
        context:  ParseContext,
    },

    /// Opened a delimiter but never closed it
    UnclosedDelimiter {
        delimiter:  char,          // '(', '{', '['
        opened_at:  Span,
        closed_by:  Option<char>,  // what we found instead
        span:       Span,
    },

    /// A syntactically valid token that's illegal here
    /// e.g. `await` in a @tier(low) function
    IllegalInContext {
        what:       String,
        reason:     String,
        span:       Span,
        suggestion: Option<String>,
    },

    /// lalrpop's raw error wrapped + enriched
    /// We catch these at the boundary and convert to the variants above
    /// This is the escape hatch for anything we haven't mapped yet
    Raw {
        message: String,
        span:    Span,
    },
}

/// Lets error messages say "while parsing function declaration" etc.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseContext {
    TopLevel,
    FunctionDecl,
    StructDecl,
    EnumDecl,
    TraitDecl,
    ImplBlock,
    ExtendDecl,
    FunctionParam,
    ReturnType,
    TypeExpr,
    Expr,
    Statement,
    Block,
    MatchArm,
    Pattern,
    ImportDecl,
    TierAnnotation,
}

impl ParseContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParseContext::TopLevel        => "top-level declaration",
            ParseContext::FunctionDecl    => "function declaration",
            ParseContext::StructDecl      => "struct declaration",
            ParseContext::EnumDecl        => "enum declaration",
            ParseContext::TraitDecl       => "trait declaration",
            ParseContext::ImplBlock       => "impl block",
            ParseContext::ExtendDecl      => "extend declaration",
            ParseContext::FunctionParam   => "function parameter",
            ParseContext::ReturnType      => "return type",
            ParseContext::TypeExpr        => "type expression",
            ParseContext::Expr            => "expression",
            ParseContext::Statement       => "statement",
            ParseContext::Block           => "block",
            ParseContext::MatchArm        => "match arm",
            ParseContext::Pattern         => "pattern",
            ParseContext::ImportDecl      => "import declaration",
            ParseContext::TierAnnotation  => "tier annotation",
        }
    }
}

impl ParseError {
    pub fn span(&self) -> Span {
        match self {
            ParseError::UnexpectedToken   { span, .. } => *span,
            ParseError::UnexpectedEof     { span, .. } => *span,
            ParseError::UnclosedDelimiter { span, .. } => *span,
            ParseError::IllegalInContext  { span, .. } => *span,
            ParseError::Raw               { span, .. } => *span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ParseError::UnexpectedToken { found, expected, context, .. } => {
                let exp = expected.join(", ");
                format!(
                    "Unexpected token `{:?}` while parsing {}, expected: {}",
                    found,
                    context.as_str(),
                    exp
                )
            }
            ParseError::UnexpectedEof { expected, context, .. } => {
                let exp = expected.join(", ");
                format!(
                    "Unexpected end of file while parsing {}, expected: {}",
                    context.as_str(),
                    exp
                )
            }
            ParseError::UnclosedDelimiter { delimiter, closed_by, .. } => {
                match closed_by {
                    Some(c) => format!(
                        "Unclosed `{}` — found `{}` instead of closing delimiter",
                        delimiter, c
                    ),
                    None => format!(
                        "Unclosed `{}` — reached end of file without closing",
                        delimiter
                    ),
                }
            }
            ParseError::IllegalInContext { what, reason, .. } => {
                format!("`{}` is not allowed here: {}", what, reason)
            }
            ParseError::Raw { message, .. } => message.clone(),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            ParseError::UnexpectedToken { found, expected, .. } => {
                if expected.len() == 1 {
                    Some(format!("Try replacing `{:?}` with {}", found, expected[0]))
                } else {
                    None
                }
            }
            ParseError::UnclosedDelimiter { delimiter, .. } => {
                let closing = match delimiter {
                    '(' => ')',
                    '{' => '}',
                    '[' => ']',
                    _   => *delimiter,
                };
                Some(format!("Add a closing `{}`", closing))
            }
            ParseError::IllegalInContext { suggestion, .. } => suggestion.clone(),
            _ => None,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ParseError {}
