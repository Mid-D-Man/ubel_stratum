//! Block and documentation comment parsing

use crate::lexer::{Token, TokenType, Span};
use crate::error_management::error_types::LexicalError;

pub struct CommentParser<'a> {
    input: &'a str,
    position: usize,
    line: usize,
    column: usize,
}

impl<'a> CommentParser<'a> {
    pub fn new(input: &'a str, start_pos: usize, line: usize, column: usize) -> Self {
        CommentParser {
            input,
            position: start_pos,
            line,
            column,
        }
    }

    /// Parse block comment with nesting support: /* ... /* nested */ ... */
    pub fn parse_block_comment(&mut self) -> Result<(Token, usize, usize, usize), LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        // Skip /*
        self.position += 2;
        self.column += 2;

        let mut content = String::new();
        let mut depth = 1;

        while self.position < self.input.len() && depth > 0 {
            let ch = self.char_at(self.position);

            // Check for nested /*
            if ch == '/' && self.position + 1 < self.input.len()
                && self.char_at(self.position + 1) == '*' {
                depth += 1;
                content.push('/');
                content.push('*');
                self.position += 2;
                self.column += 2;
                continue;
            }

            // Check for closing */
            if ch == '*' && self.position + 1 < self.input.len()
                && self.char_at(self.position + 1) == '/' {
                depth -= 1;
                if depth > 0 {
                    content.push('*');
                    content.push('/');
                }
                self.position += 2;
                self.column += 2;
                continue;
            }

            // Regular character
            content.push(ch);
            self.position += 1;

            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }

        if depth == 0 {
            let span = Span::new(start_pos, self.position, start_line, start_column);
            let lexeme = &self.input[start_pos..self.position];

            Ok((
                Token::new(TokenType::Comment(content), span, lexeme.to_string()),
                self.position,
                self.line,
                self.column,
            ))
        } else {
            Err(LexicalError::UnterminatedBlockComment {
                span: Span::new(start_pos, self.position, start_line, start_column),
                nesting_level: depth,
            })
        }
    }

    /// Parse doc comment: /** ... */ or /*! ... */
    pub fn parse_doc_comment(&mut self, start_marker: &str) -> Result<(Token, usize, usize, usize), LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        // Skip /** or /*!
        let marker_len = start_marker.len();
        self.position += marker_len;
        self.column += marker_len;

        let mut content = String::new();

        while self.position < self.input.len() {
            let ch = self.char_at(self.position);

            // Check for closing */
            if ch == '*' && self.position + 1 < self.input.len()
                && self.char_at(self.position + 1) == '/' {
                self.position += 2;
                self.column += 2;

                let span = Span::new(start_pos, self.position, start_line, start_column);
                let lexeme = &self.input[start_pos..self.position];

                return Ok((
                    Token::new(TokenType::DocComment(content.trim().to_string()), span, lexeme.to_string()),
                    self.position,
                    self.line,
                    self.column,
                ));
            }

            content.push(ch);
            self.position += 1;

            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }

        // Unterminated doc comment
        Err(LexicalError::UnterminatedBlockComment {
            span: Span::new(start_pos, self.position, start_line, start_column),
            nesting_level: 1,
        })
    }

    #[inline]
    fn char_at(&self, pos: usize) -> char {
        self.input.chars().nth(pos).unwrap_or('\0')
    }
}