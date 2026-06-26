// src/ast/common.rs
//! Shared primitives reused across every other AST module.
//!
//! Nothing in here depends on expressions, statements, or declarations —
//! it is the foundation layer.

#![allow(dead_code)]

// Re-export the lexer Span so the rest of the AST only needs one import.
pub use crate::lexer::Span;

// ── Identifiers ───────────────────────────────────────────────────

/// An identifier together with its source location.
#[derive(Debug, Clone, Copy)]
pub struct Ident<'ast> {
    pub name: &'ast str,
    pub span: Span,
}

/// A dotted path such as `std.collections.List`.
/// Segments are the individual identifiers split on `.`.
#[derive(Debug, Clone, Copy)]
pub struct QualifiedIdent<'ast> {
    pub segments: &'ast [&'ast str],
    pub span: Span,
}

impl<'ast> QualifiedIdent<'ast> {
    /// Returns `true` if the path has exactly one segment (a bare name).
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1
    }

    /// The final segment — e.g. `List` in `std.collections.List`.
    pub fn last(&self) -> Option<&'ast str> {
        self.segments.last().copied()
    }
}

// ── Visibility ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

// ── Tier annotation ───────────────────────────────────────────────

/// Which memory tier a function, method, or block runs in.
///
/// The **default** when no `@tier(...)` annotation is present is `High`.
/// Developers opt *down* into `Mid` (arena) or `Low` (manual ownership)
/// when they need the performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TierAnnotation {
    /// Garbage collected — full async support, LINQ syntax available.
    /// This is the default.
    #[default]
    High,
    /// Arena allocated — fast, zero-copy via callback/iterator/view patterns.
    Mid,
    /// Manual ownership — borrow-checker enforced, zero runtime overhead.
    Low,
}

// ── Attributes ────────────────────────────────────────────────────

/// A single `@name(args)` annotation.
#[derive(Debug, Clone, Copy)]
pub struct Attribute<'ast> {
    pub name: &'ast str,
    pub args: &'ast [AttrArg<'ast>],
    pub span: Span,
}

/// Possible argument forms inside an attribute.
#[derive(Debug, Clone, Copy)]
pub enum AttrArg<'ast> {
    /// Bare identifier: `@deprecated`
    Ident(&'ast str),
    /// String value: `@doc("hello")`
    Str(&'ast str),
    /// Integer value: `@version(2)`
    Int(i64),
    /// Boolean: `@inline(true)`
    Bool(bool),
    /// Key-value pair: `@cfg(target = "wasm")`
    Named {
        key: &'ast str,
        value: AttrValue<'ast>,
    },
}

/// The right-hand side of a named attribute argument.
#[derive(Debug, Clone, Copy)]
pub enum AttrValue<'ast> {
    Str(&'ast str),
    Int(i64),
    Bool(bool),
    Ident(&'ast str),
}

// ── Lifetime parameters ───────────────────────────────────────────

/// `[lifetime L]` or `[lifetime L where L outlives M]`
#[derive(Debug, Clone, Copy)]
pub struct LifetimeParam<'ast> {
    pub name: &'ast str,
    pub constraint: Option<LifetimeConstraint<'ast>>,
    pub span: Span,
}

/// The `L outlives M` part of a lifetime constraint.
#[derive(Debug, Clone, Copy)]
pub struct LifetimeConstraint<'ast> {
    /// The lifetime that must be at least as long as `shorter`.
    pub longer: &'ast str,
    pub shorter: &'ast str,
    pub span: Span,
}

// ── Generic parameters ────────────────────────────────────────────

/// A single generic type parameter: `T` or `T: Trait + OtherTrait`.
#[derive(Debug, Clone, Copy)]
pub struct GenericParam<'ast> {
    pub name: &'ast str,
    /// Trait bound names (empty = unconstrained).
    pub bounds: &'ast [&'ast str],
    pub span: Span,
}

// ── Binary operators ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,    // +
    Sub,    // -
    Mul,    // *
    Div,    // /
    Rem,    // %
    // Bitwise
    BitAnd, // &
    BitOr,  // |
    BitXor, // ^
    Shl,    // 
    Shr,    // >>
    // Comparison
    Eq,     // ==
    Ne,     // !=
    Lt,     // 
    Le,     // <=
    Gt,     // >
    Ge,     // >=
    // Logical
    And,    // and / &&
    Or,     // or  / ||
    // Range
    Range,      // ..
    RangeIncl,  // ..=
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,    // -
    BitNot, // ~
    Not,    // not / !
    Await,  // await (handled as prefix in the grammar)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,         // =
    AddAssign,      // +=
    SubAssign,      // -=
    MulAssign,      // *=
    DivAssign,      // /=
    RemAssign,      // %=
    BitAndAssign,   // &=
    BitOrAssign,    // |=
    BitXorAssign,   // ^=
    ShlAssign,      // <<=
    ShrAssign,      // >>=
}
