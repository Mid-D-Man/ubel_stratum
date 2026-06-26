// src/ast/patterns.rs
//! Pattern nodes used in `match` arms, `let` bindings, and `extract` statements.

#![allow(dead_code)]

use crate::ast::common::Span;
use crate::ast::literals::Literal;

/// A pattern with its source location.
#[derive(Debug, Clone, Copy)]
pub struct Pattern<'ast> {
    pub kind: PatternKind<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum PatternKind<'ast> {
    /// `_` — discard
    Wildcard,

    /// A literal value: `42`, `"hello"`, `true`
    Literal(Literal<'ast>),

    /// A name binding: `x` or `mut x`
    Ident {
        name: &'ast str,
        mutable: bool,
    },

    /// Tuple destructure: `(a, b, c)`
    Tuple(&'ast [Pattern<'ast>]),

    /// Array destructure: `[first, second, ...rest]`
    Array {
        elements: &'ast [Pattern<'ast>],
        /// `None` = no rest clause
        /// `Some(None)` = `...` (discard rest)
        /// `Some(Some("x"))` = `...x` (bind rest)
        rest: Option<Option<&'ast str>>,
    },

    /// Struct / object destructure: `Point { x, y }` or `{ name, age }`
    Struct {
        /// `None` for anonymous-object patterns
        name: Option<&'ast str>,
        fields: &'ast [FieldPattern<'ast>],
    },

    /// Enum variant: `Ok(x)`, `Status.Active`, `Err { code, message }`
    Enum {
        path: &'ast [&'ast str],
        payload: EnumPatternPayload<'ast>,
    },

    /// Range: `0..100` (exclusive) or `0..=99` (inclusive)
    Range {
        lo: Literal<'ast>,
        hi: Literal<'ast>,
        inclusive: bool,
    },

    /// OR pattern: `A | B`
    Or(&'ast [Pattern<'ast>]),

    /// `extract { field, ... }` inside a match arm
    Extract(&'ast [FieldPattern<'ast>]),
}

/// A single field binding inside a struct pattern.
#[derive(Debug, Clone, Copy)]
pub struct FieldPattern<'ast> {
    pub field: &'ast str,
    /// `None` = shorthand `{ name }` (bind field name directly)
    /// `Some(pat)` = `{ name = pat }`
    pub pattern: Option<Pattern<'ast>>,
    pub span: Span,
}

/// The payload shape of an enum variant pattern.
#[derive(Debug, Clone, Copy)]
pub enum EnumPatternPayload<'ast> {
    /// No payload: `Status.Active`
    None,
    /// Positional payload: `Ok(value)`
    Tuple(&'ast [Pattern<'ast>]),
    /// Named payload: `Err { code, message }`
    Struct(&'ast [FieldPattern<'ast>]),
}

// ── Destructuring patterns (used in `extract` and let-bindings) ───

/// The left-hand side of an `extract` statement or destructuring `let`.
#[derive(Debug, Clone, Copy)]
pub enum DestructurePattern<'ast> {
    /// Just a name: `let x = ...`
    Ident(&'ast str),
    Tuple(TupleDestructure<'ast>),
    Array(ArrayDestructure<'ast>),
    Struct(StructDestructure<'ast>),
}

#[derive(Debug, Clone, Copy)]
pub struct TupleDestructure<'ast> {
    pub elements: &'ast [DestructureElement<'ast>],
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct ArrayDestructure<'ast> {
    pub elements: &'ast [DestructureElement<'ast>],
    /// `None` = no rest, `Some(None)` = `...`, `Some(Some("x"))` = `...x`
    pub rest: Option<Option<&'ast str>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct StructDestructure<'ast> {
    pub fields: &'ast [FieldDestructure<'ast>],
    pub span: Span,
}

/// One element inside a tuple or array destructure.
#[derive(Debug, Clone, Copy)]
pub enum DestructureElement<'ast> {
    /// Bind to a name
    Ident(&'ast str),
    /// Discard with `_`
    Wildcard,
    /// Nested destructure
    Nested(DestructurePattern<'ast>),
}

/// One field inside a struct destructure: `name` or `name = pattern`.
#[derive(Debug, Clone, Copy)]
pub struct FieldDestructure<'ast> {
    pub field: &'ast str,
    /// `None` = shorthand (bind field name directly)
    pub pattern: Option<DestructurePattern<'ast>>,
    pub span: Span,
}
