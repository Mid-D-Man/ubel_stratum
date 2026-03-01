// benches/ast_bench.rs
//
// Benchmarks for AST arena allocation patterns.
// We measure the things the parser will actually do:
//   - allocating individual nodes
//   - building Vec-then-slice lists
//   - interning strings
//   - constructing realistic sub-trees
//   - full small/medium program ASTs

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

use ubel_stratum::ast::arena::AstArena;
use ubel_stratum::ast::common::{BinOp, Span, TierAnnotation, Visibility};
use ubel_stratum::ast::expressions::{
    Arg, ArgKind, ElifBranch, Expr, ExprKind, FieldInit, IfExpr,
    MatchArm, MatchArmBody, MatchExpr,
};
use ubel_stratum::ast::literals::Literal;
use ubel_stratum::ast::patterns::{Pattern, PatternKind};
use ubel_stratum::ast::statements::{
    AllocatorKind, BindingTarget, Block, SizeExpr, SizeUnit, Stmt, StmtKind,
};
use ubel_stratum::ast::declarations::{
    ConstDecl, EnumDecl, EnumVariant, EnumVariantPayload,
    FieldDecl, FunctionDecl, MethodDecl, Param, ParamKind, ReturnType, StructDecl, StructMember,
};
use ubel_stratum::ast::root::{Import, ImportItems, ImportKind, Item, PackageDecl, Program};
use ubel_stratum::ast::types::{Type, TypeKind};

// ── Shared helpers ────────────────────────────────────────────────

/// A dummy span used everywhere — span construction cost is not what we're measuring.
const DUMMY: Span = Span { start: 0, end: 0, line: 1, column: 1 };

fn int_type(arena: &AstArena) -> &Type<'_> {
    arena.alloc(Type { kind: TypeKind::Int, span: DUMMY })
}

fn bool_type(arena: &AstArena) -> &Type<'_> {
    arena.alloc(Type { kind: TypeKind::Bool, span: DUMMY })
}

fn string_type(arena: &AstArena) -> &Type<'_> {
    arena.alloc(Type { kind: TypeKind::Str, span: DUMMY })
}

fn int_lit(arena: &AstArena, n: i64) -> &Expr<'_> {
    arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(n)), span: DUMMY })
}

fn ident_expr<'a>(arena: &'a AstArena, name: &str) -> &'a Expr<'a> {
    let s = arena.alloc_str(name);
    arena.alloc(Expr { kind: ExprKind::Ident(s), span: DUMMY })
}

fn empty_block() -> Block<'static> {
    Block { stmts: &[], span: DUMMY }
}

// ── 1. Leaf node allocation ───────────────────────────────────────

fn bench_alloc_leaf(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/alloc_leaf");
    group.measurement_time(Duration::from_secs(4));

    group.bench_function("int_literal", |b| {
        b.iter(|| {
            let arena = AstArena::new();
            let node = arena.alloc(Expr {
                kind: ExprKind::Lit(Literal::Int(black_box(42))),
                span: DUMMY,
            });
            black_box(node);
        });
    });

    group.bench_function("string_intern_short", |b| {
        b.iter(|| {
            let arena = AstArena::new();
            let s = arena.alloc_str(black_box("hello"));
            black_box(s);
        });
    });

    group.bench_function("string_intern_long", |b| {
        let long = "handle_request_with_very_long_function_name_that_exceeds_cache";
        b.iter(|| {
            let arena = AstArena::new();
            let s = arena.alloc_str(black_box(long));
            black_box(s);
        });
    });

    group.bench_function("type_node", |b| {
        b.iter(|| {
            let arena = AstArena::new();
            let ty = arena.alloc(Type { kind: TypeKind::Int, span: DUMMY });
            black_box(ty);
        });
    });

    group.finish();
}

// ── 2. Vec → slice (the primary list-building pattern) ───────────

fn bench_vec_to_slice(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/vec_to_slice");
    group.measurement_time(Duration::from_secs(4));

    for count in [4usize, 16, 64, 256] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |b, &n| {
                b.iter(|| {
                    let arena = AstArena::new();
                    let mut v = arena.vec::<&Expr>();
                    for i in 0..n {
                        let expr = arena.alloc(Expr {
                            kind: ExprKind::Lit(Literal::Int(i as i64)),
                            span: DUMMY,
                        });
                        v.push(expr);
                    }
                    let slice = v.into_bump_slice();
                    black_box(slice);
                });
            },
        );
    }

    group.finish();
}

