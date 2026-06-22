// src/sema/type_infer.rs
//! Pass 2 — Type Inference and Arena Coloring.
//!
//! Two sub-passes:
//!
//!   2a. Signature collection — walk all top-level declarations and record
//!       `def_types` for every function signature, struct, enum, and const
//!       without touching any body. Mirrors the pre-declare step in Pass 1
//!       so bodies can forward-reference function types freely.
//!
//!   2b. Body inference — walk each function / method body doing bidirectional
//!       checking: when an expected type is known (return position, annotated
//!       `let`) we check; otherwise we infer and record.
//!
//! Arena coloring is woven into 2b. When inference enters a `with arena(…)`
//! block it pushes a fresh `ArenaId` onto a stack; every value *constructed*
//! inside that block (struct literals, array literals, tuples) is stamped as
//! `SemaType::ArenaRef { arena, .. }`. Whether an arena-ref is *allowed* at a
//! given site is tier_check's job (Pass 3) — we only tag provenance here.
//!
//! # Known rough edges (not airtight yet)
//!
//! - No occurs-check in unification. Cyclic types will loop; not a real issue
//!   until recursive types appear in user code.
//! - Generic instantiation stubbed — generic calls infer return as Unknown,
//!   no error emitted. Will land with the trait-solver in a later pass.
//! - Method-call chains (`.foo()`) resolve the receiver type but leave the
//!   result Unknown pending field/method tables being built from struct defs.
//! - Multi-element destructuring shares one Span across all bound names;
//!   they all get the collection element type or Unknown. Pre-existing AST gap.
//! - `self` parameter type is Unknown until we thread current_struct_type
//!   through InferCtx — planned for when method resolution lands.

#![allow(dead_code, unused_variables, unused_imports)]

use std::collections::HashMap;

use crate::ast::common::{BinOp, Span, UnaryOp};
use crate::ast::declarations::{
    ConstDecl, EnumDecl, ExtendDecl, FunctionDecl, ImplBlock,
    MethodDecl, Param, ParamKind, ReturnType, StructDecl, StructMember,
    TraitItem, TypeAlias,
};
use crate::ast::expressions::{
    ArgKind, Expr, ExprKind, LambdaBody, LinqClause,
    MatchArmBody, OrElseFallback,
};
use crate::ast::literals::Literal;
use crate::ast::root::{Item, Program};
use crate::ast::statements::{BindingTarget, Block, Stmt, StmtKind};
use crate::ast::types::{Type, TypeKind};
use crate::error_management::{ErrorManager, error_types::TypeError};
use crate::sema::sema_context::SemaContext;
use crate::sema::symbol_table::DefId;
use crate::sema::type_table::{ArenaId, SemaType, TypeId};

// ── Entry point ───────────────────────────────────────────────────

pub fn infer<'ast>(
    program: &Program<'ast>,
    ctx:     &mut SemaContext,
    errors:  &mut ErrorManager,
) {
    let mut icx = InferCtx::new(ctx, errors);
    icx.collect_signatures(program);
    icx.infer_bodies(program);
}

// ── Unifier ───────────────────────────────────────────────────────

/// Simple substitution-based unifier for type variables.
/// Maps type variable indices to concrete TypeIds.
/// No occurs-check — sufficient for Phase 2.
struct Unifier {
    subst: HashMap<u32, TypeId>,
}

impl Unifier {
    fn new() -> Self {
        Unifier { subst: HashMap::new() }
    }

    /// Chase the substitution chain until we hit a concrete type or an
    /// unbound variable.
    fn apply(&self, mut id: TypeId, types: &crate::sema::type_table::TypeTable) -> TypeId {
        loop {
            match types.get(id) {
                SemaType::Var(v) => match self.subst.get(v) {
                    Some(&next) if next != id => id = next,
                    _                         => return id,
                },
                _ => return id,
            }
        }
    }

    /// Bind a free type variable. Returns `false` if it was already bound to
    /// a *different* type (the caller should emit a mismatch error).
    fn bind(&mut self, var: u32, ty: TypeId) -> bool {
        match self.subst.get(&var) {
            None          => { self.subst.insert(var, ty); true }
            Some(&existing) => existing == ty,
        }
    }
}

// ── InferCtx ─────────────────────────────────────────────────────

struct InferCtx<'a> {
    ctx:              &'a mut SemaContext,
    errors:           &'a mut ErrorManager,
    unifier:          Unifier,
    /// Declared return TypeId for the function currently being inferred.
    current_return:   Option<TypeId>,
    /// Whether the current function's return type carries `!`.
    current_fallible: bool,
    /// Stack of active arena ids. The top is the current innermost arena.
    /// Using a stack so nested `with arena` blocks work correctly.
    arena_stack:      Vec<ArenaId>,
    /// Counter for issuing fresh ArenaIds.
    next_arena:       usize,
}

impl<'a> InferCtx<'a> {
    fn new(ctx: &'a mut SemaContext, errors: &'a mut ErrorManager) -> Self {
        InferCtx {
            ctx,
            errors,
            unifier: Unifier::new(),
            current_return: None,
            current_fallible: false,
            arena_stack: Vec::new(),
            next_arena: 0,
        }
    }

    // ── Cheap type constructors ───────────────────────────────────

