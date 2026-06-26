// src/parser/helpers/expr.rs
//! Builders for expression nodes.

use crate::ast::arena::AstArena;
use crate::ast::common::{AssignOp, BinOp, Span, UnaryOp};
use crate::ast::expressions::*;
use crate::ast::literals::{InterpolationPart as AstInterp, Literal};
use crate::ast::statements::Block;
use crate::ast::types::Type;
use crate::lexer::InterpolationPart as LexInterp;

// ── Alloc helpers ─────────────────────────────────────────────────

#[inline]
pub fn alloc_expr<'ast>(arena: &'ast AstArena, kind: ExprKind<'ast>, span: Span) -> &'ast Expr<'ast> {
    arena.alloc(Expr { kind, span })
}

// ── Leaf expressions ──────────────────────────────────────────────

pub fn ident_expr<'ast>(arena: &'ast AstArena, name: &'ast str, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Ident(name), span)
}

pub fn lit_expr<'ast>(arena: &'ast AstArena, lit: Literal<'ast>, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Lit(lit), span)
}

pub fn self_expr<'ast>(arena: &'ast AstArena, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::SelfExpr, span)
}

pub fn null_expr<'ast>(arena: &'ast AstArena, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Lit(Literal::Null), span)
}

// ── Binary / unary ops ────────────────────────────────────────────

pub fn binop<'ast>(
    arena: &'ast AstArena,
    op:    BinOp,
    lhs:   &'ast Expr<'ast>,
    rhs:   &'ast Expr<'ast>,
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::BinOp { op, lhs, rhs }, span)
}

pub fn unary<'ast>(
    arena:   &'ast AstArena,
    op:      UnaryOp,
    operand: &'ast Expr<'ast>,
    span:    Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::UnaryOp { op, operand }, span)
}

// ── Call / access / postfix ───────────────────────────────────────

pub fn call_expr<'ast>(
    arena:  &'ast AstArena,
    callee: &'ast Expr<'ast>,
    args:   &'ast [Arg<'ast>],
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Call { callee, args }, span)
}

pub fn field_expr<'ast>(
    arena:  &'ast AstArena,
    target: &'ast Expr<'ast>,
    field:  &'ast str,
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Field { target, field }, span)
}

pub fn index_expr<'ast>(
    arena:  &'ast AstArena,
    target: &'ast Expr<'ast>,
    index:  &'ast Expr<'ast>,
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Index { target, index }, span)
}

pub fn try_expr<'ast>(arena: &'ast AstArena, inner: &'ast Expr<'ast>, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Try(inner), span)
}

pub fn await_expr<'ast>(arena: &'ast AstArena, inner: &'ast Expr<'ast>, span: Span) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Await(inner), span)
}

pub fn as_expr<'ast>(
    arena: &'ast AstArena,
    expr:  &'ast Expr<'ast>,
    ty:    &'ast Type<'ast>,
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::As { expr, ty }, span)
}

// ── Pipe ──────────────────────────────────────────────────────────

pub fn pipe_expr<'ast>(
    arena: &'ast AstArena,
    left:  &'ast Expr<'ast>,
    right: &'ast Expr<'ast>,
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Pipe { left, right }, span)
}

// ── Assignment ────────────────────────────────────────────────────

pub fn assign_expr<'ast>(
    arena:  &'ast AstArena,
    op:     AssignOp,
    target: &'ast Expr<'ast>,
    value:  &'ast Expr<'ast>,
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Assign { op, target, value }, span)
}

pub fn short_decl_expr<'ast>(
    arena: &'ast AstArena,
    name:  &'ast str,
    value: &'ast Expr<'ast>,
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::ShortDecl { name, value }, span)
}

// ── Complex expressions (heap-allocated) ─────────────────────────