// ── 3. Binary expression tree ─────────────────────────────────────
//
// Builds `((1 + 2) * (3 - 4)) + (5 / 6)` — a balanced tree 3 levels deep.

fn make_binop<'a>(
    arena: &'a AstArena,
    op: BinOp,
    lhs: &'a Expr<'a>,
    rhs: &'a Expr<'a>,
) -> &'a Expr<'a> {
    arena.alloc(Expr {
        kind: ExprKind::BinOp { op, lhs, rhs },
        span: DUMMY,
    })
}

fn bench_binop_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/binop_tree");
    group.measurement_time(Duration::from_secs(4));

    group.bench_function("depth_3", |b| {
        b.iter(|| {
            let arena = AstArena::new();
            let l1 = make_binop(&arena, BinOp::Add, int_lit(&arena, 1), int_lit(&arena, 2));
            let l2 = make_binop(&arena, BinOp::Sub, int_lit(&arena, 3), int_lit(&arena, 4));
            let l3 = make_binop(&arena, BinOp::Div, int_lit(&arena, 5), int_lit(&arena, 6));
            let m1 = make_binop(&arena, BinOp::Mul, l1, l2);
            let root = make_binop(&arena, BinOp::Add, m1, l3);
            black_box(root);
        });
    });

    // 7-level deep right-associative chain: 1 + (2 + (3 + ... ))
    group.bench_function("chain_128", |b| {
        b.iter(|| {
            let arena = AstArena::new();
            let mut acc = int_lit(&arena, 0);
            for i in (1i64..=128).rev() {
                acc = make_binop(&arena, BinOp::Add, int_lit(&arena, i), acc);
            }
            black_box(acc);
        });
    });

    group.finish();
}

// ── 4. If / elif / else expression ───────────────────────────────

fn bench_if_expr(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/if_expr");
    group.measurement_time(Duration::from_secs(4));

    // Simple `if cond { return 1 } else { return 0 }`
    group.bench_function("simple_if_else", |b| {
        b.iter(|| {
            let arena = AstArena::new();

            let cond = ident_expr(&arena, "condition");

            let then_stmt = arena.alloc(Stmt {
                kind: StmtKind::Return(Some(int_lit(&arena, 1))),
                span: DUMMY,
            });
            let then_stmts = arena.alloc_slice_copy(&[*then_stmt]);
            let then_block = Block { stmts: then_stmts, span: DUMMY };

            let else_stmt = arena.alloc(Stmt {
                kind: StmtKind::Return(Some(int_lit(&arena, 0))),
                span: DUMMY,
            });
            let else_stmts = arena.alloc_slice_copy(&[*else_stmt]);
            let else_block = Block { stmts: else_stmts, span: DUMMY };

            let if_expr = arena.alloc(IfExpr {
                condition: cond,
                then_block,
                elif_branches: &[],
                else_block: Some(else_block),
                span: DUMMY,
            });

            black_box(if_expr);
        });
    });

    // 4-branch if/elif/elif/else
    group.bench_function("if_3_elif_else", |b| {
        b.iter(|| {
            let arena = AstArena::new();

            let make_branch = |arena: &AstArena, name: &str, ret: i64| {
                let cond = ident_expr(arena, name);
                let stmt = arena.alloc(Stmt {
                    kind: StmtKind::Return(Some(int_lit(arena, ret))),
                    span: DUMMY,
                });
                let stmts = arena.alloc_slice_copy(&[*stmt]);
                (cond, Block { stmts, span: DUMMY })
            };

            let (cond, then_block) = make_branch(&arena, "is_admin", 3);
            let (c1, b1) = make_branch(&arena, "is_mod", 2);
            let (c2, b2) = make_branch(&arena, "is_user", 1);

            let mut elifs = arena.vec::<ElifBranch>();
            elifs.push(ElifBranch { condition: c1, block: b1, span: DUMMY });
            elifs.push(ElifBranch { condition: c2, block: b2, span: DUMMY });
            let elif_branches = elifs.into_bump_slice();

            let else_stmt = arena.alloc(Stmt {
                kind: StmtKind::Return(Some(int_lit(&arena, 0))),
                span: DUMMY,
            });
            let else_block = Block {
                stmts: arena.alloc_slice_copy(&[*else_stmt]),
                span: DUMMY,
            };

            let if_node = arena.alloc(IfExpr {
                condition: cond,
                then_block,
                elif_branches,
                else_block: Some(else_block),
                span: DUMMY,
            });

            black_box(if_node);
        });
    });

    group.finish();
}

