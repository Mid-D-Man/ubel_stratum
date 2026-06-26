// src/ast/types.rs
//! Type expression nodes.

#![allow(dead_code)]

use crate::ast::common::{Span, GenericParam, LifetimeParam};

/// A type expression with its source location.
#[derive(Debug, Clone, Copy)]
pub struct Type<'ast> {
    pub kind: TypeKind<'ast>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum TypeKind<'ast> {
    // ── Primitive types ──────────────────────────────────────────
    Int, Uint, Long, Ulong, Short, Ushort,
    Byte, Ubyte, Float, Double,
    Bool, Char, Str, Void,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64, Isize, Usize,

    // ── Built-in generic collections ────────────────────────────
    /// `List` or `List<T>`
    List(Option<&'ast Type<'ast>>),
    /// `Dictionary<K, V>`
    Dictionary(Option<(&'ast Type<'ast>, &'ast Type<'ast>)>),
    /// `Set<T>`
    Set(Option<&'ast Type<'ast>>),
    /// `Queue<T>`
    Queue(Option<&'ast Type<'ast>>),
    /// `Stack<T>`
    Stack(Option<&'ast Type<'ast>>),

    // ── User-defined / imported types ────────────────────────────
    /// A named type, possibly with generic arguments: `Foo<T, U>`
    Named {
        /// Qualified path segments, e.g. `["std", "io", "File"]`
        path: &'ast [&'ast str],
        /// Type arguments (empty = non-generic use)
        args: &'ast [&'ast Type<'ast>],
    },

    // ── Composite types ──────────────────────────────────────────
    /// `(int, string, bool)` — two or more elements
    Tuple(&'ast [&'ast Type<'ast>]),

    /// `[4]int` — fixed-size array
    Array {
        len: u64,
        elem: &'ast Type<'ast>,
    },

    /// `[]int` — dynamically-sized slice
    Slice(&'ast Type<'ast>),

    // ── Modifier types ───────────────────────────────────────────
    /// `T!` — the operation may fail and must be handled
    Fallible(&'ast Type<'ast>),

    /// `Task<T>` — an asynchronous computation
    Task(Option<&'ast Type<'ast>>),

    /// `&T`, `&mut T`, or `&L T` (with explicit lifetime)
    Reference {
        mutable: bool,
        lifetime: Option<&'ast str>,
        inner: &'ast Type<'ast>,
    },

    /// `T?` — an optional / nullable value
    Optional(&'ast Type<'ast>),

    // ── Function type ─────────────────────────────────────────────
    /// `fn(A, B) C` or `fn(A, B) C!`
    Function(FunctionType<'ast>),

    // ── Wildcard ─────────────────────────────────────────────────
    /// `_` — ask the compiler to infer the type
    Infer,
}

/// The signature shape of a function-typed value.
#[derive(Debug, Clone, Copy)]
pub struct FunctionType<'ast> {
    pub params: &'ast [&'ast Type<'ast>],
    pub return_type: Option<&'ast Type<'ast>>,
    /// Whether the return type carries `!` (may-fail).
    pub is_fallible: bool,
}
