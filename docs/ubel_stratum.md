# ubel_stratum

## Overview

Core compiler crate for Ubel Stratum: the AST definitions, semantic
analysis passes (name resolution, type inference, tier checking, borrow
checking, move checking), the tree-walking interpreter, and error
management. Lexing and parsing live in the separate `ubel_stratum_rd`
crate.

## Modules

### `ast/declarations.rs`

**What it does:** Declaration node types (functions, structs, enums,
traits, impls, parameters).

**Decisions:**
- Added `ParamKind::Discard { ty }` for `_: Type` parameters. Kept
  separate from `ParamKind::Named` rather than reusing `Named` with the
  string `"_"` as a name, since a discarded parameter has no binding at
  all: `_` is its own lexer token, never `Ident("_")`, and there is no
  expression production for a bare `_`, so nothing could ever read it
  back even if it were stored as a name.
- No `default` field on `Discard`. A caller-omittable default is about
  what callers can skip supplying; that is orthogonal to whether the
  callee can read the parameter, so pairing the two did not make sense.

**Tests:** see `sema/tests.rs`, referenced under `sema/name_resolution.rs`
and `sema/type_infer.rs` below.

### `sema/name_resolution.rs`

**What it does:** Pass 1 of semantic analysis. Builds the symbol table
and resolves every name reference to a definition.

**Tests:** `sema/tests.rs`,
`test_sema_discard_param_on_free_function_does_not_report_self_outside_method`.

### `sema/type_infer.rs`

**What it does:** Pass 2 of semantic analysis. Builds the type table,
infers expression types, and checks function signatures against call
sites.

**Tests:** `sema/tests.rs`,
`test_sema_discard_param_type_is_enforced_at_call_site`.

### `interpreter/eval/mod.rs`

**What it does:** The `Interpreter` struct and its top-level program
registration: function and method tables, the struct-derive table, and
the driver loop that walks the parsed program before execution starts.

**Decisions:**
- `register_fn`/`register_method` now build their `params: Vec<String>`
  list with `enumerate()` and a synthesized `$discardN` name for each
  `ParamKind::Discard` slot, instead of dropping it. That list is later
  zipped positionally against real call arguments, so dropping a slot
  would shift every later argument onto the wrong parameter name. The
  `$` prefix cannot collide with a real identifier, since identifiers
  cannot start with `$`.

## CI and Workflows

- `.github/workflows/ci-check.yml`, "Ubel Stratum, Fast Compile Check":
  runs on every push and pull request to `master`/`main`.
- `.github/workflows/pipeline-dashboard.yml`: builds the full
  tokenize, parse, sema, interpret diagnostic report plus benchmarks
  for every fixture in `tests/fixtures`, publishes as a static site via
  GitHub Pages. Kept separate from `ci-check.yml` since the benchmark
  step is slow and this produces a deployable artifact rather than a
  pass/fail gate.
- `.github/workflows/run-replacements.yml`: applies a `.mdix/replacements`
  bundle to the repo, dry run only unless manually dispatched with
  `dry_run: false`.

## Fixes and Problems

### `sema/name_resolution.rs`

- `resolve_param`'s catch-all arm was written with only the `self`
  family of `ParamKind` variants in mind: anything that was not
  `Named` fell into a check for `NameError::SelfOutsideMethod`. Once
  `ParamKind::Discard` was added, a `_: Type` parameter on a plain free
  function incorrectly triggered that error. Fixed by giving `Discard`
  its own arm ahead of the `self`-family catch-all.

### `sema/type_infer.rs`

- Three separate signature-collection call sites used
  `ParamKind::Named { ty, .. } => ty.map(...), _ => None` with
  `filter_map`, which silently dropped a `Discard` parameter's declared
  type from the function's computed signature entirely, not merely its
  name. This meant the parameter's type was never checked against call
  sites and the effective arity used for type checking was one short
  per discarded parameter. Fixed by matching `Named` and `Discard`
  together, since both contribute a real type to the signature; only
  `self` variants are correctly excluded.

### `interpreter/eval/mod.rs`

- `register_fn` and `register_method` built their parameter name list
  with `filter_map`, dropping `Discard` slots the same way the
  `type_infer.rs` sites did. Since that list is zipped positionally
  against call arguments at call time, a dropped slot shifted every
  later argument onto the wrong parameter name, silently binding the
  wrong value. Fixed with a synthesized per-position placeholder name
  instead of dropping the slot; see the Decisions note above.
