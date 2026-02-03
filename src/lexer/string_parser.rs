//! String interpolation and verbatim string parsing

use crate::lexer::{Token, TokenType, Span, InterpolationPart};
use crate::error_management::error_types::{LexicalError, StringType};

pub struct StringParser<'a> {
    input: &'a str,
    position: usize,
    line: usize,
    column: usize,
}

impl<'a> StringParser<'a> {
    pub fn new(input: &'a str, start_pos: usize, line: usize, column: usize) -> Self {
        StringParser {
            input,
            position: start_pos,
            line,
            column,
        }
    }

    /// Parse interpolated string: $"Hello {name}!"
    pub fn parse_interpolated_string(&mut self) -> Result<(Token, usize, usize, usize), LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        // Skip $"
        self.position += 2;
        self.column += 2;

        let mut parts = Vec::new();
        let mut current_text = String::new();
        let mut depth = 0; // Track brace nesting in expressions

        while self.position < self.input.len() {
            let ch = self.char_at(self.position);

            match ch {
                '"' if depth == 0 => {
                    // End of string
                    if !current_text.is_empty() {
                        parts.push(InterpolationPart::Text(current_text.clone()));
                    }
                    self.position += 1;
                    self.column += 1;

                    let span = Span::new(start_pos, self.position, start_line, start_column);
                    let lexeme = &self.input[start_pos..self.position];

                    return Ok((
                        Token::new(TokenType::InterpolatedString(parts), span, lexeme.to_string()),
                        self.position,
                        self.line,
                        self.column,
                    ));
                }

                '{' if depth == 0 => {
                    // Start of interpolation
                    if !current_text.is_empty() {
                        parts.push(InterpolationPart::Text(current_text.clone()));
                        current_text.clear();
                    }

                    // Parse expression
                    let expr = self.parse_interpolation_expr()?;
                    parts.push(InterpolationPart::Expr(expr));
                }

                '{' if depth > 0 => {
                    // Nested brace inside expression
                    depth += 1;
                    current_text.push(ch);
                    self.position += 1;
                    self.column += 1;
                }

                '}' if depth > 0 => {
                    depth -= 1;
                    current_text.push(ch);
                    self.position += 1;
                    self.column += 1;
                }

                '\\' => {
                    // Escape sequence
                    self.position += 1;
                    self.column += 1;

                    if self.position < self.input.len() {
                        let escaped = self.char_at(self.position);
                        current_text.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            '\\' => '\\',
                            '"' => '"',
                            '{' => '{',
                            '}' => '}',
                            _ => {
                                // Invalid escape
                                return Err(LexicalError::InvalidEscape {
                                    sequence: format!("\\{}", escaped),
                                    span: Span::new(
                                        self.position - 1,
                                        self.position + 1,
                                        self.line,
                                        self.column - 1,
                                    ),
                                    valid_escapes: vec![
                                        "\\n".to_string(),
                                        "\\t".to_string(),
                                        "\\r".to_string(),
                                        "\\\\".to_string(),
                                        "\\\"".to_string(),
                                        "\\{".to_string(),
                                        "\\}".to_string(),
                                    ],
                                });
                            }
                        });
                        self.position += 1;
                        self.column += 1;
                    }
                }

                '\n' => {
                    current_text.push(ch);
                    self.position += 1;
                    self.line += 1;
                    self.column = 1;
                }

