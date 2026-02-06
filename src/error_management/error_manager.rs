//! Central error manager - collects all errors

use crate::error_management::error_types::LexicalError;
use crate::error_management::logger::Logger;
use crate::lexer::Span;

#[derive(Debug)]  // ← ADDED THIS - Now ErrorManager implements Debug!
pub struct ErrorManager {
    lexical_errors: Vec<LexicalError>,
    source: String,
    max_errors: usize,
}

impl ErrorManager {
    pub fn new(source: String) -> Self {
        ErrorManager {
            lexical_errors: Vec::new(),
            source,
            max_errors: 100, // Stop after 100 errors
        }
    }

    pub fn add_lexical_error(&mut self, error: LexicalError) {
        if self.lexical_errors.len() < self.max_errors {
            self.lexical_errors.push(error);
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.lexical_errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.lexical_errors.len()
    }

    pub fn report_all(&self) {
        if self.lexical_errors.is_empty() {
            return;
        }

        Logger::error(&format!("\n{} lexical error(s) found:", self.lexical_errors.len()));

        for (idx, error) in self.lexical_errors.iter().enumerate() {
            Logger::formatted_error(error, &error.span(), &self.source);

            if let Some(suggestion) = error.suggestion() {
                eprintln!("   \x1b[33mSuggestion:\x1b[0m {}", suggestion);
            }

            if idx < self.lexical_errors.len() - 1 {
                eprintln!();
            }
        }
    }

    pub fn take_errors(&mut self) -> Vec<LexicalError> {
        std::mem::take(&mut self.lexical_errors)
    }
}
