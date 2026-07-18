# Ubel Stratum — Builtins Implementation Rules

> **Canonical reference for `crates/core/src/builtins`.**
> Covers constructor conventions, argument validation (`ParamType`), and
> what counts as a `Value` for builtin-dispatch purposes. Every contributor
> reads this before adding or changing a builtin.
>
> **Status legend:** ✅ IMPLEMENTED · ⚠️ GAP · 🔭 PROPOSED
>
> Reflects repo state as of commit `6d65dcb` (fresh clone).

---

## 1. Directory Layout (for orientation)

```
crates/core/src/builtins/
├── mod.rs                 ← ParamType, BUILTIN_NAMESPACES, dispatch registry
├── validate.rs             ← argument validation against ParamType (⚠️ not wired in, see §4)
├── constructors.rs         ← static Namespace.new() constructors
├── global/
│   ├── io.rs
│   ├── math.rs
│   ├── diagnostics.rs
│   └── conversions.rs
└── instance/
    ├── list_methods.rs
    ├── dict_methods.rs
    ├── queue_methods.rs
    ├── stack_methods.rs
    ├── string_methods.rs
    └── tuple_methods.rs
```

`Set` has a reserved `KwSet` token and shows up in the collection-keyword
lists throughout the lexer/parser, but has no `Value::Set`, no
`SET_NAMESPACE`, and no `instance/set_methods.rs` yet. Reserved, not
implemented — noted here so nobody assumes it works.

---

## 2. Constructor Convention: Zero-Arg `.new()` + Chainable Config

**🔭 Rule, adopted going forward for every builtin collection constructor,
current and future:**

```
List.new().with_capacity(13)
Pool<Bullet>.new().with_capacity(200).growable()
Pool<Entity>.new().with_capacity(1024).fifo()
```

`Namespace.new()` stays uniform and zero-argument everywhere. Every
configuration knob is a named, chainable follow-up method instead of a
positional constructor argument.

**Why:**

- **No positional-argument guessing.** `List.new(13)` is ambiguous — capacity
  hint, or a single starting element? A chained `.with_capacity(13)` cannot be
  misread.
- **Scales to types with several optional knobs.** `Pool<T>` alone needs
  capacity, growability, free-list order, and handle safety — a four-argument
  positional constructor is unreadable; a chain isn't.
- **Cheap to implement for HIGH-tier collections today.** `List` / `Dict` /
  `Queue` / `Stack` already live behind `Rc<RefCell<...>>`, so
  `.with_capacity(n)` can call `.reserve(n)` in place and return the same
  aliased `Value` — no new mutation model required, consistent with the
  existing "assignment shares, doesn't deep-copy" semantics already
  documented in `value.rs`.

`constructors.rs` today (✅ implemented, zero-arg only, no `.with_*()` chain
yet):

```rust
pub fn list_new(_args: &[Value]) -> EvalResult { Ok(Value::new_list()) }
pub fn dictionary_new(_args: &[Value]) -> EvalResult { Ok(Value::new_dict()) }
pub fn queue_new(_args: &[Value]) -> EvalResult { Ok(Value::new_queue()) }
pub fn stack_new(_args: &[Value]) -> EvalResult { Ok(Value::new_stack()) }
```

Adding `.with_capacity()` / `.growable()` / `.fifo()` etc. as instance
methods on the constructed value is separate follow-up work, not yet started.

---

## 3. `ParamType` — What a Builtin Can Currently Require

✅ IMPLEMENTED, current full set (`builtins/mod.rs`):

```rust
pub enum ParamType {
    Int, Float, Double, Numeric, Bool, Str, Char, List, Dict, Tuple, Any,
}
```

⚠️ **GAP: no `Struct` variant, no `Enum` variant.** A builtin's declared
signature cannot express "this argument must be a struct" or "this argument
must be an enum" — the only available escape hatch is `ParamType::Any`, which
also accepts an int, a string, or anything else, defeating the purpose of
declaring a constrained signature for that parameter at all.

This matters because `Value::Struct` and `Value::Enum` are both fully
first-class runtime values already (see §5) — the gap is specifically in the
coarser `ParamType` layer builtins declare against, not in what the
interpreter is capable of holding.

**Not urgent in isolation** — no current builtin needs it. Worth revisiting
the moment an ECS-facing or reflection-style builtin wants to accept a struct
or enum argument specifically (Pool's constructors are a plausible first
candidate, if `Pool<T>.new()` ever needs to validate `T` is a struct type at
the call site).

---

## 4. Validation Layer Is Not Wired Into Sema Yet

⚠️ **GAP.** `validate.rs`'s own doc comment states it is written and
unit-tested, but **not yet called from `sema::type_infer`'s `ExprKind::Call`
handling** — wiring it in requires converting the real `SemaType` into the
coarser `ParamType` this module works with, at the call site. This is a
second, independent gap from §3 — even once `Struct`/`Enum` variants exist,
they won't do anything until this wiring lands.

---

## 5. Is a Struct a `Value`? Yes — Confirmed Directly

```rust
Value::Struct {
    type_name: String,
    fields: Rc<RefCell<HashMap<String, Value>>>,
},
```

✅ IMPLEMENTED. Struct instances are a fully first-class `Value` variant,
using the same `Rc<RefCell<...>>` shared-aliasing model as `List` and `Dict`.
At HIGH tier, "a struct value" and "a reference to a struct" are already the
same thing by construction — cloning a `Value::Struct` bumps the `Rc`, and
every alias sees the same mutable fields. This isn't an oversight; it's the
correct shape for a GC'd reference type (matches how a C# class instance
behaves, consistent with Ubel's "scripting language for the engine" framing).

The value/reference distinction only becomes meaningfully different once
MID/LOW-tier structs exist (an `OwnedRef<MyStruct>` would be a genuinely more
restrictive kind of reference than HIGH's implicit-everywhere aliasing) — see
`MEMORY_MODEL.md`. Deliberately out of scope for this document.

---

## 6. Open Decisions

| # | Question | Status |
|---|---|---|
| 1 | Add `ParamType::Struct` / `ParamType::Enum`, or wait for a concrete builtin that needs them? | Open — leaning "wait" |
| 2 | Does `ParamType::Struct` need an optional "required type name" payload (`Struct(Option<&'static str>)`), or is "any struct" sufficient for the first use case? | Open |
| 3 | Who wires `validate.rs` into `type_infer.rs`'s `ExprKind::Call` path, and does it block on the `ParamType` gap above or land independently first? | Open |
