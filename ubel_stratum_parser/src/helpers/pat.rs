// src/parser/helpers/pat.rs
//! Builders for pattern nodes.

use crate::ast::arena::AstArena;
use crate::ast::common::Span;
use crate::ast::literals::Literal;
use crate::ast::patterns::*;

pub fn wildcard_pat(span: Span) -> Pattern<'static> {
    Pattern { kind: PatternKind::Wildcard, span }
}

pub fn ident_pat<'ast>(name: &'ast str, mutable: bool, span: Span) -> Pattern<'ast> {
    Pattern { kind: PatternKind::Ident { name, mutable }, span }
}

pub fn literal_pat<'ast>(lit: Literal<'ast>, span: Span) -> Pattern<'ast> {
    Pattern { kind: PatternKind::Literal(lit), span }
}

pub fn tuple_pat<'ast>(
    arena: &'ast AstArena,
    elems: Vec<Pattern<'ast>>,
    span:  Span,
) -> Pattern<'ast> {
    let elems = arena.alloc_slice_clone(&elems);
    Pattern { kind: PatternKind::Tuple(elems), span }
}

pub fn array_pat<'ast>(
    arena: &'ast AstArena,
    elems: Vec<Pattern<'ast>>,
    rest:  Option<Option<&'ast str>>,
    span:  Span,
) -> Pattern<'ast> {
    let elems = arena.alloc_slice_clone(&elems);
    Pattern { kind: PatternKind::Array { elements: elems, rest }, span }
}

pub fn struct_pat<'ast>(
    arena:  &'ast AstArena,
    name:   Option<&'ast str>,
    fields: Vec<FieldPattern<'ast>>,
    span:   Span,
) -> Pattern<'ast> {
    let fields = arena.alloc_slice_clone(&fields);
    Pattern { kind: PatternKind::Struct { name, fields }, span }
}

pub fn enum_pat<'ast>(
    arena:   &'ast AstArena,
    path:    &'ast [&'ast str],
    payload: EnumPatternPayload<'ast>,
    span:    Span,
) -> Pattern<'ast> {
    Pattern { kind: PatternKind::Enum { path, payload }, span }
}

pub fn or_pat<'ast>(
    arena: &'ast AstArena,
    alts:  Vec<Pattern<'ast>>,
    span:  Span,
) -> Pattern<'ast> {
    let alts = arena.alloc_slice_clone(&alts);
    Pattern { kind: PatternKind::Or(alts), span }
}

pub fn range_pat<'ast>(
    lo:        Literal<'ast>,
    hi:        Literal<'ast>,
    inclusive: bool,
    span:      Span,
) -> Pattern<'ast> {
    Pattern { kind: PatternKind::Range { lo, hi, inclusive }, span }
}

pub fn extract_pat<'ast>(
    arena:  &'ast AstArena,
    fields: Vec<FieldPattern<'ast>>,
    span:   Span,
) -> Pattern<'ast> {
    let fields = arena.alloc_slice_clone(&fields);
    Pattern { kind: PatternKind::Extract(fields), span }
}
