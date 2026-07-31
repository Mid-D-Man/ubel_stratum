# Ubel Stratum — Memory Model & Tier Rules

> **Canonical reference for how HIGH / MID / LOW tiers govern data structures,
> allocation, and references.**
> Spans `crates/core/src/sema/tier_check.rs`, `crates/core/src/sema/type_infer.rs`,
> `crates/core/src/sema/type_table.rs`, and any interpreter code that touches
> collections or allocators. Every contributor reads this before touching
> tier-related code.
>
> **Status legend used throughout this document:**
> - ✅ **IMPLEMENTED** — true in the codebase today, verified by direct inspection.
> - ⚠️ **GAP** — designed/expected but not enforced yet; a known hole.
> - 🔭 **PROPOSED** — a rule we are adopting going forward; no code exists for it yet.
>
> Reflects repo state as of commit `6d65dcb` (fresh clone) **plus this
> session's Gap 2 changes to `type_infer.rs`/`tier_check.rs`/`tests.rs`,
> not yet pushed as of this writing.** Re-verify against `git log` before
> relying on any ✅ claim in this doc after significant sema changes.

---

## 1. The Three Tiers, and What Actually Backs Them

| Tier | Sema reference kind | Backing model | Status |
|---|---|---|---|
| **HIGH** | `GcRef(TypeId)` | Shared, `Rc<RefCell<...>>`-style GC aliasing, no lifetime | ✅ IMPLEMENTED |
| **MID** | `ArenaRef { arena: ArenaId, mutable: bool, inner: TypeId }` | Bump-style allocation, scoped to a specific `with arena(...)` block | ⚠️ PARTIAL — type-level only, see §5–§6 |
| **LOW** | `OwnedRef { mutable: bool, inner: TypeId }` | Move semantics + borrow checker | ❌ NOT STARTED — codebase's own comment marks this "Phase 4" |

These three variants already exist in `type_table.rs` and are the correct shape —
this document is about closing the gaps around `ArenaRef`, not redesigning the
reference kinds themselves.

---

## 2. Rule: Tier Lives in the Reference, Never in the Type Name

`List<T>`, `Dictionary<K,V>`, `Queue<T>`, `Stack<T>`, and any future `Pool<T>`
are **tier-agnostic shapes**. A given instance's tier is determined entirely by
which reference kind wraps it (`GcRef<List<T>>` / `ArenaRef<List<T>>` /
`OwnedRef<List<T>>`), which is in turn determined by *where the value was
constructed* — not by anything written in the type annotation itself.

```
let a = List.new()                    // GcRef<List<T>> — default, HIGH
with arena(1MB) {
    let b = List.new()                // ArenaRef<List<T>> — MID, tagged to this arena
}
```

### Why this shape, and not Rust's

Rust bakes the allocator into the container's type parameter —
`Vec<T, A: Allocator = Global>`, backed by `RawVec<T, A>` — so `Vec<T, Global>`
and `Vec<T, MyArena>` are distinct monomorphized types. That buys zero-cost
static dispatch at the price of type proliferation (any function generic over
"a Vec" must also be generic over `A`).

Zig instead keeps one concrete container shape (`ArrayList(T)`) and passes the
allocator as an explicit runtime value — closer to what we're doing. The one
important refinement Zig's design surfaces that ours is still missing:
**Zig's managed `ArrayList` stores its allocator internally**, while the
`Unmanaged` variant requires passing it to every call. See §7 — our current
`ArenaRef` is sema-only and erased at runtime, which is the "Unmanaged"
problem without even the escape hatch of passing the arena in by hand.

**Decision:** stick with tagging the reference, not parameterizing the
container type. This matches the existing `List<T>`-reads-the-same-everywhere
goal. Documented here so it's a deliberate choice, not an unexamined default.

---

## 3. `ArenaId` Scoping — Nested Arenas Are Not Interchangeable

✅ IMPLEMENTED: each `with arena(...)` block gets its own `ArenaId` via
`push_arena()` / `pop_arena()` on an `arena_stack`. Two nested arena blocks
produce two distinct ids, even though both are "MID tier."

