// src/parser/helpers/ty.rs
//! Builders for type-expression nodes.

use crate::ast::arena::AstArena;
use crate::ast::common::{GenericParam, Span};
use crate::ast::types::*;

#[inline]
pub fn mk_type<'ast>(arena: &'ast AstArena, kind: TypeKind<'ast>, span: Span) -> &'ast Type<'ast> {
    arena.alloc(Type { kind, span })
}

// ── Primitives ────────────────────────────────────────────────────

pub fn prim_type<'ast>(arena: &'ast AstArena, kind: TypeKind<'ast>, span: Span) -> &'ast Type<'ast> {
    mk_type(arena, kind, span)
}

// ── Collections ───────────────────────────────────────────────────

pub fn list_type<'ast>(
    arena: &'ast AstArena,
    inner: Option<&'ast Type<'ast>>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::List(inner), span)
}

pub fn dict_type<'ast>(
    arena: &'ast AstArena,
    k:     &'ast Type<'ast>,
    v:     &'ast Type<'ast>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Dictionary(Some((k, v))), span)
}

pub fn set_type<'ast>(
    arena: &'ast AstArena,
    inner: Option<&'ast Type<'ast>>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Set(inner), span)
}

// ── Named / generic ───────────────────────────────────────────────

pub fn named_type<'ast>(
    arena: &'ast AstArena,
    path:  &'ast [&'ast str],
    args:  &'ast [&'ast Type<'ast>],
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Named { path, args }, span)
}

// ── Modifiers ─────────────────────────────────────────────────────

pub fn optional_type<'ast>(
    arena: &'ast AstArena,
    inner: &'ast Type<'ast>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Optional(inner), span)
}

pub fn fallible_type<'ast>(
    arena: &'ast AstArena,
    inner: &'ast Type<'ast>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Fallible(inner), span)
}

pub fn task_type<'ast>(
    arena: &'ast AstArena,
    inner: Option<&'ast Type<'ast>>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Task(inner), span)
}

pub fn reference_type<'ast>(
    arena:    &'ast AstArena,
    mutable:  bool,
    lifetime: Option<&'ast str>,
    inner:    &'ast Type<'ast>,
    span:     Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Reference { mutable, lifetime, inner }, span)
}

pub fn slice_type<'ast>(
    arena: &'ast AstArena,
    elem:  &'ast Type<'ast>,
    span:  Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Slice(elem), span)
}

pub fn tuple_type<'ast>(
    arena:  &'ast AstArena,
    fields: &'ast [&'ast Type<'ast>],
    span:   Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Tuple(fields), span)
}

pub fn fn_type<'ast>(
    arena:       &'ast AstArena,
    params:      &'ast [&'ast Type<'ast>],
    return_type: Option<&'ast Type<'ast>>,
    is_fallible: bool,
    span:        Span,
) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Function(FunctionType { params, return_type, is_fallible }), span)
}

pub fn infer_type<'ast>(arena: &'ast AstArena, span: Span) -> &'ast Type<'ast> {
    mk_type(arena, TypeKind::Infer, span)
}

// ── GenericParam helper ───────────────────────────────────────────

pub fn generic_param<'ast>(
    arena:  &'ast AstArena,
    name:   &'ast str,
    bounds: Vec<&'ast str>,
    span:   Span,
) -> GenericParam<'ast> {
    let bounds = arena.alloc_slice_clone(&bounds);
    GenericParam { name, bounds, span }
}
