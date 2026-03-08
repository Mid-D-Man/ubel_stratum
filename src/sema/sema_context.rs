// src/sema/sema_context.rs
//! `SemaContext` — the accumulated output of all semantic analysis passes.
//!
//! Instead of modifying the (immutable, arena-allocated) AST, every pass
//! records its discoveries in side tables stored here.
//! The interpreter and later the LLVM backend receive both the original `Program`
//! and a `SemaContext` and use them together.
//!
//! # Side table layout
//!
//! | Table              | Key        | Value              | Filled by        |
//! |--------------------|------------|--------------------|------------------|
//! | `resolutions`      | use `Span` | `DefId`            | name_resolution  |
//! | `expr_types`       | `Span`     | `TypeId`           | type_infer       |
//! | `symbols`          | —          | `SymbolTable`      | name_resolution  |
//! | `types`            | —          | `TypeTable`        | type_infer       |

#![allow(dead_code)]

use std::collections::HashMap;
use crate::ast::common::Span;
use crate::sema::symbol_table::{DefId, ResolutionMap, SymbolTable};
use crate::sema::type_table::{TypeId, TypeTable};

/// The complete output of semantic analysis for one compilation unit.
///
/// Pass this alongside `Program<'ast>` to any downstream consumer.
#[derive(Debug, Default)]
pub struct SemaContext {
    /// Symbol table: all definitions in the program.
    pub symbols: SymbolTable,

    /// Resolution map: every identifier use → the definition it refers to.
    pub resolutions: ResolutionMap,

    /// Type table: all types encountered / inferred.
    pub types: TypeTable,

    /// Inferred type for every expression node, keyed by the expression's `Span`.
    ///
    /// After type inference this is fully populated for all non-error expressions.
    pub expr_types: HashMap<Span, TypeId>,

    /// Inferred type for every let-binding / parameter, keyed by the binding's `Span`.
    pub binding_types: HashMap<Span, TypeId>,
}

impl SemaContext {
    pub fn new() -> Self { SemaContext::default() }

    // ── Resolution helpers ─────────────────────────────────────────

    /// Look up the `DefId` for an identifier use at `span`.
    /// Returns `None` if the identifier was unresolved (error already reported).
    pub fn resolution(&self, span: Span) -> Option<DefId> {
        self.resolutions.get(span)
    }

    /// Convenience: look up the `Def` for an identifier use at `span`.
    pub fn def_at(&self, span: Span) -> Option<&crate::sema::symbol_table::Def> {
        self.resolutions.get(span).map(|id| self.symbols.lookup(id))
    }

    // ── Type helpers ───────────────────────────────────────────────

    /// The inferred type of the expression at `span`.
    /// Returns `None` if this expression had a type error or was not visited.
    pub fn expr_type(&self, span: Span) -> Option<TypeId> {
        self.expr_types.get(&span).copied()
    }

    /// Record the inferred type of an expression.
    pub fn set_expr_type(&mut self, span: Span, ty: TypeId) {
        self.expr_types.insert(span, ty);
    }

    /// Record the inferred type of a binding (let / parameter).
    pub fn set_binding_type(&mut self, span: Span, ty: TypeId) {
        self.binding_types.insert(span, ty);
    }
}
