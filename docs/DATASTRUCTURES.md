# Data Structures — Design & Discussion Notes

**Status:** Decisions below are locked in (discussed and confirmed
directly). `Pool<T>`'s `.growable()`/`.fifo()`/iteration (§1),
`InlineList<T>` (§5), `Linqerizer<T>` (§6), and `Unique<T>`/`Shared<T>`/
`SyncShared<T>`'s type-level wiring (below; see `MEMORY_MODEL.md` §9 for
the full writeup) are now implemented — everything else is still just
the design record. This is also the deferred item already flagged in a prior
session's handover: `Hive<T>`, `Unique<T>`/`Shared<T>`/`SyncShared<T>`,
FFI wrappers (`FfiSpan`/`ExternBuffer`/`MemGuard`), `Span<T>`, and
tier-aware `List<T>` backend switching were all noted as "deserves a
dedicated design document rather than ad-hoc conversation" — this is
that document.

## Decisions locked in

| Question | Decision |
|---|---|
| Rust `Box<T>` equivalent naming | **`Unique<T>`** — not `Heap<T>`. The family is `Unique`/`Shared`/`SyncShared`; `Unique`/`Shared`/`SyncShared` all describe *ownership model*, which is the axis that actually matters (and is what the borrow checker cares about). `Heap<T>` would break that pattern by describing *location* instead — and arena/pool are heap-backed too, so "heap" doesn't even uniquely distinguish this type from Ubel's other tiers the way "unique ownership" does. **Type-level wiring implemented** — three new `SemaType` variants, orthogonal to the tier wrappers the same way `Reference` already is, not nested inside `OwnedRef`. Real type annotations (`Unique<T>`, struct fields, return types) resolve, unify, and display correctly; `Unique<T>`/`Shared<T>` confirmed *not* structurally interchangeable even with an identical inner type. No construction syntax, no runtime `Value` representation, and no tier restriction yet — none of those have enough surrounding surface to design meaningfully until the move checker (still ahead) needs them. Full writeup: `MEMORY_MODEL.md` §9. |
| FFI boundary wrapper naming | **`FfiSpan`** |
| Hive-shaped capability | **Extended `Pool<T>`, no separate `Hive<T>` type — implemented.** `.growable()` (block-chained: a new `count`-sized block appended on exhaustion, existing blocks never reallocated/copied) and `for x in pool { }` (skipfield-style, holes skipped — no `.iter()` method needed, matching every other collection). Plus `.fifo()` (oldest-freed-first reuse, opt-in) alongside the growability work since it touched the same free-list code. Matches the project's existing precedent of unifying rather than duplicating (`pool<T>(count)`/`Pool<T>` themselves were explicitly unified for the same reason). |
| `List<T>` arena-tier `.remove()` | **Swap-remove.** O(1), no reclamation needed (fits arena's append-only nature). Reorders the list, which invalidates a *plain index* into the swapped element — pair with `Handle<T>`-style generational references (same pattern `Pool<T>` already uses) from the start rather than raw indices now and a migration later. |
| Fixed-capacity inline vector | **`InlineList<T>` — implemented, its own structure, not a face of `Pool<T>`.** `Pool<T>` is inherently heap-backed (`PoolData.blocks` is a `Vec<Vec<Option<Value>>>`; `Handle<T>` exists specifically to indirect into that heap allocation). An inline vector's entire premise is the opposite — storage lives on the stack or embedded directly in a parent struct, zero heap allocation, zero indirection. Capacity ended up as a checked constructor argument (`InlineList.new(64)`) rather than a type-level parameter — see §5. |
| LINQ replacement naming | **`Linqerizer<T>` — implemented.** Beat `Query<T>`/`Pipeline<T>` on the same "the language already has a sense of humor about itself" precedent as `summon` for imports — a real, already-shipped keyword, not a hypothetical. Method vocabulary stayed LINQ-flavored (`.where()`/`.select()`, not `.filter()`/`.map()`) on the same theory. See §6. |
| "Static Core, Dynamic Shell" | **Not real, established terminology anywhere** — searched two different phrasings (`"static core" "dynamic shell" ECS`, `"static core dynamic shell" compiler`), nothing in any engine, language, or writeup uses this term. Almost certainly flavor-text framing (from the earlier external conversation) for a real, describable concept — the compiler transparently batching clean OOP-looking code into flat ECS storage — dressed up with a name that sounds established but isn't. Recommendation: drop the phrase; describe the mechanism plainly if/when it gets written up, rather than anchoring to a borrowed name nobody else will recognize. |

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

**(a) Growable without a reallocation-copy — implemented.** `PoolData`
was a plain `Vec<Option<Value>>` (`crates/core/src/interpreter/value.rs`)
— a naive `.growable()` implemented as "just `Vec::resize`" would **not**
have dangled any `Handle<T>` (indices stay valid across a resize, unlike
a raw pointer), but would've paid a real O(n) copy on every grow and
ruled out ever handing out a raw, stable address later (relevant to (b)
and to the FFI section). Went with genuine block-chaining instead —
`PoolData.blocks: Vec<Vec<Option<Value>>>`, a fresh `count`-sized block
appended on exhaustion when `.growable()` was called, existing blocks
never move or get copied — `.growable()` for real, no copy cost, and it
leaves a future raw-address FFI escape hatch possible without a redesign.

**(b) Skipfield-style fast iteration — implemented.** `for x in pool { }`
now works directly (no `.iter()` method — matches every other collection,
none of which have one either), walking every occupied slot in index
order via `PoolData::iter_occupied`, holes skipped entirely — the actual
skipfield behavior, not hand-rolled per call site.

### Decision

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
`MemGuard` naming discussion from an earlier session — **naming resolved:
`FfiSpan`.** Still genuinely open: whether it lives as its own type or as
a validated construction path into an existing one (e.g. a checked way
to build a `Span<T>`-if-that-ships, or a checked way to build a raw
`@tier(low)` slice) — not a naming question, an architecture one, not
addressed yet.

---

## 4. `List<T>` tier-chameleon behavior

The core idea — one `List<T>` syntax, tier-appropriate backend chosen by
the compiler (GC-backed array in `@tier(high)`, arena/pool-backed in
`@tier(mid)`, raw manual pointers in `@tier(low)`) — is consistent with
the design philosophy already established for `Pool<T>` and the tier
system generally: the developer writes one thing, the compiler picks the
mechanism the declared tier actually allows. Worth taking seriously as a
real direction, not just a nice-sounding idea.

**Resolved: swap-remove.** O(1), fits arena's append-only nature exactly
(no reclamation needed), no new mechanism to build beyond what a plain
index-swap already is. The real cost — reordering invalidates a *plain
index* into the swapped element — is handled by using `Handle<T>`-style
generational references for anything that needs to survive a `.remove()`
elsewhere in the list, rather than raw indices, from the start. This
also means the "is this actually `Pool<T>` under a different name"
question raised while weighing tombstone/skipfield against swap-remove
turned out not to matter — swap-remove sidesteps needing a skipfield on
`List<T>` at all; the skipfield idea stays scoped to `Pool<T>`'s own
`.iter()` (§1), not duplicated here.

---

## 5. Fixed-capacity inline vector — `InlineList<T>`, implemented

**Confirmed as its own structure**, not a face of `Pool<T>` — see the
decision table at the top for the concrete reason (`Pool<T>` is
inherently heap+indirection-backed; an inline vector's entire premise is
the opposite).

**Naming:** `InlineList<T>` — reads consistently next to `List`/
`Dictionary`/`Pool`, signals both "List-like" and the storage
distinction in one word.

**Capacity is deliberately not part of the type.** `InlineList<T, N>`
(capacity as a type-level parameter) was the original framing, but it
turned out not to be necessary — two `InlineList`s with different
capacities don't need to be different *types*, the same way `List<T>`'s
current length isn't part of its type either. Capacity is a checked,
literal-integer-only *constructor argument* instead — `InlineList.new(64)`
— sema-validated directly against the argument's own AST node (must be
`ExprKind::Lit(Literal::Int(n))`, `TypeError::InlineListCapacityNotLiteral`
/ `TYPE-113` otherwise). This sidesteps needing any const-generics
grammar at all — confirmed the language has none anywhere
(`parse_generic_params` only ever parses `Ident (: Bound)?`, checked at
the parser level, not just sema) — while still guaranteeing the capacity
is known at compile time, which genuine inline storage requires.

