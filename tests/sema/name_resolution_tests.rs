// tests/sema/name_resolution_tests.rs
//
// Integration tests for Pass 1 — Name Resolution.
//
// Each test loads a real .ubl fixture file, runs the full
//   lex → parse → name_resolve
// pipeline, and asserts on what came out of the ErrorManager
// and SemaContext.
//
// Fixture files live in tests/sema/fixtures/.
// Happy-path fixtures are prefixed ok_.
// Error-case fixtures are prefixed err_.
//
// HOW TO RUN:
//   cargo test --test name_resolution_tests
//
// HOW TO ADD A TEST:
//   1. Drop a new .ubl file in tests/sema/fixtures/
//   2. Add a #[test] fn below following the existing pattern
//   3. cargo test

use std::path::PathBuf;

// ── Pipeline helper ───────────────────────────────────────────────
//
// Drives lex → parse → resolve for one fixture file.
// Returns (Option<SemaContext>, ErrorManager) so tests can assert
// on both the resolved output and any accumulated errors.
//
// TODO: wire this up to your actual lexer / parser / sema once the
// integration points are stable. The structure here exactly mirrors
// how main.rs will call the pipeline.

struct PipelineResult {
    /// None if parsing failed before sema could run.
    ctx:          Option<ubel_stratum::sema::SemaContext>,
    name_errors:  usize,
    type_errors:  usize,
    parse_errors: usize,
}

fn run_fixture(filename: &str) -> PipelineResult {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/sema/fixtures");
    path.push(filename);

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()));

    // -- Lex --------------------------------------------------------
    let mut error_manager = ubel_stratum::error_management::ErrorManager::new(source.clone());
    let tokens = ubel_stratum::lexer::lex(&source, &mut error_manager);

    if error_manager.lexical_error_count() > 0 {
        return PipelineResult {
            ctx: None,
            name_errors:  0,
            type_errors:  0,
            parse_errors: 0,
        };
    }

    // -- Parse ------------------------------------------------------
    let arena  = ubel_stratum::ast::arena::AstArena::new();
    let program = ubel_stratum::parser::parse(&tokens, &arena, &mut error_manager);

    if error_manager.parse_error_count() > 0 {
        return PipelineResult {
            ctx:          None,
            name_errors:  0,
            type_errors:  0,
            parse_errors: error_manager.parse_error_count(),
        };
    }

    // -- Semantic analysis (Pass 1 only for now) --------------------
    match ubel_stratum::sema::analyse(&program, &arena, source) {
        Ok(ctx) => PipelineResult {
            ctx:          Some(ctx),
            name_errors:  0,
            type_errors:  0,
            parse_errors: 0,
        },
        Err(mut errs) => PipelineResult {
            ctx:          None,
            name_errors:  errs.name_error_count(),
            type_errors:  errs.type_error_count(),
            parse_errors: errs.parse_error_count(),
        },
    }
}

// ── Happy path tests ─────────────────────────────────────────────

/// The simplest possible valid program — one function, one let binding.
/// Must produce zero errors and a populated symbol table.
#[test]
fn ok_simple_resolves_cleanly() {
    let r = run_fixture("ok_simple.ubl");
    assert_eq!(r.parse_errors, 0, "unexpected parse errors");
    assert_eq!(r.name_errors,  0, "unexpected name errors");
    assert!(r.ctx.is_some(),      "expected a SemaContext");

    let ctx = r.ctx.unwrap();
    // Should have at least one definition (the `main` function).
    assert!(ctx.symbols.len() >= 1, "symbol table should not be empty");
}

/// Two functions where `main` calls `helper` which is declared *after* it.
/// This validates that top-level pre-declaration works correctly.
#[test]
fn ok_forward_reference_resolves_cleanly() {
    let r = run_fixture("ok_forward_ref.ubl");
    assert_eq!(r.name_errors, 0, "forward ref should resolve without errors");
    assert!(r.ctx.is_some());
}

/// Multiple nested scopes and variable shadowing.
/// Inner `x` should shadow outer `x`; no errors expected.
#[test]
fn ok_nested_scopes_resolve_cleanly() {
    let r = run_fixture("ok_nested_scopes.ubl");
    assert_eq!(r.name_errors, 0);
    assert!(r.ctx.is_some());
}

