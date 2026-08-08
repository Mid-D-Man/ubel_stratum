# Data Structures — Design & Discussion Notes

**Status:** Discussion only. Nothing in this document is implemented or
decided — it's the promised home for the data-structure conversation
that kept surfacing mid-session, so it stops being ad-hoc. This is also
the deferred item already flagged in a prior session's handover: `Hive<T>`,
`Unique<T>`/`Shared<T>`/`SyncShared<T>`, FFI wrappers (`FfiSpan`/
`ExternBuffer`/`MemGuard`), `Span<T>`, and tier-aware `List<T>` backend
switching were all noted as "deserves a dedicated design document rather
than ad-hoc conversation" — this is that document.

---

## 1. `std::hive` — what it actually is, and how it maps onto Ubel

`std::hive` (née `plf::colony`) is real and landing in C++26. The
pasted description was accurate on the mechanics: block-based storage
(new block on overflow, never a single-array reallocation), a skipfield
for O(1)-amortized iteration over live slots only, slot reuse on erase,
and — the part that matters most for comparing it to anything Ubel
already has — **strict raw-pointer stability**. An address handed out by
`std::hive` stays valid until that specific element is erased, full stop.

### This already substantially overlaps with `Pool<T>`, not with arena

`Pool<T>` (implemented, `docs/MEMORY_MODEL.md` §10) already provides
fixed-capacity slots, LIFO reuse via a free list, and — the load-bearing
difference from raw pointers — **generational `Handle<T>` stability**:
a `Handle { index, generation }` safely detects staleness instead of
dangling. This is the same problem `std::hive`/`plf::colony` solves for
C++ (where the borrow checker doesn't exist and raw pointers are the only
option), solved the way Rust's own ecosystem actually solves it —
the pasted analysis's own "Rust doesn't have a 1:1 equivalent, but
slotmap/generational-arena are the spiritual successors" point is
correct, and **Ubel already built the spiritual successor**, not as a
future idea but as shipped code.

So the two real, separable pieces of "add Hive" are:

**(a) Growable without a reallocation-copy.** `PoolData.slots` is
currently a plain `Vec<Option<Value>>` (checked directly:
`crates/core/src/interpreter/value.rs`) — a naive `.growable()` (already
flagged as an unbuilt gap in §10 item 2) implemented as "just
`Vec::resize`" would **not** dangle any `Handle<T>` — indices stay valid
across a resize, unlike a raw pointer. What it would cost is a real
O(n) copy on every grow, and it would rule out ever handing out a raw,
stable address (relevant to (b) below and to the FFI section). A
block-chained pool (new block on overflow, existing blocks never move
or copy) gets `.growable()` for real, gets it without the copy cost, and
additionally makes a future raw-address FFI escape hatch possible later
without a redesign.

**(b) Skipfield-style fast iteration.** This one's genuinely new — Pool<T>
today has no bulk-iteration story at all, only point access
(`.acquire()`/`.release()`/`.at()`). A component table with mostly-live
slots and a handful of holes currently has no way to skip the holes in
one instruction; every consumer would hand-roll `for i in
0..capacity { if slots[i].is_some() { ... } }`. This is a distinct
capability from (a) — worth tracking as its own follow-up
(`.iter()` on `Pool<T>`), not bundled into "growable."

### Recommendation

Extend `Pool<T>` with these two capabilities rather than introducing a
separate `Hive<T>` type. This matches the project's own established
precedent — `pool<T>(count)` and `Pool<T>` were explicitly *unified*
rather than kept as two overlapping things (§10 item 6) — and a second,
parallel "Pool but block-chained and with a skipfield" type would
immediately raise the same "which one do I reach for" question that
unification was chosen to avoid. If growable + fast-iteration Pool<T>
ever needs a different enough shape that it can't share a type, that's
a real fork worth its own conversation then — not a reason to start with
two types now.

### A correction on the pasted material: arena is not the right place for this