**Rule:** any check involving arena membership must compare the *specific*
`ArenaId`, never just "is/isn't tagged `ArenaRef`." A value tagged with arena
`#1` assigned somewhere only arena `#2` is valid in must still be rejected —
both are MID, neither is interchangeable with the other.

```
with arena(1MB) {                  // ArenaId(1)
    let outer_list = List.new()
    with arena(1MB) {              // ArenaId(2) — nested, distinct
        outer_list = List.new()    // WRONG TIER: ArenaId(2) value assigned
                                    // into an ArenaId(1)-scoped binding
    }
}
```

---

## 4. `contains_arena_ref` — How Tier-ness Propagates Through Generics

✅ IMPLEMENTED in `type_table.rs`. Recursively walks `List`, `Set`, `Queue`,
`Stack`, `Dictionary`, `Tuple`, `Slice`, `Fallible`, `Task`, `Optional`,
`Named` (user generics), and `Function` types looking for a buried
`ArenaRef`. This is the actual "arena coloring" mechanism — it's how
`List<SomeStructContainingAnArenaRef>` is correctly detected as
arena-tainted even though the taint is nested two levels deep.

No changes proposed here — this part is solid. Referenced by name in later
sections because §6 and §8 depend on it.

---

## 5. Construction-Site Tagging — GAP #1

✅ **IMPLEMENTED.** `maybe_arena_ref` — the function that wraps a
freshly-constructed value's type in `ArenaRef` when inference is currently
inside a `with arena(...)` block — was originally only called from four
places in `type_infer.rs`: array-literal, tuple-literal, dict-literal, and
struct-literal inference. It was never called from the `Call`-expression
inference path, so `List.new()` and friends type-checked as `Unknown` and
silently skipped arena tagging entirely (see history below for the original
writeup).

**Fixed via `InferCtx::builtin_constructor_type`**, a small helper matching
on the namespace name (`"List"`, `"Dictionary"`, `"Queue"`, `"Stack"` —
mirrors `constructors.rs` / `BUILTIN_NAMESPACES` exactly) that produces the
correct fresh `SemaType` for each constructor. The `ExprKind::Call` arm now
recognizes the `Field { target: Ident(ns), field: "new" }` shape, calls this
helper, and — if it matches — returns `maybe_arena_ref(ctor_ty)` instead of
falling through to the generic (still-`Unknown`-producing) call path.
General field/method resolution is untouched; this is narrowly scoped to
recognized constructor calls only.

```
with arena(1MB) {
    let a = [1, 2, 3]      // ✅ tagged ArenaRef — literal path (already worked)
    let b = List.new()     // ✅ tagged ArenaRef — call path (this fix)
}
let c = List.new()          // ✅ stays a plain List — no arena in scope
```

**Verified**, not just claimed: `cargo check --lib` clean (zero new warnings),
full `cargo test --lib -p ubel_stratum` — 54/54 passing, including 5 new
tests added to `sema/tests.rs` (`test_sema_list_new_in_arena_is_arena_ref`
and siblings for `Dictionary`/`Queue`/`Stack`, plus a negative case
confirming `List.new()` stays untagged outside any arena). This was also the
first test coverage arena coloring had at all — previously zero tests
touched `ArenaRef`, `maybe_arena_ref`, or any constructor call.

**Still open, deliberately out of scope for this fix:** the `.with_capacity()`
/ `.growable()` chain convention from `BUILTINS_RULES.md` §2 doesn't exist
yet as actual instance methods — those will hit the *same* "no field table"
problem this fix worked around for `.new()` specifically, but for arbitrary
instance method calls on an already-typed receiver. That's a separate,
larger piece of work (general method resolution), not folded into this fix.

---

## 6. Escape Boundary Enforcement — GAP #2

✅ **IMPLEMENTED.** `ArenaRefEscapesBoundary` was a fully defined error
variant — `Display` impl, span handling, everything — but was never
constructed anywhere in the codebase. Only one escape path was caught at
all, and only by accident:

- Return position was rejected, but via a generic `TypeMismatch` (declared
  `List<int>` vs. actual `&arena List<int>`) — the *dedicated*
  `MidReturnContainsArenaRef` rule checks the function's **declared**
  return type for an `ArenaRef` marker, and there is no surface syntax for
  writing `ArenaRef` in a type annotation at all, so that rule can
  structurally never fire. `TypeMismatch` was doing the real work, for the
  wrong stated reason.

