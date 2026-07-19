// crates/core/src/sema/tests.rs
//! Sema unit tests — AST built directly from the arena, no parser needed.
//!
//! NOTE: sema enforces return-type annotation when a value is returned.
//! Tests that want to verify expressions work use StmtKind::Expr (expression
//! statements) rather than StmtKind::Return(Some(...)) to avoid triggering
//! the return-type check on functions with no declared return type.

use crate::ast::arena::AstArena;
use crate::ast::common::{Span, TierAnnotation, Visibility};
use crate::ast::declarations::{FunctionDecl, StructDecl};
use crate::ast::expressions::{Arg, Expr, ExprKind};
use crate::ast::literals::Literal;
use crate::ast::root::{Program, Item};
use crate::ast::statements::{
    AllocatorKind, Block, SizeExpr, SizeUnit, Stmt, StmtKind, BindingTarget,
};
use crate::sema;
use crate::sema::type_table::SemaType;

// ── Helpers ───────────────────────────────────────────────────────────────────

const Z: Span = Span { start: 0, end: 0, line: 0, column: 0 };

fn empty_prog(arena: &AstArena) -> Program<'_> {
    Program { package: None, imports: &[], items: arena.alloc_slice_copy(&[]), span: Z }
}

fn make_fn<'a>(arena: &'a AstArena, name: &str, body: Block<'a>) -> FunctionDecl<'a> {
    FunctionDecl {
        tier:            TierAnnotation::default(),
        attributes:      &[],
        visibility:      Visibility::default(),
        is_async:        false,
        name:            arena.alloc_str(name),
        lifetime_params: &[],
        generic_params:  &[],
        params:          &[],
        return_type:     None,
        body,
        span:            Z,
    }
}

fn prog_with<'a>(arena: &'a AstArena, items: &[Item<'a>]) -> Program<'a> {
    Program { package: None, imports: &[], items: arena.alloc_slice_copy(items), span: Z }
}

fn assert_ok(arena: &AstArena, prog: &Program<'_>) {
    assert!(
        sema::analyse(prog, arena, String::new()).is_ok(),
        "sema should succeed but returned an error"
    );
}

fn expr_stmt<'a>(arena: &'a AstArena, kind: ExprKind<'a>) -> Stmt<'a> {
    let e = arena.alloc(Expr { kind, span: Z });
    Stmt { kind: StmtKind::Expr(e), span: Z }
}

/// Like `make_fn`, but with an explicit tier — needed for `with arena(...)`
/// tests, since `with arena` is only legal inside `@tier(mid)`.
fn make_fn_tiered<'a>(
    arena: &'a AstArena, name: &str, tier: TierAnnotation, body: Block<'a>,
) -> FunctionDecl<'a> {
    FunctionDecl {
        tier,
        attributes:      &[],
        visibility:      Visibility::default(),
        is_async:        false,
        name:            arena.alloc_str(name),
        lifetime_params: &[],
        generic_params:  &[],
        params:          &[],
        return_type:     None,
        body,
        span:            Z,
    }
}

