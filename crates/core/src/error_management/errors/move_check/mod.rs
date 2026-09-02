// src/error_management/errors/move_check/mod.rs
//! Errors produced by the move/ownership fixed point (see
//! `crates/core/src/sema/move_check.rs`) — the reachability combination
//! of `move_facts.rs`'s move-candidate facts into real diagnostics.
//! New family, not a split-out of an existing one, same call `borrow`
//! made for the loan half (see that family's own module doc) — moving
//! and aliasing are genuinely different questions about a value even
//! though both live under LOW-tier's umbrella.

use crate::lexer::Span;

/// Every error the LOW-tier move checker can raise.
#[derive(Debug, Clone)]
pub enum MoveError {
    /// A `Unique<T>`-typed local was used again after an earlier bare
    /// (non-`&`/`&mut`) use had already consumed it, on some path that
    /// reaches this one with no redefinition in between. May-analysis,
    /// same philosophy `borrow_check.rs`'s own `ConflictingAccessWhileBorrowed`
    /// uses for loans: a value that *might* already be moved on some
    /// path is rejected unconditionally, matching real move semantics —
    /// not just when it's moved on literally every path. See
    /// `move_check.rs`'s module doc for exactly how "reaches" is
    /// computed, including the loop-back-edge case where a value moved
    /// on one iteration is used again on the next.
    UseAfterMove {
        /// The local that was moved (e.g. `a` in `let b = a`).
        place: String,
        /// Where the earlier, consuming use happened.
        moved_span: Span,
        /// Where the later, invalid use happens.
        used_span: Span,
    },
}

impl MoveError {
    pub fn span(&self) -> Span {
        match self {
            MoveError::UseAfterMove { used_span, .. } => *used_span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            MoveError::UseAfterMove { place, .. } =>
                format!("use of `{}` after it was already moved", place),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            MoveError::UseAfterMove { place, .. } =>
                Some(format!(
                    "`{}` is `Unique<T>` -- move-only, not `Copy`. Borrow it instead \
                     (`&{}`/`&mut {}`) if the earlier use didn't need to consume it, \
                     or reassign a fresh value to `{}` before this point",
                    place, place, place, place
                )),
        }
    }
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for MoveError {}

impl crate::error_management::render::Diagnosable for MoveError {
    // See docs/DIAGNOSTICS_RULES.md, "Error Code Registry" — MOVE-0xx.
    fn code(&self) -> &'static str {
        match self {
            MoveError::UseAfterMove { .. } => "MOVE-001",
        }
    }
    fn span(&self) -> Span { self.span() }
    fn message(&self) -> String { self.message() }
    fn suggestion(&self) -> Option<String> { self.suggestion() }

    fn secondary_spans(&self) -> Vec<(Span, String)> {
        match self {
            MoveError::UseAfterMove { moved_span, .. } =>
                vec![(*moved_span, "value moved here".to_string())],
        }
    }
}
