// src/sema/mod.rs
//! Semantic analysis — three passes over the arena AST.
//!
//! Pass order:
//!   1. name_resolution  → SymbolTable, ResolutionMap, top_level map
//!   2. type_infer       → TypeTable, expr_types, def_types, arena coloring
//!   3. tier_check       → enforces HIGH/MID/LOW cross-tier rules
//!
//! Each pass appends errors to a shared ErrorManager and the orchestrator
//! stops after any phase that produced errors.

pub mod symbol_table;
pub mod sema_context;
pub mod type_table;
pub mod name_resolution;
pub mod type_infer;
pub mod tier_check;

pub use symbol_table::{DefId, DefKind, Def, SymbolTable, ResolutionMap, Scope, ScopeStack};
pub use sema_context::SemaContext;
pub use type_table::{TypeId, TypeTable, SemaType, ArenaId};

use crate::ast::arena::AstArena;
use crate::ast::root::Program;
use crate::error_management::ErrorManager;

/// Run all semantic analysis passes on `program`.
/// Returns a populated `SemaContext` on success, `Err(ErrorManager)` on failure.
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

    // ── Pass 3: Tier rule enforcement ────────────────────────────
    tier_check::check(program, &ctx, &mut errors);
    if errors.has_errors() {
        return Err(errors);
    }

    Ok(ctx)
}