/// Build `<ns>.new()` as a Call expression — e.g.
/// `builtin_ctor_call(&arena, "List", CALL_SPAN)` for `List.new()`.
/// Only the outer Call gets `call_span`; the callee/target sub-expressions
/// keep span `Z`, since the test only ever looks up `call_span`.
fn builtin_ctor_call<'a>(arena: &'a AstArena, ns: &str, call_span: Span) -> &'a Expr<'a> {
    let target = arena.alloc(Expr { kind: ExprKind::Ident(arena.alloc_str(ns)), span: Z });
    let callee = arena.alloc(Expr {
        kind: ExprKind::Field { target, field: arena.alloc_str("new") },
        span: Z,
    });
    let args: &[Arg] = &[];
    arena.alloc(Expr { kind: ExprKind::Call { callee, args }, span: call_span })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_sema_empty_program_ok() {
    let arena = AstArena::new();
    assert_ok(&arena, &empty_prog(&arena));
}

#[test]
fn test_sema_fn_empty_body_ok() {
    let arena = AstArena::new();
    let f = make_fn(&arena, "helper", Block::empty(Z));
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_explicit_return_unit_ok() {
    // fn foo() { return }  — no value, matches void return type
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(None), span: Z }]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_expr_int_ok() {
    // fn foo() { 42 }  — int literal as an expression statement (no return)
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[expr_stmt(&arena, ExprKind::Lit(Literal::Int(42)))]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_expr_bool_ok() {
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[expr_stmt(&arena, ExprKind::Lit(Literal::Bool(true)))]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_expr_float_ok() {
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[expr_stmt(&arena, ExprKind::Lit(Literal::Double(3.14)))]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_expr_str_ok() {
    let arena = AstArena::new();
    let s     = arena.alloc_str("hello");
    let stmts = arena.alloc_slice_copy(&[expr_stmt(&arena, ExprKind::Lit(Literal::Str(s)))]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_expr_null_ok() {
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[expr_stmt(&arena, ExprKind::Lit(Literal::Null))]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_let_binding_ok() {
    let arena = AstArena::new();
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(10)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("x")),
            ty:      None,
            value:   val,
        },
        span: Z,
    }]);
    let f = make_fn(&arena, "with_let", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_let_then_read_ok() {
    // fn foo() { let x = 1; x }  — bind then use the name as an expr statement
    let arena = AstArena::new();
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(1)), span: Z });
    let let_s = Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("x")),
            ty:      None,
            value:   val,
        },
        span: Z,
    };
    let read_s = expr_stmt(&arena, ExprKind::Ident(arena.alloc_str("x")));
    let stmts  = arena.alloc_slice_copy(&[let_s, read_s]);
    let f      = make_fn(&arena, "foo", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_two_fns_ok() {
    let arena = AstArena::new();
    let a = make_fn(&arena, "foo", Block::empty(Z));
    let b = make_fn(&arena, "bar", Block::empty(Z));
    assert_ok(&arena, &prog_with(&arena, &[Item::Function(a), Item::Function(b)]));
}

#[test]
fn test_sema_fn_ref_sibling_as_expr_ok() {
    // fn helper() {}
    // fn caller()  { helper }  — reference sibling as an expression statement
    let arena  = AstArena::new();
    let helper = make_fn(&arena, "helper", Block::empty(Z));
    let stmts  = arena.alloc_slice_copy(&[
        expr_stmt(&arena, ExprKind::Ident(arena.alloc_str("helper")))
    ]);
    let caller = make_fn(&arena, "caller", Block { stmts, span: Z });
    assert_ok(&arena, &prog_with(&arena, &[
        Item::Function(helper),
        Item::Function(caller),
    ]));
}

#[test]
fn test_sema_empty_struct_ok() {
    let arena = AstArena::new();
    let s = StructDecl {
        attributes:      &[],
        visibility:      Visibility::default(),
        is_edge:         false,
        name:            arena.alloc_str("Point"),
        lifetime_params: &[],
        generic_params:  &[],
        members:         &[],
        span:            Z,
    };
    assert_ok(&arena, &prog_with(&arena, &[Item::Struct(s)]));
}

#[test]
fn test_sema_struct_and_fn_ok() {
    let arena = AstArena::new();
    let s = StructDecl {
        attributes:      &[],
        visibility:      Visibility::default(),
        is_edge:         false,
        name:            arena.alloc_str("Vec3"),
        lifetime_params: &[],
        generic_params:  &[],
        members:         &[],
        span:            Z,
    };
    let f = make_fn(&arena, "new_vec3", Block::empty(Z));
    assert_ok(&arena, &prog_with(&arena, &[Item::Struct(s), Item::Function(f)]));
}

#[test]
fn test_sema_many_fns_ok() {
    let arena = AstArena::new();
    let fns: Vec<Item> = ["alpha", "beta", "gamma", "delta", "epsilon"]
        .iter()
        .map(|n| Item::Function(make_fn(&arena, n, Block::empty(Z))))
        .collect();
    assert_ok(&arena, &prog_with(&arena, &fns));
}

// ── Gap 1: builtin constructor calls + arena coloring ──────────────────────
//
// Before this fix, `List.new()` etc. were parsed as
// Call { callee: Field { target: Ident(ns), field: "new" } }, and the
// generic Field arm has no field table yet — it always inferred Unknown,
// which also meant these calls silently skipped arena tagging inside a
// `with arena(...)` block (only array/tuple/dict/struct literals were
// tagged). See docs/MEMORY_MODEL.md §5.

#[test]
fn test_sema_list_new_outside_arena_is_plain_list() {
    // fn foo() { List.new() }  — no `with arena` in scope: must stay a
    // plain SemaType::List, NOT wrapped in ArenaRef. Negative-case guard —
    // constructor calls should only be arena-tagged when they're actually
    // inside a `with arena(...)` block.
    let arena = AstArena::new();
    const CALL_SPAN: Span = Span { start: 1, end: 1, line: 0, column: 0 };

    let call  = builtin_ctor_call(&arena, "List", CALL_SPAN);
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Expr(call), span: Z }]);
    let f     = make_fn(&arena, "foo", Block { stmts, span: Z });

    let prog = prog_with(&arena, &[Item::Function(f)]);
    let ctx  = sema::analyse(&prog, &arena, String::new())
        .expect("sema should succeed for a plain List.new() call");

    let ty_id = ctx.expr_type(CALL_SPAN).expect("List.new() should have an inferred type");
    match ctx.types.get(ty_id) {
        SemaType::List(_) => {} // expected
        other => panic!("expected plain SemaType::List outside any arena, got {:?}", other),
    }
}