/// Struct with methods — verifies that method bodies can reference
/// struct fields and that `self` is valid inside methods.
#[test]
fn ok_struct_methods_resolve_cleanly() {
    let r = run_fixture("ok_struct_methods.ubl");
    assert_eq!(r.name_errors, 0, "struct method resolution should be clean");
    assert!(r.ctx.is_some());

    let ctx = r.ctx.unwrap();
    // Rectangle, its two fields, and at least new + area should be defined.
    assert!(ctx.symbols.len() >= 4, "expected Rectangle + fields + methods");
}

/// `summon` import followed by use of the imported name.
#[test]
fn ok_imports_resolve_cleanly() {
    let r = run_fixture("ok_imports.ubl");
    assert_eq!(r.name_errors, 0, "imported name should be in scope");
    assert!(r.ctx.is_some());
}

/// Enum declared before a function that pattern-matches on its variants.
#[test]
fn ok_enum_variants_resolve_cleanly() {
    let r = run_fixture("ok_enum_variants.ubl");
    assert_eq!(r.name_errors, 0);
    assert!(r.ctx.is_some());
}

/// Closure / lambda whose parameter shadows an outer name.
#[test]
fn ok_lambda_scope_resolves_cleanly() {
    let r = run_fixture("ok_lambda_scope.ubl");
    assert_eq!(r.name_errors, 0);
    assert!(r.ctx.is_some());
}

// ── Error case tests ─────────────────────────────────────────────

/// `let x = y + 1` where `y` is never defined.
/// Must produce exactly one UndefinedName error for `y`.
#[test]
fn err_undefined_name_reports_one_error() {
    let r = run_fixture("err_undefined_name.ubl");
    assert_eq!(r.name_errors, 1, "expected exactly one undefined-name error");
    assert!(r.ctx.is_none(),     "ctx should be None when errors exist");
}

/// Two `let x` declarations in the same scope.
/// Must produce exactly one DuplicateDefinition error.
#[test]
fn err_duplicate_let_reports_one_error() {
    let r = run_fixture("err_duplicate_def.ubl");
    assert_eq!(r.name_errors, 1, "expected exactly one duplicate-definition error");
}

/// `self` used in a free function (not inside a method body).
/// Must produce exactly one SelfOutsideMethod error.
#[test]
fn err_self_outside_method_reports_one_error() {
    let r = run_fixture("err_self_outside_method.ubl");
    assert_eq!(r.name_errors, 1, "expected exactly one self-outside-method error");
}

/// Multiple undefined names — verifies the error manager accumulates
/// all errors rather than stopping at the first one.
#[test]
fn err_multiple_undefined_names_accumulates_all_errors() {
    let r = run_fixture("err_multiple_undefined.ubl");
    assert!(r.name_errors >= 3, "expected at least 3 undefined-name errors");
}

/// A function whose name is the same as a struct declared in the same file.
/// Top-level name collision — must report a DuplicateDefinition.
#[test]
fn err_top_level_name_collision_reports_error() {
    let r = run_fixture("err_top_level_collision.ubl");
    assert_eq!(r.name_errors, 1);
}

// ── Symbol table unit tests ────────────────────────────────────────
//
// These don't need fixture files — they test the data structures directly.

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

        stack.push(); // inner scope — x not re-defined here
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
        assert_eq!(conflict, Some(DefId(0)), "should return the first definition");
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
        stack.push();
        assert_eq!(stack.depth(), 1);
        stack.push();
        assert_eq!(stack.depth(), 2);
        stack.pop();
        assert_eq!(stack.depth(), 1);
    }

    #[test]
    fn name_not_visible_after_scope_pop() {
        let mut stack = ScopeStack::new();
        stack.push();
        stack.push();
        stack.define("temp".to_string(), DefId(0));
        assert!(stack.resolve("temp").is_some());
        stack.pop();
        assert!(stack.resolve("temp").is_none(), "name should not leak out of its scope");
    }
}

// ── Type table unit tests ─────────────────────────────────────────

#[cfg(test)]
mod type_table_unit_tests {
    use ubel_stratum::sema::type_table::{TypeTable, SemaType};

    #[test]
    fn interning_returns_same_id_for_same_primitive() {
        let mut table = TypeTable::new();
        let a = table.intern(SemaType::Int);
        let b = table.intern(SemaType::Int);
        assert_eq!(a, b, "two Int interns should return the same TypeId");
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
        assert_ne!(a, b, "each fresh_var() should return a unique TypeId");
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
        use ubel_stratum::sema::type_table::ArenaId;
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
            "List<&arena int> should be flagged as containing an arena ref"
        );
        assert!(
            !table.get(int_id).contains_arena_ref(&table),
            "plain int should not contain an arena ref"
        );
    }
  }
