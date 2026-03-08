# Ubel Stratum

**Quantum-Ready Multi-Tier Systems Language**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-early%20development-orange.svg)]()
[![Built With](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

> **"The right memory model for every function."**

---

## ⚠️ Current Status: Early Development

Ubel Stratum has completed its lexer and parser. The language specification and AST
are solidified enough to begin the next phase: semantic analysis and a tree-walking
interpreter. Implementation is active.

**Development Roadmap:**
1. ✅ **Phase 1**: Core design, memory model, lexer (Logos), parser (LALRPOP), arena AST
2. 🔄 **Phase 2**: Semantic analysis — name resolution, type checker, tier enforcer
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
source.strat
    │
    ▼
stratc (compiler)
    │
    ├── Lexer (Logos)
    ├── Parser (LALRPOP) → Arena AST
    ├── Name resolution
    ├── Type checker + Tier enforcer
    ├── Lowering to IR
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

```strat
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

```strat
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

```strat
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

```strat
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
```

---

## 📋 Language Features

### Tier annotations

```strat
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

```strat
let mut numbers = List.new()
numbers.push(1)
numbers.push(2)

let mut scores = Dictionary<string, int>.new()
scores.insert("Alice", 100)

let items = [1, 2, 3, 4, 5]          // array literal
let names = ["Alice", "Bob"]          // inferred as List<string>
```

### Unified `.` access — no `::`

```strat
summon std.collections.List
let list = List.new()        // type-level call
list.push(42)                // instance call
// it's always . — never ::
```

### Error handling

```strat
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

```strat
@tier(high)
async fn fetch_user(id: int) Task<User>! {
    let resp = await http_get($"/users/{id}")?
    return await parse_user(resp.body)?
}
```

### Structs and methods

```strat
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

```strat
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

```strat
let result = data
    |> parse?
    |> validate?
    |> transform
    |> save
```

### Extension functions

```strat
extend int {
    fn is_even(self) bool { return self % 2 == 0 }
}

if 42.is_even() { println("Even!") }
```

### Human-readable lifetimes

Only needed in LOW tier for complex cross-function borrows.
Simple cases are inferred.

```strat
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

```strat
using let file = File.open("data.txt") {
    let content = file.read()
    process(content)
}   // file.close() called automatically
```

### LINQ (HIGH tier only)

```strat
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
# Tokenize a .strat file
stratc lex path/to/file.strat --verbose

# Parse a .strat file (shows top-level item count)
stratc parse path/to/file.strat
```

---

## 🗺️ What Comes Next (Phase 2)

After the parser, the next work is **semantic analysis**. This is where the tier
system becomes real. The semantic analysis pass has three jobs:

**Name resolution** — resolve every identifier to a definition. Build the scope tree,
handle imports, report unknown names.

**Type inference and checking** — infer types for let bindings, check that function
call arguments match parameters, ensure return types are correct.

**Tier checking** — this is the novel part. The tier checker walks the typed AST and
enforces:
- `with arena` blocks only appear in `@tier(mid)` functions
- `await` only appears in `@tier(high)` functions
- No `&'arena T` type appears in the return type of a cross-tier call
- MID functions called from HIGH must use the callback, iterator, or view pattern

For the **tree-walking interpreter** (Phase 3), tiers are simulated using Rust's own
memory. HIGH-tier values are `Rc<RefCell<Value>>`. MID-tier uses `bumpalo` (already
in `Cargo.toml`). LOW-tier uses plain Rust ownership. The interpreter proves the
memory model is sound without requiring a full borrow checker implementation.

The **full borrow checker** for LOW tier is deferred to the LLVM phase (Phase 4).
Implementing a borrow checker in the interpreter phase would be as complex as Rust's
NLL checker and would block progress on proving the tier model. The interpreter
phase validates tier semantics; the LLVM phase enforces LOW-tier safety.

---

## 🏗️ Architecture Overview

```
src/
├── lexer/
│   ├── logos_lexer.rs      # Token stream via Logos
│   ├── string_parser.rs    # Interpolated / verbatim strings
│   ├── comment_parser.rs   # Nested block comments
│   └── token.rs            # TokenType, Span
│
├── parser/
│   ├── grammar.lalrpop     # Full LALRPOP grammar
│   ├── helpers/            # Arena-aware node builders
│   │   ├── decl.rs
│   │   ├── expr.rs
│   │   ├── stmt.rs
│   │   ├── pat.rs
│   │   └── ty.rs
│   └── token_iter.rs       # Bridges Vec<Token> to LALRPOP
│
├── ast/
│   ├── arena.rs            # AstArena (bumpalo wrapper)
│   ├── common.rs           # Span, Ident, TierAnnotation, operators
│   ├── literals.rs         # Literal, InterpolationPart
│   ├── types.rs            # Type, TypeKind
│   ├── patterns.rs         # Pattern, destructure patterns
│   ├── expressions.rs      # Expr, ExprKind
│   ├── statements.rs       # Stmt, Block, AllocatorKind
│   ├── declarations.rs     # FunctionDecl, StructDecl, EnumDecl, …
│   └── root.rs             # Program, Item, Import
│
├── error_management/
│   ├── error_types/
│   │   ├── lexical_error.rs
│   │   └── parse_error.rs
│   ├── error_manager.rs
│   ├── diagnostics.rs
│   └── logger.rs
│
└── main.rs                 # stratc CLI (lex / parse / check / run)
```

All AST nodes are arena-allocated via `bumpalo` and carry a `'ast` lifetime.
Every node type is `Copy`, which means the entire tree can be traversed without
cloning and multiple passes share the same backing memory.

---

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## 🌟 Why "Ubel Stratum"?

**Ubel** (German: "evil/bad") + **Stratum** (Latin: "layer/tier")

Choosing between memory models shouldn't be painful ("evil"). It should be a clear,
empowering decision made at the function level. The name is tongue-in-cheek: we took
the "evil" complexity of mixed memory models and put it in the compiler where it belongs,
so the programmer only sees the clean layered result.

---

**Ubel Stratum: The right memory model for every function.** 🚀
