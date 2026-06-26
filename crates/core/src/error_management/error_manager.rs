// src/error_management/error_manager.rs

use crate::error_management::error_types::{
    LexicalError, ParseError, NameError, TypeError,
};
use crate::error_management::logger::Logger;

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
        self.report_section(
            "lexical",
            self.lexical_errors.len(),
            self.lexical_errors.iter().map(|e| ReportEntry {
                span:       e.span(),
                message:    e.message(),
                suggestion: e.suggestion(),
            }),
        );

        self.report_section(
            "parse",
            self.parse_errors.len(),
            self.parse_errors.iter().map(|e| ReportEntry {
                span:       e.span(),
                message:    e.message(),
                suggestion: e.suggestion(),
            }),
        );

        self.report_section(
            "name resolution",
            self.name_errors.len(),
            self.name_errors.iter().map(|e| ReportEntry {
                span:       e.span(),
                message:    e.message(),
                suggestion: e.suggestion(),
            }),
        );

        self.report_section(
            "type",
            self.type_errors.len(),
            self.type_errors.iter().map(|e| ReportEntry {
                span:       e.span(),
                message:    e.message(),
                suggestion: e.suggestion(),
            }),
        );
    }

    fn report_section<'a>(
        &self,
        phase: &str,
        count: usize,
        entries: impl Iterator<Item = ReportEntry>,
    ) {
        if count == 0 { return; }

        Logger::error(&format!("\n{} {} error(s):", count, phase));

        let lines: Vec<&str> = self.source.lines().collect();
        let entries: Vec<_> = entries.collect();

        for (i, entry) in entries.iter().enumerate() {
            let span = &entry.span;
            let line_text = if span.line > 0 && span.line <= lines.len() {
                lines[span.line - 1]
            } else {
                ""
            };

            eprintln!("\x1b[31merror:\x1b[0m {}", entry.message);
            eprintln!("  \x1b[36m--> {}:{}\x1b[0m", span.line, span.column);
            eprintln!("   |");
            eprintln!("{:3} | {}", span.line, line_text);
            eprintln!(
                "   | {}{}\n",
                " ".repeat(span.column.saturating_sub(1)),
                "\x1b[31m^\x1b[0m"
            );

            if let Some(suggestion) = &entry.suggestion {
                eprintln!("   \x1b[33m= help:\x1b[0m {}", suggestion);
            }

            if i < entries.len() - 1 { eprintln!(); }
        }
    }

    // Legacy helpers used by existing call sites in parser.
    pub fn take_errors(&mut self) -> Vec<LexicalError> {
        std::mem::take(&mut self.lexical_errors)
    }
}

// ── Internal report helper ─────────────────────────────────────────

struct ReportEntry {
    span:       crate::lexer::Span,
    message:    String,
    suggestion: Option<String>,
}