#[test]
fn test_sema_list_new_in_arena_is_arena_ref() {
    // @tier(mid) fn foo() { with arena(1MB) { List.new() } }
    // Inside the with-arena block, List.new() must come back wrapped in
    // ArenaRef, exactly like an array literal already does.
    let arena = AstArena::new();
    const CALL_SPAN: Span = Span { start: 2, end: 2, line: 0, column: 0 };

    let call        = builtin_ctor_call(&arena, "List", CALL_SPAN);
    let inner_stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Expr(call), span: Z }]);
    let inner_block = arena.alloc(Block { stmts: inner_stmts, span: Z });
    let with_stmt   = Stmt {
        kind: StmtKind::With {
            allocator: AllocatorKind::Arena(SizeExpr::WithUnit { value: 1, unit: SizeUnit::MB }),
            body: inner_block,
        },
        span: Z,
    };
    let stmts = arena.alloc_slice_copy(&[with_stmt]);
    let f     = make_fn_tiered(&arena, "foo", TierAnnotation::Mid, Block { stmts, span: Z });

    let prog = prog_with(&arena, &[Item::Function(f)]);
    let ctx  = sema::analyse(&prog, &arena, String::new())
        .expect("sema should succeed for a well-formed @tier(mid) with-arena block");

    let ty_id = ctx.expr_type(CALL_SPAN).expect("List.new() should have an inferred type");
    match ctx.types.get(ty_id) {
        SemaType::ArenaRef { inner, .. } => match ctx.types.get(*inner) {
            SemaType::List(_) => {} // expected
            other => panic!("expected ArenaRef<List<_>>, inner was {:?}", other),
        },
        other => panic!("expected List.new() inside `with arena` to be ArenaRef-wrapped, got {:?}", other),
    }
}

