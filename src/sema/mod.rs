// src/sema/mod.rs
//! Semantic analysis — three passes over the arena AST.
//!
//! # Pass order
//!
//! ```text
//! Program<'ast>  (parser output)
//!      │
//!      ▼  Pass 1: name_resolution
//! SymbolTable + ResolutionMap
//!      │
//!      ▼  Pass 2: type_infer       (TODO — Phase 2b)
//! TypeTable + expr_types side table
//!      │
//!      ▼  Pass 3: tier_check       (TODO — Phase 2c)
//! Validated SemaContext
//!      │
//!      ▼
//! SemaContext  (handed to interpreter / LLVM backend)
//! ```
//!
//! Each pass appends its errors to the shared `ErrorManager`.
//! The orchestrator stops after any phase that produced errors.
//!
//! # Side-table philosophy
//!
//! The AST is `Copy` and arena-allocated — we cannot add fields to it.
//! Instead, every pass records its discoveries in the `SemaContext` side tables,
//! keyed by the expression/statement `Span`.  Downstream consumers receive both
//! the original `Program` and the `SemaContext`.

pub mod symbol_table;
pub mod sema_context;
pub mod type_table;
pub mod name_resolution;
// pub mod type_infer;   // Phase 2b — uncomment when ready
// pub mod tier_check;   // Phase 2c — uncomment when ready

pub use symbol_table::{DefId, DefKind, Def, SymbolTable, ResolutionMap, Scope, ScopeStack};
pub use sema_context::SemaContext;
pub use type_table::{TypeId, TypeTable, SemaType, ArenaId};

use crate::ast::arena::AstArena;
use crate::ast::root::Program;
use crate::error_management::ErrorManager;

/// Run all semantic analysis passes on `program`.
///
/// Returns a populated `SemaContext` on success, or `Err(errors)` if any
/// pass produced errors that prevent the next pass from running meaningfully.
pub fn analyse<'ast>(
    program: &Program<'ast>,
    _arena:  &'ast AstArena,
    source:  String,
) -> Result<SemaContext, ErrorManager> {
    let mut errors = ErrorManager::new(source);
    let mut ctx    = SemaContext::new();

    // ── Pass 1: Name resolution ──────────────────────────────────
    name_resolution::resolve(program, &mut ctx, &mut errors);

    if errors.has_errors() {
        return Err(errors);
    }

    // ── Pass 2: Type inference  (Phase 2b — not yet implemented) ─
    // type_infer::infer(program, &mut ctx, &mut errors);
    // if errors.has_errors() { return Err(errors); }

    // ── Pass 3: Tier checking   (Phase 2c — not yet implemented) ─
    // tier_check::check(program, &mut ctx, &mut errors);
    // if errors.has_errors() { return Err(errors); }

    Ok(ctx)
}