pub fn if_expr<'ast>(
    arena:   &'ast AstArena,
    if_node: IfExpr<'ast>,
    span:    Span,
) -> &'ast Expr<'ast> {
    let boxed = arena.alloc(if_node);
    alloc_expr(arena, ExprKind::If(boxed), span)
}

pub fn match_expr_node<'ast>(
    arena:  &'ast AstArena,
    node:   MatchExpr<'ast>,
    span:   Span,
) -> &'ast Expr<'ast> {
    let boxed = arena.alloc(node);
    alloc_expr(arena, ExprKind::Match(boxed), span)
}

pub fn lambda_expr<'ast>(
    arena:  &'ast AstArena,
    lambda: Lambda<'ast>,
    span:   Span,
) -> &'ast Expr<'ast> {
    let boxed = arena.alloc(lambda);
    alloc_expr(arena, ExprKind::Lambda(boxed), span)
}

pub fn block_expr<'ast>(
    arena: &'ast AstArena,
    block: Block<'ast>,
    span:  Span,
) -> &'ast Expr<'ast> {
    let boxed = arena.alloc(block);
    alloc_expr(arena, ExprKind::Block(boxed), span)
}

// ── Collection literals ───────────────────────────────────────────

pub fn array_expr<'ast>(
    arena: &'ast AstArena,
    elems: &'ast [&'ast Expr<'ast>],
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Array(elems), span)
}

pub fn tuple_expr<'ast>(
    arena: &'ast AstArena,
    elems: &'ast [&'ast Expr<'ast>],
    span:  Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Tuple(elems), span)
}

pub fn anon_object_expr<'ast>(
    arena:  &'ast AstArena,
    fields: &'ast [ObjectField<'ast>],
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::AnonObject(fields), span)
}

pub fn struct_lit_expr<'ast>(
    arena:  &'ast AstArena,
    path:   &'ast [&'ast str],
    fields: &'ast [FieldInit<'ast>],
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::StructLit { path, fields }, span)
}

pub fn dict_expr<'ast>(
    arena:   &'ast AstArena,
    entries: &'ast [DictEntry<'ast>],
    span:    Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::Dict(entries), span)
}

// ── Optional chaining ─────────────────────────────────────────────

pub fn opt_chain_field<'ast>(
    arena:  &'ast AstArena,
    target: &'ast Expr<'ast>,
    field:  &'ast str,
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(
        arena,
        ExprKind::OptionalChain { target, access: OptionalAccess::Field(field) },
        span,
    )
}

pub fn opt_chain_method<'ast>(
    arena:  &'ast AstArena,
    target: &'ast Expr<'ast>,
    name:   &'ast str,
    args:   &'ast [Arg<'ast>],
    span:   Span,
) -> &'ast Expr<'ast> {
    alloc_expr(
        arena,
        ExprKind::OptionalChain { target, access: OptionalAccess::Method { name, args } },
        span,
    )
}

// ── Interpolated string → arena conversion ────────────────────────

/// Convert lexer-owned `Vec<LexInterp>` to arena-allocated `&'ast [AstInterp<'ast>]`.
pub fn intern_interp_parts<'ast>(
    arena: &'ast AstArena,
    parts: Vec<LexInterp>,
) -> &'ast [AstInterp<'ast>] {
    let converted: Vec<AstInterp<'ast>> = parts
        .into_iter()
        .map(|p| match p {
            LexInterp::Text(s) => AstInterp::Text(arena.alloc_str(&s)),
            LexInterp::Expr(s) => AstInterp::Expr(arena.alloc_str(&s)),
        })
        .collect();
    arena.alloc_slice_clone(&converted)
}

// ── Or-else ───────────────────────────────────────────────────────

pub fn or_else_expr<'ast>(
    arena:    &'ast AstArena,
    expr:     &'ast Expr<'ast>,
    fallback: OrElseFallback<'ast>,
    span:     Span,
) -> &'ast Expr<'ast> {
    alloc_expr(arena, ExprKind::OrElse { expr, fallback }, span)
}