#[test]
fn test_sema_dictionary_new_in_arena_is_arena_ref() {
    let arena = AstArena::new();
    const CALL_SPAN: Span = Span { start: 3, end: 3, line: 0, column: 0 };

    let call        = builtin_ctor_call(&arena, "Dictionary", CALL_SPAN);
    let inner_stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Expr(call), span: Z }]);
    let inner_block = arena.alloc(Block { stmts: inner_stmts, span: Z });
    let with_stmt   = Stmt {
        kind: StmtKind::With {
            allocator: AllocatorKind::Arena(SizeExpr::WithUnit { value: 1, unit: SizeUnit::MB }),
            body: inner_block,
        },
        span: Z,
    };
    let stmts = arena.alloc_slice_copy(&[with_stmt]);
    let f     = make_fn_tiered(&arena, "foo", TierAnnotation::Mid, Block { stmts, span: Z });

    let prog = prog_with(&arena, &[Item::Function(f)]);
    let ctx  = sema::analyse(&prog, &arena, String::new())
        .expect("sema should succeed for a well-formed @tier(mid) with-arena block");

    let ty_id = ctx.expr_type(CALL_SPAN).expect("Dictionary.new() should have an inferred type");
    match ctx.types.get(ty_id) {
        SemaType::ArenaRef { inner, .. } => match ctx.types.get(*inner) {
            SemaType::Dictionary(_, _) => {} // expected
            other => panic!("expected ArenaRef<Dictionary<_,_>>, inner was {:?}", other),
        },
        other => panic!("expected Dictionary.new() inside `with arena` to be ArenaRef-wrapped, got {:?}", other),
    }
}

#[test]
fn test_sema_queue_new_in_arena_is_arena_ref() {
    let arena = AstArena::new();
    const CALL_SPAN: Span = Span { start: 4, end: 4, line: 0, column: 0 };

    let call        = builtin_ctor_call(&arena, "Queue", CALL_SPAN);
    let inner_stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Expr(call), span: Z }]);
    let inner_block = arena.alloc(Block { stmts: inner_stmts, span: Z });
    let with_stmt   = Stmt {
        kind: StmtKind::With {
            allocator: AllocatorKind::Arena(SizeExpr::WithUnit { value: 1, unit: SizeUnit::MB }),
            body: inner_block,
        },
        span: Z,
    };
    let stmts = arena.alloc_slice_copy(&[with_stmt]);
    let f     = make_fn_tiered(&arena, "foo", TierAnnotation::Mid, Block { stmts, span: Z });

    let prog = prog_with(&arena, &[Item::Function(f)]);
    let ctx  = sema::analyse(&prog, &arena, String::new())
        .expect("sema should succeed for a well-formed @tier(mid) with-arena block");

    let ty_id = ctx.expr_type(CALL_SPAN).expect("Queue.new() should have an inferred type");
    match ctx.types.get(ty_id) {
        SemaType::ArenaRef { inner, .. } => match ctx.types.get(*inner) {
            SemaType::Queue(_) => {} // expected
            other => panic!("expected ArenaRef<Queue<_>>, inner was {:?}", other),
        },
        other => panic!("expected Queue.new() inside `with arena` to be ArenaRef-wrapped, got {:?}", other),
    }
}

#[test]
fn test_sema_stack_new_in_arena_is_arena_ref() {
    let arena = AstArena::new();
    const CALL_SPAN: Span = Span { start: 5, end: 5, line: 0, column: 0 };

    let call        = builtin_ctor_call(&arena, "Stack", CALL_SPAN);
    let inner_stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Expr(call), span: Z }]);
    let inner_block = arena.alloc(Block { stmts: inner_stmts, span: Z });
    let with_stmt   = Stmt {
        kind: StmtKind::With {
            allocator: AllocatorKind::Arena(SizeExpr::WithUnit { value: 1, unit: SizeUnit::MB }),
            body: inner_block,
        },
        span: Z,
    };
    let stmts = arena.alloc_slice_copy(&[with_stmt]);
    let f     = make_fn_tiered(&arena, "foo", TierAnnotation::Mid, Block { stmts, span: Z });

    let prog = prog_with(&arena, &[Item::Function(f)]);
    let ctx  = sema::analyse(&prog, &arena, String::new())
        .expect("sema should succeed for a well-formed @tier(mid) with-arena block");

    let ty_id = ctx.expr_type(CALL_SPAN).expect("Stack.new() should have an inferred type");
    match ctx.types.get(ty_id) {
        SemaType::ArenaRef { inner, .. } => match ctx.types.get(*inner) {
            SemaType::Stack(_) => {} // expected
            other => panic!("expected ArenaRef<Stack<_>>, inner was {:?}", other),
        },
        other => panic!("expected Stack.new() inside `with arena` to be ArenaRef-wrapped, got {:?}", other),
    }
}
