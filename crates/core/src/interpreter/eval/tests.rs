// crates/core/src/interpreter/eval/tests.rs
//! Interpreter unit tests — AST constructed directly from the arena.

use crate::ast::arena::AstArena;
use crate::ast::common::{Span, TierAnnotation, Visibility};
use crate::ast::declarations::FunctionDecl;
use crate::ast::expressions::{Expr, ExprKind, Arg, ArgKind};
use crate::ast::literals::Literal;
use crate::ast::root::{Program, Item};
use crate::ast::statements::{Block, Stmt, StmtKind, BindingTarget};
use crate::interpreter::eval::Interpreter;

// ── Helpers ───────────────────────────────────────────────────────────────────

const Z: Span = Span { start: 0, end: 0, line: 0, column: 0 };

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

fn prog<'a>(arena: &'a AstArena, items: &[Item<'a>]) -> Program<'a> {
    Program {
        package: None,
        imports: &[],
        items:   arena.alloc_slice_copy(items),
        span:    Z,
    }
}

fn run<'a>(arena: &'a AstArena, program: &'a Program<'a>) -> Result<(), String> {
    Interpreter::new(arena).run_program(program)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_interp_no_main_errors() {
    let arena = AstArena::new();
    let p = prog(&arena, &[]);
    let result = run(&arena, &p);
    assert!(result.is_err());
}

#[test]
fn test_interp_error_msg_mentions_main() {
    let arena = AstArena::new();
    let p   = prog(&arena, &[]);
    let msg = run(&arena, &p).unwrap_err();
    assert!(msg.contains("main"), "expected 'main' in error, got: {msg}");
}

#[test]
fn test_interp_empty_main_ok() {
    let arena = AstArena::new();
    let main = make_fn(&arena, "main", Block::empty(Z));
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_main_explicit_return_ok() {
    let arena = AstArena::new();
    let stmts = arena.alloc_slice_copy(&[Stmt { kind: StmtKind::Return(None), span: Z }]);
    let main  = make_fn(&arena, "main", Block { stmts, span: Z });
    let p     = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_let_int_ok() {
    let arena = AstArena::new();
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(99)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("x")),
            ty:      None,
            value:   val,
        },
        span: Z,
    }]);
    let main = make_fn(&arena, "main", Block { stmts, span: Z });
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_let_bool_ok() {
    let arena = AstArena::new();
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(false)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("flag")),
            ty:      None,
            value:   val,
        },
        span: Z,
    }]);
    let main = make_fn(&arena, "main", Block { stmts, span: Z });
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_let_str_ok() {
    let arena = AstArena::new();
    let s     = arena.alloc_str("ubel stratum");
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Str(s)), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("msg")),
            ty:      None,
            value:   val,
        },
        span: Z,
    }]);
    let main = make_fn(&arena, "main", Block { stmts, span: Z });
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_let_null_ok() {
    let arena = AstArena::new();
    let val   = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Null), span: Z });
    let stmts = arena.alloc_slice_copy(&[Stmt {
        kind: StmtKind::Let {
            mutable: false,
            binding: BindingTarget::Ident(arena.alloc_str("nothing")),
            ty:      None,
            value:   val,
        },
        span: Z,
    }]);
    let main = make_fn(&arena, "main", Block { stmts, span: Z });
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_two_fns_both_registered() {
    let arena  = AstArena::new();
    let helper = make_fn(&arena, "helper", Block::empty(Z));
    let main   = make_fn(&arena, "main",   Block::empty(Z));
    let p      = prog(&arena, &[Item::Function(helper), Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_call_helper_from_main_ok() {
    let arena  = AstArena::new();
    let helper = make_fn(&arena, "helper", Block::empty(Z));

    // main() { helper() }
    let callee     = arena.alloc(Expr { kind: ExprKind::Ident(arena.alloc_str("helper")), span: Z });
    let call_expr  = arena.alloc(Expr { kind: ExprKind::Call { callee, args: &[] }, span: Z });
    let call_stmt  = Stmt { kind: StmtKind::Expr(call_expr), span: Z };
    let main_stmts = arena.alloc_slice_copy(&[call_stmt]);
    let main       = make_fn(&arena, "main", Block { stmts: main_stmts, span: Z });

    let p = prog(&arena, &[Item::Function(helper), Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
}

#[test]
fn test_interp_multiple_let_bindings_ok() {
    let arena = AstArena::new();
    let mk_let = |name: &str, val: i64| {
        let v = arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(val)), span: Z });
        Stmt {
            kind: StmtKind::Let {
                mutable: false,
                binding: BindingTarget::Ident(arena.alloc_str(name)),
                ty:      None,
                value:   v,
            },
            span: Z,
        }
    };
    let stmts = arena.alloc_slice_copy(&[
        mk_let("a", 1),
        mk_let("b", 2),
        mk_let("c", 3),
    ]);
    let main = make_fn(&arena, "main", Block { stmts, span: Z });
    let p    = prog(&arena, &[Item::Function(main)]);
    assert!(run(&arena, &p).is_ok());
                       }
