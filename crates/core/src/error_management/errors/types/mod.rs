// src/error_management/errors/types/mod.rs
//! Errors produced during ordinary type inference and type checking.
//!
//! Tier and arena enforcement errors used to live in this same enum
//! (as the TYPE-2xx range) but were physically split out into
//! `TierError` (see `../tier/mod.rs`) — see docs/DIAGNOSTICS_RULES.md
//! §9. This file now only carries TYPE-1xx.

use crate::lexer::Span;
use std::fmt;

/// Every error that can be raised during ordinary type checking
/// (not tier/arena enforcement — see `TierError` for that).
#[derive(Debug, Clone)]
pub enum TypeError {
    // ── Type mismatch ─────────────────────────────────────────────
    /// The types of two expressions that must agree do not.
    TypeMismatch {
        expected:     String,   // human-readable type name
        found:        String,
        span:         Span,
        /// Where the expected type was established, if known.
        because_of:   Option<Span>,
    },

    /// A function was called with the wrong number of arguments.
    ArgumentCountMismatch {
        expected: usize,
        found:    usize,
        span:     Span,
    },

    /// Tried to access a field that does not exist on a type.
    NoSuchField {
        field:   String,
        on_type: String,
        span:    Span,
    },

    /// Tried to call a method that does not exist on a type.
    NoSuchMethod {
        method:  String,
        on_type: String,
        span:    Span,
    },

    /// `?` operator used on a non-fallible type (not `T!`).
    TryOnNonFallible {
        found: String,
        span:  Span,
    },

    /// `await` used on a non-`Task<T>` type.
    AwaitOnNonTask {
        found: String,
        span:  Span,
    },

    /// A type could not be inferred — too ambiguous.
    CannotInferType {
        span:       Span,
        suggestion: Option<String>,
    },

    /// A generic was instantiated with the wrong number of type arguments.
    GenericArgCountMismatch {
        type_name: String,
        expected:  usize,
        found:     usize,
        span:      Span,
    },
}

impl TypeError {
    pub fn span(&self) -> Span {
        match self {
            TypeError::TypeMismatch               { span, .. } => *span,
            TypeError::ArgumentCountMismatch      { span, .. } => *span,
            TypeError::NoSuchField                { span, .. } => *span,
            TypeError::NoSuchMethod               { span, .. } => *span,
            TypeError::TryOnNonFallible           { span, .. } => *span,
            TypeError::AwaitOnNonTask             { span, .. } => *span,
            TypeError::CannotInferType            { span, .. } => *span,
            TypeError::GenericArgCountMismatch    { span, .. } => *span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            TypeError::TypeMismatch { expected, found, .. } =>
                format!("type mismatch: expected `{}`, found `{}`", expected, found),

            TypeError::ArgumentCountMismatch { expected, found, .. } =>
                format!("expected {} argument(s), found {}", expected, found),

            TypeError::NoSuchField { field, on_type, .. } =>
                format!("type `{}` has no field `{}`", on_type, field),

            TypeError::NoSuchMethod { method, on_type, .. } =>
                format!("type `{}` has no method `{}`", on_type, method),

            TypeError::TryOnNonFallible { found, .. } =>
                format!("`?` requires a fallible type (`T!`), found `{}`", found),

            TypeError::AwaitOnNonTask { found, .. } =>
                format!("`await` requires `Task<T>`, found `{}`", found),

            TypeError::CannotInferType { .. } =>
                "cannot infer type — add an explicit type annotation".to_string(),

            TypeError::GenericArgCountMismatch { type_name, expected, found, .. } =>
                format!(
                    "`{}` expects {} type argument(s), found {}",
                    type_name, expected, found
                ),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            TypeError::CannotInferType { suggestion: Some(s), .. } =>
                Some(s.clone()),

            _ => None,
        }
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for TypeError {}

impl crate::error_management::render::Diagnosable for TypeError {
    // See docs/DIAGNOSTICS_RULES.md, "Error Code Registry" — TYPE-1xx.
    // TYPE-2xx (tier & arena enforcement) is now `TierError`, its own
    // enum in `../tier/mod.rs`, with its own TIER-0xx range.
    fn code(&self) -> &'static str {
        match self {
            TypeError::TypeMismatch { .. }              => "TYPE-101",
            TypeError::ArgumentCountMismatch { .. }     => "TYPE-102",
            TypeError::NoSuchField { .. }                => "TYPE-103",
            TypeError::NoSuchMethod { .. }               => "TYPE-104",
            TypeError::TryOnNonFallible { .. }           => "TYPE-105",
            TypeError::AwaitOnNonTask { .. }             => "TYPE-106",
            TypeError::CannotInferType { .. }            => "TYPE-107",
            TypeError::GenericArgCountMismatch { .. }    => "TYPE-108",
        }
    }
    fn span(&self) -> Span { self.span() }
    fn message(&self) -> String { self.message() }
    fn suggestion(&self) -> Option<String> { self.suggestion() }

    fn secondary_spans(&self) -> Vec<(Span, String)> {
        match self {
            TypeError::TypeMismatch { because_of: Some(span), .. } =>
                vec![(*span, "expected type was established here".to_string())],
            _ => Vec::new(),
        }
    }
}
