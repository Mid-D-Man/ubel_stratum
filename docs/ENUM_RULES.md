# Ubel Stratum — Enum Rules

> **Canonical reference for enum grammar, AST, runtime representation, and
> sema.** Every contributor reads this before touching enum declaration
> parsing, `EnumVariantPayload`, `Value::Enum`, or `EnumName.Variant`
> resolution.
>
> **Status legend:** ✅ IMPLEMENTED · ⚠️ GAP · 🔭 PROPOSED

---

## 1. Current Status, Layer by Layer

| Layer | Status | Notes |
|---|---|---|
| Grammar (`docs/ubel.ebnf`) | ✅ IMPLEMENTED | Full Rust-shaped variant grammar, see §2 |
| AST (`crates/core/src/ast`) | ✅ IMPLEMENTED | `EnumVariantPayload` has all four Rust shapes |
| Runtime (`interpreter/value.rs`) | ✅ IMPLEMENTED | `Value::Enum` mirrors the AST exactly |
| Sema (`crates/core/src/sema`) | ✅ IMPLEMENTED (non-generic) | §5 — pattern type-checking, exhaustiveness, payload construction/binding, all four variant shapes. Generic enums (`Option<T>`/`Result<T,E>`-shaped) are a deliberate follow-up, not covered — see §5's "Known gaps." |

The headline finding: **structurally, our enums are close to a 1:1 match for
Rust's.** This is not the flat "named integer constant" model DixScript uses
(`AIType { PASSIVE = 0, AGGRESSIVE = 1, BOSS = 2 }`, no payload capability at
all) — ours support full tagged-union / sum-type variants.

---

## 2. Grammar and AST — What's Already There

```rust
pub enum EnumVariantPayload<'ast> {
    None,                              // unit variant
    Discriminant(&'ast Expr<'ast>),    // explicit `= value` (C-style)
    Tuple(&'ast [&'ast Type<'ast>]),   // tuple variant — Variant(T1, T2)
    Struct(&'ast [FieldDecl<'ast>]),   // struct variant — Variant { field: T }
}
```

`EnumDecl` also supports generic params (`GenericParams?` in the grammar), so
`Option<T>` / `Result<T, E>`-shaped enums are already representable
syntactically — not just fieldless enums.

Runtime side mirrors this exactly:

```rust
Value::Enum {
    type_name: String,
    variant: String,
    payload: Box<EnumPayload>,
}

pub enum EnumPayload {
    None,
    Tuple(Vec<Value>),
    Struct(HashMap<String, Value>),
}
```

No changes proposed to grammar, AST, or the runtime `Value` shape — all three
are sound as-is. Everything below is about building the missing sema layer
on top of them.

---

## 3. Decisions Needed Before Sema Work Starts (resolved — see §5)

### 3.1 Implicit discriminant auto-increment

Rust behavior: a unit variant without an explicit `= value` gets the next
integer in declaration order (0, 1, 2, ... unless a prior explicit value
resets the sequence). DixScript's enums do the same thing.

**✅ Resolved: adopted, with a caveat.** `EnumVariantPayload::None` without a
preceding `Discriminant` is accepted and behaves identically to an explicit
one for every purpose sema/runtime currently support. What's *not* built:
actually computing and storing each variant's resulting ordinal — nothing
downstream reads a discriminant's chosen value back out yet (no `as int`
cast exists), so there's nowhere for a tracked auto-increment counter to
matter yet. See §5's "Known gaps."

### 3.2 Can `Discriminant` and payload-bearing variants coexist in one enum?

Rust **disallows** this — explicit `= value` discriminants are only legal on
fieldless (C-like) enums; the moment any variant carries a payload, no
variant in that enum may have an explicit discriminant.

**✅ Resolved: adopted Rust's restriction.** An enum declaring both an
explicit discriminant variant and a payload-carrying (`Tuple`/`Struct`)
variant is rejected outright — `TypeError::MixedDiscriminantAndPayload`
(`TYPE-112`), checked once per enum during `collect_enum_sig`.

### 3.3 `EnumName.Variant` path resolution

