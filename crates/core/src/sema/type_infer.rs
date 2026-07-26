// src/sema/type_infer.rs
//! Pass 2 — Type Inference and Arena Coloring.
//!
//! 2a. Signature collection — records def_types for every top-level
//!     declaration without touching bodies, enabling forward references.
//!
//! 2b. Body inference — bidirectional checking inside each body.
//!     Expected type known → check; otherwise infer and record.
//!
//! Arena coloring is woven into 2b. Entering a `with arena(…)` block
//! pushes a fresh ArenaId; every constructed value inside gets stamped
//! SemaType::ArenaRef.
//!
//! ## Gap 2 — escape boundary enforcement (docs/MEMORY_MODEL.md §6)
//!
//! Whether an ArenaRef is *allowed* to flow to a given site is also
//! enforced here, not in tier_check — Pass 2 already tracks live arena
//! scope via `arena_stack`/`maybe_arena_ref`, so this is where the
//! answer is cheapest to compute, right as each site is inferred:
//!
//! - **Assignment** — `check_assign_arena_escape`, called from the
//!   `ExprKind::Assign` arm, compares the *specific* `ArenaId` (never
//!   just tagged/untagged — see §3) a value carries against the target's
//!   home arena: an `Ident`'s original declared type, or a `Field`/
//!   `Index` target's receiver/container's own tag.
//! - **Struct field / indexed storage** — the same function; no struct
//!   field can ever be declared `ArenaRef` (no surface syntax for it),
//!   so a receiver that isn't itself tagged has a home arena of `None`.
//! - **Closure capture** — a lambda built inside a `with arena(…)` block
//!   is conservatively `maybe_arena_ref`-tagged too, the same as any
//!   other constructed value. Lexical scoping guarantees a lambda can
//!   only reference an arena-scoped local from inside that local's own
//!   block, so this over-approximates (a closure that captures nothing
//!   arena-related still gets tagged) but never under-approximates —
//!   once tagged, the lambda value rides the same assignment/return
//!   checks as any other arena-scoped value, no separate free-variable
//!   capture analysis needed.
//! - **Return position** was already caught before Gap 2, incidentally,
//!   via a generic `TypeMismatch` in `unify` (declared return type vs.
//!   actual `ArenaRef`-tagged value). `unify` now recognizes this
//!   specific shape and reports the more precise `ArenaRefEscapesBoundary`
//!   instead — see `arena_mismatch_side`.
//!
//! # Known rough edges (not airtight yet)
//! - No occurs-check in unification.
//! - Generic instantiation returns Unknown; no error emitted.
//! - Method-call chains leave result Unknown pending field/method tables.
//! - Multi-element destructuring shares one Span; all get collection elem type.
//! - `self` param type is Unknown until current_struct_type is threaded in.

#![allow(dead_code, unused_variables, unused_imports)]

use std::collections::HashMap;

use crate::ast::common::{BinOp, Span, UnaryOp};
use crate::ast::declarations::{
    ConstDecl, EnumDecl, ExtendDecl, FunctionDecl, ImplBlock,
    MethodDecl, Param, ParamKind, ReturnType, StructDecl, StructMember,
    TraitItem, TypeAlias, MethodSig,
};
use crate::ast::expressions::{
    ArgKind, Expr, ExprKind, LambdaBody, LinqClause,
    MatchArmBody, OrElseFallback,
};
use crate::ast::literals::Literal;
use crate::ast::root::{Item, Program};
use crate::ast::statements::{BindingTarget, Block, Stmt, StmtKind};
use crate::ast::types::{Type, TypeKind};
use crate::error_management::{ErrorManager, errors::{TypeError, TierError}};
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

struct Unifier {
    subst: HashMap<u32, TypeId>,
}

impl Unifier {
    fn new() -> Self { Unifier { subst: HashMap::new() } }

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

