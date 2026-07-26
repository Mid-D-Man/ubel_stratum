// src/parser/token_iter.rs
//! Adapts `Vec<Token>` into the iterator LALRPOP's custom-lexer interface expects:
//!   `Iterator<Item = Result<(usize, TokenType, usize), ParseError>>`
//!
//! Each item is `(start_byte, token_kind, end_byte)`.

use crate::error_management::errors::ParseError;
use crate::lexer::{Token, TokenType};

pub struct TokenIter<'a> {
    tokens: &'a [Token],
    pos:    usize,
}

impl<'a> TokenIter<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        TokenIter { tokens, pos: 0 }
    }
}

impl<'a> Iterator for TokenIter<'a> {
    type Item = Result<(usize, TokenType, usize), ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.pos >= self.tokens.len() {
                return None;
            }
            let tok = &self.tokens[self.pos];
            self.pos += 1;

            match &tok.kind {
                // Exhausted token stream
                TokenType::Eof => return None,
                // Comments are transparent to the parser
                TokenType::Comment(_) | TokenType::DocComment(_) => continue,
                // All real tokens
                _ => {
                    return Some(Ok((
                        tok.span.start,
                        tok.kind.clone(),
                        tok.span.end,
                    )))
                }
            }
        }
    }
}
