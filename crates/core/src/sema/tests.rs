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
use crate::ast::expressions::{Expr, ExprKind};
use crate::ast::literals::Literal;
use crate::ast::root::{Program, Item};
use crate::ast::statements::{Block, Stmt, StmtKind, BindingTarget};
use crate::sema;

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
        visibility:     Visibility::default(),
        is_edge:        false,
        name:           arena.alloc_str("Point"),
        generic_params: &[],
        members:        &[],
        span:           Z,
    };
    assert_ok(&arena, &prog_with(&arena, &[Item::Struct(s)]));
}

#[test]
fn test_sema_struct_and_fn_ok() {
    let arena = AstArena::new();
    let s = StructDecl {
        visibility:     Visibility::default(),
        is_edge:        false,
        name:           arena.alloc_str("Vec3"),
        generic_params: &[],
        members:        &[],
        span:           Z,
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
