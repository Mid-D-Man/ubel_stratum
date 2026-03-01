// src/parser/helpers/stmt.rs
//! Builders for statement nodes.

use crate::ast::arena::AstArena;
use crate::ast::common::Span;
use crate::ast::expressions::{Expr, IfExpr, MatchArm};
use crate::ast::patterns::DestructurePattern;
use crate::ast::statements::*;
use crate::ast::types::Type;

// ── Core statements ───────────────────────────────────────────────

pub fn let_stmt<'ast>(
    mutable: bool,
    binding: BindingTarget<'ast>,
    ty:      Option<&'ast Type<'ast>>,
    value:   &'ast Expr<'ast>,
    span:    Span,
) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Let { mutable, binding, ty, value }, span }
}

pub fn return_stmt<'ast>(value: Option<&'ast Expr<'ast>>, span: Span) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Return(value), span }
}

pub fn fail_stmt<'ast>(expr: &'ast Expr<'ast>, span: Span) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Fail(expr), span }
}

pub fn expr_stmt<'ast>(expr: &'ast Expr<'ast>, span: Span) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Expr(expr), span }
}

pub fn break_stmt<'ast>(value: Option<&'ast Expr<'ast>>, span: Span) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Break(value), span }
}

pub fn continue_stmt(span: Span) -> Stmt<'static> {
    Stmt { kind: StmtKind::Continue, span }
}

pub fn defer_stmt<'ast>(expr: &'ast Expr<'ast>, span: Span) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Defer(expr), span }
}

// ── Control flow ──────────────────────────────────────────────────

pub fn if_stmt<'ast>(
    arena:   &'ast AstArena,
    if_node: IfExpr<'ast>,
    span:    Span,
) -> Stmt<'ast> {
    let boxed = arena.alloc(if_node);
    Stmt { kind: StmtKind::If(boxed), span }
}

pub fn match_stmt<'ast>(
    scrutinee: &'ast Expr<'ast>,
    arms:      &'ast [MatchArm<'ast>],
    span:      Span,
) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Match { scrutinee, arms }, span }
}

pub fn for_stmt<'ast>(
    arena:   &'ast AstArena,
    binding: BindingTarget<'ast>,
    iter:    &'ast Expr<'ast>,
    body:    Block<'ast>,
    span:    Span,
) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::For { binding, iter, body }, span }
}

pub fn while_stmt<'ast>(
    arena:     &'ast AstArena,
    condition: &'ast Expr<'ast>,
    body:      Block<'ast>,
    span:      Span,
) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::While { condition, body }, span }
}

pub fn loop_stmt<'ast>(arena: &'ast AstArena, body: Block<'ast>, span: Span) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::Loop(body), span }
}

// ── Memory management ─────────────────────────────────────────────

pub fn with_stmt<'ast>(
    arena:     &'ast AstArena,
    allocator: AllocatorKind<'ast>,
    body:      Block<'ast>,
    span:      Span,
) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::With { allocator, body }, span }
}

pub fn using_stmt<'ast>(
    arena:    &'ast AstArena,
    bindings: &'ast [UsingBinding<'ast>],
    body:     Block<'ast>,
    span:     Span,
) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::Using { bindings, body }, span }
}

// ── Destructuring ─────────────────────────────────────────────────

pub fn extract_stmt<'ast>(
    pattern: DestructurePattern<'ast>,
    value:   &'ast Expr<'ast>,
    span:    Span,
) -> Stmt<'ast> {
    Stmt { kind: StmtKind::Extract { pattern, value }, span }
}

// ── Error handling ────────────────────────────────────────────────

pub fn try_stmt<'ast>(
    arena:          &'ast AstArena,
    body:           Block<'ast>,
    catch_binding:  Option<&'ast str>,
    catch_body:     Option<Block<'ast>>,
    span:           Span,
) -> Stmt<'ast> {
    let body       = arena.alloc(body);
    let catch_body = catch_body.map(|b| &*arena.alloc(b));
    Stmt { kind: StmtKind::Try { body, catch_binding, catch_body }, span }
}

pub fn unsafe_stmt<'ast>(arena: &'ast AstArena, body: Block<'ast>, span: Span) -> Stmt<'ast> {
    let body = arena.alloc(body);
    Stmt { kind: StmtKind::Unsafe(body), span }
}

// ── Block ─────────────────────────────────────────────────────────

pub fn make_block<'ast>(
    arena: &'ast AstArena,
    stmts: Vec<Stmt<'ast>>,
    span:  Span,
) -> Block<'ast> {
    let stmts = arena.alloc_slice_clone(&stmts);
    Block { stmts, span }
}
