// src/sema/mod.rs
//! Semantic analysis — three passes over the arena AST.
//!
//! # Pass order
//!
//! ```text
//! Program<'ast>  (parser output)
//!      │
//!      ▼  Pass 1: name_resolution
//! SymbolTable + ResolutionMap + top_level map
//!      │
//!      ▼  Pass 2: type_infer
//! TypeTable + expr_types + def_types + arena coloring
//!      │
//!      ▼  Pass 3: tier_check  (Phase 2c — TODO)
//! Validated SemaContext
//!      │
//!      ▼
//! SemaContext  (handed to interpreter / LLVM backend)
//! ```
//!
//! Each pass appends its errors to a shared `ErrorManager`.
//! The orchestrator stops after any phase that produced errors.

pub mod symbol_table;
pub mod sema_context;
pub mod type_table;
pub mod name_resolution;
pub mod type_infer;
// pub mod tier_check;  // Phase 2c — uncomment when ready

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

    // ── Pass 2: Type inference + arena coloring ──────────────────
    type_infer::infer(program, &mut ctx, &mut errors);
    if errors.has_errors() {
        return Err(errors);
    }

    // ── Pass 3: Tier checking (Phase 2c — not yet implemented) ───
    // tier_check::check(program, &mut ctx, &mut errors);
    // if errors.has_errors() { return Err(errors); }

    Ok(ctx)
}