// ── 5. Match expression ───────────────────────────────────────────

fn bench_match_expr(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/match_expr");
    group.measurement_time(Duration::from_secs(4));

    for arm_count in [4usize, 8, 16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_arms", arm_count)),
            &arm_count,
            |b, &n| {
                b.iter(|| {
                    let arena = AstArena::new();
                    let scrutinee = ident_expr(&arena, "status");

                    let mut arms = arena.vec::<MatchArm>();
                    for i in 0..n {
                        let pat = Pattern {
                            kind: PatternKind::Literal(Literal::Int(i as i64)),
                            span: DUMMY,
                        };
                        let body = MatchArmBody::Expr(int_lit(&arena, i as i64 * 10));
                        arms.push(MatchArm { pattern: pat, guard: None, body, span: DUMMY });
                    }
                    let arms = arms.into_bump_slice();

                    let match_node = arena.alloc(MatchExpr {
                        scrutinee,
                        arms,
                        span: DUMMY,
                    });
                    black_box(match_node);
                });
            },
        );
    }

    group.finish();
}

// ── 6. Function declaration ───────────────────────────────────────

fn bench_function_decl(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/function_decl");
    group.measurement_time(Duration::from_secs(4));

    // Simple: fn add(x: int, y: int) int { return x + y }
    group.bench_function("simple_2_params", |b| {
        b.iter(|| {
            let arena = AstArena::new();

            let make_param = |arena: &AstArena, name: &str| -> Param {
                Param {
                    kind: ParamKind::Named {
                        mutable: false,
                        name: arena.alloc_str(name),
                        ty: Some(int_type(arena)),
                        default: None,
                    },
                    span: DUMMY,
                }
            };

            let mut params = arena.vec::<Param>();
            params.push(make_param(&arena, "x"));
            params.push(make_param(&arena, "y"));
            let params = params.into_bump_slice();

            let lhs = ident_expr(&arena, "x");
            let rhs = ident_expr(&arena, "y");
            let add = make_binop(&arena, BinOp::Add, lhs, rhs);
            let ret_stmt = arena.alloc(Stmt {
                kind: StmtKind::Return(Some(add)),
                span: DUMMY,
            });
            let stmts = arena.alloc_slice_copy(&[*ret_stmt]);
            let body = Block { stmts, span: DUMMY };

            let decl = arena.alloc(FunctionDecl {
                tier: TierAnnotation::High,
                attributes: &[],
                visibility: Visibility::Public,
                is_async: false,
                name: arena.alloc_str("add"),
                lifetime_params: &[],
                generic_params: &[],
                params,
                return_type: Some(ReturnType { ty: int_type(&arena), is_fallible: false }),
                body,
                span: DUMMY,
            });

            black_box(decl);
        });
    });

    // Async handler: resembles a real request-handling function
    group.bench_function("async_handler_5_params", |b| {
        b.iter(|| {
            let arena = AstArena::new();

            let param_names = ["req", "auth", "db", "cache", "logger"];
            let mut params = arena.vec::<Param>();
            for name in &param_names {
                params.push(Param {
                    kind: ParamKind::Named {
                        mutable: false,
                        name: arena.alloc_str(name),
                        ty: Some(string_type(&arena)),
                        default: None,
                    },
                    span: DUMMY,
                });
            }
            let params = params.into_bump_slice();

            // Body: let result = x; return result
            let let_stmt = arena.alloc(Stmt {
                kind: StmtKind::Let {
                    mutable: false,
                    binding: BindingTarget::Ident(arena.alloc_str("result")),
                    ty: None,
                    value: ident_expr(&arena, "req"),
                },
                span: DUMMY,
            });
            let ret_stmt = arena.alloc(Stmt {
                kind: StmtKind::Return(Some(ident_expr(&arena, "result"))),
                span: DUMMY,
            });
            let stmts = arena.alloc_slice_copy(&[*let_stmt, *ret_stmt]);
            let body = Block { stmts, span: DUMMY };

            let task_inner = arena.alloc(Type { kind: TypeKind::Str, span: DUMMY });
            let task_ty = arena.alloc(Type {
                kind: TypeKind::Task(Some(task_inner)),
                span: DUMMY,
            });

            let decl = arena.alloc(FunctionDecl {
                tier: TierAnnotation::High,
                attributes: &[],
                visibility: Visibility::Public,
                is_async: true,
                name: arena.alloc_str("handle_request"),
                lifetime_params: &[],
                generic_params: &[],
                params,
                return_type: Some(ReturnType { ty: task_ty, is_fallible: true }),
                body,
                span: DUMMY,
            });

            black_box(decl);
        });
    });

    group.finish();
}