**Overflow behavior:** `.push()` returns `bool` (`false` when full),
never panics, never silently truncates. Matches the *existing* codebase
convention more closely than my first instinct (`Optional`, matching
`Pool<T>.acquire()`) would have — `List<T>`'s own `pop()`/`first()`/
`last()` already return `Value::Null` on empty rather than panicking, so
"fail via a sentinel value, not a panic, for a completely ordinary
outcome" turned out to be the more established pattern already, not
something new being introduced.

**Method surface**, grounded in `List<T>`'s own actual current one
(checked directly: `len, push, pop, contains, first, last, is_empty,
reverse`) plus one new addition: `capacity()`. `for x in v { }` also
works directly, same direct-iterability as `List`/`Pool`, no `.iter()`
method anywhere in this language.

**Interpreter honesty note:** like every tier concept in this codebase,
`InlineList` is Rust-heap-backed under the hood today (there's no real
codegen yet — the tree-walking interpreter can't literally place values
in different memory regions). "Inline/stack" is currently a
*language-level contract* (bounded, checked, no silent truncation), not
yet a literal memory-placement guarantee — same honesty already applied
to how `Pool`/`Arena`/GC tiers all share Rust-heap-backed `Value`
representations today.

---

## 6. `Linqerizer<T>` — the LINQ replacement, implemented

