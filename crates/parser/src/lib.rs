// ubel_stratum_parser/src/lib.rs
//! LALRPOP parser frontend for Ubel Stratum.
//!
//! ## Re-export design
//!
//! `grammar.lalrpop` uses `crate::ast::...`, `crate::lexer::...`, and
//! `crate::error_management::...` throughout its 1400+ lines.  By
//! re-exporting those modules here, every `crate::` path in the grammar,
//! token_iter, and helpers resolves through `ubel_stratum_parser` without
//! touching a single grammar rule.
//!
//! ## One-line migration diff in grammar.lalrpop (line 32)
//!
//!   - use crate::parser::helpers::{self, *};
//!   + use crate::helpers::{self, *};

#[macro_use]
extern crate lalrpop_util;

// ── Re-exports: satisfy every `crate::X` path in grammar + helpers ───────────
pub use ubel_stratum::ast;
pub use ubel_stratum::error_management;
pub use ubel_stratum::lexer;

// ── Parser modules ───────────────────────────────────────────────────────────
// grammar.lalrpop → ubel_stratum_parser/src/grammar.lalrpop
// lalrpop generates it into OUT_DIR/grammar.rs
lalrpop_mod!(
    #[allow(clippy::all)]
    pub grammar,
    "/grammar.rs"
);

pub mod helpers;
mod token_iter;

use token_iter::TokenIter;
use lalrpop_util::ParseError as LalrError;
use ubel_stratum::ast::arena::AstArena;
use ubel_stratum::ast::expressions::Expr;
use ubel_stratum::ast::root::Program;
use ubel_stratum::error_management::{
    ErrorManager,
    error_types::{ParseError, ParseContext},
};
use ubel_stratum::lexer::{Span, Token, TokenType};

/// Parse a full `.ubl` source file into a `Program<'ast>`.
pub fn parse<'ast>(
    arena:  &'ast AstArena,
    tokens: Vec<Token>,
    source: String,
) -> Result<Program<'ast>, ErrorManager> {
    let mut errors = ErrorManager::new(source);
    let iter = TokenIter::new(&tokens);
    match grammar::ProgramParser::new().parse(arena, &mut errors, iter) {
        Ok(program) => {
            if errors.has_errors() { Err(errors) } else { Ok(program) }
        }
        Err(e) => {
            errors.add_parse_error(lalr_to_parse_error(e));
            Err(errors)
        }
    }
}

/// Parse a single expression (used by the interpreter for interpolated strings).
pub fn parse_expr<'ast>(
    arena:  &'ast AstArena,
    source: &str,
) -> Option<&'ast Expr<'ast>> {
    let tokens = ubel_stratum::lexer::tokenize(source).ok()?;
    let mut errors = ErrorManager::new(source.to_string());
    let iter = TokenIter::new(&tokens);
    grammar::ExprParser::new().parse(arena, &mut errors, iter).ok()
}

fn lalr_to_parse_error(e: LalrError<usize, TokenType, ParseError>) -> ParseError {
    match e {
        LalrError::InvalidToken { location } => ParseError::Raw {
            message: "Invalid token".to_string(),
            span:    Span::new(location, location, 0, 0),
        },
        LalrError::UnrecognizedEof { location, expected } => ParseError::UnexpectedEof {
            expected,
            span:    Span::new(location, location, 0, 0),
            context: ParseContext::TopLevel,
        },
        LalrError::UnrecognizedToken { token: (lo, tok, hi), expected } => {
            ParseError::UnexpectedToken {
                found:    tok,
                expected,
                span:     Span::new(lo, hi, 0, 0),
                context:  ParseContext::TopLevel,
            }
        }
        LalrError::ExtraToken { token: (lo, tok, hi) } => ParseError::UnexpectedToken {
            found:    tok,
            expected: vec![],
            span:     Span::new(lo, hi, 0, 0),
            context:  ParseContext::TopLevel,
        },
        LalrError::User { error } => error,
    }
  }
