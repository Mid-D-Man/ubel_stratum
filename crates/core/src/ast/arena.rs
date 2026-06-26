// src/ast/arena.rs

use bumpalo::Bump;
pub use bumpalo::collections::Vec as BumpVec;

/// Owns all memory for a single parsed AST.
/// Every node, string, and slice lives exactly as long as this struct.
///
/// Usage pattern:
/// ```
/// let arena = AstArena::new();
/// let expr  = arena.alloc(ExprKind::IntLit(42));
/// let stmts = {
///     let mut v = arena.vec::<Stmt>();
///     v.push(stmt);
///     v.into_bump_slice()
/// };
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
        AstArena {
            bump: Bump::with_capacity(capacity),
        }
    }

    /// Allocate a single value.  Returns a shared reference whose lifetime
    /// is tied to `&self` (i.e. the arena).
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
    /// Prefer `arena.vec()` / `.into_bump_slice()` for non-Copy items.
    #[inline]
    pub fn alloc_slice_copy<'ast, T: Copy>(&'ast self, vals: &[T]) -> &'ast [T] {
        self.bump.alloc_slice_copy(vals)
    }

    /// Allocate a slice from an existing slice of `Clone` values.
    #[inline]
    pub fn alloc_slice_clone<'ast, T: Clone>(&'ast self, vals: &[T]) -> &'ast [T] {
        self.bump.alloc_slice_clone(vals)
    }

    /// Create a `Vec` whose backing storage lives in this arena.
    /// Call `.into_bump_slice()` on the result to get `&'ast [T]`.
    ///
    /// This is the **primary way** to build lists during parsing:
    /// ```
    /// let mut params = arena.vec::<Param>();
    /// params.push(p1);
    /// params.push(p2);
    /// let params: &[Param] = params.into_bump_slice();
    /// ```
    #[inline]
    pub fn vec<'ast, T>(&'ast self) -> BumpVec<'ast, T> {
        BumpVec::new_in(&self.bump)
    }

    /// Convenience: move a standard `Vec<T: Copy>` into an arena slice.
    #[inline]
    pub fn alloc_vec_copy<'ast, T: Copy>(&'ast self, v: Vec<T>) -> &'ast [T] {
        self.bump.alloc_slice_copy(v.as_slice())
    }

    /// Total bytes allocated (useful for benchmarks / diagnostics).
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }
}

impl Default for AstArena {
    fn default() -> Self {
        Self::new()
    }
      }
