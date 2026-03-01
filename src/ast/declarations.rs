// src/ast/declarations.rs
//! All declaration-level AST nodes:
//! functions, structs, enums, traits, impls, extensions, constants, aliases.

#![allow(dead_code)]

use crate::ast::common::{
    Attribute, GenericParam, LifetimeParam, Span, TierAnnotation, Visibility,
};
use crate::ast::expressions::Expr;
use crate::ast::statements::Block;
use crate::ast::types::Type;

// ── Function parameters ───────────────────────────────────────────

/// A parameter in a function or method signature.
#[derive(Debug, Clone, Copy)]
pub struct Param<'ast> {
    pub kind: ParamKind<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum ParamKind<'ast> {
    /// `[mut] name [: Type] [= default]`
    Named {
        mutable: bool,
        name: &'ast str,
        ty: Option<&'ast Type<'ast>>,
        default: Option<&'ast Expr<'ast>>,
    },
    SelfVal,       // self
    SelfMut,       // mut self
    SelfRef,       // &self
    SelfRefMut,    // &mut self
}

// ── Return type ───────────────────────────────────────────────────

/// The annotated return type of a function.
///
/// For async functions the `ty` field will be `TypeKind::Task(...)`.
/// The `is_fallible` flag corresponds to the `!` suffix.
#[derive(Debug, Clone, Copy)]
pub struct ReturnType<'ast> {
    pub ty: &'ast Type<'ast>,
    /// `true` when the return type carries `!` (operation may fail).
    pub is_fallible: bool,
}

// ── Function declaration ──────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct FunctionDecl<'ast> {
    /// Defaults to `TierAnnotation::High` when no `@tier(...)` is present.
    pub tier: TierAnnotation,
    pub attributes: &'ast [Attribute<'ast>],
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: &'ast str,
    pub lifetime_params: &'ast [LifetimeParam<'ast>],
    pub generic_params: &'ast [GenericParam<'ast>],
    pub params: &'ast [Param<'ast>],
    pub return_type: Option<ReturnType<'ast>>,
    pub body: Block<'ast>,
    pub span: Span,
}

// ── Struct declaration ────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct StructDecl<'ast> {
    pub visibility: Visibility,
    /// `true` for `edge struct` — manual (LOW-tier) memory layout.
    pub is_edge: bool,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub members: &'ast [StructMember<'ast>],
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum StructMember<'ast> {
    Field(FieldDecl<'ast>),
    Method(MethodDecl<'ast>),
    Property(PropertyDecl<'ast>),
}

#[derive(Debug, Clone, Copy)]
pub struct FieldDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub ty: &'ast Type<'ast>,
    pub span: Span,
}

/// A method inside a struct, trait default impl, or impl block.
#[derive(Debug, Clone, Copy)]
pub struct MethodDecl<'ast> {
    pub tier: TierAnnotation,
    pub attributes: &'ast [Attribute<'ast>],
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub params: &'ast [Param<'ast>],
    pub return_type: Option<ReturnType<'ast>>,
    pub body: Block<'ast>,
    pub span: Span,
}

/// A computed property with `get` (and optional `set`) body.
#[derive(Debug, Clone, Copy)]
pub struct PropertyDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub ty: &'ast Type<'ast>,
    pub getter: Block<'ast>,
    pub setter: Option<Block<'ast>>,
    pub span: Span,
}

// ── Enum declaration ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct EnumDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub variants: &'ast [EnumVariant<'ast>],
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct EnumVariant<'ast> {
    pub name: &'ast str,
    pub payload: EnumVariantPayload<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum EnumVariantPayload<'ast> {
    /// `Active` — no data
    None,
    /// `Active = 1` — explicit integer discriminant
    Discriminant(&'ast Expr<'ast>),
    /// `Ok(T)` — positional fields
    Tuple(&'ast [&'ast Type<'ast>]),
    /// `Err { code: int, msg: string }` — named fields
    Struct(&'ast [FieldDecl<'ast>]),
}

// ── Trait declaration ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct TraitDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub items: &'ast [TraitItem<'ast>],
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum TraitItem<'ast> {
    /// A required method (no default body).
    MethodSig(MethodSig<'ast>),
    /// A method with a default implementation.
    DefaultMethod(MethodDecl<'ast>),
    /// An associated type: `type Output`
    AssociatedType { name: &'ast str, span: Span },
}

/// A method signature without a body.
#[derive(Debug, Clone, Copy)]
pub struct MethodSig<'ast> {
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub params: &'ast [Param<'ast>],
    pub return_type: Option<ReturnType<'ast>>,
    pub span: Span,
}

// ── Impl block ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ImplBlock<'ast> {
    /// `None` = inherent impl, `Some(path)` = trait impl
    pub trait_path: Option<&'ast [&'ast str]>,
    pub target_type: &'ast Type<'ast>,
    pub methods: &'ast [MethodDecl<'ast>],
    pub span: Span,
}

// ── Extend declaration ────────────────────────────────────────────

/// Adds methods to an existing type without modifying it.
#[derive(Debug, Clone, Copy)]
pub struct ExtendDecl<'ast> {
    pub target_type: &'ast Type<'ast>,
    pub methods: &'ast [MethodDecl<'ast>],
    pub span: Span,
}

// ── Const and type alias ──────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ConstDecl<'ast> {
    pub name: &'ast str,
    pub ty: Option<&'ast Type<'ast>>,
    pub value: &'ast Expr<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct TypeAlias<'ast> {
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub ty: &'ast Type<'ast>,
    pub span: Span,
  }
