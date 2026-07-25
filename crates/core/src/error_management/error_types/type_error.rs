// src/error_management/error_types/type_error.rs
//! Errors produced during type inference, type checking, and tier checking.

use crate::lexer::Span;
use crate::ast::common::TierAnnotation;
use std::fmt;

/// Every error that can be raised during type and tier analysis.
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

    // ── Tier violations ───────────────────────────────────────────
    /// `with arena` or arena allocator used outside `@tier(mid)`.
    ArenaInWrongTier {
        actual: TierAnnotation,
        span:   Span,
    },

    /// `await` used outside `@tier(high)`.
    AwaitInWrongTier {
        actual: TierAnnotation,
        span:   Span,
    },

    /// An async function is not `@tier(high)`.
    AsyncFunctionNotHigh {
        actual: TierAnnotation,
        span:   Span,
    },

    /// A value that carries an arena lifetime crossed a boundary where
    /// it would outlive its arena: assigned to a binding whose home
    /// arena doesn't match (including a binding declared outside any
    /// arena at all), stored into a struct field or indexed slot on a
    /// receiver from a different-or-no arena, captured by a closure that
    /// then escapes the block, or returned across a tier boundary whose
    /// declared type can't express the tag. See MEMORY_MODEL.md §6.
    ArenaRefEscapesBoundary {
        /// The type that contains the arena reference.
        escaped_type: String,
        span:         Span,
    },

    /// `@tier(high)` code tried to call `@tier(low)` code directly.
    /// (HIGH may only call MID; MID may call HIGH or LOW.)
    IllegalTierCall {
        caller_tier: TierAnnotation,
        callee_tier: TierAnnotation,
        callee_name: String,
        span:        Span,
    },

    /// A MID-tier function's return type contains an arena-lifetime type.
    /// This would make it impossible for HIGH-tier to call it safely.
    MidReturnContainsArenaRef {
        return_type: String,
        span:        Span,
    },

    // ── LINQ tier violation ───────────────────────────────────────
    /// LINQ query expressions are only valid in `@tier(high)`.
    LinqInWrongTier {
        actual: TierAnnotation,
        span:   Span,
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
            TypeError::ArenaInWrongTier           { span, .. } => *span,
            TypeError::AwaitInWrongTier           { span, .. } => *span,
            TypeError::AsyncFunctionNotHigh       { span, .. } => *span,
            TypeError::ArenaRefEscapesBoundary    { span, .. } => *span,
            TypeError::IllegalTierCall            { span, .. } => *span,
            TypeError::MidReturnContainsArenaRef  { span, .. } => *span,
            TypeError::LinqInWrongTier            { span, .. } => *span,
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

            TypeError::ArenaInWrongTier { actual, .. } =>
                format!(
                    "`with arena` is only valid in `@tier(mid)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TypeError::AwaitInWrongTier { actual, .. } =>
                format!(
                    "`await` is only valid in `@tier(high)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TypeError::AsyncFunctionNotHigh { actual, .. } =>
                format!(
                    "async functions must be `@tier(high)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TypeError::ArenaRefEscapesBoundary { escaped_type, .. } =>
                format!(
                    "value of type `{}` is scoped to a `with arena(...)` block and cannot \
                     outlive it — check the binding, struct field, or closure it's flowing into",
                    escaped_type
                ),

            TypeError::IllegalTierCall { caller_tier, callee_tier, callee_name, .. } =>
                format!(
                    "`@tier({})` code cannot call `@tier({})` function `{}`",
                    tier_name(*caller_tier), tier_name(*callee_tier), callee_name
                ),

            TypeError::MidReturnContainsArenaRef { return_type, .. } =>
                format!(
                    "return type `{}` contains an arena-lifetime reference; \
                     this makes the function uncallable from `@tier(high)`",
                    return_type
                ),

            TypeError::LinqInWrongTier { actual, .. } =>
                format!(
                    "LINQ query expressions are only valid in `@tier(high)`; \
                     this function is `@tier({})`",
                    tier_name(*actual)
                ),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            TypeError::CannotInferType { suggestion: Some(s), .. } =>
                Some(s.clone()),

            TypeError::ArenaInWrongTier { .. } =>
                Some("annotate this function with `@tier(mid)` or remove the arena block".to_string()),

            TypeError::AwaitInWrongTier { .. } =>
                Some("annotate this function with `@tier(high)` or remove the `await`".to_string()),

            TypeError::AsyncFunctionNotHigh { .. } =>
                Some("add `@tier(high)` annotation, or remove `async`".to_string()),

            TypeError::ArenaRefEscapesBoundary { .. } =>
                Some("copy the data out into a plain (non-arena) value before it leaves the \
                      `with arena(...)` block, or restructure with the callback pattern: pass a \
                      closure in and return a GC-owned value from inside it, instead of returning \
                      or storing the arena-scoped value directly".to_string()),

            TypeError::IllegalTierCall { caller_tier: TierAnnotation::High, .. } =>
                Some("HIGH tier may only call MID-tier functions directly — wrap the LOW-tier logic in a MID-tier function".to_string()),

            TypeError::LinqInWrongTier { .. } =>
                Some("move the LINQ query into a `@tier(high)` function, or use `.where()` / `.map()` method chains instead (which work in all tiers)".to_string()),

            _ => None,
        }
    }
}

fn tier_name(tier: TierAnnotation) -> &'static str {
    match tier {
        TierAnnotation::High => "high",
        TierAnnotation::Mid  => "mid",
        TierAnnotation::Low  => "low",
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for TypeError {}

impl crate::error_management::render::Diagnosable for TypeError {
    // See docs/DIAGNOSTICS_RULES.md, "Error Code Registry". Ordinary
    // type-checking variants get TYPE-1xx; tier/arena enforcement gets
    // TYPE-2xx, a deliberately separate number range (not just a
    // different starting point within one flat list) so that if
    // TypeError is ever physically split into two enums — it already
    // reads as two families, per the `// ── Type mismatch ──` /
    // `// ── Tier violations ──` banners above — the TYPE-2xx group can
    // become TIER-xxx with the numeric tail unchanged, and nothing
    // anyone has grepped for goes stale.
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
            TypeError::ArenaInWrongTier { .. }           => "TYPE-201",
            TypeError::AwaitInWrongTier { .. }           => "TYPE-202",
            TypeError::AsyncFunctionNotHigh { .. }       => "TYPE-203",
            TypeError::ArenaRefEscapesBoundary { .. }    => "TYPE-204",
            TypeError::IllegalTierCall { .. }            => "TYPE-205",
            TypeError::MidReturnContainsArenaRef { .. }  => "TYPE-206",
            TypeError::LinqInWrongTier { .. }            => "TYPE-207",
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