// ── 7. Struct declaration ─────────────────────────────────────────

fn bench_struct_decl(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/struct_decl");
    group.measurement_time(Duration::from_secs(4));

    for field_count in [4usize, 8, 16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_fields", field_count)),
            &field_count,
            |b, &n| {
                b.iter(|| {
                    let arena = AstArena::new();
                    let mut members = arena.vec::<StructMember>();

                    for i in 0..n {
                        let name = arena.alloc_str(&format!("field_{}", i));
                        members.push(StructMember::Field(FieldDecl {
                            visibility: Visibility::Public,
                            name,
                            ty: int_type(&arena),
                            span: DUMMY,
                        }));
                    }

                    // Add one method
                    let ret_stmt = arena.alloc(Stmt {
                        kind: StmtKind::Return(Some(int_lit(&arena, 0))),
                        span: DUMMY,
                    });
                    let body = Block {
                        stmts: arena.alloc_slice_copy(&[*ret_stmt]),
                        span: DUMMY,
                    };
                    members.push(StructMember::Method(MethodDecl {
                        tier: TierAnnotation::High,
                        attributes: &[],
                        visibility: Visibility::Public,
                        is_async: false,
                        name: arena.alloc_str("total"),
                        generic_params: &[],
                        params: &[],
                        return_type: Some(ReturnType { ty: int_type(&arena), is_fallible: false }),
                        body,
                        span: DUMMY,
                    }));

                    let decl = arena.alloc(StructDecl {
                        visibility: Visibility::Public,
                        is_edge: false,
                        name: arena.alloc_str("MyStruct"),
                        generic_params: &[],
                        members: members.into_bump_slice(),
                        span: DUMMY,
                    });
                    black_box(decl);
                });
            },
        );
    }

    group.finish();
}

// ── 8. Arena reuse — simulate parsing multiple files ──────────────

fn bench_arena_reuse(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/arena_reuse");
    group.measurement_time(Duration::from_secs(5));

    // Each iteration creates a fresh arena (simulates one file per arena).
    group.bench_function("fresh_arena_per_file", |b| {
        b.iter(|| {
            let arena = AstArena::with_capacity(64 * 1024);

            // Simulate 20 functions, each with 3 params and 4 stmts
            let mut items = arena.vec::<Item>();
            for i in 0..20usize {
                let name = arena.alloc_str(&format!("fn_{}", i));
                let mut params = arena.vec::<Param>();
                for j in 0..3usize {
                    params.push(Param {
                        kind: ParamKind::Named {
                            mutable: false,
                            name: arena.alloc_str(&format!("p{}", j)),
                            ty: Some(int_type(&arena)),
                            default: None,
                        },
                        span: DUMMY,
                    });
                }
                let params = params.into_bump_slice();

                let mut stmts = arena.vec::<Stmt>();
                for k in 0..4usize {
                    stmts.push(Stmt {
                        kind: StmtKind::Let {
                            mutable: false,
                            binding: BindingTarget::Ident(
                                arena.alloc_str(&format!("v{}", k))
                            ),
                            ty: None,
                            value: int_lit(&arena, k as i64),
                        },
                        span: DUMMY,
                    });
                }
                let body = Block { stmts: stmts.into_bump_slice(), span: DUMMY };

                items.push(Item::Function(arena.alloc(FunctionDecl {
                    tier: TierAnnotation::High,
                    attributes: &[],
                    visibility: Visibility::Public,
                    is_async: false,
                    name,
                    lifetime_params: &[],
                    generic_params: &[],
                    params,
                    return_type: Some(ReturnType {
                        ty: int_type(&arena),
                        is_fallible: false,
                    }),
                    body,
                    span: DUMMY,
                }).clone()));
            }

            let prog = Program {
                package: Some(PackageDecl {
                    path: arena.alloc_slice_clone(&["my_package"]),
                    span: DUMMY,
                }),
                imports: &[],
                items: items.into_bump_slice(),
                span: DUMMY,
            };
            black_box(prog);
            black_box(arena.allocated_bytes());
        });
    });

    group.finish();}

