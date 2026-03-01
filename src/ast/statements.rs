// src/ast/statements.rs
//! Statement nodes and the `Block` type.
//!
//! Compound statements (If, For, While, …) store their body `Block`
//! behind an `&'ast Block<'ast>` pointer to avoid blowing up the
//! `StmtKind` enum discriminant size.

#![allow(dead_code)]

use crate::ast::common::Span;
use crate::ast::expressions::{Expr, IfExpr, MatchArm};
use crate::ast::patterns::DestructurePattern;
use crate::ast::types::Type;

// ── Block ─────────────────────────────────────────────────────────

/// A brace-enclosed sequence of statements forming a new scope.
///
/// `Block` is `Copy` because it is just a fat pointer + span.
/// The actual `Stmt` values live in the arena slice.
#[derive(Debug, Clone, Copy)]
pub struct Block<'ast> {
    pub stmts: &'ast [Stmt<'ast>],
    pub span: Span,
}

impl<'ast> Block<'ast> {
    pub fn empty(span: Span) -> Self {
        Block { stmts: &[], span }
    }

    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty()
    }
}

// ── Statement ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Stmt<'ast> {
    pub kind: StmtKind<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum StmtKind<'ast> {
    // ── Variable binding ─────────────────────────────────────────
    /// `let [mut] binding [: Type] = expr`
    Let {
        mutable: bool,
        binding: BindingTarget<'ast>,
        ty: Option<&'ast Type<'ast>>,
        value: &'ast Expr<'ast>,
    },

    // ── Expression statement ──────────────────────────────────────
    Expr(&'ast Expr<'ast>),

    // ── Return / fail ─────────────────────────────────────────────
    Return(Option<&'ast Expr<'ast>>),
    /// `fail expr` — raise a typed error (like throw but fits the ! type)
    Fail(&'ast Expr<'ast>),

    // ── Conditionals ──────────────────────────────────────────────
    /// Stored behind a pointer because IfExpr embeds two Blocks
    If(&'ast IfExpr<'ast>),

    // ── Match ──────────────────────────────────────────────────────
    Match {
        scrutinee: &'ast Expr<'ast>,
        arms: &'ast [MatchArm<'ast>],
    },

    // ── Loops ──────────────────────────────────────────────────────
    For {
        binding: BindingTarget<'ast>,
        iter: &'ast Expr<'ast>,
        /// Body is behind a pointer to cap StmtKind's size
        body: &'ast Block<'ast>,
    },
    While {
        condition: &'ast Expr<'ast>,
        body: &'ast Block<'ast>,
    },
    Loop(&'ast Block<'ast>),
    Break(Option<&'ast Expr<'ast>>),
    Continue,

    // ── Memory management ─────────────────────────────────────────
    /// `with arena(1MB) { ... }`  (MID / LOW tier)
    With {
        allocator: AllocatorKind<'ast>,
        body: &'ast Block<'ast>,
    },
    /// `using let x = expr, let y = expr { ... }`  (RAII cleanup)
    Using {
        bindings: &'ast [UsingBinding<'ast>],
        body: &'ast Block<'ast>,
    },

    // ── Destructuring ─────────────────────────────────────────────
    /// `extract (a, b) = tuple`
    Extract {
        pattern: DestructurePattern<'ast>,
        value: &'ast Expr<'ast>,
    },

    // ── Deferred execution ────────────────────────────────────────
    /// `defer expr` — run at end of enclosing scope
    Defer(&'ast Expr<'ast>),

    // ── Error handling ────────────────────────────────────────────
    /// `try { ... } catch (e) { ... }`
    Try {
        body: &'ast Block<'ast>,
        catch_binding: Option<&'ast str>,
        catch_body: Option<&'ast Block<'ast>>,
    },

    // ── Unsafe block ──────────────────────────────────────────────
    Unsafe(&'ast Block<'ast>),
}

// ── Binding targets ───────────────────────────────────────────────

/// The left-hand side of a `let` or `for` binding.
#[derive(Debug, Clone, Copy)]
pub enum BindingTarget<'ast> {
    /// `let x = ...`
    Ident(&'ast str),
    /// `let (a, b) = ...` / `extract { name, age } = ...`
    Destructure(DestructurePattern<'ast>),
}

// ── Allocator kinds ───────────────────────────────────────────────

/// The allocator expression inside a `with` statement.
#[derive(Debug, Clone, Copy)]
pub enum AllocatorKind<'ast> {
    /// `arena(1MB)` or `arena(dynamic_size_expr)`
    Arena(SizeExpr<'ast>),
    /// `pool<Type>(count)`
    Pool {
        ty: &'ast Type<'ast>,
        count: &'ast Expr<'ast>,
    },
    Gc,
    Heap,
}

/// A size expression: either a literal with unit (`1MB`) or a general expression.
#[derive(Debug, Clone, Copy)]
pub enum SizeExpr<'ast> {
    Expr(&'ast Expr<'ast>),
    WithUnit { value: u64, unit: SizeUnit },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeUnit {
    Bytes,
    KB,
    MB,
    GB,
}

impl SizeUnit {
    /// Convert to a byte multiplier.
    pub fn multiplier(self) -> u64 {
        match self {
            SizeUnit::Bytes => 1,
            SizeUnit::KB    => 1_024,
            SizeUnit::MB    => 1_024 * 1_024,
            SizeUnit::GB    => 1_024 * 1_024 * 1_024,
        }
    }
}

// ── Using bindings ────────────────────────────────────────────────

/// One `let [mut] name = expr` inside a `using` statement.
#[derive(Debug, Clone, Copy)]
pub struct UsingBinding<'ast> {
    pub mutable: bool,
    pub name: &'ast str,
    pub value: &'ast Expr<'ast>,
    pub span: Span,
  }
