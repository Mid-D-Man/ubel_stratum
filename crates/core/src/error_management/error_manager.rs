// src/error_management/error_manager.rs

use crate::error_management::error_types::{
    LexicalError, ParseError, NameError, TypeError,
};
use crate::error_management::logger::Logger;
use crate::error_management::Diagnosable;

/// Central error accumulator for the entire compiler pipeline.
///
/// Each phase appends its errors here.  The manager never stops the
/// pipeline immediately — callers check `has_errors()` at phase
/// boundaries and decide whether to continue.
///
/// Phase order:
///   1. Lex    → `add_lexical_error`
///   2. Parse  → `add_parse_error`
///   3. Resolve → `add_name_error`
///   4. TypeCheck + TierCheck → `add_type_error`
#[derive(Debug)]
pub struct ErrorManager {
    lexical_errors: Vec<LexicalError>,
    parse_errors:   Vec<ParseError>,
    name_errors:    Vec<NameError>,
    type_errors:    Vec<TypeError>,
    source:         String,
    max_errors:     usize,
}

impl ErrorManager {
    pub fn new(source: String) -> Self {
        ErrorManager {
            lexical_errors: Vec::new(),
            parse_errors:   Vec::new(),
            name_errors:    Vec::new(),
            type_errors:    Vec::new(),
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

    pub fn lexical_error_count(&self) -> usize { self.lexical_errors.len() }

    pub fn take_lexical_errors(&mut self) -> Vec<LexicalError> {
        std::mem::take(&mut self.lexical_errors)
    }

    // ── Parse ────────────────────────────────────────────────────

    pub fn add_parse_error(&mut self, error: ParseError) {
        if self.total_errors() < self.max_errors {
            self.parse_errors.push(error);
        }
    }

    pub fn parse_error_count(&self) -> usize { self.parse_errors.len() }

    pub fn take_parse_errors(&mut self) -> Vec<ParseError> {
        std::mem::take(&mut self.parse_errors)
    }

    // ── Name resolution ──────────────────────────────────────────

    pub fn add_name_error(&mut self, error: NameError) {
        if self.total_errors() < self.max_errors {
            self.name_errors.push(error);
        }
    }

    pub fn name_error_count(&self) -> usize { self.name_errors.len() }

    pub fn take_name_errors(&mut self) -> Vec<NameError> {
        std::mem::take(&mut self.name_errors)
    }

    // ── Type / tier checking ──────────────────────────────────────

    pub fn add_type_error(&mut self, error: TypeError) {
        if self.total_errors() < self.max_errors {
            self.type_errors.push(error);
        }
    }

    pub fn type_error_count(&self) -> usize { self.type_errors.len() }

    pub fn take_type_errors(&mut self) -> Vec<TypeError> {
        std::mem::take(&mut self.type_errors)
    }

    // ── Combined ─────────────────────────────────────────────────

    pub fn has_errors(&self) -> bool {
        !self.lexical_errors.is_empty()
            || !self.parse_errors.is_empty()
            || !self.name_errors.is_empty()
            || !self.type_errors.is_empty()
    }

    pub fn total_errors(&self) -> usize {
        self.lexical_errors.len()
            + self.parse_errors.len()
            + self.name_errors.len()
            + self.type_errors.len()
    }

    /// Backwards-compatible alias used by older call sites.
    pub fn error_count(&self) -> usize { self.total_errors() }

    /// Print every accumulated error grouped by phase.
    pub fn report_all(&self) {
        self.report_section("lexical", &self.lexical_errors);
        self.report_section("parse", &self.parse_errors);
        self.report_section("name resolution", &self.name_errors);
        self.report_section("type", &self.type_errors);
    }

    fn report_section<E: Diagnosable>(
        &self,
        phase: &str,
        errors: &[E],
    ) {
        if errors.is_empty() { return; }

        Logger::error(&format!("\n{} {} error(s):", errors.len(), phase));

        for (i, error) in errors.iter().enumerate() {
            let diag = error.to_diagnostic();
            eprint!("{}", crate::error_management::render(&diag, &self.source));
            if i < errors.len() - 1 { eprintln!(); }
        }
    }

    // Legacy helpers used by existing call sites in parser.
    pub fn take_errors(&mut self) -> Vec<LexicalError> {
        std::mem::take(&mut self.lexical_errors)
    }
}