    fn fresh_var(&mut self) -> TypeId { self.ctx.types.fresh_var() }
    fn unknown(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Unknown) }
    fn void_ty(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Void) }
    fn bool_ty(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Bool) }
    fn int_ty(&mut self)    -> TypeId { self.ctx.types.intern(SemaType::Int) }
    fn str_ty(&mut self)    -> TypeId { self.ctx.types.intern(SemaType::Str) }

    /// Resolve type variables through the current substitution.
    fn apply(&self, id: TypeId) -> TypeId {
        self.unifier.apply(id, &self.ctx.types)
    }

    // ── Arena tracking ────────────────────────────────────────────

    fn push_arena(&mut self) -> ArenaId {
        let id = ArenaId(self.next_arena);
        self.next_arena += 1;
        self.arena_stack.push(id);
        id
    }

    fn pop_arena(&mut self) {
        self.arena_stack.pop();
    }

    fn current_arena(&self) -> Option<ArenaId> {
        self.arena_stack.last().copied()
    }

    /// If we are inside a `with arena` block, wrap `inner_ty` as an ArenaRef.
    /// Called on any freshly *constructed* value (struct lit, array lit, tuple).
    /// Function-call results and field accesses propagate arena-ness through
    /// the type system naturally once ArenaRef appears in a type.
    fn maybe_arena_ref(&mut self, inner_ty: TypeId) -> TypeId {
        match self.current_arena() {
            Some(arena) => self.ctx.types.insert(SemaType::ArenaRef {
                arena,
                mutable: false,
                inner: inner_ty,
            }),
            None => inner_ty,
        }
    }

    // ── AST type → SemaType ───────────────────────────────────────

    /// Convert a syntactic `Type<'ast>` node into a `TypeId` interned in the
    /// type table. Generic args are recursively converted. Unresolved Named
    /// types fall back to Unknown.
    fn ast_type_to_sema<'ast>(&mut self, ty: &Type<'ast>) -> TypeId {
        match &ty.kind {
            TypeKind::Int    => self.ctx.types.intern(SemaType::Int),
            TypeKind::Uint   => self.ctx.types.intern(SemaType::Uint),
            TypeKind::Long   => self.ctx.types.intern(SemaType::Long),
            TypeKind::Ulong  => self.ctx.types.intern(SemaType::Ulong),
            // Short/Ushort widen to Int/Uint for now; byte forms map to i8/u8.
            TypeKind::Short  => self.ctx.types.intern(SemaType::Int),
            TypeKind::Ushort => self.ctx.types.intern(SemaType::Uint),
            TypeKind::Byte   => self.ctx.types.intern(SemaType::I8),
            TypeKind::Ubyte  => self.ctx.types.intern(SemaType::U8),
            TypeKind::Float  => self.ctx.types.intern(SemaType::Float),
            TypeKind::Double => self.ctx.types.intern(SemaType::Double),
            TypeKind::Bool   => self.ctx.types.intern(SemaType::Bool),
            TypeKind::Char   => self.ctx.types.intern(SemaType::Char),
            TypeKind::Str    => self.ctx.types.intern(SemaType::Str),
            TypeKind::Void   => self.ctx.types.intern(SemaType::Void),
            TypeKind::I8     => self.ctx.types.intern(SemaType::I8),
            TypeKind::I16    => self.ctx.types.intern(SemaType::I16),
            TypeKind::I32    => self.ctx.types.intern(SemaType::I32),
            TypeKind::I64    => self.ctx.types.intern(SemaType::I64),
            TypeKind::U8     => self.ctx.types.intern(SemaType::U8),
            TypeKind::U16    => self.ctx.types.intern(SemaType::U16),
            TypeKind::U32    => self.ctx.types.intern(SemaType::U32),
            TypeKind::U64    => self.ctx.types.intern(SemaType::U64),
            TypeKind::F32    => self.ctx.types.intern(SemaType::F32),
            TypeKind::F64    => self.ctx.types.intern(SemaType::F64),
            TypeKind::Isize  => self.ctx.types.intern(SemaType::Isize),
            TypeKind::Usize  => self.ctx.types.intern(SemaType::Usize),

            TypeKind::List(inner) => {
                let elem = inner.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                self.ctx.types.intern(SemaType::List(elem))
            }
            TypeKind::Dictionary(kv) => {
                let (k, v) = kv.map(|(k, v)| (self.ast_type_to_sema(k), self.ast_type_to_sema(v)))
                    .unwrap_or_else(|| (self.fresh_var(), self.fresh_var()));
                self.ctx.types.insert(SemaType::Dictionary(k, v))
            }
            TypeKind::Set(inner) => {
                let elem = inner.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                self.ctx.types.intern(SemaType::Set(elem))
            }
            TypeKind::Queue(inner) => {
                let elem = inner.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                self.ctx.types.insert(SemaType::Queue(elem))
            }
            TypeKind::Stack(inner) => {
                let elem = inner.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                self.ctx.types.insert(SemaType::Stack(elem))
            }

            TypeKind::Named { path, args } => {
                let root = path.first().copied().unwrap_or("");
                let arg_ids: Vec<TypeId> = args.iter()
                    .map(|a| self.ast_type_to_sema(a))
                    .collect();
                match self.ctx.top_level_def(root) {
                    Some(def_id) => self.ctx.types.insert(SemaType::Named { def: def_id, args: arg_ids }),
                    None         => self.unknown(),
                }
            }

            TypeKind::Tuple(fields) => {
                let ids: Vec<TypeId> = fields.iter().map(|f| self.ast_type_to_sema(f)).collect();
                self.ctx.types.insert(SemaType::Tuple(ids))
            }
            TypeKind::Array { len, elem } => {
                let elem_id = self.ast_type_to_sema(elem);
                self.ctx.types.insert(SemaType::Array { len: *len, elem: elem_id })
            }
            TypeKind::Slice(elem) => {
                let elem_id = self.ast_type_to_sema(elem);
                self.ctx.types.intern(SemaType::Slice(elem_id))
            }
            TypeKind::Fallible(inner) => {
                let inner_id = self.ast_type_to_sema(inner);
                self.ctx.types.intern(SemaType::Fallible(inner_id))
            }
            TypeKind::Task(inner) => {
                let inner_id = inner.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.void_ty());
                self.ctx.types.intern(SemaType::Task(inner_id))
            }
            TypeKind::Reference { inner, .. } => {
                // Without a lifetime annotation we model as GcRef for now.
                // tier_check will flag uses in LOW-tier code.
                let inner_id = self.ast_type_to_sema(inner);
                self.ctx.types.insert(SemaType::GcRef(inner_id))
            }
            TypeKind::Optional(inner) => {
                let inner_id = self.ast_type_to_sema(inner);
                self.ctx.types.intern(SemaType::Optional(inner_id))
            }
            TypeKind::Function(ft) => {
                let params: Vec<TypeId> = ft.params.iter()
                    .map(|p| self.ast_type_to_sema(p))
                    .collect();
                let ret = ft.return_type
                    .map(|r| self.ast_type_to_sema(r))
                    .unwrap_or_else(|| self.void_ty());
                self.ctx.types.insert(SemaType::Function {
                    params,
                    return_type: ret,
                    is_fallible: ft.is_fallible,
                })
            }
            TypeKind::Infer => self.fresh_var(),
        }
    }

    /// Convert an optional `ReturnType<'ast>` annotation to `(TypeId, is_fallible)`.
    /// `None` → `(Void, false)`.
    fn return_type_to_sema<'ast>(&mut self, ret: Option<&ReturnType<'ast>>) -> (TypeId, bool) {
        match ret {
            None => (self.void_ty(), false),
            Some(rt) => {
                let inner = self.ast_type_to_sema(rt.ty);
                let ty = if rt.is_fallible {
                    self.ctx.types.intern(SemaType::Fallible(inner))
                } else {
                    inner
                };
                (ty, rt.is_fallible)
            }
        }
    }

    // ── Phase 2a: Signature collection ────────────────────────────

    /// Walk every top-level declaration and build `def_types` entries for
    /// function signatures, struct/enum identities, field types, and consts
    /// — without touching any function body. Lets bodies forward-reference
    /// types freely.
    fn collect_signatures<'ast>(&mut self, program: &Program<'ast>) {
        for item in program.items {
            match item {
                Item::Function(f)  => { self.collect_fn_sig(f); }
                Item::Struct(s)    => { self.collect_struct_sig(s); }
                Item::Enum(e)      => { self.collect_enum_sig(e); }
                Item::Const(c)     => { self.collect_const_sig(c); }
                Item::Impl(i)      => { self.collect_impl_sigs(i); }
                Item::Extend(x)    => { self.collect_extend_sigs(x); }
                Item::Trait(t)     => {
                    for it in t.items {
                        if let TraitItem::DefaultMethod(m) | TraitItem::MethodSig(m) = it {
                            // MethodSig has the same fields we need.
                            // We handle both via their span-recorded DefId.
                        }
                        if let TraitItem::DefaultMethod(m) = it {
                            self.collect_method_sig(m);
                        }
                    }
                }
                Item::TypeAlias(_) => { /* alias expansion deferred */ }
            }
        }
    }

    fn collect_fn_sig<'ast>(&mut self, f: &FunctionDecl<'ast>) -> TypeId {
        let param_tys: Vec<TypeId> = f.params.iter()
            .filter_map(|p| match &p.kind {
                ParamKind::Named { ty, .. } => ty.map(|t| self.ast_type_to_sema(t)),
                _ => None, // self params typed during struct context
            })
            .collect();

        let (ret_ty, is_fallible) = self.return_type_to_sema(f.return_type.as_ref());
        let fn_ty = self.ctx.types.insert(SemaType::Function {
            params: param_tys,
            return_type: ret_ty,
            is_fallible,
        });

        if let Some(def_id) = self.ctx.top_level_def(f.name) {
            self.ctx.set_def_type(def_id, fn_ty);
        }
        fn_ty
    }

    fn collect_method_sig<'ast>(&mut self, m: &MethodDecl<'ast>) -> TypeId {
        let param_tys: Vec<TypeId> = m.params.iter()
            .filter_map(|p| match &p.kind {
                ParamKind::Named { ty, .. } => ty.map(|t| self.ast_type_to_sema(t)),
                _ => None,
            })
            .collect();

        let (ret_ty, is_fallible) = self.return_type_to_sema(m.return_type.as_ref());
        let fn_ty = self.ctx.types.insert(SemaType::Function {
            params: param_tys,
            return_type: ret_ty,
            is_fallible,
        });

        // The method's DefId was recorded by name_resolution against its span.
        if let Some(def_id) = self.ctx.resolutions.get(m.span) {
            self.ctx.set_def_type(def_id, fn_ty);
        }
        fn_ty
    }

    fn collect_struct_sig<'ast>(&mut self, s: &StructDecl<'ast>) {
        let Some(def_id) = self.ctx.top_level_def(s.name) else { return; };

        // The struct type itself: Named { def, args: [] }.
        let struct_ty = self.ctx.types.insert(SemaType::Named { def: def_id, args: vec![] });
        self.ctx.set_def_type(def_id, struct_ty);

        for member in s.members {
            match member {
                StructMember::Field(f) => {
                    let field_ty = self.ast_type_to_sema(f.ty);
                    if let Some(field_def_id) = self.ctx.resolutions.get(f.span) {
                        self.ctx.set_def_type(field_def_id, field_ty);
                    }
                }
                StructMember::Method(m)   => { self.collect_method_sig(m); }
                StructMember::Property(_) => { /* TODO: property types */ }
            }
        }
    }

    fn collect_enum_sig<'ast>(&mut self, e: &EnumDecl<'ast>) {
        let Some(def_id) = self.ctx.top_level_def(e.name) else { return; };
        let enum_ty = self.ctx.types.insert(SemaType::Named { def: def_id, args: vec![] });
        self.ctx.set_def_type(def_id, enum_ty);

        // Each variant has the enum's type.
        for variant in e.variants {
            if let Some(var_def) = self.ctx.resolutions.get(variant.span) {
                self.ctx.set_def_type(var_def, enum_ty);
            }
        }
    }

    fn collect_const_sig<'ast>(&mut self, c: &ConstDecl<'ast>) {
        let Some(def_id) = self.ctx.top_level_def(c.name) else { return; };
        let ty = c.ty.map(|t| self.ast_type_to_sema(t))
            .unwrap_or_else(|| self.fresh_var());
        self.ctx.set_def_type(def_id, ty);
    }

    fn collect_impl_sigs<'ast>(&mut self, i: &ImplBlock<'ast>) {
        for m in i.methods { self.collect_method_sig(m); }
    }

    fn collect_extend_sigs<'ast>(&mut self, x: &ExtendDecl<'ast>) {
        for m in x.methods { self.collect_method_sig(m); }
    }

    // ── Phase 2b: Body inference ──────────────────────────────────

    fn infer_bodies<'ast>(&mut self, program: &Program<'ast>) {
        for item in program.items {
            match item {
                Item::Function(f) => self.infer_function_body(f),
                Item::Struct(s)   => self.infer_struct_bodies(s),
                Item::Const(c)    => self.infer_const_body(c),
                Item::Impl(i)     => {
                    for m in i.methods { self.infer_method_body(m); }
                }
                Item::Extend(x) => {
                    for m in x.methods { self.infer_method_body(m); }
                }
                Item::Trait(t) => {
                    for it in t.items {
                        if let TraitItem::DefaultMethod(m) = it {
                            self.infer_method_body(m);
                        }
                    }
                }
                Item::Enum(_) | Item::TypeAlias(_) => {}
            }
        }
    }

    fn infer_function_body<'ast>(&mut self, f: &FunctionDecl<'ast>) {
        // Seed param def_types so Ident lookups inside the body work.
        for param in f.params { self.seed_param(param); }

        let (ret_ty, is_fallible) = self.return_type_to_sema(f.return_type.as_ref());
        let prev_ret      = self.current_return.replace(ret_ty);
        let prev_fallible = self.current_fallible;
        self.current_fallible = is_fallible;

        self.infer_block(&f.body);

        self.current_return   = prev_ret;
        self.current_fallible = prev_fallible;
    }

    fn infer_method_body<'ast>(&mut self, m: &MethodDecl<'ast>) {
        for param in m.params { self.seed_param(param); }

        let (ret_ty, is_fallible) = self.return_type_to_sema(m.return_type.as_ref());
        let prev_ret      = self.current_return.replace(ret_ty);
        let prev_fallible = self.current_fallible;
        self.current_fallible = is_fallible;

        self.infer_block(&m.body);

        self.current_return   = prev_ret;
        self.current_fallible = prev_fallible;
    }

    /// Record a parameter's type against its DefId so body Ident lookups
    /// can find it via def_types.
    fn seed_param<'ast>(&mut self, param: &Param<'ast>) {
        match &param.kind {
            ParamKind::Named { ty, .. } => {
                let ty_id = ty.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                if let Some(def_id) = self.ctx.resolutions.get(param.span) {
                    self.ctx.set_def_type(def_id, ty_id);
                    self.ctx.set_binding_type(param.span, ty_id);
                }
            }
            _ => {} // self params — TODO when current_struct_type is threaded in
        }
    }

    fn infer_struct_bodies<'ast>(&mut self, s: &StructDecl<'ast>) {
        for member in s.members {
            if let StructMember::Method(m) = member {
                self.infer_method_body(m);
            }
        }
    }

    fn infer_const_body<'ast>(&mut self, c: &ConstDecl<'ast>) {
        let inferred = self.infer_expr(c.value);
        if let Some(def_id) = self.ctx.top_level_def(c.name) {
            if let Some(declared) = self.ctx.def_type(def_id) {
                self.unify(declared, inferred, c.value.span);
            } else {
                self.ctx.set_def_type(def_id, inferred);
            }
        }
    }

    // ── Block ─────────────────────────────────────────────────────

    /// Returns the type of the last expression statement, or Void if the
    /// block is empty or ends on a control-flow / binding statement.
    fn infer_block<'ast>(&mut self, block: &Block<'ast>) -> TypeId {
        let mut last = self.void_ty();
        for stmt in block.stmts {
            last = self.infer_stmt(stmt);
        }
        last
    }

    // ── Statements ────────────────────────────────────────────────

    fn infer_stmt<'ast>(&mut self, stmt: &Stmt<'ast>) -> TypeId {
        match &stmt.kind {

            StmtKind::Let { mutable, binding, ty, value } => {
                let rhs_ty = self.infer_expr(value);
                let bind_ty = match ty {
                    Some(ann) => {
                        let ann_id = self.ast_type_to_sema(ann);
                        self.unify(ann_id, rhs_ty, stmt.span);
                        ann_id
                    }
                    None => rhs_ty,
                };
                self.record_binding(binding, bind_ty, stmt.span);
                self.void_ty()
            }

            StmtKind::Expr(e) => self.infer_expr(e),

            StmtKind::Return(maybe_e) => {
                let ret = maybe_e.map(|e| self.infer_expr(e))
                    .unwrap_or_else(|| self.void_ty());
                if let Some(expected) = self.current_return {
                    // Strip the Fallible wrapper — `return x` inside `fn f() T!`
                    // returns the inner T, not T!.
                    let expected_inner = match self.ctx.types.get(expected) {
                        SemaType::Fallible(inner) => *inner,
                        _ => expected,
                    };
                    self.unify(expected_inner, ret, stmt.span);
                }
                self.void_ty()
            }

            StmtKind::Fail(e) => {
                self.infer_expr(e);
                self.void_ty()
            }

            StmtKind::Break(maybe_e) => {
                if let Some(e) = maybe_e { self.infer_expr(e); }
                self.void_ty()
            }

            StmtKind::Continue => self.void_ty(),

            StmtKind::Defer(e) => {
                self.infer_expr(e);
                self.void_ty()
            }

            StmtKind::If(if_node) => {
                let cond = self.infer_expr(if_node.condition);
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, cond, if_node.condition.span);

                let then_ty = self.infer_block(&if_node.then_block);

                for elif in if_node.elif_branches {
                    let ct = self.infer_expr(elif.condition);
                    self.unify(bool_ty, ct, elif.condition.span);
                    self.infer_block(&elif.block);
                }

                if let Some(else_b) = &if_node.else_block {
                    let else_ty = self.infer_block(else_b);
                    let void_ty = self.void_ty();
                    if then_ty != void_ty && else_ty != void_ty {
                        self.unify(then_ty, else_ty, stmt.span);
                    }
                }

                self.void_ty()
            }

            StmtKind::Match { scrutinee, arms } => {
                self.infer_expr(scrutinee);
                for arm in arms.iter() {
                    if let Some(g) = arm.guard { self.infer_expr(g); }
                    match &arm.body {
                        MatchArmBody::Expr(e)  => { self.infer_expr(e); }
                        MatchArmBody::Block(b) => { self.infer_block(b); }
                    }
                }
                self.void_ty()
            }

            StmtKind::For { binding, iter, body } => {
                let iter_ty = self.infer_expr(iter);
                let elem_ty = self.element_type_of(iter_ty);
                self.record_binding(binding, elem_ty, stmt.span);
                self.infer_block(body);
                self.void_ty()
            }

            StmtKind::While { condition, body } => {
                let cond = self.infer_expr(condition);
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, cond, condition.span);
                self.infer_block(body);
                self.void_ty()
            }

            StmtKind::Loop(body) => {
                self.infer_block(body);
                self.void_ty()
            }

            StmtKind::With { allocator: _, body } => {
                // Push a fresh arena context for the duration of this block.
                // Every value constructed inside gets arena-colored.
                let _arena_id = self.push_arena();
                self.infer_block(body);
                self.pop_arena();
                self.void_ty()
            }

            StmtKind::Using { bindings, body } => {
                for b in bindings.iter() {
                    let ty = self.infer_expr(b.value);
                    if let Some(def_id) = self.ctx.resolutions.get(b.span) {
                        self.ctx.set_def_type(def_id, ty);
                        self.ctx.set_binding_type(b.span, ty);
                    }
                }
                self.infer_block(body);
                self.void_ty()
            }

            StmtKind::Extract { pattern: _, value } => {
                // TODO: split the RHS type across destructure pattern bindings.
                self.infer_expr(value);
                self.void_ty()
            }

            StmtKind::Try { body, catch_binding: _, catch_body } => {
                self.infer_block(body);
                if let Some(cb) = catch_body { self.infer_block(cb); }
                self.void_ty()
            }

            StmtKind::Unsafe(body) => {
                self.infer_block(body);
                self.void_ty()
            }
        }
    }

    /// Record a binding's type into `def_types` and `binding_types`.
    /// For destructure patterns we record the whole collection type against
    /// the statement span; per-element typing is deferred.
    fn record_binding<'ast>(&mut self, target: &BindingTarget<'ast>, ty: TypeId, span: Span) {
        match target {
            BindingTarget::Ident(_) => {
                if let Some(def_id) = self.ctx.resolutions.get(span) {
                    self.ctx.set_def_type(def_id, ty);
                    self.ctx.set_binding_type(span, ty);
                }
            }
            BindingTarget::Destructure(_) => {
                // TODO: split types when per-element spans are available.
                self.ctx.set_binding_type(span, ty);
            }
        }
    }

    /// Get the element type of a collection type.
    /// If the collection type is Unknown or a fresh var, returns a fresh var
    /// so loop bodies can still proceed.
    fn element_type_of(&mut self, collection_ty: TypeId) -> TypeId {
        let resolved = self.apply(collection_ty);
        match self.ctx.types.get(resolved) {
            SemaType::List(e)
            | SemaType::Set(e)
            | SemaType::Queue(e)
            | SemaType::Stack(e)
            | SemaType::Slice(e)          => *e,
            SemaType::Array { elem, .. }  => *elem,
            SemaType::ArenaRef { inner, .. } => {
                // Peel one ArenaRef layer and retry.
                let inner = *inner;
                self.element_type_of(inner)
            }
            _ => self.fresh_var(),
        }
    }

    // ── Expressions ───────────────────────────────────────────────

    /// Infer the type of an expression and record it in `expr_types`.
    fn infer_expr<'ast>(&mut self, expr: &Expr<'ast>) -> TypeId {
        let ty = self.infer_expr_inner(expr);
        self.ctx.set_expr_type(expr.span, ty);
        ty
    }

    fn infer_expr_inner<'ast>(&mut self, expr: &Expr<'ast>) -> TypeId {
        match &expr.kind {

            ExprKind::Lit(lit) => self.infer_literal(lit),

            ExprKind::Ident(_) => {
                if let Some(def_id) = self.ctx.resolutions.get(expr.span) {
                    self.ctx.def_type(def_id).unwrap_or_else(|| self.fresh_var())
                } else {
                    self.unknown()
                }
            }

            ExprKind::SelfExpr => {
                // TODO: thread current_struct_type through InferCtx.
                self.unknown()
            }

            // `x := expr` — resolve rhs, declare x with that type.
            ExprKind::ShortDecl { value, .. } => {
                let ty = self.infer_expr(value);
                if let Some(def_id) = self.ctx.resolutions.get(expr.span) {
                    self.ctx.set_def_type(def_id, ty);
                    self.ctx.set_binding_type(expr.span, ty);
                }
                ty
            }

            ExprKind::Assign { target, value, .. } => {
                let target_ty = self.infer_expr(target);
                let value_ty  = self.infer_expr(value);
                self.unify(target_ty, value_ty, expr.span);
                self.void_ty()
            }

            ExprKind::Pipe { left, right } => {
                self.infer_expr(left);
                // Right side is a function applied to left's value.
                // Simplified: just infer right and use its result.
                self.infer_expr(right)
            }

            ExprKind::BinOp { op, lhs, rhs } => {
                let lhs_ty = self.infer_expr(lhs);
                let rhs_ty = self.infer_expr(rhs);
                self.binop_result(*op, lhs_ty, rhs_ty, expr.span)
            }

            ExprKind::UnaryOp { op, operand } => {
                let op_ty = self.infer_expr(operand);
                match op {
                    UnaryOp::Not    => self.bool_ty(),
                    UnaryOp::Neg    => op_ty,
                    UnaryOp::BitNot => op_ty,
                    UnaryOp::Await  => self.unwrap_task(op_ty, expr.span),
                }
            }

            ExprKind::Call { callee, args } => {
                let callee_ty = self.infer_expr(callee);
                for arg in args.iter() {
                    match &arg.kind {
                        ArgKind::Positional(e)    => { self.infer_expr(e); }
                        ArgKind::Named { value, .. } => { self.infer_expr(value); }
                    }
                }
                self.call_return_type(callee_ty, expr.span)
            }

            ExprKind::Field { target, field: _ } => {
                self.infer_expr(target);
                // TODO: field table lookup once struct types are complete.
                self.unknown()
            }

            ExprKind::Index { target, index } => {
                let coll = self.infer_expr(target);
                self.infer_expr(index);
                self.element_type_of(coll)
            }

            ExprKind::OptionalChain { target, .. } => {
                self.infer_expr(target);
                self.unknown() // TODO: field/method table
            }

            ExprKind::Try(inner) => {
                let inner_ty = self.infer_expr(inner);
                self.unwrap_fallible(inner_ty)
            }

            ExprKind::Await(inner) => {
                let inner_ty = self.infer_expr(inner);
                self.unwrap_task(inner_ty, expr.span)
            }

            ExprKind::As { expr: inner, ty } => {
                self.infer_expr(inner);
                self.ast_type_to_sema(ty)
            }

            ExprKind::Array(elems) => {
                let elem_ty = if elems.is_empty() {
                    self.fresh_var()
                } else {
                    let first = self.infer_expr(elems[0]);
                    for e in &elems[1..] {
                        let ty = self.infer_expr(e);
                        self.unify(first, ty, e.span);
                    }
                    first
                };
                let list_ty = self.ctx.types.intern(SemaType::List(elem_ty));
                self.maybe_arena_ref(list_ty)
            }

            ExprKind::Tuple(elems) => {
                let ids: Vec<TypeId> = elems.iter().map(|e| self.infer_expr(e)).collect();
                let tuple_ty = self.ctx.types.insert(SemaType::Tuple(ids));
                self.maybe_arena_ref(tuple_ty)
            }

            ExprKind::Dict(entries) => {
                let (k0, v0) = if entries.is_empty() {
                    (self.fresh_var(), self.fresh_var())
                } else {
                    (self.infer_expr(entries[0].key), self.infer_expr(entries[0].value))
                };
                for entry in entries.iter().skip(1) {
                    let kt = self.infer_expr(entry.key);
                    let vt = self.infer_expr(entry.value);
                    self.unify(k0, kt, entry.span);
                    self.unify(v0, vt, entry.span);
                }
                let dict_ty = self.ctx.types.insert(SemaType::Dictionary(k0, v0));
                self.maybe_arena_ref(dict_ty)
            }

            // Anonymous object `{ x = 1, y = 2 }` — no structural record type yet.
            ExprKind::AnonObject(fields) => {
                for f in fields.iter() { self.infer_expr(f.value); }
                self.unknown()
            }

            ExprKind::StructLit { path, fields } => {
                for f in fields.iter() { self.infer_expr(f.value); }
                let root = path.first().copied().unwrap_or("");
                let struct_ty = self.ctx.top_level_def(root)
                    .and_then(|id| self.ctx.def_type(id))
                    .unwrap_or_else(|| self.unknown());
                self.maybe_arena_ref(struct_ty)
            }

            ExprKind::Lambda(lambda) => {
                let param_tys: Vec<TypeId> = lambda.params.iter().map(|p| {
                    let ty = p.ty.map(|t| self.ast_type_to_sema(t))
                        .unwrap_or_else(|| self.fresh_var());
                    if let Some(def_id) = self.ctx.resolutions.get(p.span) {
                        self.ctx.set_def_type(def_id, ty);
                        self.ctx.set_binding_type(p.span, ty);
                    }
                    ty
                }).collect();

                let ret_var       = self.fresh_var();
                let prev_ret      = self.current_return.replace(ret_var);
                let prev_fallible = self.current_fallible;
                self.current_fallible = false;

                let body_ty = match &lambda.body {
                    LambdaBody::Block(b) => self.infer_block(b),
                    LambdaBody::Expr(e)  => self.infer_expr(e),
                };
                self.unify(ret_var, body_ty, lambda.span);

                self.current_return   = prev_ret;
                self.current_fallible = prev_fallible;

                let ret_resolved = self.apply(ret_var);
                self.ctx.types.insert(SemaType::Function {
                    params:      param_tys,
                    return_type: ret_resolved,
                    is_fallible: false,
                })
            }

            ExprKind::Block(b) => self.infer_block(b),

            ExprKind::If(if_node) => {
                let cond = self.infer_expr(if_node.condition);
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, cond, if_node.condition.span);

                let then_ty = self.infer_block(&if_node.then_block);
                for elif in if_node.elif_branches {
                    let ct = self.infer_expr(elif.condition);
                    self.unify(bool_ty, ct, elif.condition.span);
                    self.infer_block(&elif.block);
                }
                if let Some(else_b) = &if_node.else_block {
                    let else_ty = self.infer_block(else_b);
                    let void_ty = self.void_ty();
                    if then_ty != void_ty { self.unify(then_ty, else_ty, expr.span); }
                    then_ty
                } else {
                    self.void_ty()
                }
            }

            ExprKind::Match(m) => {
                self.infer_expr(m.scrutinee);
                let void_ty = self.void_ty();
                let mut result = void_ty;
                for arm in m.arms.iter() {
                    if let Some(g) = arm.guard { self.infer_expr(g); }
                    let arm_ty = match &arm.body {
                        MatchArmBody::Expr(e)  => self.infer_expr(e),
                        MatchArmBody::Block(b) => self.infer_block(b),
                    };
                    if arm_ty != void_ty { result = arm_ty; }
                }
                result
            }

            ExprKind::Linq(linq) => {
                self.infer_expr(linq.source);
                for clause in linq.clauses.iter() {
                    match clause {
                        LinqClause::Where(e)
                        | LinqClause::OrderBy { expr: e, .. }
                        | LinqClause::GroupBy(e) => { self.infer_expr(e); }
                        LinqClause::Let { value, .. } => { self.infer_expr(value); }
                    }
                }
                let select_ty = self.infer_expr(linq.select);
                self.ctx.types.intern(SemaType::List(select_ty))
            }

            ExprKind::OrElse { expr: inner, fallback } => {
                let inner_ty = self.infer_expr(inner);
                if let OrElseFallback::Expr(fb) = fallback {
                    let fb_ty = self.infer_expr(fb);
                    self.unify(inner_ty, fb_ty, expr.span);
                }
                // `x or default` unwraps Optional<T> → T.
                self.unwrap_optional(inner_ty)
            }
        }
    }

    // ── Literal typing ────────────────────────────────────────────

    fn infer_literal<'ast>(&mut self, lit: &Literal<'ast>) -> TypeId {
        match lit {
            Literal::Int(_)                       => self.ctx.types.intern(SemaType::Int),
            Literal::Float(_)                     => self.ctx.types.intern(SemaType::Float),
            Literal::Double(_)                    => self.ctx.types.intern(SemaType::Double),
            Literal::Bool(_)                      => self.ctx.types.intern(SemaType::Bool),
            Literal::Char(_)                      => self.ctx.types.intern(SemaType::Char),
            Literal::Null                         => self.ctx.types.intern(SemaType::Null),
            Literal::Str(_) | Literal::VerbatimStr(_)
            | Literal::InterpolatedStr(_)
            | Literal::InterpolatedVerbatimStr(_) => self.ctx.types.intern(SemaType::Str),
        }
    }

    // ── BinOp result type ─────────────────────────────────────────

    fn binop_result(&mut self, op: BinOp, lhs: TypeId, rhs: TypeId, span: Span) -> TypeId {
        match op {
            // Arithmetic + bitwise: operands must unify; result is their type.
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
            | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                self.unify(lhs, rhs, span);
                self.apply(lhs)
            }
            // Comparison: any comparable types; result is bool.
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                self.unify(lhs, rhs, span);
                self.bool_ty()
            }
            // Logical: operands should be bool; result is bool.
            BinOp::And | BinOp::Or => {
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, lhs, span);
                self.unify(bool_ty, rhs, span);
                bool_ty
            }
            // Range: result is Unknown until we add a Range SemaType.
            BinOp::Range | BinOp::RangeIncl => self.unknown(),
        }
    }

    // ── Type unwrapping helpers ───────────────────────────────────

    fn unwrap_task(&mut self, ty: TypeId, span: Span) -> TypeId {
        let resolved = self.apply(ty);
        match self.ctx.types.get(resolved) {
            SemaType::Task(inner) => *inner,
            SemaType::Var(_) => {
                // Constrain: ty = Task<fresh_var>.
                let inner = self.fresh_var();
                let task  = self.ctx.types.intern(SemaType::Task(inner));
                self.unify(resolved, task, span);
                inner
            }
            _ => {
                self.errors.add_type_error(TypeError::AwaitOnNonTask {
                    found: self.display_type(ty),
                    span,
                });
                TypeId::ERROR
            }
        }
    }

    fn unwrap_fallible(&mut self, ty: TypeId) -> TypeId {
        let resolved = self.apply(ty);
        match self.ctx.types.get(resolved) {
            SemaType::Fallible(inner) => *inner,
            _ => resolved,
        }
    }

    fn unwrap_optional(&mut self, ty: TypeId) -> TypeId {
        let resolved = self.apply(ty);
        match self.ctx.types.get(resolved) {
            SemaType::Optional(inner) => *inner,
            _ => resolved,
        }
    }

    /// Extract the return type of a function type. Emits an error and returns
    /// ERROR if the callee is definitely not callable.
    fn call_return_type(&mut self, callee_ty: TypeId, span: Span) -> TypeId {
        let resolved = self.apply(callee_ty);
        match self.ctx.types.get(resolved) {
            SemaType::Function { return_type, .. } => *return_type,
            SemaType::Var(_) => {
                // Callee type not known yet — return a fresh var.
                self.fresh_var()
            }
            SemaType::Unknown => self.unknown(),
            _ => {
                self.errors.add_type_error(TypeError::NoSuchMethod {
                    method:  "<call>".into(),
                    on_type: self.display_type(callee_ty),
                    span,
                });
                TypeId::ERROR
            }
        }
    }

    // ── Unification ───────────────────────────────────────────────

    fn unify(&mut self, a: TypeId, b: TypeId, span: Span) {
        let a = self.apply(a);
        let b = self.apply(b);
        if a == b { return; }

        // Bind type variables.
        let a_var = matches!(self.ctx.types.get(a), SemaType::Var(_));
        let b_var = matches!(self.ctx.types.get(b), SemaType::Var(_));
        if a_var {
            if let SemaType::Var(v) = *self.ctx.types.get(a) {
                self.unifier.bind(v, b);
                return;
            }
        }
        if b_var {
            if let SemaType::Var(v) = *self.ctx.types.get(b) {
                self.unifier.bind(v, a);
                return;
            }
        }

        // Unknown / ERROR absorb without cascading errors.
        let a_is_unk = matches!(self.ctx.types.get(a), SemaType::Unknown);
        let b_is_unk = matches!(self.ctx.types.get(b), SemaType::Unknown);
        if a_is_unk || b_is_unk { return; }
        if a == TypeId::ERROR || b == TypeId::ERROR { return; }

        // Structural compatibility for composite types.
        if self.structurally_compatible(a, b) { return; }

        // Genuine mismatch.
        self.errors.add_type_error(TypeError::TypeMismatch {
            expected:   self.display_type(a),
            found:      self.display_type(b),
            span,
            because_of: None,
        });
    }

    /// Returns true if two concrete types are structurally compatible and
    /// recursively unifies their type arguments. Handles the common
    /// wrapper types so unify doesn't always have to reach the error branch.
    fn structurally_compatible(&mut self, a: TypeId, b: TypeId) -> bool {
        // We must extract the inner ids before calling self.unify recursively
        // to avoid a borrow conflict on self.ctx.types.
        let inner = match (self.ctx.types.get(a), self.ctx.types.get(b)) {
            (SemaType::List(ia),      SemaType::List(ib))      => Some((*ia, *ib)),
            (SemaType::Optional(ia),  SemaType::Optional(ib))  => Some((*ia, *ib)),
            (SemaType::Fallible(ia),  SemaType::Fallible(ib))  => Some((*ia, *ib)),
            (SemaType::Task(ia),      SemaType::Task(ib))      => Some((*ia, *ib)),
            (SemaType::Slice(ia),     SemaType::Slice(ib))     => Some((*ia, *ib)),
            (SemaType::GcRef(ia),     SemaType::GcRef(ib))     => Some((*ia, *ib)),
            (SemaType::Named { def: da, .. }, SemaType::Named { def: db, .. }) => {
                if da == db { return true; } else { return false; }
            }
            _ => None,
        };
        if let Some((ia, ib)) = inner {
            self.unify(ia, ib, Span::at(0));
            true
        } else {
            false
        }
    }

    // ── Display helper ────────────────────────────────────────────

    fn display_type(&self, id: TypeId) -> String {
        if id == TypeId::ERROR { return "<error>".into(); }
        self.ctx.types.get(id).display(&self.ctx.types)
    }
}
