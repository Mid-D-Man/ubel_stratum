// src/sema/type_table.rs
//! Type representation used during semantic analysis.
//!
//! This is distinct from `ast::types::Type<'ast>` (which is the *syntactic*
//! type written by the programmer). `SemaType` is the *semantic* type after
//! resolution: type variables have been eliminated, aliases expanded,
//! and arena lifetimes tracked.
//!
//! # Why two type representations?
//!
//! `ast::types::Type<'ast>` is exactly what the parser produced — it may
//! contain unresolved names (`TypeKind::Named { path: ["Foo"], .. }`),
//! infer placeholders (`TypeKind::Infer`), and unevaluated generics.
//!
//! `SemaType` is what the type checker produces after it has resolved all of
//! those. A `TypeId` is a cheap integer handle into the `TypeTable`.

#![allow(dead_code)]

use std::collections::HashMap;
use crate::sema::symbol_table::DefId;

// ── TypeId ────────────────────────────────────────────────────────

/// A stable integer handle for a semantic type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

impl TypeId {
    /// Sentinel for "we don't know yet" or "this expression had an error."
    pub const ERROR: TypeId = TypeId(usize::MAX);

    pub fn is_error(self) -> bool { self.0 == usize::MAX }
}

// ── Arena lifetime tracking ───────────────────────────────────────

/// A unique identity for one arena created by a `with arena` block.
/// Used to track which values carry arena references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaId(pub usize);

// ── Semantic type ─────────────────────────────────────────────────

/// A fully-resolved semantic type.
///
/// Note: this is an owned, heap-allocated structure (not arena-allocated)
/// because it lives in the `TypeTable` which outlives any individual AST.
#[derive(Debug, Clone, PartialEq)]
pub enum SemaType {
    // ── Primitives ──────────────────────────────────────────────
    Int, Uint, Long, Ulong,
    Float, Double,
    Bool, Char, Str, Void,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64, Isize, Usize,

    // ── Composite ───────────────────────────────────────────────
    List(TypeId),
    Dictionary(TypeId, TypeId),
    Set(TypeId),
    Queue(TypeId),
    Stack(TypeId),
    Tuple(Vec<TypeId>),
    Array { len: u64, elem: TypeId },
    Slice(TypeId),

    // ── User-defined ─────────────────────────────────────────────
    /// A resolved named type with generic arguments substituted in.
    Named {
        def:  DefId,
        args: Vec<TypeId>,
    },

    // ── Modifiers ────────────────────────────────────────────────
    /// `T!` — may fail.
    Fallible(TypeId),
    /// `Task<T>` — async result.
    Task(TypeId),
    /// `T?` — optional / nullable.
    Optional(TypeId),

    // ── References ───────────────────────────────────────────────
    /// A GC-managed reference (HIGH tier).  No lifetime needed.
    GcRef(TypeId),

    /// A reference into an arena.  Carries the `ArenaId` of its origin.
    /// The tier checker uses this to detect cross-boundary escapes.
    ArenaRef {
        arena:   ArenaId,
        mutable: bool,
        inner:   TypeId,
    },

    /// A LOW-tier owned reference (borrow-checked in Phase 4).
    OwnedRef {
        mutable: bool,
        inner:   TypeId,
    },

    // ── Function types ───────────────────────────────────────────
    Function {
        params:      Vec<TypeId>,
        return_type: TypeId,
        is_fallible: bool,
    },

    // ── Inference variables ──────────────────────────────────────
    /// An unsolved type variable — replaced by unification.
    Var(u32),

    // ── Null / unknown ───────────────────────────────────────────
    Null,
    Unknown,
}

impl SemaType {
    /// Returns `true` if this type (or any type it contains) is an `ArenaRef`.
    /// Used by the tier checker to detect cross-boundary escapes.
    pub fn contains_arena_ref(&self, table: &TypeTable) -> bool {
        match self {
            SemaType::ArenaRef { .. } => true,
            SemaType::List(t)
            | SemaType::Set(t)
            | SemaType::Queue(t)
            | SemaType::Stack(t)
            | SemaType::Slice(t)
            | SemaType::Fallible(t)
            | SemaType::Task(t)
            | SemaType::Optional(t)
            | SemaType::GcRef(t)
            | SemaType::OwnedRef { inner: t, .. } => {
                table.get(*t).contains_arena_ref(table)
            }
            SemaType::Dictionary(k, v) => {
                table.get(*k).contains_arena_ref(table)
                    || table.get(*v).contains_arena_ref(table)
            }
            SemaType::Tuple(ts) => ts.iter().any(|t| table.get(*t).contains_arena_ref(table)),
            SemaType::Named { args, .. } => {
                args.iter().any(|t| table.get(*t).contains_arena_ref(table))
            }
            SemaType::Function { params, return_type, .. } => {
                params.iter().any(|t| table.get(*t).contains_arena_ref(table))
                    || table.get(*return_type).contains_arena_ref(table)
            }
            _ => false,
        }
    }