                _ => {
                    current_text.push(ch);
                    self.position += 1;
                    self.column += 1;
                }
            }
        }

        // Unterminated string
        Err(LexicalError::UnterminatedString {
            span: Span::new(start_pos, self.position, start_line, start_column),
            string_type: StringType::Interpolated,
        })
    }

    /// Parse expression inside { }
    fn parse_interpolation_expr(&mut self) -> Result<String, LexicalError> {
        // Skip {
        self.position += 1;
        self.column += 1;

        let mut expr = String::new();
        let mut depth = 1; // We're inside one {

        while self.position < self.input.len() && depth > 0 {
            let ch = self.char_at(self.position);

            match ch {
                '{' => {
                    depth += 1;
                    expr.push(ch);
                    self.position += 1;
                    self.column += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth > 0 {
                        expr.push(ch);
                    }
                    self.position += 1;
                    self.column += 1;
                }
                '\n' => {
                    expr.push(ch);
                    self.position += 1;
                    self.line += 1;
                    self.column = 1;
                }
                _ => {
                    expr.push(ch);
                    self.position += 1;
                    self.column += 1;
                }
            }
        }

        if depth == 0 {
            Ok(expr.trim().to_string())
        } else {
            Err(LexicalError::InvalidInterpolation {
                message: "Unclosed interpolation expression".to_string(),
                span: Span::new(self.position, self.position, self.line, self.column),
                suggestion: Some("Add closing }".to_string()),
            })
        }
    }

    /// Parse verbatim string: @"C:\path\to\file"
    pub fn parse_verbatim_string(&mut self) -> Result<(Token, usize, usize, usize), LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        // Skip @"
        self.position += 2;
        self.column += 2;

        let mut content = String::new();

        while self.position < self.input.len() {
            let ch = self.char_at(self.position);

            match ch {
                '"' => {
                    // Check for doubled quote ""
                    if self.position + 1 < self.input.len()
                        && self.char_at(self.position + 1) == '"' {
                        // Escaped quote
                        content.push('"');
                        self.position += 2;
                        self.column += 2;
                    } else {
                        // End of string
                        self.position += 1;
                        self.column += 1;

                        let span = Span::new(start_pos, self.position, start_line, start_column);
                        let lexeme = &self.input[start_pos..self.position];

                        return Ok((
                            Token::new(TokenType::VerbatimString(content), span, lexeme.to_string()),
                            self.position,
                            self.line,
                            self.column,
                        ));
                    }
                }

                '\n' => {
                    content.push(ch);
                    self.position += 1;
                    self.line += 1;
                    self.column = 1;
                }

                _ => {
                    content.push(ch);
                    self.position += 1;
                    self.column += 1;
                }
            }
        }

        // Unterminated string
        Err(LexicalError::UnterminatedString {
            span: Span::new(start_pos, self.position, start_line, start_column),
            string_type: StringType::Verbatim,
        })
    }

    /// Parse interpolated verbatim string: $@"C:\path\{file}"
    pub fn parse_interpolated_verbatim_string(&mut self) -> Result<(Token, usize, usize, usize), LexicalError> {
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;

        // Skip $@"
        self.position += 3;
        self.column += 3;

        let mut parts = Vec::new();
        let mut current_text = String::new();

        while self.position < self.input.len() {
            let ch = self.char_at(self.position);

            match ch {
                '"' => {
                    // Check for doubled quote
                    if self.position + 1 < self.input.len()
                        && self.char_at(self.position + 1) == '"' {
                        // Escaped quote
                        current_text.push('"');
                        self.position += 2;
                        self.column += 2;
                    } else {
                        // End of string
                        if !current_text.is_empty() {
                            parts.push(InterpolationPart::Text(current_text));
                        }
                        self.position += 1;
                        self.column += 1;

                        let span = Span::new(start_pos, self.position, start_line, start_column);
                        let lexeme = &self.input[start_pos..self.position];

                        return Ok((
                            Token::new(TokenType::InterpolatedString(parts), span, lexeme.to_string()),
                            self.position,
                            self.line,
                            self.column,
                        ));
                    }
                }

                '{' => {
                    // Start of interpolation
                    if !current_text.is_empty() {
                        parts.push(InterpolationPart::Text(current_text.clone()));
                        current_text.clear();
                    }

                    let expr = self.parse_interpolation_expr()?;
                    parts.push(InterpolationPart::Expr(expr));
                }

                '\n' => {
                    current_text.push(ch);
                    self.position += 1;
                    self.line += 1;
                    self.column = 1;
                }

                _ => {
                    current_text.push(ch);
                    self.position += 1;
                    self.column += 1;
                }
            }
        }

        Err(LexicalError::UnterminatedString {
            span: Span::new(start_pos, self.position, start_line, start_column),
            string_type: StringType::InterpolatedVerbatim,
        })
    }

    #[inline]
    fn char_at(&self, pos: usize) -> char {
        self.input.chars().nth(pos).unwrap_or('\0')
    }
}