The "Elevating `@tier(mid)` (Arena Memory)... introducing a Hive to
`@tier(mid)`... delete entities and recycle their memory slots mid-frame"
framing conflates two allocators the design deliberately keeps separate.
Arena is a pure bump allocator — append-only, freed in one bulk
operation when its `with arena(...)` block ends, by design, precisely
*because* it never needs per-object bookkeeping (`MEMORY_MODEL.md`).
"Delete and recycle mid-frame" is not an arena feature to add — it's
**literally what `Pool<T>` already is**. Blending recycling into arena
would blur a distinction the tier system draws on purpose. Anything
Hive-shaped belongs on `Pool<T>`, not as an arena mode.

### Syntax corrections in the pasted example

The illustrative snippet used Rust syntax that isn't Ubel's:

```rust
// As pasted — not valid Ubel:
let mut particles = Hive::<Particle>::new();
let p_ref = particles.insert(Particle { x: 0.0, y: 100.0, life: 5.0 });
```

- `Hive::<Particle>::new()` uses Rust's `::` turbofish/path syntax.
  Ubel doesn't have `::` anywhere in this position — constructors are
  called with `.`, matching every existing example
  (`Pool.new()`, `List.new()`, `Box.new(42)`).
- `Particle { x: 0.0, y: 100.0, life: 5.0 }` uses `:` for struct-literal
  fields. Ubel uses `=` — confirmed by every real fixture
  (`Rectangle { width = w, height = h }`).

If/when a growable+iterable `Pool<T>` gets built, real Ubel syntax would
read like:

```ubel
@tier(low)
fn spawn_explosion() void {
    with pool<Particle>(1024) {
        let particles = Pool.new()
        let p_ref = particles.acquire(Particle { x = 0.0, y = 100.0, life = 5.0 })
    }
}
```

### Two things I don't have grounding for

- **"Static Core, Dynamic Shell" ECS architecture** — not a term that
  appears anywhere in the project docs I have access to (`MEMORY_MODEL.md`,
  `ENUM_RULES.md`, `GENERICS_RULES.md`, the handover). Might be from an
  earlier part of the external conversation not pasted here, or might be
  the external model's own framing rather than something already agreed.
  Worth confirming before it gets treated as settled vocabulary.
- **"Your 50-line ASM bump-pointer arena"** — the arena implementation
  actually in the repo is plain Rust (a bump allocator over a `Vec<u8>`-
  style backing), not hand-written assembly. Might be a mis-recollection
  on the external model's part, or shorthand for "small and fast" taken
  too literally. Flagging so it doesn't get repeated as fact.

---

## 2. Other real data structures worth cataloging

These are legitimate, well-established structures (not fabricated), and
some map onto Mid Engine specifically rather than Ubel-the-language:

| Structure | What it's for | Where it'd fit |
|---|---|---|
| **Sparse Set** | O(1) lookup/insert/delete with 100%-contiguous dense iteration (sparse array → dense array + back-pointers). Foundational to real ECS implementations (EnTT and similar). | Mid Engine's archetype/component storage — this is closer to Mid Engine's own problem than Ubel-the-language's |
| **Fixed-capacity inline vector** (`std::inplace_vector`, C++26; `boost::static_vector` before that) | `List`-like ergonomics with storage inline (stack or parent struct), zero heap allocation, compile-time max capacity | `@tier(mid)`/`@tier(low)` short-lived per-frame buffers (e.g. gathering query results) — a real, distinct thing from `Pool<T>`, worth its own naming discussion later rather than conflating with Pool |
| **Intrusive linked list** | Next/prev pointers stored *inside* the user struct, not a separate node wrapper — zero allocation on insert/remove | Engine-internal queues (render command lists, job dependencies) — Mid Engine territory |
| **SPSC/MPMC ring buffer** | Fixed-size circular buffer, atomic read/write pointers, lock-free | Cross-thread communication (input → gameplay thread, gameplay → render thread) — Mid Engine territory |
| **Hierarchical bitset** | Bitwise flag arrays, SIMD-friendly mass filtering | Broad-phase culling, ECS query filtering — Mid Engine territory |

