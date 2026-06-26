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
    SelfVal,    // self
    SelfMut,    // mut self
    SelfRef,    // &self
    SelfRefMut, // &mut self
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

/// A top-level or impl-block function.
///
/// `tier` defaults to `TierAnnotation::High` when no `@tier(...)` annotation
/// is present — HIGH is the language default.
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

/// A `struct` or `edge struct` declaration.
///
/// `edge struct` marks a type that lives in LOW tier and uses
/// manual / pointer-based memory layout.
#[derive(Debug, Clone, Copy)]
pub struct StructDecl<'ast> {
    pub visibility: Visibility,
    /// `true` for `edge struct` — manual (LOW-tier) heap layout.
    pub is_edge: bool,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub members: &'ast [StructMember<'ast>],
    pub span: Span,
}

/// One member of a struct body.
#[derive(Debug, Clone, Copy)]
pub enum StructMember<'ast> {
    Field(FieldDecl<'ast>),
    Method(MethodDecl<'ast>),
    Property(PropertyDecl<'ast>),
}

/// A named field inside a struct: `pub name: Type`
#[derive(Debug, Clone, Copy)]
pub struct FieldDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub ty: &'ast Type<'ast>,
    pub span: Span,
}

/// A method declaration — appears inside structs, trait default impls,
/// impl blocks, and extend declarations.
#[derive(Debug, Clone, Copy)]
pub struct MethodDecl<'ast> {
    /// Inherits the enclosing struct/impl tier unless overridden.
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

/// A computed property with a `get` accessor and optional `set` accessor.
///
/// ```strat
/// pub score: int {
///     get { return self.raw_score * self.multiplier }
///     set { self.raw_score = value / self.multiplier }
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PropertyDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub ty: &'ast Type<'ast>,
    pub getter: Block<'ast>,
    /// `None` = read-only property.
    pub setter: Option<Block<'ast>>,
    pub span: Span,
}

// ── Enum declaration ──────────────────────────────────────────────

/// An `enum` declaration.
///
/// Ubel enums support:
///   - simple variants:            `Active`
///   - discriminant variants:      `Active = 1`
///   - tuple variants:             `Ok(T)`
///   - struct variants:            `Err { code: int, message: string }`
#[derive(Debug, Clone, Copy)]
pub struct EnumDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub variants: &'ast [EnumVariant<'ast>],
    pub span: Span,
}

/// A single variant inside an enum body.
#[derive(Debug, Clone, Copy)]
pub struct EnumVariant<'ast> {
    pub name: &'ast str,
    pub payload: EnumVariantPayload<'ast>,
    pub span: Span,
}

/// The data carried by an enum variant.
#[derive(Debug, Clone, Copy)]
pub enum EnumVariantPayload<'ast> {
    /// `Active` — no data attached.
    None,
    /// `Active = 1` — explicit integer discriminant, no additional data.
    Discriminant(&'ast Expr<'ast>),
    /// `Ok(T)` — positional (tuple-style) fields.
    Tuple(&'ast [&'ast Type<'ast>]),
    /// `Err { code: int, message: string }` — named fields.
    Struct(&'ast [FieldDecl<'ast>]),
}

// ── Trait declaration ─────────────────────────────────────────────

/// A `trait` declaration.
///
/// Traits may contain:
///   - required method signatures (no body)
///   - default method implementations (with body)
///   - associated type declarations
#[derive(Debug, Clone, Copy)]
pub struct TraitDecl<'ast> {
    pub visibility: Visibility,
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub items: &'ast [TraitItem<'ast>],
    pub span: Span,
}

/// An item inside a trait body.
#[derive(Debug, Clone, Copy)]
pub enum TraitItem<'ast> {
    /// A required method — implementors must provide a body.
    MethodSig(MethodSig<'ast>),
    /// A method with a default implementation — implementors may override.
    DefaultMethod(MethodDecl<'ast>),
    /// An associated type: `type Output`
    AssociatedType {
        name: &'ast str,
        span: Span,
    },
}

/// A method signature without a body — the required-method form inside traits.
#[derive(Debug, Clone, Copy)]
pub struct MethodSig<'ast> {
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub params: &'ast [Param<'ast>],
    pub return_type: Option<ReturnType<'ast>>,
    pub span: Span,
}

// ── Impl block ────────────────────────────────────────────────────

/// An `impl` block — either inherent (`impl Foo`) or trait (`impl Bar for Foo`).
///
/// ```strat
/// impl Drawable for Circle {
///     pub fn draw(self) { ... }
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ImplBlock<'ast> {
    /// `None`      = inherent impl: `impl Foo { ... }`
    /// `Some(path)` = trait impl:   `impl Trait for Foo { ... }`
    pub trait_path: Option<&'ast [&'ast str]>,
    pub target_type: &'ast Type<'ast>,
    pub methods: &'ast [MethodDecl<'ast>],
    pub span: Span,
}

// ── Extend declaration ────────────────────────────────────────────

/// An `extend` declaration — adds methods to an existing type without
/// modifying the type's original definition.
///
/// ```strat
/// extend int {
///     pub fn is_even(self) bool { return self % 2 == 0 }
/// }
/// ```
///
/// Extension methods follow the same tier rules as regular methods.
/// You cannot add fields via `extend`.
#[derive(Debug, Clone, Copy)]
pub struct ExtendDecl<'ast> {
    pub target_type: &'ast Type<'ast>,
    pub methods: &'ast [MethodDecl<'ast>],
    pub span: Span,
}

// ── Const and type alias ──────────────────────────────────────────

/// A top-level constant: `const PI: double = 3.14159`
///
/// Constants are evaluated at compile time and are immutable.
/// The type annotation is optional — the compiler infers from the value
/// expression when omitted.
#[derive(Debug, Clone, Copy)]
pub struct ConstDecl<'ast> {
    pub name: &'ast str,
    /// `None` = let the compiler infer the type.
    pub ty: Option<&'ast Type<'ast>>,
    pub value: &'ast Expr<'ast>,
    pub span: Span,
}

/// A type alias: `type Result<T> = Fallible<T, Error>`
///
/// Generic aliases are supported.  Ubel uses type aliases for
/// common patterns like `type Handler = fn(Request) Task<Response>!`.
#[derive(Debug, Clone, Copy)]
pub struct TypeAlias<'ast> {
    pub name: &'ast str,
    pub generic_params: &'ast [GenericParam<'ast>],
    pub ty: &'ast Type<'ast>,
    pub span: Span,
    }
