// src/parser/helpers/decl.rs
//! Builders for declaration-level AST nodes.

use crate::ast::arena::AstArena;
use crate::ast::common::{
    Attribute, GenericParam, LifetimeParam, Span, TierAnnotation, Visibility,
};
use crate::ast::declarations::*;
use crate::ast::expressions::Expr;
use crate::ast::statements::Block;
use crate::ast::types::Type;

// ── Span shorthand ────────────────────────────────────────────────

/// Construct a zero-line/column Span from byte offsets.
/// Used by grammar actions in place of the `sp!(lo, hi)` macro,
/// which LALRPOP cannot process (it pre-dates rustc).
#[inline]
pub fn sp(lo: usize, hi: usize) -> Span {
    Span::new(lo, hi, 0, 0)
}

// ── Slices ────────────────────────────────────────────────────────

/// Intern a heap `String` into the arena and return `&'ast str`.
#[inline]
pub fn intern<'ast>(arena: &'ast AstArena, s: String) -> &'ast str {
    arena.alloc_str(&s)
}

/// Move a `Vec<T: Copy>` into an arena slice.
#[inline]
pub fn copy_slice<'ast, T: Copy>(arena: &'ast AstArena, v: Vec<T>) -> &'ast [T] {
    arena.alloc_vec_copy(v)
}

/// Move a `Vec<T: Clone>` into an arena slice.
#[inline]
pub fn clone_slice<'ast, T: Clone>(arena: &'ast AstArena, v: Vec<T>) -> &'ast [T] {
    arena.alloc_slice_clone(&v)
}

/// Build an arena slice of `&'ast str` from an owned `Vec<String>`.
pub fn str_slice<'ast>(arena: &'ast AstArena, v: Vec<String>) -> &'ast [&'ast str] {
    let interned: Vec<&'ast str> = v.into_iter().map(|s| arena.alloc_str(&s)).collect();
    clone_slice(arena, interned)
}

// ── FunctionDecl ──────────────────────────────────────────────────

pub struct FnBuilder<'ast> {
    pub tier:            TierAnnotation,
    pub attributes:      &'ast [Attribute<'ast>],
    pub visibility:      Visibility,
    pub is_async:        bool,
    pub name:            &'ast str,
    pub lifetime_params: &'ast [LifetimeParam<'ast>],
    pub generic_params:  &'ast [GenericParam<'ast>],
    pub params:          &'ast [Param<'ast>],
    pub return_type:     Option<ReturnType<'ast>>,
    pub body:            Block<'ast>,
    pub span:            Span,
}

impl<'ast> FnBuilder<'ast> {
    pub fn build(self) -> FunctionDecl<'ast> {
        FunctionDecl {
            tier:            self.tier,
            attributes:      self.attributes,
            visibility:      self.visibility,
            is_async:        self.is_async,
            name:            self.name,
            lifetime_params: self.lifetime_params,
            generic_params:  self.generic_params,
            params:          self.params,
            return_type:     self.return_type,
            body:            self.body,
            span:            self.span,
        }
    }
}

// ── StructDecl ────────────────────────────────────────────────────

pub fn build_struct<'ast>(
    visibility:     Visibility,
    is_edge:        bool,
    name:           &'ast str,
    generic_params: &'ast [GenericParam<'ast>],
    members:        &'ast [StructMember<'ast>],
    span:           Span,
) -> StructDecl<'ast> {
    StructDecl { visibility, is_edge, name, generic_params, members, span }
}

// ── MethodDecl ────────────────────────────────────────────────────

pub fn build_method<'ast>(
    tier:           TierAnnotation,
    visibility:     Visibility,
    is_async:       bool,
    name:           &'ast str,
    generic_params: &'ast [GenericParam<'ast>],
    params:         &'ast [Param<'ast>],
    return_type:    Option<ReturnType<'ast>>,
    body:           Block<'ast>,
    span:           Span,
) -> MethodDecl<'ast> {
    MethodDecl {
        tier,
        attributes: &[],
        visibility,
        is_async,
        name,
        generic_params,
        params,
        return_type,
        body,
        span,
    }
}

// ── Param helpers ─────────────────────────────────────────────────

pub fn named_param<'ast>(
    mutable: bool,
    name:    &'ast str,
    ty:      Option<&'ast Type<'ast>>,
    default: Option<&'ast Expr<'ast>>,
    span:    Span,
) -> Param<'ast> {
    Param { kind: ParamKind::Named { mutable, name, ty, default }, span }
}

// ── ReturnType helper ─────────────────────────────────────────────

pub fn ret_type<'ast>(ty: &'ast Type<'ast>, is_fallible: bool) -> ReturnType<'ast> {
    ReturnType { ty, is_fallible }
}

// ── EnumDecl ──────────────────────────────────────────────────────

pub fn build_enum<'ast>(
    visibility:     Visibility,
    name:           &'ast str,
    generic_params: &'ast [GenericParam<'ast>],
    variants:       &'ast [EnumVariant<'ast>],
    span:           Span,
) -> EnumDecl<'ast> {
    EnumDecl { visibility, name, generic_params, variants, span }
}

// ── TraitDecl ─────────────────────────────────────────────────────

pub fn build_trait<'ast>(
    visibility:     Visibility,
    name:           &'ast str,
    generic_params: &'ast [GenericParam<'ast>],
    items:          &'ast [TraitItem<'ast>],
    span:           Span,
) -> TraitDecl<'ast> {
    TraitDecl { visibility, name, generic_params, items, span }
}

// ── ConstDecl ─────────────────────────────────────────────────────

pub fn build_const<'ast>(
    name:  &'ast str,
    ty:    Option<&'ast Type<'ast>>,
    value: &'ast Expr<'ast>,
    span:  Span,
) -> ConstDecl<'ast> {
    ConstDecl { name, ty, value, span }
}

// ── TypeAlias ─────────────────────────────────────────────────────

pub fn build_type_alias<'ast>(
    name:           &'ast str,
    generic_params: &'ast [GenericParam<'ast>],
    ty:             &'ast Type<'ast>,
    span:           Span,
) -> TypeAlias<'ast> {
    TypeAlias { name, generic_params, ty, span }
}

// ── ImplBlock ─────────────────────────────────────────────────────

pub fn build_impl<'ast>(
    trait_path:  Option<&'ast [&'ast str]>,
    target_type: &'ast Type<'ast>,
    methods:     &'ast [MethodDecl<'ast>],
    span:        Span,
) -> ImplBlock<'ast> {
    ImplBlock { trait_path, target_type, methods, span }
}

// ── ExtendDecl ────────────────────────────────────────────────────

pub fn build_extend<'ast>(
    target_type: &'ast Type<'ast>,
    methods:     &'ast [MethodDecl<'ast>],
    span:        Span,
) -> ExtendDecl<'ast> {
    ExtendDecl { target_type, methods, span }
    }