    fn bind(&mut self, var: u32, ty: TypeId) -> bool {
        match self.subst.get(&var) {
            None            => { self.subst.insert(var, ty); true }
            Some(&existing) => existing == ty,
        }
    }
}

// ── InferCtx ─────────────────────────────────────────────────────

struct InferCtx<'a> {
    ctx:              &'a mut SemaContext,
    errors:           &'a mut ErrorManager,
    unifier:          Unifier,
    current_return:   Option<TypeId>,
    current_fallible: bool,
    arena_stack:      Vec<ArenaId>,
    next_arena:       usize,
}

impl<'a> InferCtx<'a> {
    fn new(ctx: &'a mut SemaContext, errors: &'a mut ErrorManager) -> Self {
        InferCtx {
            ctx,
            errors,
            unifier:          Unifier::new(),
            current_return:   None,
            current_fallible: false,
            arena_stack:      Vec::new(),
            next_arena:       0,
        }
    }

    // ── Cheap constructors ────────────────────────────────────────

    fn fresh_var(&mut self) -> TypeId { self.ctx.types.fresh_var() }
    fn unknown(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Unknown) }
    fn void_ty(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Void) }
    fn bool_ty(&mut self)   -> TypeId { self.ctx.types.intern(SemaType::Bool) }
    fn int_ty(&mut self)    -> TypeId { self.ctx.types.intern(SemaType::Int) }
    fn str_ty(&mut self)    -> TypeId { self.ctx.types.intern(SemaType::Str) }

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

    fn pop_arena(&mut self) { self.arena_stack.pop(); }

    fn current_arena(&self) -> Option<ArenaId> {
        self.arena_stack.last().copied()
    }

    /// Wrap `inner_ty` as an ArenaRef if inside a `with arena` block.
    /// Called on freshly *constructed* values only.
    fn maybe_arena_ref(&mut self, inner_ty: TypeId) -> TypeId {
        match self.current_arena() {
            Some(arena) => self.ctx.types.insert(SemaType::ArenaRef {
                arena,
                mutable: false,
                inner:   inner_ty,
            }),
            None => inner_ty,
        }
    }

    /// GAP 2 — resolve `ty` and return `Some(arena)` only if its
    /// *outermost* layer is `SemaType::ArenaRef`. This is deliberately
    /// shallow (unlike `SemaType::contains_arena_ref`, which recurses):
    /// it answers "is this specific reference itself bound to an arena",
    /// which is what determines a binding's or receiver's *own* home
    /// arena for escape comparisons — not "does this type contain an
    /// arena reference somewhere inside it."
    fn top_level_arena(&mut self, ty: TypeId) -> Option<ArenaId> {
        let resolved = self.apply(ty);
        match self.ctx.types.get(resolved) {
            SemaType::ArenaRef { arena, .. } => Some(*arena),
            _ => None,
        }
    }

    /// GAP 2 — report `ty` as a value that has crossed an arena boundary
    /// it shouldn't have.
    fn arena_escape(&mut self, ty: TypeId, span: Span) {
        let escaped_type = self.display_type(ty);
        self.errors.add_tier_error(TierError::ArenaRefEscapesBoundary {
            escaped_type,
            span,
        });
    }

    /// Type produced by `<Namespace>.new()` for the builtin collection
    /// constructors (`crates/core/src/builtins/constructors.rs`). `None`
    /// for anything not in that list — including builtin namespaces with
    /// no constructor member, like `Math` — so callers fall through to
    /// generic call inference.
    ///
    /// This exists so `List.new()` / `Dictionary.new()` / `Queue.new()` /
    /// `Stack.new()` get a real collection type *and* participate in arena
    /// coloring via `maybe_arena_ref`, the same way array/tuple/dict/struct
    /// literals already do. Without it, these calls fall through the
    /// generic `Field`/`Call` path, which has no field table yet (see that
    /// arm's own TODO) and always infers `Unknown` — silently skipping
    /// arena tagging inside a `with arena(...)` block. See
    /// `docs/MEMORY_MODEL.md` §5 ("Gap 1").
    ///
    /// Keep this in sync with `constructors.rs` and `BUILTIN_NAMESPACES` if
    /// a new collection constructor is added (e.g. a future `Pool.new()`).
    fn builtin_constructor_type(&mut self, namespace: &str) -> Option<TypeId> {
        match namespace {
            "List" => {
                let elem = self.fresh_var();
                Some(self.ctx.types.intern(SemaType::List(elem)))
            }
            "Dictionary" => {
                let k = self.fresh_var();
                let v = self.fresh_var();
                Some(self.ctx.types.insert(SemaType::Dictionary(k, v)))
            }
            "Queue" => {
                let elem = self.fresh_var();
                Some(self.ctx.types.insert(SemaType::Queue(elem)))
            }
            "Stack" => {
                let elem = self.fresh_var();
                Some(self.ctx.types.insert(SemaType::Stack(elem)))
            }
            _ => None,
        }
    }

    // ── AST type → SemaType ───────────────────────────────────────

    /// Convert a syntactic `Type<'ast>` node into a TypeId.
    /// Uses `match ty.kind` (not `&ty.kind`) since TypeKind is Copy,
    /// avoiding match-ergonomics reference confusion throughout.
    fn ast_type_to_sema<'ast>(&mut self, ty: &Type<'ast>) -> TypeId {
        match ty.kind {
            TypeKind::Int    => self.ctx.types.intern(SemaType::Int),
            TypeKind::Uint   => self.ctx.types.intern(SemaType::Uint),
            TypeKind::Long   => self.ctx.types.intern(SemaType::Long),
            TypeKind::Ulong  => self.ctx.types.intern(SemaType::Ulong),
            TypeKind::Short  => self.ctx.types.intern(SemaType::Int),   // widen
            TypeKind::Ushort => self.ctx.types.intern(SemaType::Uint),  // widen
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
                    Some(def_id) => self.ctx.types.insert(SemaType::Named {
                        def: def_id, args: arg_ids,
                    }),
                    None => self.unknown(),
                }
            }
            TypeKind::Tuple(fields) => {
                let ids: Vec<TypeId> = fields.iter()
                    .map(|f| self.ast_type_to_sema(f))
                    .collect();
                self.ctx.types.insert(SemaType::Tuple(ids))
            }
            TypeKind::Array { len, elem } => {
                let elem_id = self.ast_type_to_sema(elem);
                self.ctx.types.insert(SemaType::Array { len, elem: elem_id })
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

    /// Convert an optional return annotation to `(TypeId, is_fallible)`.
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

    fn collect_signatures<'ast>(&mut self, program: &Program<'ast>) {
        for item in program.items {
            match item {
                Item::Function(f) => { self.collect_fn_sig(f); }
                Item::Struct(s)   => { self.collect_struct_sig(s); }
                Item::Enum(e)     => { self.collect_enum_sig(e); }
                Item::Const(c)    => { self.collect_const_sig(c); }
                Item::Impl(i)     => {
                    for m in i.methods { self.collect_method_sig(m); }
                }
                Item::Extend(x) => {
                    for m in x.methods { self.collect_method_sig(m); }
                }
                Item::Trait(t) => {
                    // FIX: split DefaultMethod (MethodDecl) and MethodSig
                    // into separate arms — they are different types.
                    for it in t.items {
                        match it {
                            TraitItem::DefaultMethod(m) => {
                                self.collect_method_sig(m);
                            }
                            TraitItem::MethodSig(sig) => {
                                self.collect_trait_method_sig(sig);
                            }
                            TraitItem::AssociatedType { .. } => {}
                        }
                    }
                }
                Item::TypeAlias(_) => {} // alias expansion deferred
            }
        }
    }

    fn collect_fn_sig<'ast>(&mut self, f: &FunctionDecl<'ast>) -> TypeId {
        let param_tys: Vec<TypeId> = f.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { ty, .. } => ty.map(|t| self.ast_type_to_sema(t)),
                _ => None,
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
            .filter_map(|p| match p.kind {
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
        if let Some(def_id) = self.ctx.resolutions.get(m.span) {
            self.ctx.set_def_type(def_id, fn_ty);
        }
        fn_ty
    }

    /// Collect type for a required method signature (no body) in a trait.
    fn collect_trait_method_sig<'ast>(&mut self, sig: &MethodSig<'ast>) {
        let param_tys: Vec<TypeId> = sig.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { ty, .. } => ty.map(|t| self.ast_type_to_sema(t)),
                _ => None,
            })
            .collect();
        let (ret_ty, is_fallible) = self.return_type_to_sema(sig.return_type.as_ref());
        let fn_ty = self.ctx.types.insert(SemaType::Function {
            params: param_tys,
            return_type: ret_ty,
            is_fallible,
        });
        if let Some(def_id) = self.ctx.resolutions.get(sig.span) {
            self.ctx.set_def_type(def_id, fn_ty);
        }
    }

    fn collect_struct_sig<'ast>(&mut self, s: &StructDecl<'ast>) {
        let Some(def_id) = self.ctx.top_level_def(s.name) else { return; };
        let struct_ty = self.ctx.types.insert(SemaType::Named { def: def_id, args: vec![] });
        self.ctx.set_def_type(def_id, struct_ty);
        for member in s.members {
            match member {
                StructMember::Field(f) => {
                    let field_ty = self.ast_type_to_sema(f.ty);
                    if let Some(field_def) = self.ctx.resolutions.get(f.span) {
                        self.ctx.set_def_type(field_def, field_ty);
                    }
                }
                StructMember::Method(m)   => { self.collect_method_sig(m); }
                StructMember::Property(_) => {}
            }
        }
    }

    fn collect_enum_sig<'ast>(&mut self, e: &EnumDecl<'ast>) {
        let Some(def_id) = self.ctx.top_level_def(e.name) else { return; };
        let enum_ty = self.ctx.types.insert(SemaType::Named { def: def_id, args: vec![] });
        self.ctx.set_def_type(def_id, enum_ty);
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

    // ── Phase 2b: Body inference ──────────────────────────────────

    fn infer_bodies<'ast>(&mut self, program: &Program<'ast>) {
        for item in program.items {
            match item {
                Item::Function(f) => self.infer_function_body(f),
                Item::Struct(s)   => self.infer_struct_bodies(s),
                Item::Const(c)    => self.infer_const_body(c),
                Item::Impl(i) => {
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

    fn seed_param<'ast>(&mut self, param: &Param<'ast>) {
        match param.kind {
            ParamKind::Named { ty, .. } => {
                let ty_id = ty.map(|t| self.ast_type_to_sema(t))
                    .unwrap_or_else(|| self.fresh_var());
                if let Some(def_id) = self.ctx.resolutions.get(param.span) {
                    self.ctx.set_def_type(def_id, ty_id);
                    self.ctx.set_binding_type(param.span, ty_id);
                }
            }
            _ => {} // self params — TODO when current_struct_type threaded in
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
            StmtKind::Let { binding, ty, value, .. } => {
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
                    let expected_inner = match self.ctx.types.get(expected) {
                        SemaType::Fallible(inner) => *inner,
                        _ => expected,
                    };
                    self.unify(expected_inner, ret, stmt.span);
                }
                // NOT void_ty(): a block's trailing statement type matters
                // for expression-bodied lambdas (`fn(x) x * 2`), which unify
                // it against the lambda's inferred return type. A `return`
                // statement diverges — it has no meaningful "value" of its
                // own here, already unified against current_return above —
                // so this must be an unconstrained fresh var (unifies with
                // anything) rather than void (which would conflict with
                // whatever `ret` actually was, exactly like the never-taken
                // `Some` branch bug this fixes: correct type computed, then
                // silently overwritten by a hardcoded wrong one).
                self.fresh_var()
            }
            StmtKind::Fail(e)        => { self.infer_expr(e); self.void_ty() }
            StmtKind::Break(maybe_e) => { maybe_e.map(|e| self.infer_expr(e)); self.void_ty() }
            StmtKind::Continue       => self.void_ty(),
            StmtKind::Defer(e)       => { self.infer_expr(e); self.void_ty() }

            StmtKind::If(if_node) => {
                let cond    = self.infer_expr(if_node.condition);
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
                let cond    = self.infer_expr(condition);
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, cond, condition.span);
                self.infer_block(body);
                self.void_ty()
            }
            StmtKind::Loop(body) => { self.infer_block(body); self.void_ty() }
            StmtKind::With { body, .. } => {
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
            StmtKind::Extract { value, .. } => { self.infer_expr(value); self.void_ty() }
            StmtKind::Try { body, catch_body, .. } => {
                self.infer_block(body);
                if let Some(cb) = catch_body { self.infer_block(cb); }
                self.void_ty()
            }
            StmtKind::Unsafe(body) => { self.infer_block(body); self.void_ty() }
        }
    }

    fn record_binding<'ast>(&mut self, target: &BindingTarget<'ast>, ty: TypeId, span: Span) {
        match target {
            BindingTarget::Ident(_) => {
                if let Some(def_id) = self.ctx.resolutions.get(span) {
                    self.ctx.set_def_type(def_id, ty);
                    self.ctx.set_binding_type(span, ty);
                }
            }
            BindingTarget::Destructure(_) => {
                self.ctx.set_binding_type(span, ty);
            }
        }
    }

    fn element_type_of(&mut self, collection_ty: TypeId) -> TypeId {
        let resolved = self.apply(collection_ty);
        match self.ctx.types.get(resolved) {
            SemaType::List(e)
            | SemaType::Set(e)
            | SemaType::Queue(e)
            | SemaType::Stack(e)
            | SemaType::Slice(e)         => *e,
            SemaType::Array { elem, .. } => *elem,
            SemaType::ArenaRef { inner, .. } => {
                let inner = *inner;
                self.element_type_of(inner)
            }
            _ => self.fresh_var(),
        }
    }

    // ── Expressions ───────────────────────────────────────────────

    fn infer_expr<'ast>(&mut self, expr: &Expr<'ast>) -> TypeId {
        let ty = self.infer_expr_inner(expr);
        self.ctx.set_expr_type(expr.span, ty);
        ty
    }

    /// GAP 2 — detect an `ArenaRef` value flowing into a binding, struct
    /// field, or indexed slot whose own home arena (if any) doesn't
    /// match. Returns `true` if an escape was detected and reported (the
    /// caller should skip the generic `unify` to avoid a duplicate,
    /// less-specific diagnostic on the same expression).
    ///
    /// "Home arena" per target shape:
    ///   - `Ident`  — the ArenaId (if any) the binding's *original*
    ///     `def_type` carries. Reassignment never mutates `def_types`
    ///     (only the unifier's substitution table changes), so re-reading
    ///     it here always reflects the declaration site, not the current
    ///     statement — exactly the "which arena does this binding belong
    ///     to" question we need.
    ///   - `Field { target: receiver, .. }` — the receiver's own
    ///     top-level ArenaId, if the receiver itself is arena-tagged
    ///     (i.e. the whole struct instance lives in that arena, so
    ///     storing a same-arena value into one of its fields is safe).
    ///     No struct field can be *declared* `ArenaRef` (no surface
    ///     syntax exists for it), so an untagged receiver's home arena
    ///     is always `None` — any arena-tagged value assigned into a
    ///     field on it is unconditionally an escape.
    ///   - `Index { target: container, .. }` — same idea, using the
    ///     indexed container's own top-level ArenaId.
    ///   - anything else (assignment through `Try`/`OptionalChain`, etc.)
    ///     — skipped. Rare enough in practice not to risk a false
    ///     positive from a shape this function doesn't model yet.
    ///
    /// Deliberately conservative: this only compares *top-level* tags,
    /// and for `Ident` targets it flags a mismatch even on the very
    /// first assignment to an untyped, not-yet-arena-tagged binding
    /// (e.g. an untyped function parameter). Proving that case safe in
    /// general requires liveness analysis this pass doesn't do — see
    /// docs/MEMORY_MODEL.md §6 for the reasoning; over-flagging a
    /// borderline-safe pattern is the right default for a memory-safety
    /// check, the same way a real borrow checker sometimes does.
    fn check_assign_arena_escape<'ast>(
        &mut self,
        target: &Expr<'ast>,
        target_ty: TypeId,
        value_ty: TypeId,
        span: Span,
    ) -> bool {
        let Some(value_arena) = self.top_level_arena(value_ty) else { return false; };

        let home_arena: Option<ArenaId> = match &target.kind {
            ExprKind::Ident(_) => self.top_level_arena(target_ty),
            ExprKind::Field { target: receiver, .. } => {
                let receiver_ty = self.ctx.expr_type(receiver.span).unwrap_or(target_ty);
                self.top_level_arena(receiver_ty)
            }
            ExprKind::Index { target: container, .. } => {
                let container_ty = self.ctx.expr_type(container.span).unwrap_or(target_ty);
                self.top_level_arena(container_ty)
            }
            _ => return false,
        };

        if home_arena == Some(value_arena) { return false; }

        self.arena_escape(value_ty, span);
        true
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

            ExprKind::SelfExpr => self.unknown(), // TODO: current_struct_type

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
                // GAP 2 — check for an arena escape first. If the assign
                // itself is the escape, report that specifically and skip
                // the generic structural unify below: since ArenaRef pairs
                // with mismatched arenas are never structurally compatible
                // (see `structurally_compatible`), unify would otherwise
                // immediately pile a second, less useful TypeMismatch on
                // top of the same problem.
                if !self.check_assign_arena_escape(target, target_ty, value_ty, expr.span) {
                    self.unify(target_ty, value_ty, expr.span);
                }
                self.void_ty()
            }

            ExprKind::Pipe { left, right } => {
                self.infer_expr(left);
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
                        ArgKind::Positional(e)       => { self.infer_expr(e); }
                        ArgKind::Named { value, .. } => { self.infer_expr(value); }
                    }
                }

                // GAP 1 FIX — `<Collection>.new()` (e.g. `List.new()`) is
                // parsed as Call { callee: Field { target: Ident(ns), field:
                // "new" } }. The generic Field arm below has no field table
                // yet and always returns Unknown, so without this check
                // every builtin constructor call would type as Unknown and
                // silently skip arena coloring. Narrowly scoped to the
                // known constructor namespaces in builtin_constructor_type;
                // general field/method resolution is untouched. See
                // docs/MEMORY_MODEL.md §5.
                if let ExprKind::Field { target, field } = callee.kind {
                    if field == "new" {
                        if let ExprKind::Ident(ns) = target.kind {
                            if let Some(ctor_ty) = self.builtin_constructor_type(ns) {
                                return self.maybe_arena_ref(ctor_ty);
                            }
                        }
                    }
                }

                self.call_return_type(callee_ty, expr.span)
            }

            ExprKind::Field { target, .. }
            | ExprKind::OptionalChain { target, .. } => {
                self.infer_expr(target);
                self.unknown() // TODO: field table
            }

            ExprKind::Index { target, index } => {
                let coll = self.infer_expr(target);
                self.infer_expr(index);
                self.element_type_of(coll)
            }

            ExprKind::Try(inner)   => {
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

            ExprKind::AnonObject(fields) => {
                for f in fields.iter() { self.infer_expr(f.value); }
                self.unknown() // no structural record type yet
            }

            ExprKind::StructLit { path, fields } => {
                for f in fields.iter() { self.infer_expr(f.value); }
                let root      = path.first().copied().unwrap_or("");
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
                let fn_ty = self.ctx.types.insert(SemaType::Function {
                    params:      param_tys,
                    return_type: ret_resolved,
                    is_fallible: false,
                });
                // GAP 2 — closure capture. A lambda built inside a
                // `with arena(…)` block may capture arena-scoped locals
                // from the enclosing scope; lexical scoping guarantees
                // any such capture can only happen from inside that same
                // block. Tag every lambda constructed here the same way
                // array/tuple/dict/struct literals already are, so that
                // if *this lambda value* is later assigned outward,
                // returned, or stored in a struct field, the existing
                // escape checks catch it — no separate free-variable
                // capture analysis required. See module doc comment.
                self.maybe_arena_ref(fn_ty)
            }

            ExprKind::Block(b) => self.infer_block(b),

            ExprKind::If(if_node) => {
                let cond    = self.infer_expr(if_node.condition);
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
                self.unwrap_optional(inner_ty)
            }
        }
    }

    // ── Literal typing ────────────────────────────────────────────

    fn infer_literal<'ast>(&mut self, lit: &Literal<'ast>) -> TypeId {
        match lit {
            Literal::Int(_)   => self.ctx.types.intern(SemaType::Int),
            Literal::Float(_) => self.ctx.types.intern(SemaType::Float),
            Literal::Double(_)=> self.ctx.types.intern(SemaType::Double),
            Literal::Bool(_)  => self.ctx.types.intern(SemaType::Bool),
            Literal::Char(_)  => self.ctx.types.intern(SemaType::Char),
            Literal::Null     => self.ctx.types.intern(SemaType::Null),
            Literal::Str(_)
            | Literal::VerbatimStr(_)
            | Literal::InterpolatedStr(_)
            | Literal::InterpolatedVerbatimStr(_) => self.ctx.types.intern(SemaType::Str),
        }
    }

    // ── BinOp result type ─────────────────────────────────────────

    fn binop_result(&mut self, op: BinOp, lhs: TypeId, rhs: TypeId, span: Span) -> TypeId {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
            | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                self.unify(lhs, rhs, span);
                self.apply(lhs)
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                self.unify(lhs, rhs, span);
                self.bool_ty()
            }
            BinOp::And | BinOp::Or => {
                let bool_ty = self.bool_ty();
                self.unify(bool_ty, lhs, span);
                self.unify(bool_ty, rhs, span);
                bool_ty
            }
            BinOp::Range | BinOp::RangeIncl => self.unknown(),
        }
    }

    // ── Type unwrapping helpers ───────────────────────────────────

    fn unwrap_task(&mut self, ty: TypeId, span: Span) -> TypeId {
        let resolved = self.apply(ty);
        match self.ctx.types.get(resolved) {
            SemaType::Task(inner) => *inner,
            SemaType::Var(_) => {
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

    fn call_return_type(&mut self, callee_ty: TypeId, span: Span) -> TypeId {
        let resolved = self.apply(callee_ty);
        match self.ctx.types.get(resolved) {
            SemaType::Function { return_type, .. } => *return_type,
            SemaType::Var(_)   => self.fresh_var(),
            SemaType::Unknown  => self.unknown(),
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

        // FIX: use reference patterns so we don't try to move out of &SemaType.
        // u32 is Copy so binding through &SemaType::Var(v) gives v: u32.
        let a_is_var = matches!(self.ctx.types.get(a), SemaType::Var(_));
        let b_is_var = matches!(self.ctx.types.get(b), SemaType::Var(_));

        if a_is_var {
            if let &SemaType::Var(v) = self.ctx.types.get(a) {
                self.unifier.bind(v, b);
                return;
            }
        }
        if b_is_var {
            if let &SemaType::Var(v) = self.ctx.types.get(b) {
                self.unifier.bind(v, a);
                return;
            }
        }

        // Unknown / ERROR absorb without cascading.
        let a_unk = matches!(self.ctx.types.get(a), SemaType::Unknown);
        let b_unk = matches!(self.ctx.types.get(b), SemaType::Unknown);
        if a_unk || b_unk { return; }
        if a == TypeId::ERROR || b == TypeId::ERROR { return; }

        if self.structurally_compatible(a, b) { return; }

        // GAP 2 — an ArenaRef meeting an incompatible context (a
        // different arena, or a context with no arena at all — e.g. a
        // MID-tier function's declared return type, which can never
        // itself express ArenaRef) is specifically an arena escape, not
        // a generic shape mismatch. Prefer the dedicated diagnostic
        // whenever either side carries the tag.
        if let Some(escaped) = self.arena_mismatch_side(a, b) {
            self.arena_escape(escaped, span);
            return;
        }

        self.errors.add_type_error(TypeError::TypeMismatch {
            expected:   self.display_type(a),
            found:      self.display_type(b),
            span,
            because_of: None,
        });
    }

    /// GAP 2 — if either resolved type is `SemaType::ArenaRef`, return
    /// that side's `TypeId` (preferring `b`, conventionally the "actual"
    /// / "found" side in an `expected, found` unify call, which reads
    /// better in the "Value of type `X` ... cannot cross" message).
    /// Returns `None` when neither side is arena-tagged, i.e. this truly
    /// is an unrelated shape mismatch.
    fn arena_mismatch_side(&mut self, a: TypeId, b: TypeId) -> Option<TypeId> {
        let a_is_arena = matches!(self.ctx.types.get(a), SemaType::ArenaRef { .. });
        let b_is_arena = matches!(self.ctx.types.get(b), SemaType::ArenaRef { .. });
        if b_is_arena { Some(b) } else if a_is_arena { Some(a) } else { None }
    }

    /// Returns true if two concrete types are structurally compatible,
    /// recursively unifying their type arguments. Extracts inner TypeIds
    /// (which are Copy) before calling unify to avoid borrow conflicts.
    fn structurally_compatible(&mut self, a: TypeId, b: TypeId) -> bool {
        // Extract inner id pairs first, releasing the immutable borrow on
        // self.ctx.types before we call self.unify (which needs &mut self).
        let inner: Option<(TypeId, TypeId)> = {
            let at = self.ctx.types.get(a);
            let bt = self.ctx.types.get(b);
            match (at, bt) {
                (SemaType::List(ia),     SemaType::List(ib))     => Some((*ia, *ib)),
                (SemaType::Optional(ia), SemaType::Optional(ib)) => Some((*ia, *ib)),
                (SemaType::Fallible(ia), SemaType::Fallible(ib)) => Some((*ia, *ib)),
                (SemaType::Task(ia),     SemaType::Task(ib))     => Some((*ia, *ib)),
                (SemaType::Slice(ia),    SemaType::Slice(ib))    => Some((*ia, *ib)),
                (SemaType::GcRef(ia),    SemaType::GcRef(ib))    => Some((*ia, *ib)),
                // GAP 2 — two ArenaRefs are only structurally compatible
                // when they carry the *same* ArenaId (MEMORY_MODEL.md §3:
                // arena membership must always be compared by specific
                // id, never just "is/isn't tagged ArenaRef"). A mismatch
                // here is a real escape, not an ordinary shape mismatch —
                // `unify`'s caller reports it via `arena_mismatch_side`
                // rather than falling through to a generic TypeMismatch.
                (SemaType::ArenaRef { arena: aa, inner: ia, .. },
                 SemaType::ArenaRef { arena: ab, inner: ib, .. }) => {
                    if aa != ab { return false; }
                    Some((*ia, *ib))
                }
                (SemaType::Named { def: da, .. }, SemaType::Named { def: db, .. }) => {
                    return da == db;
                }
                _ => None,
            }
        }; // immutable borrow released here
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
