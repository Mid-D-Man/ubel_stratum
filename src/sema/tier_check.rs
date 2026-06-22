// src/sema/tier_check.rs
//! Pass 3 — Tier Enforcement.
//!
//! Walks the fully-resolved, fully-typed AST and enforces the memory-tier
//! rules that only make sense after both name-resolution and type-inference
//! have run.
//!
//! # Rules checked
//!
//! | Rule                                                  | Error                    |
//! |-------------------------------------------------------|--------------------------|
//! | `with arena` inside non-`@tier(mid)` function        | `ArenaInWrongTier`       |
//! | `await` inside non-`@tier(high)` function            | `AwaitInWrongTier`       |
//! | `async fn` not `@tier(high)`                         | `AsyncFunctionNotHigh`   |
//! | LINQ query inside non-`@tier(high)` function         | `LinqInWrongTier`        |
//! | `@tier(low)` calling `@tier(high)` or `@tier(mid)`  | `IllegalTierCall`        |
//! | `@tier(mid)` calling `@tier(high)`                   | `IllegalTierCall`        |
//! | Value with `ArenaRef` type escapes to `@tier(high)`  | `ArenaRefEscapesBoundary`|

#![allow(dead_code)]

use crate::ast::common::{Span, TierAnnotation};
use crate::ast::declarations::{
    ExtendDecl, FunctionDecl, ImplBlock, MethodDecl,
    StructDecl, StructMember, TraitDecl, TraitItem,
};
use crate::ast::expressions::{
    ArgKind, Expr, ExprKind, LambdaBody, LinqClause,
    MatchArmBody, OrElseFallback,
};
use crate::ast::root::{Item, Program};
use crate::ast::statements::{AllocatorKind, BindingTarget, Block, Stmt, StmtKind};
use crate::error_management::{ErrorManager, error_types::TypeError};
use crate::sema::sema_context::SemaContext;
use crate::sema::symbol_table::{DefId, DefKind};

// ── Entry point ────────────────────────────────────────────────────────────

pub fn check<'ast>(
    program: &Program<'ast>,
    ctx:     &mut SemaContext,
    errors:  &mut ErrorManager,
) {
    let mut checker = TierChecker::new(ctx, errors);
    checker.check_program(program);
}

// ── TierChecker ───────────────────────────────────────────────────────────

struct TierChecker<'a> {
    ctx:           &'a mut SemaContext,
    errors:        &'a mut ErrorManager,
    current_tier:  TierAnnotation,
    current_async: bool,
}

impl<'a> TierChecker<'a> {
    fn new(ctx: &'a mut SemaContext, errors: &'a mut ErrorManager) -> Self {
        TierChecker {
            ctx,
            errors,
            current_tier:  TierAnnotation::High,
            current_async: false,
        }
    }

    // ── Top-level ─────────────────────────────────────────────────────────

