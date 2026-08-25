// src/error_management/errors/borrow/mod.rs
//! Errors produced by Phase D of the LOW-tier borrow checker (see
//! `crates/core/src/sema/borrow_check.rs`) — the liveness-gated
//! combination of `facts.rs`'s loan/kill/invalidation-candidate facts
//! into real diagnostics. New family, not a split-out of an existing
//! one; see `borrow_check.rs`'s own module doc for exactly what is and
//! isn't checked in this first version.

use crate::lexer::Span;

/// Every error the LOW-tier borrow checker can raise.
#[derive(Debug, Clone)]
pub enum BorrowError {
    /// A place was read (or re-borrowed) while a `&mut` loan on it was
    /// still live — the loan hasn't been killed on the path that reached
    /// here, *and* its bound reference is still going to be read later.
    /// See `borrow_check.rs`'s module doc for exactly what "live" means
    /// (liveness-gated, not lexical-scope) and its scope-limits note for
    /// what this first version does and doesn't check.
    ConflictingAccessWhileBorrowed {
        /// The place the outstanding loan borrows (e.g. `n` in `&mut n`).
        place: String,
        /// Where the conflicting outstanding loan is issued.
        loan_span: Span,
        /// Where the conflicting access happens.
        conflict_span: Span,
    },
}

impl BorrowError {
    pub fn span(&self) -> Span {
        match self {
            BorrowError::ConflictingAccessWhileBorrowed { conflict_span, .. } => *conflict_span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            BorrowError::ConflictingAccessWhileBorrowed { place, .. } =>
                format!(
                    "cannot use `{}` while it is mutably borrowed",
                    place
                ),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            BorrowError::ConflictingAccessWhileBorrowed { .. } =>
                Some("move this use before the borrow's last use, or restructure so the borrow \
                      doesn't need to outlive it".to_string()),
        }
    }
}

impl std::fmt::Display for BorrowError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for BorrowError {}

impl crate::error_management::render::Diagnosable for BorrowError {
    // See docs/DIAGNOSTICS_RULES.md, "Error Code Registry" — BORROW-0xx.
    fn code(&self) -> &'static str {
        match self {
            BorrowError::ConflictingAccessWhileBorrowed { .. } => "BORROW-001",
        }
    }
    fn span(&self) -> Span { self.span() }
    fn message(&self) -> String { self.message() }
    fn suggestion(&self) -> Option<String> { self.suggestion() }

    fn secondary_spans(&self) -> Vec<(Span, String)> {
        match self {
            BorrowError::ConflictingAccessWhileBorrowed { loan_span, .. } =>
                vec![(*loan_span, "mutable borrow occurs here".to_string())],
        }
    }
}