Three more escape paths caught nothing at all:

- Assignment to an outer-scope binding
- Storage into a struct field on a struct that isn't itself arena-scoped
- Capture by a closure that can outlive the block

**Fixed entirely in `type_infer.rs`'s Pass 2** — not in `tier_check.rs`,
even though that file's own header comment used to claim the job. Pass 2
already tracks live arena scope via `arena_stack`/`maybe_arena_ref`, so
that's where the answer is cheapest to compute, right as each site is
inferred:

- **`InferCtx::check_assign_arena_escape`**, called from the `Assign`
  expression arm, computes each side's **specific** `ArenaId` (never just
  "tagged or not" — see §3) and compares them:
  - `Ident` target — the arena (if any) baked into the binding's
    *original* declared type. Reassignment never mutates `def_types`, so
    re-reading it always reflects the declaration site.
  - `Field { target: receiver, .. }` target — the receiver's own
    top-level `ArenaId`, if the receiver itself is arena-tagged (storing a
    same-arena value into one of its own fields is safe — both die
    together). No struct field can be *declared* `ArenaRef` (no surface
    syntax exists for it), so an untagged receiver's home arena is always
    `None`, making any arena-tagged value assigned into one of its fields
    unconditionally an escape.
  - `Index { target: container, .. }` target — same idea, using the
    indexed container's own top-level `ArenaId`.
- **`structurally_compatible`** gained a same-arena-required
  `ArenaRef`↔`ArenaRef` arm. This was also a real, previously-undetected
  bug in its own right: with no arm for the pair at all, two `ArenaRef`s
  with the **identical** `ArenaId` were never recognized as compatible
  either, so ordinary, safe same-arena reassignment
  (`with arena(1MB) { let mut x = List.new(); x = List.new() }`) would
  have spuriously failed with a generic `TypeMismatch` the moment anyone
  wrote it.
- **`unify`** now prefers `ArenaRefEscapesBoundary` over the generic
  `TypeMismatch` fallback whenever either side of a failed unification
  carries the tag — this is what upgrades the return-position diagnostic
  "for free," with no changes needed to `tier_check.rs` at all.
- **Closures** — every lambda built inside a `with arena(...)` block is
  now conservatively `maybe_arena_ref`-tagged too, the same as any other
  constructed value. Lexical scoping guarantees a lambda can only
  reference an arena-scoped local from inside that local's own block, so
  this over-approximates (a closure that captures nothing arena-related
  still gets tagged) but never under-approximates — once tagged, the
  lambda value rides the exact same assignment/field/return checks as any
  other arena-scoped value. No separate free-variable capture analysis
  was needed.

```
let mut outer = List.new()          // ❌ was uncaught, now rejected
with arena(1MB) {
    outer = List.new()              // ArenaRefEscapesBoundary
}

with arena(1MB) {                   // ArenaId(1)
    let mut a = List.new()
    with arena(1MB) {               // ArenaId(2) — distinct arena
        a = List.new()              // ❌ was uncaught, now rejected
    }                                // (compared by specific id, per §3 —
}                                    //  both are "tagged", but not the same one)

with arena(1MB) {
    let mut a = List.new()
    a = List.new()                  // ✅ same arena both times — legal,
}                                    //    and no longer spuriously rejected
                                     //    by the structurally_compatible fix

struct Holder { data: List<int> }
let holder = Holder { data = List.new() }
with arena(1MB) {
    holder.data = List.new()        // ❌ was uncaught, now rejected
}
```

**Verified**, not just claimed: `cargo check --lib` clean, full
`cargo test --lib -p ubel_stratum` — 56/56 passing (54 prior + 2 new:
`test_sema_arena_escape_outer_binding_rejected` and
`test_sema_arena_same_arena_reassign_ok`, the latter a regression guard
for the `structurally_compatible` fix specifically). Four new end-to-end
fixtures added and confirmed failing sema exactly as intended
(`err_arena_escapes_outer_binding.ubl`, `err_arena_escapes_struct_field.ubl`,
`err_arena_escapes_nested_mismatch.ubl`, `err_arena_escapes_closure_capture.ubl`),
plus one new fixture confirming the legitimate same-arena case still
passes end to end (`ok_arena_same_arena_reassign.ubl`). All 19
pre-existing fixtures re-verified unaffected.
`err_mid_fn_returns_arena_value.ubl`'s header comment updated — it now
documents (and the fixture confirms) that it fails via the dedicated
`ArenaRefEscapesBoundary`, not the old accidental `TypeMismatch`.

