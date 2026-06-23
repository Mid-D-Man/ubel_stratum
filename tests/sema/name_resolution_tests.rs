// tests/sema/name_resolution_tests.rs
//
// Integration tests for the full sema pipeline (Pass 1 + 2 + 3).
//
// FIX: replaced non-existent `lexer::lex(&source, &mut em)` API with
// the real `lexer::tokenize(&source)` → Result<Vec<Token>, ErrorManager>.
// FIX: corrected parser call order to match pub fn parse(&arena, tokens, source).
// FIX: changed fixture path from tests/sema/fixtures/ to tests/fixtures/
//      where the .ubl files actually live.
//
// HOW TO RUN:
//   cargo test --test name_resolution_tests

use std::path::PathBuf;

struct PipelineResult {
    ctx:          Option<ubel_stratum::sema::SemaContext>,
    name_errors:  usize,
    type_errors:  usize,
    parse_errors: usize,
}

fn run_fixture(filename: &str) -> PipelineResult {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // FIX: files are at tests/fixtures/, not tests/sema/fixtures/
    path.push("tests/fixtures");
    path.push(filename);

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()));

    // ── Lex ──────────────────────────────────────────────────────
    // FIX: real API is tokenize(&source) → Result<Vec<Token>, ErrorManager>
    // not lex(&source, &mut error_manager).
    let tokens = match ubel_stratum::lexer::tokenize(&source) {
        Ok(t)  => t,
        Err(_) => return PipelineResult {
            ctx:          None,
            name_errors:  0,
            type_errors:  0,
            parse_errors: 0,
        },
    };

    // ── Parse ─────────────────────────────────────────────────────
    // FIX: real API is parse(&arena, tokens, source) not parse(&tokens, &arena, …)
    let arena = ubel_stratum::ast::arena::AstArena::new();
    let program = match ubel_stratum::parser::parse(&arena, tokens, source.clone()) {
        Ok(p)  => p,
        Err(em) => return PipelineResult {
            ctx:          None,
            name_errors:  0,
            type_errors:  0,
            parse_errors: em.parse_error_count(),
        },
    };

    // ── Semantic analysis (all three passes) ──────────────────────
    match ubel_stratum::sema::analyse(&program, &arena, source) {
        Ok(ctx) => PipelineResult {
            ctx:          Some(ctx),
            name_errors:  0,
            type_errors:  0,
            parse_errors: 0,
        },
        Err(errs) => PipelineResult {
            ctx:          None,
            name_errors:  errs.name_error_count(),
            type_errors:  errs.type_error_count(),
            parse_errors: errs.parse_error_count(),
        },
    }
}

// ── Happy path ────────────────────────────────────────────────────

#[test]
fn ok_simple_resolves_cleanly() {
    let r = run_fixture("ok_simple.ubl");
    assert_eq!(r.parse_errors, 0, "unexpected parse errors");
    assert_eq!(r.name_errors,  0, "unexpected name errors");
    assert_eq!(r.type_errors,  0, "unexpected type errors");
    assert!(r.ctx.is_some(), "expected a SemaContext");
    let ctx = r.ctx.unwrap();
    assert!(ctx.symbols.len() >= 1, "symbol table should not be empty");
}

