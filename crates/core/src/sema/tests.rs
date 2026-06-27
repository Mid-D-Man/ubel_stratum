// crates/core/src/sema/tests.rs
//! Sema unit tests — build AST nodes directly from the arena; no parser needed.

use crate::ast::arena::AstArena;
use crate::ast::common::{Span, TierAnnotation, Visibility};
use crate::ast::declarations::{FunctionDecl, StructDecl};
use crate::ast::expressions::{Expr, ExprKind};
use crate::ast::literals::Literal;
use crate::ast::root::{Program, Item};
use crate::ast::statements::{Block, Stmt, StmtKind, BindingTarget};
use crate::sema;

// ── Helpers ──────────────────────────────────────────────────────────────────

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
        span: Z,
    }
}

fn prog_with_items<'a>(arena: &'a AstArena, items: &[Item<'a>]) -> Program<'a> {
    Program {
        package: None,
        imports: &[],
        items:   arena.alloc_slice_copy(items),
        span:    Z,
    }
}

fn ok(arena: &AstArena, prog: &Program<'_>) {
    assert!(
        sema::analyse(prog, arena, String::new()).is_ok(),
        "sema should succeed but failed"
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_sema_empty_program_ok() {
    let arena = AstArena::new();
    ok(&arena, &empty_prog(&arena));
}

#[test]
fn test_sema_fn_empty_body_ok() {
    let arena = AstArena::new();
    let f = make_fn(&arena, "helper", Block::empty(Z));
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_return_int_ok() {
    let arena = AstArena::new();
    let lit = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(42)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(lit)), span: Z }]);
    let f = make_fn(&arena, "answer", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_return_bool_ok() {
    let arena = AstArena::new();
    let lit = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(true)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(lit)), span: Z }]);
    let f = make_fn(&arena, "flag", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_return_str_ok() {
    let arena = AstArena::new();
    let s   = arena.alloc_str("hello");
    let lit = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Str(s)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(lit)), span: Z }]);
    let f = make_fn(&arena, "greet", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_return_null_ok() {
    let arena = AstArena::new();
    let lit = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Null), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(lit)), span: Z }]);
    let f = make_fn(&arena, "nothing", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_return_float_ok() {
    let arena = AstArena::new();
    let lit = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Double(3.14)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(lit)), span: Z }]);
    let f = make_fn(&arena, "pi", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_fn_let_binding_ok() {
    let arena = AstArena::new();
    let val = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(10)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable:  false,
            binding:  BindingTarget::Ident(arena.alloc_str("x")),
            ty:       None,
            value:    val,
        },
        span: Z,
    }]);
    let f = make_fn(&arena, "with_let", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[Item::Function(f)]));
}

#[test]
fn test_sema_two_fns_ok() {
    let arena = AstArena::new();
    let a = make_fn(&arena, "foo", Block::empty(Z));
    let b = make_fn(&arena, "bar", Block::empty(Z));
    ok(&arena, &prog_with_items(&arena, &[Item::Function(a), Item::Function(b)]));
}

#[test]
fn test_sema_fn_ref_sibling_fn_ok() {
    let arena = AstArena::new();
    // fn helper() {}
    // fn caller() { return helper }  — references sibling by name
    let helper = make_fn(&arena, "helper", Block::empty(Z));
    let ident  = arena.alloc(Expr { kind: ExprKind::Ident(arena.alloc_str("helper")), span: Z });
    let stmts  = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(Some(ident)), span: Z }]);
    let caller = make_fn(&arena, "caller", Block { stmts, span: Z });
    ok(&arena, &prog_with_items(&arena, &[
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
    ok(&arena, &prog_with_items(&arena, &[Item::Struct(s)]));
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
    ok(&arena, &prog_with_items(&arena, &[Item::Struct(s), Item::Function(f)]));
}