**✅ Resolved: implemented**, for both expression and pattern position —
§5. Bare unqualified variant names (`North`, not `Direction.North`) are also
supported, resolved contextually against the scrutinee's known type rather
than requiring the qualified form everywhere.

### 3.4 Exhaustiveness checking

**✅ Resolved: implemented**, pragmatically — §5. `match`'s grammar turned
out to already be fully settled (RD + Pratt, `MatchArmBody::Expr`/`Block`,
full `PatternKind` variety) by the time this work started; it was never
actually a blocker.

---

## 4. Open Decisions (all resolved — see §5)

| # | Question | Status |
|---|---|---|
| 1 | Implicit discriminant auto-increment — adopt Rust's behavior as proposed in §3.1? | ✅ Resolved — adopted (validation only; values not tracked, §5 "Known gaps") |
| 2 | Disallow mixing `Discriminant` with `Tuple`/`Struct` variants in one enum, matching Rust? | ✅ Resolved — adopted, `TYPE-112` |
| 3 | Order of implementation: name resolution (§3.3) before or after discriminant assignment (§3.1)? | ✅ Resolved — moot; both fold into the same `collect_enum_sig` pass |
| 4 | Does exhaustiveness checking (§3.4) block on the `match`/switch grammar being settled first? | ✅ Resolved — no; grammar was already settled |

---

## 5. Implementation (non-generic enums)

✅ **Implemented**: fieldless, discriminant, tuple-payload, and struct-payload
variants, fully — pattern type-checking, exhaustiveness, payload
construction and binding, both expression and pattern position, both bare
(`North`, `Ok(x)`, `Move { x, y }`) and qualified (`Direction.North`,
`Result.Ok(x)`) forms. **Generic enums** (`Option<T>`/`Result<T,E>`-shaped,
where the enum itself takes type parameters) are a deliberate follow-up —
real generic substitution through variant construction and pattern payload
types is a distinct jump in complexity from what's here, scoped out rather
than silently expanded into this round.

### A live, previously-silent bug this section found and fixed

Before any of this, `tests/fixtures/ok_enum_variants.ubl` already existed
and already passed — for the wrong reason. A bare unqualified variant name
in a match arm (`North => ...`) parses as `PatternKind::Ident` — the parser
can't tell it apart from a fresh catch-all binding without type info, which
doesn't exist yet at parse time. The interpreter's own pre-existing comment
on that pattern kind said it plainly: *"always matches, binds name."* That
meant the fixture's `North` arm fired unconditionally regardless of what the
scrutinee actually held — calling `describe(Direction.South)` would have
still printed "heading north." Verified genuinely fixed (not just
plausible) via `tests/fixtures/ok_enum_bare_name_dispatch.ubl`, which
specifically exercises every variant that *isn't* first.

The fix has two independent halves, since sema and the interpreter each
resolve this ambiguity differently and both needed it:

- **Sema** (`InferCtx::check_pattern`'s `PatternKind::Ident` arm) resolves
  it *statically*: once the scrutinee's type is known (which it wasn't in
  the old code — `StmtKind::Match`/`ExprKind::Match` never looked at
  `arm.pattern` at all), check whether the bare name matches one of that
  enum's fieldless/discriminant variants; if so, it's a variant pattern
  (validated, contributes to exhaustiveness, introduces no binding), else a
  genuine catch-all.