// ── 9. Full small program AST ─────────────────────────────────────
//
// Builds the complete AST for:
//
//   package demo
//   summon std.io
//   const MAX: int = 100
//   struct Point { x: int, y: int }
//   fn main() { let p = Point { x = 0, y = 0 }; return }

fn bench_full_small_program(c: &mut Criterion) {
    let mut group = c.benchmark_group("ast/full_program");
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("small_program", |b| {
        b.iter(|| {
            let arena = AstArena::new();

            // package demo
            let package = PackageDecl {
                path: arena.alloc_slice_clone(&["demo"]),
                span: DUMMY,
            };

            // summon std.io
            let mut imports = arena.vec::<Import>();
            imports.push(Import {
                kind: ImportKind::Summon {
                    path: arena.alloc_slice_clone(&["std", "io"]),
                    alias: None,
                },
                span: DUMMY,
            });
            let imports = imports.into_bump_slice();

            let mut items = arena.vec::<Item>();

            // const MAX: int = 100
            items.push(Item::Const(ConstDecl {
                name: arena.alloc_str("MAX"),
                ty: Some(int_type(&arena)),
                value: int_lit(&arena, 100),
                span: DUMMY,
            }));

            // struct Point { x: int, y: int }
            let fields = {
                let mut v = arena.vec::<StructMember>();
                for name in ["x", "y"] {
                    v.push(StructMember::Field(FieldDecl {
                        visibility: Visibility::Public,
                        name: arena.alloc_str(name),
                        ty: int_type(&arena),
                        span: DUMMY,
                    }));
                }
                v.into_bump_slice()
            };
            items.push(Item::Struct(arena.alloc(StructDecl {
                visibility: Visibility::Public,
                is_edge: false,
                name: arena.alloc_str("Point"),
                generic_params: &[],
                members: fields,
                span: DUMMY,
            }).clone()));

            // fn main() { let p = Point { x = 0, y = 0 }; return }
            let field_inits = {
                let mut v = arena.vec::<FieldInit>();
                v.push(FieldInit { name: arena.alloc_str("x"), value: int_lit(&arena, 0), span: DUMMY });
                v.push(FieldInit { name: arena.alloc_str("y"), value: int_lit(&arena, 0), span: DUMMY });
                v.into_bump_slice()
            };
            let point_lit = arena.alloc(Expr {
                kind: ExprKind::StructLit {
                    path: arena.alloc_slice_clone(&["Point"]),
                    fields: field_inits,
                },
                span: DUMMY,
            });
            let let_p = arena.alloc(Stmt {
                kind: StmtKind::Let {
                    mutable: false,
                    binding: BindingTarget::Ident(arena.alloc_str("p")),
                    ty: None,
                    value: point_lit,
                },
                span: DUMMY,
            });
            let ret = arena.alloc(Stmt { kind: StmtKind::Return(None), span: DUMMY });
            let main_body = Block {
                stmts: arena.alloc_slice_copy(&[*let_p, *ret]),
                span: DUMMY,
            };
            items.push(Item::Function(arena.alloc(FunctionDecl {
                tier: TierAnnotation::High,
                attributes: &[],
                visibility: Visibility::Public,
                is_async: false,
                name: arena.alloc_str("main"),
                lifetime_params: &[],
                generic_params: &[],
                params: &[],
                return_type: None,
                body: main_body,
                span: DUMMY,
            }).clone()));

            let prog = Program {
                package: Some(package),
                imports,
                items: items.into_bump_slice(),
                span: DUMMY,
            };
            black_box(prog);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_alloc_leaf,
    bench_vec_to_slice,
    bench_binop_tree,
    bench_if_expr,
    bench_match_expr,
    bench_function_decl,
    bench_struct_decl,
    bench_arena_reuse,
    bench_full_small_program,
);
criterion_main!(benches);
