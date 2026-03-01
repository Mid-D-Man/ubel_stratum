// src/parser/mod.rs

use lalrpop_util::ParseError as LalrError;
use crate::ast::root::Program;
use crate::ast::arena::AstArena;
use crate::error_management::{ErrorManager, error_types::{ParseError, ParseContext}};
use crate::lexer::Token;

pub fn parse<'ast>(
    arena:  &'ast AstArena,
    tokens: Vec<Token>,
    source: String,
) -> Result<Program<'ast>, ErrorManager> {
    let mut errors = ErrorManager::new(source);

    // lalrpop generates a parser struct — call it here
    // and map its errors immediately
    match grammar::ProgramParser::new().parse(&arena, &mut errors, token_iter(&tokens)) {
        Ok(program) => {
            if errors.has_errors() {
                Err(errors)   // recovered but still had issues
            } else {
                Ok(program)
            }
        }
        Err(e) => {
            errors.add_parse_error(lalr_to_parse_error(e));
            Err(errors)
        }
    }
}

fn lalr_to_parse_error(e: LalrError<usize, crate::lexer::TokenType, ParseError>) -> ParseError {
    match e {
        LalrError::InvalidToken { location } => ParseError::Raw {
            message: "Invalid token".to_string(),
            span: crate::lexer::Span::new(location, location, 0, 0),
        },
        LalrError::UnrecognizedEof { location, expected } => ParseError::UnexpectedEof {
            expected,
            span:    crate::lexer::Span::new(location, location, 0, 0),
            context: ParseContext::TopLevel,
        },
        LalrError::UnrecognizedToken { token: (lo, tok, hi), expected } => {
            ParseError::UnexpectedToken {
                found:    tok,
                expected,
                span:     crate::lexer::Span::new(lo, hi, 0, 0),
                context:  ParseContext::TopLevel,
            }
        }
        LalrError::ExtraToken { token: (lo, tok, hi) } => ParseError::UnexpectedToken {
            found:    tok,
            expected: vec![],
            span:     crate::lexer::Span::new(lo, hi, 0, 0),
            context:  ParseContext::TopLevel,
        },
        LalrError::User { error } => error,  // already a ParseError we emitted
    }
  }