**Still open, deliberately out of scope for this fix:** the escape check
is deliberately conservative rather than precise — it has no liveness
analysis, so it can flag a pattern that's arguably safe if the escaped
value is provably never read again (see the code comment on
`check_assign_arena_escape` for the specific untyped-parameter case this
affects). Over-flagging a borderline-safe pattern is the right default
for a memory-safety check; a real borrow checker makes the same tradeoff.
Passing an arena-tagged value as an ordinary function **argument** is
still unenforced — not one of the three escape paths this gap targeted,
and there's a separate, pre-existing gap that call arguments aren't
type-checked against declared parameter types at all yet (see §12/open
gaps list in the handover notes), so there's currently no mechanism by
which an argument's tag could propagate into the callee's own reasoning
regardless.

---

## 7. Runtime Representation — What's Real Now, What's Deferred

**🔭 PROPOSED, explicit decision:** sema (§2–§6, §8) fully enforces the tier
*contract*. The tree-walking interpreter is permitted to keep sharing one
Rust-heap-backed `Value` representation across all three tiers for now — no
`Value::ArenaList` yet. Physical memory-layout divergence (real bump
allocation for MID, moved/owned buffers for LOW) is deferred to LLVM
lowering, a phase that doesn't exist yet. Building three parallel runtime
collection engines by hand in the interpreter, before there's an LLVM backend
to benefit from the performance difference, would be wasted motion.

**One forward-looking requirement carried over from the Zig research (§2):**
whenever a MID-tier runtime representation *does* get built, it must carry a
live handle back to its originating arena — not just a type-level tag that
disappears after sema. This is required for §8's "allocate the result into
the same arena as the receiver" rule to be implementable at all. Flagging it
now so it isn't designed away later.

---

## 8. Method Availability Per Tier ("Method Tier Inheritance")

✅ **Existing precedent:** `await` (`ExprKind::Await`) and LINQ
(`ExprKind::Linq`) are already hard-gated to `@tier(high)` functions only, at
the expression level.

✅ **Implemented.** Getting here required more than the two rules below —
builtin instance methods (`myList.push(x)`, `"s".to_upper()`, ...) had *no*
type checking at all beforehand. `ExprKind::Field`'s generic arm had a literal
`// TODO: field table` and always inferred `Unknown`, and
`instance::method_names(ReceiverKind)` — meant to be sema's source of truth
for valid method names — said so itself: "used by sema (once wired in)." It
wasn't. So this section's actual scope ended up being: build real
method-call type checking (`instance::resolve_receiver` / `signature` /
`is_high_only`, wired into `type_infer.rs`'s `ExprKind::Call` handler —
`NoSuchMethod` and `ArgumentCountMismatch` now fire for real, not just at
runtime), *then* the two rules below on top of it.

