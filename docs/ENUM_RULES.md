# Ubel Stratum — Enum Rules

> **Canonical reference for enum grammar, AST, runtime representation, and
> (once built) sema.** Every contributor reads this before touching enum
> declaration parsing, `EnumVariantPayload`, `Value::Enum`, or
> `EnumName.Variant` resolution.
>
> **Status legend:** ✅ IMPLEMENTED · ⚠️ GAP · 🔭 PROPOSED
>
> Reflects repo state as of commit `6d65dcb` (fresh clone).

---

## 1. Current Status, Layer by Layer

| Layer | Status | Notes |
|---|---|---|
| Grammar (`docs/ubel.ebnf`) | ✅ IMPLEMENTED | Full Rust-shaped variant grammar, see §2 |
| AST (`crates/core/src/ast`) | ✅ IMPLEMENTED | `EnumVariantPayload` has all four Rust shapes |
| Runtime (`interpreter/value.rs`) | ✅ IMPLEMENTED | `Value::Enum` mirrors the AST exactly |
| Sema (`crates/core/src/sema`) | ❌ NOT STARTED | Zero references to `EnumVariantPayload` or `EnumPayload` anywhere in `crates/core/src/sema` |

The headline finding: **structurally, our enums are close to a 1:1 match for
Rust's.** This is not the flat "named integer constant" model DixScript uses
(`AIType { PASSIVE = 0, AGGRESSIVE = 1, BOSS = 2 }`, no payload capability at
all) — ours support full tagged-union / sum-type variants. The gap is
entirely in sema: nothing resolves, validates, or enforces any of this yet.

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

## 3. Decisions Needed Before Sema Work Starts

### 3.1 Implicit discriminant auto-increment

Rust behavior: a unit variant without an explicit `= value` gets the next
integer in declaration order (0, 1, 2, ... unless a prior explicit value
resets the sequence). DixScript's enums do the same thing.

**🔭 PROPOSED:** adopt the same behavior — `EnumVariantPayload::None` without
a preceding `Discriminant` gets an implicit ordinal assigned during name
resolution, continuing from the last explicit `Discriminant` seen (or 0, if
none yet).

### 3.2 Can `Discriminant` and payload-bearing variants coexist in one enum?

Rust **disallows** this — explicit `= value` discriminants are only legal on
fieldless (C-like) enums; the moment any variant carries a payload, no
variant in that enum may have an explicit discriminant.

Our grammar does **not** currently forbid mixing them structurally — nothing
stops writing:

```
enum Foo {
    A = 1,
    B(int),
}
```

**🔭 PROPOSED:** match Rust's restriction — reject this combination as a sema
error once enum sema exists, rather than allowing it and having to define
what it even means at the runtime representation level (would `A`'s
`EnumPayload::None` need an implicit associated integer that coexists with
`B`'s `EnumPayload::Tuple`? Simpler to just disallow the mix, matching
established precedent). **Not yet finally decided** — flagging because the
grammar's permissiveness here is a real question, not an oversight to quietly
paper over.

### 3.3 `EnumName.Variant` path resolution

Not started. Name resolution needs to recognize `EnumName.Variant` as a valid
expression referring to a specific enum constructor (matching the existing
`ImportedEnumAccess`-style pattern already used for imported enums in the
DixScript reference implementation, though Ubel's own resolution pass for
this doesn't exist yet).

### 3.4 Exhaustiveness checking

Not started. Whatever Ubel's `match`/switch construct ends up being (the
grammar and parser rules don't fully settle this yet), exhaustiveness
checking against all declared variants of an enum is standard sema work that
hasn't begun.

---

## 4. Open Decisions

| # | Question | Status |
|---|---|---|
| 1 | Implicit discriminant auto-increment — adopt Rust's behavior as proposed in §3.1? | Leaning yes |
| 2 | Disallow mixing `Discriminant` with `Tuple`/`Struct` variants in one enum, matching Rust? | Leaning yes, not finalized |
| 3 | Order of implementation: name resolution (§3.3) before or after discriminant assignment (§3.1)? | Open |
| 4 | Does exhaustiveness checking (§3.4) block on the `match`/switch grammar being settled first? | Open |
