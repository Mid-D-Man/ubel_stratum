// src/error_management/errors/tier/mod.rs
//! Errors produced during tier and arena-boundary enforcement.
//!
//! Physically split out of `TypeError` (see `../types/mod.rs`) per
//! docs/DIAGNOSTICS_RULES.md §9. Variant names and their meaning are
//! unchanged from when they lived as `TypeError`'s TYPE-2xx range —
//! only the enum they belong to and their code prefix changed
//! (`TYPE-201..207` → `TIER-001..007`, same relative order, tail
//! renumbered from 1 per §9's own "keep the tail stable" promise not
//! applying here since the whole range moved to a new prefix).

use crate::lexer::Span;
use crate::ast::common::TierAnnotation;
use std::fmt;

/// Every error that can be raised while enforcing tier rules
/// (`@tier(high|mid|low)`) and arena-lifetime boundaries.
#[derive(Debug, Clone)]
pub enum TierError {
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

impl TierError {
    pub fn span(&self) -> Span {
        match self {
            TierError::ArenaInWrongTier           { span, .. } => *span,
            TierError::AwaitInWrongTier           { span, .. } => *span,
            TierError::AsyncFunctionNotHigh       { span, .. } => *span,
            TierError::ArenaRefEscapesBoundary    { span, .. } => *span,
            TierError::IllegalTierCall            { span, .. } => *span,
            TierError::MidReturnContainsArenaRef  { span, .. } => *span,
            TierError::LinqInWrongTier            { span, .. } => *span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            TierError::ArenaInWrongTier { actual, .. } =>
                format!(
                    "`with arena` is only valid in `@tier(mid)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TierError::AwaitInWrongTier { actual, .. } =>
                format!(
                    "`await` is only valid in `@tier(high)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TierError::AsyncFunctionNotHigh { actual, .. } =>
                format!(
                    "async functions must be `@tier(high)`; this function is `@tier({})`",
                    tier_name(*actual)
                ),

            TierError::ArenaRefEscapesBoundary { escaped_type, .. } =>
                format!(
                    "value of type `{}` is scoped to a `with arena(...)` block and cannot \
                     outlive it — check the binding, struct field, or closure it's flowing into",
                    escaped_type
                ),

            TierError::IllegalTierCall { caller_tier, callee_tier, callee_name, .. } =>
                format!(
                    "`@tier({})` code cannot call `@tier({})` function `{}`",
                    tier_name(*caller_tier), tier_name(*callee_tier), callee_name
                ),

            TierError::MidReturnContainsArenaRef { return_type, .. } =>
                format!(
                    "return type `{}` contains an arena-lifetime reference; \
                     this makes the function uncallable from `@tier(high)`",
                    return_type
                ),

            TierError::LinqInWrongTier { actual, .. } =>
                format!(
                    "LINQ query expressions are only valid in `@tier(high)`; \
                     this function is `@tier({})`",
                    tier_name(*actual)
                ),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            TierError::ArenaInWrongTier { .. } =>
                Some("annotate this function with `@tier(mid)` or remove the arena block".to_string()),

            TierError::AwaitInWrongTier { .. } =>
                Some("annotate this function with `@tier(high)` or remove the `await`".to_string()),

            TierError::AsyncFunctionNotHigh { .. } =>
                Some("add `@tier(high)` annotation, or remove `async`".to_string()),

            TierError::ArenaRefEscapesBoundary { .. } =>
                Some("copy the data out into a plain (non-arena) value before it leaves the \
                      `with arena(...)` block, or restructure with the callback pattern: pass a \
                      closure in and return a GC-owned value from inside it, instead of returning \
                      or storing the arena-scoped value directly".to_string()),

            TierError::IllegalTierCall { caller_tier: TierAnnotation::High, .. } =>
                Some("HIGH tier may only call MID-tier functions directly — wrap the LOW-tier logic in a MID-tier function".to_string()),

            TierError::LinqInWrongTier { .. } =>
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

impl fmt::Display for TierError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for TierError {}

impl crate::error_management::render::Diagnosable for TierError {
    // See docs/DIAGNOSTICS_RULES.md, "Error Code Registry" — TIER-0xx.
    // None of these variants carry a secondary span, so `secondary_spans`
    // is left at the trait's default (empty) rather than overridden here.
    fn code(&self) -> &'static str {
        match self {
            TierError::ArenaInWrongTier { .. }           => "TIER-001",
            TierError::AwaitInWrongTier { .. }           => "TIER-002",
            TierError::AsyncFunctionNotHigh { .. }       => "TIER-003",
            TierError::ArenaRefEscapesBoundary { .. }    => "TIER-004",
            TierError::IllegalTierCall { .. }            => "TIER-005",
            TierError::MidReturnContainsArenaRef { .. }  => "TIER-006",
            TierError::LinqInWrongTier { .. }            => "TIER-007",
        }
    }
    fn span(&self) -> Span { self.span() }
    fn message(&self) -> String { self.message() }
    fn suggestion(&self) -> Option<String> { self.suggestion() }
}
