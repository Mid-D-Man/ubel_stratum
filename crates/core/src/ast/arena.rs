// src/ast/arena.rs

use bumpalo::Bump;
pub use bumpalo::collections::Vec as BumpVec;

/// Owns all memory for a single parsed AST.
/// Every node, string, and slice lives exactly as long as this struct.
///
/// Usage pattern:
/// ```
/// use ubel_stratum::ast::{AstArena, Expr, ExprKind, Literal, Span, Stmt, StmtKind};
///
/// let arena = AstArena::new();
/// let span  = Span::new(0, 0, 1, 1);
/// let expr  = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(42)), span });
/// let stmt  = Stmt { kind: StmtKind::Expr(expr), span };
/// let stmts = {
///     let mut v = arena.vec::<Stmt>();
///     v.push(stmt);
///     v.into_bump_slice()
/// };
/// assert_eq!(stmts.len(), 1);
/// ```
pub struct AstArena {
    bump: Bump,
}

impl AstArena {
    pub fn new() -> Self {
        AstArena { bump: Bump::new() }
    }

    /// Pre-allocate at least `capacity` bytes to avoid early re-allocations.
    /// A good starting value for a typical source file is 256 KB.
    pub fn with_capacity(capacity: usize) -> Self {
        AstArena { bump: Bump::with_capacity(capacity) }
    }

    /// Allocate a single value into the arena.
    #[inline]
    pub fn alloc<'ast, T>(&'ast self, val: T) -> &'ast T {
        self.bump.alloc(val)
    }

    /// Intern a string slice into the arena.
    #[inline]
    pub fn alloc_str<'ast>(&'ast self, s: &str) -> &'ast str {
        self.bump.alloc_str(s)
    }

    /// Allocate a slice from an existing slice of `Copy` values.
    #[inline]
    pub fn alloc_slice_copy<'ast, T: Copy>(&'ast self, vals: &[T]) -> &'ast [T] {
        self.bump.alloc_slice_copy(vals)
    }

    /// Allocate a slice from an existing slice of `Clone` values.
    #[inline]
    pub fn alloc_slice_clone<'ast, T: Clone>(&'ast self, vals: &[T]) -> &'ast [T] {
        self.bump.alloc_slice_clone(vals)
    }

    /// Create a `BumpVec` backed by this arena.
    /// Call `.into_bump_slice()` to get `&'ast [T]` when done building.
    #[inline]
    pub fn vec<'ast, T>(&'ast self) -> BumpVec<'ast, T> {
        BumpVec::new_in(&self.bump)
    }

    /// Create a `BumpVec` with pre-allocated capacity.
    /// Always prefer this over `vec()` when the approximate size is known.
    /// Avoids mid-list reallocations — see PARSER_RULES.md §3.5.
    #[inline]
    pub fn vec_with_capacity<'ast, T>(&'ast self, cap: usize) -> BumpVec<'ast, T> {
        BumpVec::with_capacity_in(cap, &self.bump)
    }

    /// Convenience: move a standard `Vec<T: Copy>` into an arena slice.
    #[inline]
    pub fn alloc_vec_copy<'ast, T: Copy>(&'ast self, v: Vec<T>) -> &'ast [T] {
        self.bump.alloc_slice_copy(v.as_slice())
    }

    /// Total bytes allocated — useful for benchmarks / diagnostics.
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }
}

impl Default for AstArena {
    fn default() -> Self { Self::new() }
}
