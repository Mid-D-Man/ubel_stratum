# Ubel Stratum

**Multi-Tier Systems Language**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-early%20development-orange.svg)]()
[![Built With](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Extension](https://img.shields.io/badge/source%20files-.ubl-blueviolet.svg)]()

> **"The right memory model for every function."**

---

## ⚠️ Current Status: Early Development

Ubel Stratum has completed its lexer, parser, and is actively building semantic analysis.
The language specification and AST are solidified. Source files use the `.ubl` extension.
The compiler binary is called `ublc`.

**Development Roadmap:**
1. ✅ **Phase 1**: Core design, memory model, lexer (Logos), parser (LALRPOP), arena AST
2. 🔄 **Phase 2**: Semantic analysis — name resolution, type inference, tier enforcer
3. 📋 **Phase 3**: Tree-walking interpreter in Rust
4. 📋 **Phase 4**: LLVM backend → native binary
5. 📋 **Phase 5**: Standard library, tooling, package manager

---

## 🚀 What is Ubel Stratum?

Ubel Stratum is a systems programming language that compiles to **native machine code**
via LLVM. Its defining feature is a **tier system** — every function declares which
memory strategy it uses, and the compiler enforces the rules statically:

| Tier | Annotation | Memory | Async | Use case |
|------|-----------|--------|-------|----------|
| HIGH | `@tier(high)` (default) | Garbage collected | ✅ | Business logic, I/O |
| MID | `@tier(mid)` | Arena allocated | ❌ | Parsers, hot paths |
| LOW | `@tier(low)` | Manual + borrow checker | ❌ | Systems, FFI, packets |

Functions without a `@tier` annotation default to **HIGH**. You opt *down* for
performance, not up for convenience.

### Why does this matter?

Every serious language makes one memory bet for the whole program:

| Language | Bet | Price |
|----------|-----|-------|
| Go / Java / C# | GC everywhere | Pauses, GC pressure, heap bloat |
| Rust | Ownership everywhere | Steep learning curve, slow compile |
| C / C++ | Manual everywhere | Safety bugs, UB |

Ubel lets each **function** make its own bet. GC where it's easy; arenas where it's
fast; manual ownership where it's critical. The compiler guarantees the bets never
clash at runtime.

---

## 🏗️ How the Final Product Works

**Ubel Stratum compiles to a native binary.** It is not a VM language.
It is closer to Rust or C++ in execution model than to Java or Python.

When you ship an Ubel program, the binary contains:

- **Your code** compiled to machine instructions via LLVM
- **A small GC runtime** linked in for HIGH-tier allocations (similar to Go's runtime,
  but much smaller — it only manages HIGH-tier objects)
- **An arena allocator** for MID-tier (a bump-pointer; ~50 lines of code at the asm level)
- **Nothing extra** for LOW-tier — it compiles to the same raw code Rust or C would

The result feels like Rust from the outside (fast startup, small binary, no JVM) but
with a built-in, opt-in GC for the parts of your code that need it.

### Execution flow for a real app

```
source.ubl
    │
    ▼
ublc (compiler)
    │
    ├── Lexer (Logos)
    ├── Parser (LALRPOP) → Arena AST
    ├── Name resolution       ← Phase 2 (in progress)
    ├── Type checker          ← Phase 2
    ├── Tier enforcer         ← Phase 2
    ├── Lowering to IR        ← Phase 4
    │
    ▼
LLVM IR
    │
    ▼
Native Binary
    ├── HIGH-tier functions → calls into tiny GC runtime
    ├── MID-tier functions  → bump-allocates in local arenas
    └── LOW-tier functions  → raw machine code, zero overhead
```

---

## 🔗 Inter-Tier Communication

This is the hardest part of the language to get right. Here is the precise model.

### The Core Constraint

A MID-tier function allocates data in an arena. When that arena is freed, all pointers
into it become invalid. HIGH-tier code must never hold a live pointer into a freed arena.
The type system enforces this at **compile time** — no runtime check needed.

The mechanism: any value that lives in an arena `A` has a type parameterised by the
arena's lifetime `'a` (written `&'a T`). The compiler refuses to compile any program
where `&'a T` appears in a type that outlives arena `A`.

Three sanctioned patterns for crossing the MID→HIGH boundary:

---

### Pattern 1: Callback (single result)

The safest and most common pattern. MID parses into an arena, calls your HIGH closure
with a *borrow* into the arena, the closure produces a GC-owned `R`, then the arena
is freed. The closure's borrow is strictly contained inside the arena's lifetime.

```ubl
// MID tier: fast JSON parser
@tier(mid)
fn parse_json_with<F, R>(input: string, callback: F) R
    where F: fn(&JsonView) R   // R must contain no arena refs
{
    with arena(1MB) {
        let view = build_json_view(input)  // arena-allocated
        return callback(&view)             // borrow passed in; R copied out
    }
    // arena freed here — view is gone, but R (GC-owned) lives on
}

// HIGH tier: calls MID, gets back a clean GC value
@tier(high)
fn handle_request(req: Request) Response {
    parse_json_with(req.body, fn(json) {
        // json: &JsonView — borrow only valid inside this closure
        let user_id = json.get("user_id").as_int()
        return fetch_user(user_id)   // Response is GC-owned — safe to return
    })
}
```

The type system enforces that `R` (the return type) contains no `&'arena T`. If you
try to smuggle an arena reference out through `R`, the tier-checker rejects it.

---

### Pattern 2: Iterator (streaming results)

For processing large datasets without allocating the whole result at once.
MID drives the iteration internally; HIGH only ever sees GC-owned values one at a time.

```ubl
// MID tier: produces transformed items one at a time
@tier(mid)
fn transform_each<R>(items: &[Item], f: fn(&TransformedItem) R) List<R> {
    with arena(10MB) {
        let mut results = List.new()
        for item in items {
            let transformed = expensive_transform(item)  // arena-allocated
            results.push(f(&transformed))                // R is copied to GC heap
        }
        return results  // List<R> is GC-owned; no arena refs inside
    }
}
```

---

### Pattern 3: View (read-only borrow, syntactic sugar)

When you only need to *read* MID-tier data without extracting values,
the `view` pattern gives a scoped read-only window into the arena.
This is syntactic sugar over the callback pattern and follows identical rules.

```ubl
@tier(high)
fn parse_config(path: string) Config {
    let config_view = read_toml_view(path)  // MID tier call; view scoped here
    using let v = config_view {
        let host = v.get("host").to_owned()  // .to_owned() copies to GC heap
        let port = v.get("port").as_int()    // int is Copy — always safe
        return Config { host, port }
    }
    // view freed; only Config (GC) survives
}
```

---

### What is explicitly forbidden

The compiler will **reject** these patterns at compile time:

```ubl
// ❌ Storing an arena reference in a GC-managed struct
@tier(high)
struct BadCache {
    data: &JsonView   // Error: JsonView contains arena lifetime; can't live in HIGH struct
}

// ❌ HIGH tier constructing an arena directly
@tier(high)
fn bad() {
    with arena(1MB) { ... }   // Error: 'with arena' is only valid in MID tier
}

// ❌ Returning an arena ref from a cross-tier function
@tier(mid)
fn bad_leak(input: string) &JsonView {   // Error: return type contains arena lifetime
    with arena(1MB) {
        return &parse(input)  // compiler catches this
    }
}

// ❌ await inside MID tier
@tier(mid)
fn bad_async(input: string) string {
    let result = await some_future()   // Error: await is only valid in @tier(high)
    return result
}

// ❌ LOW tier calling HIGH tier directly
@tier(low)
fn bad_low_call() {
    let x = some_high_fn()   // Error: @tier(low) cannot call @tier(high) directly
}
```

---

## 📋 Language Features

### Tier annotations

```ubl
// Default: HIGH — no annotation needed for most code
fn handle_request(req: Request) Response {
    let user = fetch_user(req.user_id)
    return Response.ok(user.to_json())
}

// Opt into MID for a hot path
@tier(mid)
fn parse_payload(body: string) ParsedData {
    with arena(1MB) { ... }
}

// Opt into LOW for systems code
@tier(low)
fn write_packet(buf: &mut [u8]) usize {
    // Raw ownership; borrow checker active
}
```

### Collections (C#-style names)

```ubl
let mut numbers = List.new()
numbers.push(1)
numbers.push(2)

let mut scores = Dictionary<string, int>.new()
scores.insert("Alice", 100)

let items = [1, 2, 3, 4, 5]          // array literal
let names = ["Alice", "Bob"]          // inferred as List<string>
```

### Unified `.` access — no `::`

```ubl
summon std.collections.List
let list = List.new()        // type-level call
list.push(42)                // instance call
// it's always . — never ::
```

### Error handling

```ubl
// ! suffix means "may fail"
fn parse_int(s: string) int! {
    // ... returns Result
}

// ? propagates errors
fn process(s: string) int! {
    let n = parse_int(s)?   // propagates on failure
    return n * 2
}
```

### Async (HIGH tier only)

Async is only allowed in HIGH tier. MID and LOW are synchronous. This is a deliberate
constraint: arenas have lexical lifetimes, which are incompatible with the way async
suspends and resumes across await points.

```ubl
@tier(high)
async fn fetch_user(id: int) Task<User>! {
    let resp = await http_get($"/users/{id}")?
    return await parse_user(resp.body)?
}
```

### Structs and methods

```ubl
struct Rectangle {
    width: int,
    height: int

    pub fn new(w: int, h: int) Rectangle {
        return Rectangle { width = w, height = h }
    }

    pub fn area(self) int {
        return self.width * self.height
    }
}
```

### Pattern matching

```ubl
match response {
    Ok(data) where data.status == 200 => process_success(data),
    Ok(data) => log_warning($"Status: {data.status}"),
    Err(NetworkError(extract { code, message })) => {
        log_error($"Network error {code}: {message}")
    }
    Err(e) => log_error($"Unknown: {e}"),
}
```

### Pipe operator

```ubl
let result = data
    |> parse?
    |> validate?
    |> transform
    |> save
```

### Extension functions

```ubl
extend int {
    fn is_even(self) bool { return self % 2 == 0 }
}

if 42.is_even() { println("Even!") }
```

### Human-readable lifetimes

Only needed in LOW tier for complex cross-function borrows.
Simple cases are inferred.

```ubl
// Inferred — no annotation needed
fn first(list: &List<int>) &int {
    return &list[0]
}

// Complex case — explicit lifetime
fn longest[lifetime L](x: &L str, y: &L str) &L str {
    if x.len() > y.len() { return x } else { return y }
}
```

### RAII with `using`

```ubl
using let file = File.open("data.txt") {
    let content = file.read()
    process(content)
}   // file.close() called automatically
```

### LINQ (HIGH tier only)

```ubl
@tier(high)
fn active_adults(users: List<User>) List<string> {
    return from u in users
           where u.age >= 18 and u.status == "active"
           orderby u.name
           select u.name
}
```

---

## 🛠️ Building

### Prerequisites

- Rust toolchain (stable)
- LALRPOP (invoked via `build.rs`)

### Build

```bash
cargo build
cargo test
cargo bench
```

### CLI Usage

```bash
# Tokenize a .ubl file
ublc lex path/to/file.ubl --verbose

# Parse a .ubl file (shows top-level item count)
ublc parse path/to/file.ubl

# Run semantic analysis
ublc check path/to/file.ubl
```

---

## 🧪 Testing

Tests are organized at three levels, each catching different classes of bugs.

### Level 1 — Unit tests (inside source files)

Data structure tests live as `#[cfg(test)]` blocks inside their own module.
`symbol_table.rs` tests scope shadowing, duplicate detection, and resolution.
`type_table.rs` tests interning correctness. Run with:

```bash
cargo test
```

### Level 2 — Integration tests (fixtures)

Integration tests live under `tests/sema/` and drive the full lex → parse → resolve
pipeline against real `.ubl` fixture files:

```
tests/
└── sema/
    ├── name_resolution_tests.rs
    └── fixtures/
        ├── ok_simple.ubl
        ├── ok_forward_ref.ubl
        ├── ok_nested_scopes.ubl
        ├── ok_struct_methods.ubl
        ├── ok_imports.ubl
        ├── err_undefined_name.ubl
        ├── err_duplicate_def.ubl
        └── err_self_outside_method.ubl
```

Each test compiles a fixture and asserts on the error count and error kinds:

```bash
cargo test --test name_resolution_tests
```

### Level 3 — Snapshot tests (insta)

Add to `Cargo.toml`:

```toml
[dev-dependencies]
insta = "1"
```

Snapshot tests capture the full `SemaContext` debug output on the first run and
automatically detect regressions on every subsequent run. Review changed snapshots:

```bash
cargo insta review
```

---

## 🗺️ Architecture Overview

```
src/
├── lexer/
│   ├── logos_lexer.rs       # Token stream via Logos
│   ├── string_parser.rs     # Interpolated / verbatim strings
│   ├── comment_parser.rs    # Nested block comments
│   └── token.rs             # TokenType, Span
│
├── parser/
│   ├── grammar.lalrpop      # Full LALRPOP grammar
│   ├── helpers/             # Arena-aware node builders
│   │   ├── decl.rs
│   │   ├── expr.rs
│   │   ├── stmt.rs
│   │   ├── pat.rs
│   │   └── ty.rs
│   └── token_iter.rs        # Bridges Vec<Token> to LALRPOP
│
├── ast/
│   ├── arena.rs             # AstArena (bumpalo wrapper)
│   ├── common.rs            # Span, Ident, TierAnnotation, operators
│   ├── literals.rs          # Literal, InterpolationPart
│   ├── types.rs             # Type, TypeKind
│   ├── patterns.rs          # Pattern, destructure patterns
│   ├── expressions.rs       # Expr, ExprKind
│   ├── statements.rs        # Stmt, Block, AllocatorKind
│   ├── declarations.rs      # FunctionDecl, StructDecl, EnumDecl, …
│   └── root.rs              # Program, Item, Import
│
├── sema/                    # ← Phase 2 (in progress)
│   ├── mod.rs               # Orchestrates all three passes
│   ├── symbol_table.rs      # DefId, Def, DefKind, SymbolTable, ScopeStack
│   ├── sema_context.rs      # SemaContext — all side tables together
│   ├── type_table.rs        # TypeId, SemaType, TypeTable (with interning)
│   ├── name_resolution.rs   # Pass 1: resolve identifiers → DefId
│   ├── type_infer.rs        # Pass 2: constraint gen + unification (TODO)
│   └── tier_check.rs        # Pass 3: tier rule enforcement (TODO)
│
├── error_management/
│   ├── error_types/
│   │   ├── lexical_error.rs
│   │   ├── parse_error.rs
│   │   ├── name_error.rs    # ← new: Pass 1 errors
│   │   └── type_error.rs    # ← new: Pass 2 + 3 errors
│   ├── error_manager.rs     # Central accumulator for all phases
│   ├── diagnostics.rs
│   └── logger.rs
│
└── main.rs                  # ublc CLI (lex / parse / check / run)

tests/
└── sema/
    ├── name_resolution_tests.rs
    └── fixtures/            # Real .ubl source used as test input
```

All AST nodes are arena-allocated via `bumpalo` and carry a `'ast` lifetime.
Every node type is `Copy`, which means the entire tree can be traversed without
cloning and multiple passes share the same backing memory.

Semantic analysis produces a `SemaContext` — a set of side tables keyed by `Span`.
The AST is never mutated. Downstream consumers (interpreter, LLVM backend) receive
both `Program<'ast>` and `SemaContext` together.

---

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## 🌟 Why "Ubel Stratum"?

**Stratum** is Latin for "layer" — a direct reference to the tier system at the
heart of the language. Every program is a stack of strata, each with its own
memory contract.

**Ubel** has no deep etymology behind it. It sounded right. 

---

**Ubel Stratum: The right memory model for every function.** 🚀