Lives in `type_infer.rs` (Pass 2), not `tier_check.rs` alongside
await/LINQ's otherwise-identical rule — `tier_check` (Pass 3) has no
`Unifier` of its own, and `expr_types` entries can still be raw unresolved
`Var`s at record time, so resolving a receiver's type correctly needs the
live `apply()` Pass 2 already has. `InferCtx` gained a `current_tier` field
(mirroring `tier_check`'s) for exactly this reason.

**Rule, HIGH-only methods:** a MID- or LOW-tier collection should reject any
HIGH-only method (LINQ-adjacent, anything requiring GC-only semantics) as a
sema error — not silently allow it, and not let it become a runtime
surprise. Mechanism is real and verified (`TierError::MethodInWrongTier`,
`TIER-008`) — confirmed firing correctly by temporarily flagging a real
method HIGH-only and running it from `@tier(mid)`. But every
`HIGH_ONLY` const across all six receiver kinds is empty today: nothing
currently implemented is actually LINQ-adjacent or GC-only (checked all ~43
methods across List/Str/Dict/Tuple/Queue/Stack). So the rule is real
infrastructure, not a stub, but inert until a method that needs it exists.

**Rule, allocation-producing methods:** any instance method that constructs
a *new* collection or value from an existing receiver (`to_upper`,
`to_lower`, `trim`/`trim_start`/`trim_end`, `split`, `replace`, `chars` on
`Str`; `keys`, `values` on `Dictionary`) must allocate that result into the
**same `ArenaId`** as the receiver, not a fresh, untracked allocation —
`instance::MethodReturn::allocates()` marks exactly these, and
`type_infer.rs`'s `method_return_type`/`apply_receiver_wrap` re-wrap the
result in the receiver's own `ReceiverWrap` (`Gc`/`Arena`/`Owned`) rather
than defaulting to untagged. Verified end-to-end: constructing a
`Dictionary` inside `with arena(...)`, calling `.keys()` on it, and
assigning the result to a binding declared outside the block correctly
raises `ArenaRefEscapesBoundary` — proving the tag survived the method
call. Regression-guarded by
`tests/fixtures/err_arena_escapes_via_method_result.ubl`. This is the
concrete tie-in to §7's "arena handle at runtime" requirement — resolved at
the sema/type level only, per §7's own scope; the interpreter still shares
one untyped `Value` representation across tiers.

One further correctness fix fell out of building this for real:
`pop`/`first`/`last`/`dequeue`/`peek` (List/Queue/Stack) and `at`
(Dictionary) all fall back to `Value::Null` on an empty/missing receiver at
runtime (`unwrap_or(Value::Null)` in every case) — so their sema return type
had to become `Optional(elem)`, not bare `elem`, or `x.pop() == null` (the
documented way to check for "empty") would fail a real `TypeMismatch` that
didn't exist before this section had any type checking at all.
`structurally_compatible` gained two matching rules: `Optional<T>` accepts
`Null` unconditionally, and compares directly against a bare `T` (not just
`Optional<T>`) — the latter needed for the many existing call sites like
`items.first() == 10` to keep working.

---

## 9. LOW Tier — Explicit Non-Support Until Phase 4

`OwnedRef` and the borrow checker are marked "Phase 4" in the codebase's own
comments, and nothing has been built for either yet.

✅ **Implemented.** A `@tier(low)` function that constructs a builtin
collection (`List.new()`, `Dictionary.new()`, `Queue.new()`, `Stack.new()`)
now produces a clear, explicit sema error instead of silently falling
through to HIGH-tier (`GcRef`) behavior nobody has actually designed for LOW.

Lives in `type_infer.rs`, at the exact point `builtin_constructor_type`
already resolves — right before `maybe_arena_ref` would otherwise hand back
a bare, untagged type. Checks `self.current_tier == TierAnnotation::Low` and
emits `TierError::CollectionConstructionInLowTier` (`TIER-009`) if so. Same
emit-and-continue pattern as `ArenaInWrongTier`/`MethodInWrongTier` —
deliberately *not* returning `Unknown`, so one disallowed constructor call
doesn't cascade spurious follow-on errors through the rest of the function.
Verified via `tests/fixtures/err_collection_new_in_low_tier.ubl`.

Building this fixture's expected error message surfaced an unrelated,
pre-existing bug: `SemaType::display()` (the function every diagnostic uses
to render a type) fell through a silent `_ => "<type>".into()` catch-all for
most non-trivial types — `Dictionary`, `Queue`, `Stack`, `Named` (every
struct/enum), `Function` (every closure), and more. Every existing arena-
escape fixture happened to use `List`, so the gap had never been exercised.
Fixed alongside this section (every `SemaType` variant now has an explicit
arm, no catch-all) since §8's `ArenaRefEscapesBoundary` messages and this
section's own `TIER-009` message both depend on `display()` being honest.
Full case study, including why `Named` needed a new `symbols: &SymbolTable`
parameter threaded through, lives in `DIAGNOSTICS_RULES.md`.

---

## 10. `Pool<T>` — Design Principles

Not yet implemented. These are the constraints agreed on so far, to design
against once Gaps 1–2 and §8's method-tier filtering are in place.

1. **Tier-agnostic name, tier via wrapper** — same rule as §2. `Pool<T>` is
   one shape; `GcRef<Pool<T>>` / `ArenaRef<Pool<T>>` are what differ.
2. **Fixed capacity by default; `.growable()` opts in.** Matches the existing
   `pool<Type>(count)` allocator statement's "count" semantics — no tension
   between the low-level allocator primitive and the high-level collection.
3. **LIFO free list by default; `.fifo()` opts in.** LIFO is the
   least-surprising default (matches most general-purpose object pools:
   .NET's `ObjectPool<T>`, most C++ game pools) — cache-friendly, same slot
   stays hot. FIFO is the deliberate opt-in for delayed-reuse needs (entity-ID
   recycling, avoiding same-frame slot reuse while another system still holds
   a stale reference).
4. **Generational handles by default; raw-index available as an explicit
   opt-out.** A raw-index pool has the classic ABA problem — an old handle
   silently reading a different live object after its slot gets recycled.
   Generational handles (`(index, generation)`, `slotmap`/`generational-arena`
   style) make staleness detectable for the cost of one extra counter per
   slot. This is also the pattern Mid Engine's not-yet-built entity allocator
   will want, so getting it right here is directly reusable there.
5. **Must NOT have `sync.Pool`-style GC-eviction semantics.** Go's `sync.Pool`
   may silently clear entries between GC cycles (softened only by a
   "victim cache" giving roughly one extra cycle) — it exists for fungible,
   re-creatable scratch objects, never for anything with real identity. A
   checked-out or idle-but-reserved slot in Ubel's `Pool<T>` must persist
   deterministically until explicit release or pool teardown. Worth stating
   outright given the name overlap invites the wrong mental model.
6. **Naming — leaning toward unifying with the existing `pool<T>(count)`
   allocator statement**, rather than treating the name collision as
   something to avoid. Proposal: `pool<T>(count)` stays the low-level
   allocator primitive (how the memory gets carved out); `Pool<T>` becomes
   the ergonomic value-level handle you acquire/release against, scoped to
   that `with pool<T>(count) { }` block — same relationship `with arena(...)`
   already has to whatever lives inside it. **Not yet finally decided** — the
   alternative (rename the collection to `ObjectPool<T>` or similar to avoid
   any collision at all) is still on the table.

---

## 11. Implementation Order

1. ✅ **Gap 1** (§5) — wire constructor calls through `maybe_arena_ref`. Done,
   verified via `cargo test --lib` (54/54 passing).
2. ✅ **Gap 2** (§6) — general `ArenaRefEscapesBoundary` emission
   (assignment / field / indexed storage / closure capture), compared by
   `ArenaId`. Done, verified via `cargo test --lib` (56/56 passing) plus
   5 new end-to-end fixtures.
3. ✅ **§8** — tier filter on instance method dispatch; arena-consistent
   allocation for result-producing methods. Done, verified via
   `cargo test --lib` (58/58 passing) plus 3 new end-to-end fixtures —
   ended up including building real method-call type checking from
   scratch, which turned out not to exist yet (see §8's own writeup).
4. ✅ **§9** — explicit LOW-tier rejection for collection construction.
   Done, verified via `cargo test --lib` (58/58 passing, unchanged) plus
   1 new end-to-end fixture — also surfaced and fixed an unrelated
   `SemaType::display()` catch-all bug affecting every diagnostic that
   renders a non-trivial type (see §9's own writeup).
5. **Only then** — design and implement `Pool<T>` (§10) on a tier contract
   that's actually trustworthy underneath it. **Next up.**

---

## 12. Open Decisions

| # | Question | Status |
|---|---|---|
| 1 | Unify `pool<T>(n)` / `Pool<T>` (§10.6), or rename the collection to avoid the collision entirely? | Open |
| 2 | Generational handles as opt-out default, or opt-in? | Leaning opt-out (default = generational) |
| 3 | Does a `@tier(mid)` function require an explicit `with arena(...)` for every collection, or should entering a `@tier(mid)` function implicitly open a function-scoped arena? | Open |
| 4 | Does `with pool<T>(n)` get the same `@tier(mid)`-only restriction `with arena(...)` currently has (`ArenaInWrongTier`), or should fixed-capacity pools be legal from HIGH-tier code too (an "unsafe-block"-style local optimization)? | Open |