- **Interpreter** (`eval::pattern::match_inner`'s `PatternKind::Ident` arm)
  resolves the same ambiguity *dynamically*, off the concrete runtime
  value's own tag rather than a static type — simpler, and actually more
  precise. This needed `Interpreter::enum_table` (already existed, tracking
  which names are real variants of which enum) threaded through the whole
  pattern-matching call chain (`match_pattern`/`try_match`/`match_inner`
  and its handful of recursive helper functions), since the check isn't
  just "does this name match my *own* variant" — an arm's pattern name has
  to fail to match (and let the next arm try) when it names a *different*
  real variant of the *same* enum type, not fall through to catch-all
  binding just because this particular comparison didn't hit. Getting only
  half of that right was a real bug caught during this section's own
  verification, not a hypothetical.

A second, structurally identical instance of the same ambiguity was found
while building the struct-payload fixtures: `Move { x, y }` with a
1-segment name *also* can't be told apart from a plain struct pattern
(`Point { x, y }`) without type info — the parser's own disambiguation rule
(`parse_pattern.rs`) only treats 2+-segment names as unambiguously enum
(`Result.Err { code }`), so a 1-segment name with braces always parses as
`PatternKind::Struct`, never `PatternKind::Enum`, regardless of what it
turns out to name. Fixed the same way, in both places: sema's
`PatternKind::Struct { name: Some(n), .. }` arm and the interpreter's
matching arm each now check the scrutinee/value's enum-ness first and
reinterpret accordingly.

### Where it lives

- `type_infer.rs`: `VariantShape`/`PatternCoverage` types,
  `enum_variants: HashMap<DefId, Vec<(String, VariantShape)>>` populated by
  a rewritten `collect_enum_sig` (also where the discriminant/mixing checks
  live), `check_pattern`/`check_enum_pattern`/`check_enum_payload_fallback`/
  `check_match_exhaustiveness`, wired into both `StmtKind::Match` and
  `ExprKind::Match`. Expression-position access
  (`ExprKind::Field`/`ExprKind::Call`/`ExprKind::StructLit`) extended for
  bare fieldless access, tuple-payload construction, and struct-payload
  construction respectively — each recognized syntactically at the same
  "no general field table yet" call sites `Pool.new()`/`List.new()`
  already use, not through a new general mechanism.
- `name_resolution.rs`: one real Pass-1 bug fixed alongside this —
  `PatternKind::Enum`'s handler unconditionally called the strict,
  top-level-only `resolve_qual_path` even for a 1-segment path with a
  payload (`Ok(value)`), which fails immediately since `Ok` is a nested
  variant of `Result`, never a top-level declaration on its own. Now skips
  resolution for the 1-segment case, deferring to Pass 2's
  `check_enum_pattern` the same way multi-segment paths already defer
  their later segments.
- `error_management/errors/types/mod.rs`: four new `TypeError` variants,
  `TYPE-109`–`112` — `UnknownVariant`, `VariantArityMismatch`,
  `NonExhaustiveMatch`, `MixedDiscriminantAndPayload`.
- `interpreter/eval/mod.rs`: `Interpreter::enum_table`'s type generalized
  from a flat `HashSet<String>` (fieldless-only) to
  `HashMap<String, HashMap<String, VariantKind>>`, so it can tell the
  interpreter which construction path (`Fieldless`/`Tuple`/`Struct`) a
  given variant needs — previously only fieldless variants were even
  representable at runtime; tuple/struct payload construction was a
  literal `// TODO` with payload-carrying variants silently excluded.
- `interpreter/eval/{expr,pattern}.rs`: tuple-payload construction
  (`Result.Ok(5)`, in `eval_call_with_receiver`) and struct-payload
  construction (`Message.Move { x = 1, y = 2 }`, in `ExprKind::StructLit`)
  both implemented for the first time. `eval::pattern`'s existing
  `PatternKind::Enum` matching logic (both payload shapes) was already
  fully correct — only `PatternKind::Ident` and `PatternKind::Struct`
  needed the enum-reinterpretation fix above.

### Known gaps

- **Generic enums** (`Option<T>`/`Result<T,E>`-shaped) — not covered, see
  above. Deliberately scoped out, not an oversight.
- **Discriminant values aren't tracked past validation.** `collect_enum_sig`
  confirms a written discriminant expression is int-shaped and enforces the
  mixing restriction, but doesn't compute or store the resulting ordinal
  anywhere — `Value::Enum`'s runtime representation has no slot for one, and
  there's no `as int` cast to read it back if it did. Discriminant variants
  behave identically to fieldless ones everywhere. Real follow-up if/when
  something needs to consume the actual chosen integer (FFI, serialization,
  explicit ordinal comparison).
- **No explicit `Pool<T>`/`Handle<T>`-style surface type-annotation parity
  check was needed here** — enums didn't inherit that particular gap, since
  `EnumName` was already a real `TypeKind`-adjacent nominal type via
  `SemaType::Named`, not a new builtin needing its own grammar variant.