    /// A short human-readable name for error messages.
    pub fn display(&self, table: &TypeTable) -> String {
        match self {
            SemaType::Int     => "int".into(),
            SemaType::Uint    => "uint".into(),
            SemaType::Long    => "long".into(),
            SemaType::Float   => "float".into(),
            SemaType::Double  => "double".into(),
            SemaType::Bool    => "bool".into(),
            SemaType::Char    => "char".into(),
            SemaType::Str     => "string".into(),
            SemaType::Void    => "void".into(),
            SemaType::Null    => "null".into(),

            SemaType::List(t) => format!("List<{}>", table.get(*t).display(table)),
            SemaType::Optional(t) => format!("{}?", table.get(*t).display(table)),
            SemaType::Fallible(t) => format!("{}!", table.get(*t).display(table)),
            SemaType::Task(t)     => format!("Task<{}>", table.get(*t).display(table)),

            SemaType::ArenaRef { inner, .. } =>
                format!("&arena {}", table.get(*inner).display(table)),

            SemaType::Var(n) => format!("?T{}", n),
            SemaType::Unknown => "<unknown>".into(),

            _ => "<type>".into(),
        }
    }
}

// ── TypeTable ────────────────────────────────────────────────────

/// The flat table of all semantic types, indexed by `TypeId`.
///
/// Interning: `intern` checks whether an identical `SemaType` already exists
/// and returns the existing `TypeId` if so.  This keeps the table small and
/// makes equality checks O(1) (just compare `TypeId`s).
#[derive(Debug, Default)]
pub struct TypeTable {
    types:  Vec<SemaType>,
    /// Reverse map for interning.
    intern: HashMap<Internable, TypeId>,
    /// Counter for fresh type variables.
    next_var: u32,
}

/// The subset of `SemaType` variants that support O(1) structural interning.
/// Complex variants (Tuple, Named, …) are inserted without interning.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Internable {
    Int, Uint, Long, Ulong,
    Float, Double, Bool, Char, Str, Void,
    I8, I16, I32, I64, U8, U16, U32, U64,
    F32, F64, Isize, Usize,
    Null, Unknown,
    List(TypeId),
    Optional(TypeId),
    Fallible(TypeId),
    Task(TypeId),
    Slice(TypeId),
}

impl TypeTable {
    pub fn new() -> Self { TypeTable::default() }

    /// Insert a new type unconditionally and return its `TypeId`.
    /// Prefer `intern` for primitive and simple composite types.
    pub fn insert(&mut self, ty: SemaType) -> TypeId {
        let id = TypeId(self.types.len());
        self.types.push(ty);
        id
    }

    /// Insert a type with interning — returns the existing `TypeId` if this
    /// exact type has been seen before.
    pub fn intern(&mut self, ty: SemaType) -> TypeId {
        let key: Option<Internable> = match &ty {
            SemaType::Int     => Some(Internable::Int),
            SemaType::Uint    => Some(Internable::Uint),
            SemaType::Long    => Some(Internable::Long),
            SemaType::Float   => Some(Internable::Float),
            SemaType::Double  => Some(Internable::Double),
            SemaType::Bool    => Some(Internable::Bool),
            SemaType::Char    => Some(Internable::Char),
            SemaType::Str     => Some(Internable::Str),
            SemaType::Void    => Some(Internable::Void),
            SemaType::Null    => Some(Internable::Null),
            SemaType::Unknown => Some(Internable::Unknown),
            SemaType::List(t) => Some(Internable::List(*t)),
            SemaType::Optional(t) => Some(Internable::Optional(*t)),
            SemaType::Fallible(t) => Some(Internable::Fallible(*t)),
            SemaType::Task(t)     => Some(Internable::Task(*t)),
            SemaType::Slice(t)    => Some(Internable::Slice(*t)),
            _ => None,
        };

        if let Some(k) = key {
            if let Some(&existing) = self.intern.get(&k) {
                return existing;
            }
            let id = TypeId(self.types.len());
            self.types.push(ty);
            self.intern.insert(k, id);
            return id;
        }

        self.insert(ty)
    }

    /// Retrieve a type by its `TypeId`. Panics on an invalid id.
    pub fn get(&self, id: TypeId) -> &SemaType {
        &self.types[id.0]
    }

    /// Allocate a fresh type-inference variable.
    pub fn fresh_var(&mut self) -> TypeId {
        let var_id = self.next_var;
        self.next_var += 1;
        self.insert(SemaType::Var(var_id))
    }

    /// Return the well-known `TypeId` for a primitive.
    /// Call this after the table has been seeded with `seed_builtins`.
    pub fn builtin_int(&mut self)    -> TypeId { self.intern(SemaType::Int) }
    pub fn builtin_bool(&mut self)   -> TypeId { self.intern(SemaType::Bool) }
    pub fn builtin_str(&mut self)    -> TypeId { self.intern(SemaType::Str) }
    pub fn builtin_void(&mut self)   -> TypeId { self.intern(SemaType::Void) }
    pub fn builtin_float(&mut self)  -> TypeId { self.intern(SemaType::Float) }
    pub fn builtin_double(&mut self) -> TypeId { self.intern(SemaType::Double) }
    pub fn builtin_char(&mut self)   -> TypeId { self.intern(SemaType::Char) }
    pub fn builtin_null(&mut self)   -> TypeId { self.intern(SemaType::Null) }
              }