None of these are urgent — flagged here so they're not lost, not because
any of them are blocking something. Worth revisiting once Mid Engine's
ECS work actually starts (still not begun, per the standing project
state).

---

## 3. FFI boundary wrapper

Validated as a sound idea — this is standard, established practice in
systems programming, not a novel risk. The debug/release dual-mode
pattern described (deep checks — alignment, bounds, corruption canaries —
in debug; compiles away to a transparent zero-cost view in release)
mirrors how `debug_assert!` and similar checked-in-debug-only patterns
already work elsewhere. The claim that Rust has no single crate doing
FFI-safety + bounds-checking + SIMD-alignment all at once is accurate —
`safer-ffi` and `bytemuck`/`zerocopy` are real, separate, narrower crates;
nothing bundles all three.

One factual correction on the reasoning: unaligned-access-causes-a-
hardware-exception is true for older ARM (ARMv6 and earlier) and for
specific instruction classes (exclusive loads, some SIMD ops) on modern
ARM, but ARMv8+ actually tolerates unaligned ordinary loads/stores —
with a real performance penalty, not a crash. Doesn't change the
conclusion (alignment still matters, still worth enforcing), just softens
"triggers a hardware exception" to "sometimes crashes, always costs
performance depending on target."

This connects directly to the already-flagged `FfiSpan`/`ExternBuffer`/
`MemGuard` naming discussion from an earlier session — same open
question, still open: which name, and does it live as its own type or as
a validated construction path into an existing one (e.g. a checked way
to build a `Span<T>`-if-that-ships, or a checked way to build a raw
`@tier(low)` slice).

---

## 4. `List<T>` tier-chameleon behavior

The core idea — one `List<T>` syntax, tier-appropriate backend chosen by
the compiler (GC-backed array in `@tier(high)`, arena/pool-backed in
`@tier(mid)`, raw manual pointers in `@tier(low)`) — is consistent with
the design philosophy already established for `Pool<T>` and the tier
system generally: the developer writes one thing, the compiler picks the
mechanism the declared tier actually allows. Worth taking seriously as a
real direction, not just a nice-sounding idea.

The specific open question raised — what `.remove()` does inside a
`@tier(mid)` arena-backed list, where individual deallocation isn't a
thing arenas do — has a few real answers worth weighing against each
other later, not resolved here:

- **Disallow it outright** for arena-tier lists (compile error) — safest,
  least useful.
- **Swap-remove** (swap the last live element into the removed slot,
  shrink logical length) — O(1), no memory reclamation needed at all
  (fits arena's append-only nature exactly), but reorders the list and
  invalidates any *plain index* into the swapped element. Note this
  invalidation risk goes away if the reference type is a generational
  `Handle<T>` rather than a raw index — another point of overlap with
  `Pool<T>`'s already-solved staleness story.
- **Tombstone/skipfield** — mark removed, skip during iteration, never
  reclaim within the block's lifetime — this is the Hive idea again,
  applied to `List<T>` instead of `Pool<T>`, and raises the same
  "is this actually a `Pool<T>` under a different name" question from
  §1.

Not deciding this now — flagging the real options and how they connect
to the Hive/Pool discussion above, since they're clearly the same
underlying question wearing different names.

---

## 5. Open questions (none decided here)

1. Does `Pool<T>` get `.growable()` (block-chained) and `.iter()`
   (skipfield-style) as two separate follow-ups, or one combined round?
2. `FfiSpan` vs `ExternBuffer` vs `MemGuard` vs something else — still
   open from the earlier session.
3. Is a fixed-capacity inline vector a distinct type from `Pool<T>`, or
   does it turn out to be another face of the same underlying mechanism
   once actually designed?
4. `List<T>`'s arena-tier `.remove()` semantics — swap-remove vs
   tombstone vs disallow.
5. Whether "Static Core, Dynamic Shell" is established terminology from
   elsewhere that this doc should be using, or not.
