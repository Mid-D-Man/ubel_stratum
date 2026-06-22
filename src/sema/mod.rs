// src/sema/mod.rs
//! Semantic analysis — three passes over the arena AST.

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
pub fn analyse<'ast>(
    program: &Program<'ast>,
    _arena:  &'ast AstArena,
    source:  String,
) -> Result<SemaContext, ErrorManager> {
    let mut errors = ErrorManager::new(source);
    let mut ctx    = SemaContext::new();

    // Pass 1: name resolution
    name_resolution::resolve(program, &mut ctx, &mut errors);
    if errors.has_errors() {
        return Err(errors);
    }

    // Pass 2: type inference + arena coloring
    type_infer::infer(program, &mut ctx, &mut errors);
    if errors.has_errors() {
        return Err(errors);
    }

    // Pass 3: tier enforcement
    tier_check::check(program, &mut ctx, &mut errors);
    if errors.has_errors() {
        return Err(errors);
    }

    Ok(ctx)
    }
