// src/error_management/error_manager.rs  — add parse error support

use crate::error_management::error_types::{LexicalError, ParseError};
use crate::error_management::logger::Logger;

#[derive(Debug)]
pub struct ErrorManager {
    lexical_errors: Vec<LexicalError>,
    parse_errors:   Vec<ParseError>,   // ← new
    source:         String,
    max_errors:     usize,
}

impl ErrorManager {
    pub fn new(source: String) -> Self {
        ErrorManager {
            lexical_errors: Vec::new(),
            parse_errors:   Vec::new(),
            source,
            max_errors: 100,
        }
    }

    // ── Lexical ──────────────────────────────────────────────────

    pub fn add_lexical_error(&mut self, error: LexicalError) {
        if self.total_errors() < self.max_errors {
            self.lexical_errors.push(error);
        }
    }

    // ── Parse ────────────────────────────────────────────────────

    pub fn add_parse_error(&mut self, error: ParseError) {
        if self.total_errors() < self.max_errors {
            self.parse_errors.push(error);
        }
    }

    pub fn parse_error_count(&self) -> usize {
        self.parse_errors.len()
    }

    pub fn take_parse_errors(&mut self) -> Vec<ParseError> {
        std::mem::take(&mut self.parse_errors)
    }

    // ── Combined ─────────────────────────────────────────────────

    pub fn has_errors(&self) -> bool {
        !self.lexical_errors.is_empty() || !self.parse_errors.is_empty()
    }

    pub fn total_errors(&self) -> usize {
        self.lexical_errors.len() + self.parse_errors.len()
    }

    // keep old name as alias so existing call sites don't break
    pub fn error_count(&self) -> usize {
        self.total_errors()
    }

    pub fn report_all(&self) {
        if !self.lexical_errors.is_empty() {
            Logger::error(&format!(
                "\n{} lexical error(s):", self.lexical_errors.len()
            ));
            for (i, error) in self.lexical_errors.iter().enumerate() {
                Logger::formatted_error(error, &error.span(), &self.source);
                if let Some(s) = error.suggestion() {
                    eprintln!("   \x1b[33mSuggestion:\x1b[0m {}", s);
                }
                if i < self.lexical_errors.len() - 1 { eprintln!(); }
            }
        }

        if !self.parse_errors.is_empty() {
            Logger::error(&format!(
                "\n{} parse error(s):", self.parse_errors.len()
            ));
            for (i, error) in self.parse_errors.iter().enumerate() {
                Logger::formatted_error(error, &error.span(), &self.source);
                if let Some(s) = error.suggestion() {
                    eprintln!("   \x1b[33mSuggestion:\x1b[0m {}", s);
                }
                if i < self.parse_errors.len() - 1 { eprintln!(); }
            }
        }
    }

    pub fn take_errors(&mut self) -> Vec<LexicalError> {
        std::mem::take(&mut self.lexical_errors)
    }
                                                    }