    fn check_program<'ast>(&mut self, program: &Program<'ast>) {
        for item in program.items {
            self.check_item(item);
        }
    }

    fn check_item<'ast>(&mut self, item: &Item<'ast>) {
        match item {
            Item::Function(f) => self.check_function(f),
            Item::Struct(s)   => self.check_struct(s),
            Item::Impl(i)     => self.check_impl(i),
            Item::Extend(x)   => self.check_extend(x),
            Item::Trait(t)    => self.check_trait(t),
            _                 => {}
        }
    }

    // ── Function / method ─────────────────────────────────────────────────

    fn check_function<'ast>(&mut self, f: &FunctionDecl<'ast>) {
        // async fn must be @tier(high)
        if f.is_async && f.tier != TierAnnotation::High {
            self.errors.add_type_error(TypeError::AsyncFunctionNotHigh {
                actual: f.tier,
                span:   f.span,
            });
        }

        // MID-tier return type must not contain ArenaRef
        if f.tier == TierAnnotation::Mid {
            if let Some(ret) = &f.return_type {
                self.check_return_for_arena_escape(ret.ty.span, f.span);
            }
        }

        let prev_tier  = self.current_tier;
        let prev_async = self.current_async;
        self.current_tier  = f.tier;
        self.current_async = f.is_async;
        self.check_block(&f.body);
        self.current_tier  = prev_tier;
        self.current_async = prev_async;
    }

    fn check_method<'ast>(&mut self, m: &MethodDecl<'ast>) {
        if m.is_async && m.tier != TierAnnotation::High {
            self.errors.add_type_error(TypeError::AsyncFunctionNotHigh {
                actual: m.tier,
                span:   m.span,
            });
        }

        if m.tier == TierAnnotation::Mid {
            if let Some(ret) = &m.return_type {
                self.check_return_for_arena_escape(ret.ty.span, m.span);
            }
        }

        let prev_tier  = self.current_tier;
        let prev_async = self.current_async;
        self.current_tier  = m.tier;
        self.current_async = m.is_async;
        self.check_block(&m.body);
        self.current_tier  = prev_tier;
        self.current_async = prev_async;
    }

    fn check_struct<'ast>(&mut self, s: &StructDecl<'ast>) {
        for member in s.members {
            if let StructMember::Method(m) = member {
                self.check_method(m);
            }
        }
    }

    fn check_impl<'ast>(&mut self, i: &ImplBlock<'ast>) {
        for m in i.methods { self.check_method(m); }
    }

    fn check_extend<'ast>(&mut self, x: &ExtendDecl<'ast>) {
        for m in x.methods { self.check_method(m); }
    }

    fn check_trait<'ast>(&mut self, t: &TraitDecl<'ast>) {
        for item in t.items {
            if let TraitItem::DefaultMethod(m) = item {
                self.check_method(m);
            }
        }
    }

    // ── Block / statement ─────────────────────────────────────────────────

    fn check_block<'ast>(&mut self, block: &Block<'ast>) {
        for stmt in block.stmts { self.check_stmt(stmt); }
    }

    fn check_stmt<'ast>(&mut self, stmt: &Stmt<'ast>) {
        match &stmt.kind {

            StmtKind::With { allocator, body } => {
                // `with arena(...)` only valid in @tier(mid)
                if matches!(allocator, AllocatorKind::Arena(_))
                    && self.current_tier != TierAnnotation::Mid
                {
                    self.errors.add_type_error(TypeError::ArenaInWrongTier {
                        actual: self.current_tier,
                        span:   stmt.span,
                    });
                }
                self.check_block(body);
            }

            StmtKind::Return(maybe_e) => {
                if let Some(e) = maybe_e {
                    self.check_expr(e);
                    // MID-tier: returning an arena-ref would escape the boundary
                    if self.current_tier == TierAnnotation::Mid {
                        self.check_expr_for_arena_escape(e.span, stmt.span);
                    }
                }
            }

            StmtKind::Let { value, .. } => {
                self.check_expr(value);
                // HIGH-tier: a let-binding holding an arena-ref escapes
                if self.current_tier == TierAnnotation::High {
                    self.check_expr_for_arena_escape(value.span, stmt.span);
                }
            }

            StmtKind::Expr(e)   => self.check_expr(e),
            StmtKind::Fail(e)   => self.check_expr(e),
            StmtKind::Defer(e)  => self.check_expr(e),
            StmtKind::Break(e)  => { if let Some(e) = e { self.check_expr(e); } }
            StmtKind::Continue  => {}

            StmtKind::If(if_node) => {
                self.check_expr(if_node.condition);
                self.check_block(&if_node.then_block);
                for elif in if_node.elif_branches {
                    self.check_expr(elif.condition);
                    self.check_block(&elif.block);
                }
                if let Some(else_b) = &if_node.else_block {
                    self.check_block(else_b);
                }
            }

            StmtKind::Match { scrutinee, arms } => {
                self.check_expr(scrutinee);
                for arm in arms.iter() {
                    if let Some(g) = arm.guard { self.check_expr(g); }
                    match &arm.body {
                        MatchArmBody::Expr(e)  => self.check_expr(e),
                        MatchArmBody::Block(b) => self.check_block(b),
                    }
                }
            }

            StmtKind::For { iter, body, .. } => {
                self.check_expr(iter);
                self.check_block(body);
            }

            StmtKind::While { condition, body } => {
                self.check_expr(condition);
                self.check_block(body);
            }

            StmtKind::Loop(body)   => self.check_block(body),

            StmtKind::Using { bindings, body } => {
                for b in bindings.iter() { self.check_expr(b.value); }
                self.check_block(body);
            }

            StmtKind::Extract { value, .. } => self.check_expr(value),

            StmtKind::Try { body, catch_body, .. } => {
                self.check_block(body);
                if let Some(cb) = catch_body { self.check_block(cb); }
            }

            StmtKind::Unsafe(body) => self.check_block(body),
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────

    fn check_expr<'ast>(&mut self, expr: &Expr<'ast>) {
        match &expr.kind {

            ExprKind::Await(inner) => {
                if self.current_tier != TierAnnotation::High {
                    self.errors.add_type_error(TypeError::AwaitInWrongTier {
                        actual: self.current_tier,
                        span:   expr.span,
                    });
                }
                self.check_expr(inner);
            }

            ExprKind::Linq(_) => {
                if self.current_tier != TierAnnotation::High {
                    self.errors.add_type_error(TypeError::LinqInWrongTier {
                        actual: self.current_tier,
                        span:   expr.span,
                    });
                }
                // Don't recurse — root error is sufficient, avoids cascades.
            }

            ExprKind::Call { callee, args } => {
                self.check_expr(callee);
                for arg in args.iter() {
                    match &arg.kind {
                        ArgKind::Positional(e)       => self.check_expr(e),
                        ArgKind::Named { value, .. } => self.check_expr(value),
                    }
                }
                self.check_call_tier(callee.span, expr.span);
            }

            ExprKind::BinOp { lhs, rhs, .. } => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            ExprKind::UnaryOp { operand, .. } => self.check_expr(operand),

            ExprKind::Assign { target, value, .. } => {
                self.check_expr(target);
                self.check_expr(value);
            }

            ExprKind::Pipe { left, right } => {
                self.check_expr(left);
                self.check_expr(right);
            }

            ExprKind::Field { target, .. }
            | ExprKind::OptionalChain { target, .. } => self.check_expr(target),

            ExprKind::Index { target, index } => {
                self.check_expr(target);
                self.check_expr(index);
            }

            ExprKind::Try(e) => self.check_expr(e),
            ExprKind::As { expr: e, .. } => self.check_expr(e),

            ExprKind::Tuple(es) | ExprKind::Array(es) => {
                for e in es.iter() { self.check_expr(e); }
            }

            ExprKind::Dict(entries) => {
                for entry in entries.iter() {
                    self.check_expr(entry.key);
                    self.check_expr(entry.value);
                }
            }

            ExprKind::AnonObject(fields) => {
                for f in fields.iter() { self.check_expr(f.value); }
            }

            ExprKind::StructLit { fields, .. } => {
                for f in fields.iter() { self.check_expr(f.value); }
            }

            ExprKind::Lambda(lambda) => {
                // Lambdas inherit the enclosing tier.
                match &lambda.body {
                    LambdaBody::Block(b) => self.check_block(b),
                    LambdaBody::Expr(e)  => self.check_expr(e),
                }
            }

            ExprKind::Block(b) => self.check_block(b),

            ExprKind::If(if_node) => {
                self.check_expr(if_node.condition);
                self.check_block(&if_node.then_block);
                for elif in if_node.elif_branches {
                    self.check_expr(elif.condition);
                    self.check_block(&elif.block);
                }
                if let Some(else_b) = &if_node.else_block {
                    self.check_block(else_b);
                }
            }

            ExprKind::Match(m) => {
                self.check_expr(m.scrutinee);
                for arm in m.arms.iter() {
                    if let Some(g) = arm.guard { self.check_expr(g); }
                    match &arm.body {
                        MatchArmBody::Expr(e)  => self.check_expr(e),
                        MatchArmBody::Block(b) => self.check_block(b),
                    }
                }
            }

            ExprKind::OrElse { expr: e, fallback } => {
                self.check_expr(e);
                if let OrElseFallback::Expr(fb) = fallback {
                    self.check_expr(fb);
                }
            }

            ExprKind::ShortDecl { value, .. } => self.check_expr(value),

            // Leaves
            ExprKind::Lit(_) | ExprKind::Ident(_) | ExprKind::SelfExpr => {}
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Check whether a call from the current tier to `callee_span`'s definition
    /// is a legal cross-tier call.
    fn check_call_tier(&mut self, callee_span: Span, call_span: Span) {
        let callee_id = match self.ctx.resolutions.get(callee_span) {
            Some(id) => id,
            None     => return,
        };

        let callee_tier = match self.def_tier(callee_id) {
            Some(t) => t,
            None    => return,
        };

        let callee_name = self.ctx.symbols.lookup(callee_id).name.clone();

        match (self.current_tier, callee_tier) {
            // LOW cannot call HIGH or MID
            (TierAnnotation::Low, TierAnnotation::High)
            | (TierAnnotation::Low, TierAnnotation::Mid) => {
                self.errors.add_type_error(TypeError::IllegalTierCall {
                    caller_tier: self.current_tier,
                    callee_tier,
                    callee_name,
                    span: call_span,
                });
            }
            // MID cannot call HIGH
            (TierAnnotation::Mid, TierAnnotation::High) => {
                self.errors.add_type_error(TypeError::IllegalTierCall {
                    caller_tier: self.current_tier,
                    callee_tier,
                    callee_name,
                    span: call_span,
                });
            }
            _ => {}
        }
    }

    /// Check whether the expression at `expr_span` has an `ArenaRef` type,
    /// which would escape the arena boundary if used in this context.
    fn check_expr_for_arena_escape(&mut self, expr_span: Span, stmt_span: Span) {
        let type_id = match self.ctx.expr_types.get(&expr_span).copied() {
            Some(id) => id,
            None     => return,
        };
        if self.ctx.types.get(type_id).contains_arena_ref(&self.ctx.types) {
            let type_name = self.ctx.types.get(type_id).display(&self.ctx.types);
            self.errors.add_type_error(TypeError::ArenaRefEscapesBoundary {
                escaped_type: type_name,
                span:         stmt_span,
            });
        }
    }

    /// Check whether a type-annotation span has an ArenaRef type (for return types).
    fn check_return_for_arena_escape(&mut self, type_span: Span, fn_span: Span) {
        let type_id = match self.ctx.expr_types.get(&type_span).copied() {
            Some(id) => id,
            None     => return,
        };
        if self.ctx.types.get(type_id).contains_arena_ref(&self.ctx.types) {
            let type_name = self.ctx.types.get(type_id).display(&self.ctx.types);
            self.errors.add_type_error(TypeError::MidReturnContainsArenaRef {
                return_type: type_name,
                span:        fn_span,
            });
        }
    }

    /// Look up the tier of a definition. Returns `None` for non-function defs.
    fn def_tier(&self, id: DefId) -> Option<TierAnnotation> {
        let def = self.ctx.symbols.lookup(id);
        match &def.kind {
            DefKind::Function { tier, .. } => Some(*tier),
            DefKind::Method   { tier, .. } => Some(*tier),
            _                              => None,
        }
    }
  }
