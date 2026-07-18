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
> Reflects repo state as of commit `6d65dcb` (fresh clone). Re-verify before
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

⚠️ **GAP.** `maybe_arena_ref` — the function that wraps a freshly-constructed
value's type in `ArenaRef` when inference is currently inside a
`with arena(...)` block — is only called from four places in
`type_infer.rs`: array-literal, tuple-literal, dict-literal, and
struct-literal inference.

**It is never called from the `Call`-expression inference path.** Concretely,
this means today:

```
with arena(1MB) {
    let a = [1, 2, 3]      // ✅ tagged ArenaRef — literal path
    let b = List.new()     // ❌ NOT tagged — call path, silently HIGH-shaped
}
```

`b` above type-checks as if it were an ordinary GC'd list. This directly
collides with the `.new().with_capacity(n)` constructor convention (see
`BUILTINS_RULES.md` §2) — MID-tier code cannot produce a correctly-tagged
collection through the intended constructor syntax, only through bracket
literals, which don't exist for every collection shape (there's no dict/queue/
stack "literal" syntax at all).

**🔭 PROPOSED rule going forward:** any call expression that resolves to a
known builtin collection constructor (`List.new`, `Dictionary.new`,
`Queue.new`, `Stack.new`, future `Pool.new`, and their `.with_*()` chain
continuations) must be arena-tagged identically to a literal when evaluated
inside a `with arena(...)` block. This is the next concrete implementation
task — tracked as **Gap 1** in the working plan.

---

## 6. Escape Boundary Enforcement — GAP #2

⚠️ **GAP, and the more serious one.** `ArenaRefEscapesBoundary` is a fully
defined error variant — `Display` impl, span handling, everything — but is
**never constructed anywhere in the codebase.**

Only one escape path is currently caught:

- ✅ **Return position** — `check_mid_return_type` rejects a `@tier(mid)`
  function returning a type containing `ArenaRef` (`MidReturnContainsArenaRef`).

Three more escape paths currently catch nothing:

- ❌ **Assignment to an outer-scope binding**
  ```
  let outer;
  with arena(1MB) {
      outer = [1, 2, 3];   // uncaught — outer now dangles once the arena drops
  }
  ```
- ❌ **Storage into a struct field on a struct that isn't itself arena-scoped**
- ❌ **Capture by a closure that can outlive the block**

**🔭 PROPOSED rule:** extend the same enforcement that already exists for
return position to all three of the above, comparing the specific `ArenaId`
(per §3) rather than a boolean "in an arena or not." This is **Gap 2**, and
it's the change that actually makes MID tier memory-safe rather than merely
type-decorated — it should follow immediately after Gap 1, since Gap 1 makes
constructor calls taggable at all, and Gap 2 is what stops the tag from being
meaningless.

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

**🔭 PROPOSED rule:** instance methods (`crates/core/src/builtins/instance.rs`,
`method_names(ReceiverKind)`) need the same treatment. A MID- or LOW-tier
collection should reject any HIGH-only method (LINQ-adjacent, anything
requiring GC-only semantics) as a sema error — not silently allow it, and not
let it become a runtime surprise.

**🔭 PROPOSED rule, allocation-producing methods:** any instance method that
constructs a *new* collection from an existing MID-tier receiver
(`.concat()`, `.filter()` returning a new list, similar) must allocate that
result into the **same `ArenaId`** as the receiver, not a fresh, untracked
allocation. Without this, the result silently reverts to being untagged and
becomes an unguarded escape the moment it's returned or stored — defeating
Gap 2's enforcement from the other direction. This is the concrete tie-in to
§7's "arena handle at runtime" requirement.

---

## 9. LOW Tier — Explicit Non-Support Until Phase 4

`OwnedRef` and the borrow checker are marked "Phase 4" in the codebase's own
comments, and nothing has been built for either yet.

**🔭 PROPOSED rule:** a `@tier(low)` function that attempts to construct any
collection should produce a clear, explicit "not yet supported" sema
diagnostic. It must **not** silently fall through to HIGH-tier (`GcRef`)
behavior — that would compile successfully into semantics nobody has actually
designed, and the failure would surface as a confusing runtime bug much later
instead of an honest compile-time message now.

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

1. **Gap 1** (§5) — wire constructor calls through `maybe_arena_ref`.
2. **Gap 2** (§6) — implement general `ArenaRefEscapesBoundary` emission
   (assignment / field storage / closure capture), compared by `ArenaId`.
3. **§8** — tier filter on instance method dispatch; arena-consistent
   allocation for result-producing methods.
4. **§9** — explicit LOW-tier rejection for collection construction.
5. **Only then** — design and implement `Pool<T>` (§10) on a tier contract
   that's actually trustworthy underneath it.

---

## 12. Open Decisions

| # | Question | Status |
|---|---|---|
| 1 | Unify `pool<T>(n)` / `Pool<T>` (§10.6), or rename the collection to avoid the collision entirely? | Open |
| 2 | Generational handles as opt-out default, or opt-in? | Leaning opt-out (default = generational) |
| 3 | Does a `@tier(mid)` function require an explicit `with arena(...)` for every collection, or should entering a `@tier(mid)` function implicitly open a function-scoped arena? | Open |
| 4 | Does `with pool<T>(n)` get the same `@tier(mid)`-only restriction `with arena(...)` currently has (`ArenaInWrongTier`), or should fixed-capacity pools be legal from HIGH-tier code too (an "unsafe-block"-style local optimization)? | Open |