Replaces the old `from x in expr where ... select ...` query-comprehension
grammar (removed entirely — see `docs/PARSER_RULES.md` §6 for why removal
beat keeping both). `Linqerizer<T>` is what real LINQ actually is under
the hood: `from`/`where`/`select` in C# is sugar that the compiler itself
desugars into `.Where(...)`.Select(...)` calls on `IEnumerable<T>` — the
method-chain form is the primitive, query syntax is optional decoration
on top of it. Ubel just never built the decoration layer, and ships the
primitive.

### Getting one

```ubel
let pipeline = someList.query()
```

`List<T>.query()` is the only real tier gate — `HIGH_ONLY`-gated
(`list_methods::HIGH_ONLY`), reusing the exact `MethodInWrongTier`/
`TIER-008` machinery every other `HIGH_ONLY` registry already had sitting
empty, not new tier-checking code written for this specifically. Nothing
downstream on `Linqerizer<T>` itself is individually tier-gated — by the
time you're holding one, you're already past the one checkpoint that
matters, same principle `List`'s own methods have relative to `List.new()`.

### Shape

- **Chainable** (each returns a *new* `Linqerizer`, never mutates the one
  it was called on — same reason every other LINQ-shaped thing works this
  way, C#/Rust iterators/JS included): `.where(predicate)`,
  `.select(selector)`, `.order_by(key_selector)`, `.order_by_desc(key_selector)`.
- **Terminal** (materializes — runs every pending op against the
  snapshot, in order): `.to_list()` → `List<T>`, `.first()` → `T` or
  `null` (same "nothing there" convention `pop`/`first`/`last`/`get`
  already use), `.count()` → `int`, `.group_by(key_selector)` →
  `Dictionary<K, List<T>>`.

`.select()` and `.group_by()` both have a return type that depends on the
*argument's* own inferred type — the selector's return type, not
anything fixed about the receiver — which the generic
`(MethodReturn, ReceiverWrap, TypeId)` return-shape machinery every other
method here uses has no way to express. Both are intercepted in
`type_infer.rs` *before* that generic path runs, extracting the already-
inferred type of the lambda argument directly. Same relationship
`Pool.new()`'s own constructor special case already has to the generic
`builtin_constructor_type` path — bespoke handling for the one shape that
genuinely needs it, not a new general mechanism.

### Laziness

`source` (a snapshot, taken once at `.query()` time) and `ops` (a list of
pending operations) are stored separately. Chaining only ever appends to
`ops` — nothing runs. A terminal call is what actually walks `source`
applying every op in order. That's the real, deferred-until-materialized
property the old `eval_linq` never had (it was a single eager pass, done
the moment the whole query expression was evaluated).

### `group_by` is real this time

The old `eval_linq`'s `groupby` clause was a literal `// TODO` — parsed,
type-checked, never actually implemented. `Linqerizer<T>.group_by(key)`
actually groups: walks the materialized results, buckets by key into a
real `Dictionary<K, List<T>>`, first-seen order preserved both across
groups and within each group.

### A parser fix this needed, worth remembering

`.where(...)` doesn't parse for free — `where` is a real reserved token
(match-arm guards: `pattern where expr => ...`), and `. Identifier`
parsing only accepted plain identifiers before this. Fixed narrowly, in
exactly the two places that needed it (the prefix dotted-path builder and
the postfix `.` handler in `parse_expr.rs`), not by widening `eat_ident()`
itself — that function is used far more broadly (variable names,
parameters, ...) where letting `where` through everywhere would have been
a bigger, less-audited change than this specific spot needed. Match-guard
parsing checks the raw `TokenType::Where` token directly and is completely
unaffected. Same category of problem as `get`/`set` earlier — a reserved
word blocking a name a method genuinely wanted — but this one only
surfaced once real fixtures with `.where(fn(x) ...)` as the *last* call in
a chain were actually written and run, not during design.

### Interpreter honesty note

Same as `InlineList` and everything else here — `Linqerizer<T>`'s
`source`/`ops` are plain Rust-heap structures today (`Rc<Vec<Value>>` /
`Vec<LinqOp>`), because the tree-walking interpreter has no separate
memory-region concept yet. The laziness is real (ops genuinely don't run
until a terminal call), but "HIGH-tier lazy value" isn't yet backed by a
distinct runtime representation from any other GC value — same honesty
already applied elsewhere in this document.

---

## 7. Remaining open questions

Everything raised in this conversation is resolved except:

1. **`FfiSpan`'s architecture** — own type, or a validated construction
   path into something else (§3).