#[test]
fn ok_forward_reference_resolves_cleanly() {
    let r = run_fixture("ok_forward_ref.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  0, "forward ref should resolve without errors");
    assert!(r.ctx.is_some());
}

#[test]
fn ok_nested_scopes_resolve_cleanly() {
    let r = run_fixture("ok_nested_scopes.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  0);
    assert!(r.ctx.is_some());
}

#[test]
fn ok_struct_methods_resolve_cleanly() {
    let r = run_fixture("ok_struct_methods.ubl");
    assert_eq!(r.parse_errors, 0, "struct method resolution should parse cleanly");
    assert_eq!(r.name_errors,  0, "struct method resolution should be clean");
    assert!(r.ctx.is_some());
    let ctx = r.ctx.unwrap();
    // Rectangle, its two fields, and at least new + area should be defined.
    assert!(ctx.symbols.len() >= 4, "expected Rectangle + fields + methods");
}

#[test]
fn ok_imports_resolve_cleanly() {
    let r = run_fixture("ok_imports.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  0, "imported name should be in scope");
    assert!(r.ctx.is_some());
}

#[test]
fn ok_enum_variants_resolve_cleanly() {
    let r = run_fixture("ok_enum_variants.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  0);
    assert!(r.ctx.is_some());
}

#[test]
fn ok_lambda_scope_resolves_cleanly() {
    let r = run_fixture("ok_lambda_scope.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  0);
    assert!(r.ctx.is_some());
}

// ── Error cases ───────────────────────────────────────────────────

#[test]
fn err_undefined_name_reports_one_error() {
    let r = run_fixture("err_undefined_name.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  1, "expected exactly one undefined-name error");
    assert!(r.ctx.is_none(), "ctx should be None when errors exist");
}

#[test]
fn err_duplicate_let_reports_one_error() {
    let r = run_fixture("err_duplicate_def.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  1, "expected exactly one duplicate-definition error");
}

#[test]
fn err_self_outside_method_reports_one_error() {
    let r = run_fixture("err_self_outside_method.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  1, "expected exactly one self-outside-method error");
}

#[test]
fn err_multiple_undefined_names_accumulates_all_errors() {
    let r = run_fixture("err_multiple_undefined.ubl");
    assert_eq!(r.parse_errors, 0);
    assert!(r.name_errors >= 3, "expected at least 3 undefined-name errors, got {}", r.name_errors);
}

#[test]
fn err_top_level_name_collision_reports_error() {
    let r = run_fixture("err_top_level_collision.ubl");
    assert_eq!(r.parse_errors, 0);
    assert_eq!(r.name_errors,  1);
}

// ── Symbol table unit tests ────────────────────────────────────────

#[cfg(test)]
mod symbol_table_unit_tests {
    use ubel_stratum::sema::symbol_table::{DefId, ScopeStack};

    #[test]
    fn scope_resolves_name_in_current_scope() {
        let mut stack = ScopeStack::new();
        stack.push();
        let id = DefId(0);
        stack.define("foo".to_string(), id);
        assert_eq!(stack.resolve("foo"), Some(id));
    }

    #[test]
    fn scope_returns_none_for_unknown_name() {
        let mut stack = ScopeStack::new();
        stack.push();
        assert_eq!(stack.resolve("ghost"), None);
    }

    #[test]
    fn scope_resolves_outer_name_from_inner_scope() {
        let mut stack = ScopeStack::new();
        stack.push();
        let outer = DefId(0);
        stack.define("x".to_string(), outer);
        stack.push();
        assert_eq!(stack.resolve("x"), Some(outer), "should see outer x");
        stack.pop();
    }

    #[test]
    fn inner_scope_shadows_outer_scope() {
        let mut stack = ScopeStack::new();
        stack.push();
        let outer = DefId(0);
        stack.define("x".to_string(), outer);
        stack.push();
        let inner = DefId(1);
        stack.define("x".to_string(), inner);
        assert_eq!(stack.resolve("x"), Some(inner), "inner x should shadow outer x");
        stack.pop();
        assert_eq!(stack.resolve("x"), Some(outer), "outer x should be visible again after pop");
    }

    #[test]
    fn duplicate_in_same_scope_returns_existing_id() {
        let mut stack = ScopeStack::new();
        stack.push();
        stack.define("x".to_string(), DefId(0));
        let conflict = stack.define("x".to_string(), DefId(1));
        assert_eq!(conflict, Some(DefId(0)));
    }

    #[test]
    fn same_name_in_different_scopes_is_not_a_duplicate() {
        let mut stack = ScopeStack::new();
        stack.push();
        stack.define("x".to_string(), DefId(0));
        stack.push();
        let conflict = stack.define("x".to_string(), DefId(1));
        assert!(conflict.is_none(), "shadowing in a child scope is not a duplicate");
        stack.pop();
    }

    #[test]
    fn scope_depth_tracks_push_and_pop() {
        let mut stack = ScopeStack::new();
        assert_eq!(stack.depth(), 0);
        stack.push(); assert_eq!(stack.depth(), 1);
        stack.push(); assert_eq!(stack.depth(), 2);
        stack.pop();  assert_eq!(stack.depth(), 1);
    }

    #[test]
    fn name_not_visible_after_scope_pop() {
        let mut stack = ScopeStack::new();
        stack.push();
        stack.push();
        stack.define("temp".to_string(), DefId(0));
        assert!(stack.resolve("temp").is_some());
        stack.pop();
        assert!(stack.resolve("temp").is_none());
    }
}

// ── Type table unit tests ─────────────────────────────────────────

#[cfg(test)]
mod type_table_unit_tests {
    use ubel_stratum::sema::type_table::{ArenaId, SemaType, TypeTable};

    #[test]
    fn interning_returns_same_id_for_same_primitive() {
        let mut table = TypeTable::new();
        let a = table.intern(SemaType::Int);
        let b = table.intern(SemaType::Int);
        assert_eq!(a, b);
    }

    #[test]
    fn different_primitives_get_different_ids() {
        let mut table = TypeTable::new();
        let int_id  = table.intern(SemaType::Int);
        let bool_id = table.intern(SemaType::Bool);
        assert_ne!(int_id, bool_id);
    }

    #[test]
    fn fresh_var_ids_are_unique() {
        let mut table = TypeTable::new();
        let a = table.fresh_var();
        let b = table.fresh_var();
        assert_ne!(a, b);
    }

    #[test]
    fn get_returns_correct_type() {
        let mut table = TypeTable::new();
        let id = table.intern(SemaType::Bool);
        assert_eq!(table.get(id), &SemaType::Bool);
    }

    #[test]
    fn list_of_int_interned_twice_returns_same_id() {
        let mut table  = TypeTable::new();
        let int_id     = table.intern(SemaType::Int);
        let list_a     = table.intern(SemaType::List(int_id));
        let list_b     = table.intern(SemaType::List(int_id));
        assert_eq!(list_a, list_b);
    }

    #[test]
    fn contains_arena_ref_detects_nested_arena_ref() {
        let mut table = TypeTable::new();
        let int_id    = table.intern(SemaType::Int);
        let arena_ref = table.insert(SemaType::ArenaRef {
            arena:   ArenaId(0),
            mutable: false,
            inner:   int_id,
        });
        let list_with_ref = table.insert(SemaType::List(arena_ref));
        assert!(
            table.get(list_with_ref).contains_arena_ref(&table),
            "List<&arena int> should be flagged"
        );
        assert!(
            !table.get(int_id).contains_arena_ref(&table),
            "plain int should not contain an arena ref"
        );
    }
    }
