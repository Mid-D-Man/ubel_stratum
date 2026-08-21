// src/interpreter/value.rs
//! Runtime value representation.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::rc::Rc;

/// Stable index into `Interpreter::functions`.
/// Stored inside `Value::Function` so Value carries no AST lifetimes.
pub type FunctionId = usize;

/// Every runtime value an Ubel program can produce.
///
/// Scalars are stored inline. Heap values (`List`, `Dict`, `Struct`) are
/// `Rc<RefCell<…>>` so:
///   - Assignment shares (does not deep-copy) — consistent with HIGH-tier GC semantics.
///   - Mutation inside a shared structure is visible through all aliases.
///   - `Clone` on a `Value` is always O(1) (bumps an Rc, never allocates).
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Void,
    Bool(bool),
    /// Ubel's default integer — i64 matches AST `Literal::Int(i64)`.
    Int(i64),
    Float(f32),
    Double(f64),
    Char(char),
    /// Immutable string — Rc makes cloning O(1).
    Str(Rc<String>),
    /// Mutable ordered list — shared between aliases.
    List(Rc<RefCell<Vec<Value>>>),
    /// Ordered key-value store — Vec<pair> because Value doesn't impl Hash.
    /// O(n) lookup is fine for the tree-walking interpreter.
    Dict(Rc<RefCell<Vec<(Value, Value)>>>),
    /// FIFO queue.
    Queue(Rc<RefCell<VecDeque<Value>>>),
    /// LIFO stack.
    Stack(Rc<RefCell<Vec<Value>>>),
    /// Immutable fixed-size tuple.
    Tuple(Vec<Value>),
    /// Named struct instance with mutable fields.
    Struct {
        type_name: String,
        fields:    Rc<RefCell<HashMap<String, Value>>>,
    },
    /// Enum variant with optional payload.
    Enum {
        type_name: String,
        variant:   String,
        payload:   Box<EnumPayload>,
    },
    /// Index into the interpreter's function table.
    /// Both named functions and lambdas (closures) are represented this way.
    Function(FunctionId),
    /// `Pool<T>` — fixed-capacity slot table with a LIFO free list and a
    /// generation counter per slot (MEMORY_MODEL.md §11). Capacity comes
    /// from the innermost enclosing `with pool<T>(count) { }` at
    /// construction time — see `Interpreter::pool_capacity_stack`.
    Pool(Rc<RefCell<PoolData>>),
    /// A generational handle returned by `Pool<T>.acquire()`. Small and
    /// `Copy`-cheap by construction (no `Rc`); deliberately not
    /// constructible from a tuple literal or any other user-facing
    /// value, so a handle can only ever come from a real `acquire()`.
    Handle { index: usize, generation: u64 },
    /// `InlineList<T>` — fixed-capacity collection (DATASTRUCTURES.md
    /// §5). `items.len() <= capacity` always; `.push()` is checked
    /// (`bool` return, never grows past capacity, no silent
    /// truncation). Rust-heap-backed like every other `Value` variant
    /// here — there's no real codegen yet, so "inline/stack" is
    /// currently a language-level *contract* (bounded, checked) rather
    /// than a literal memory-placement guarantee, same honesty already
    /// applied to how Pool/Arena/GC tiers all share Rust-heap-backed
    /// representations today.
    InlineList(Rc<RefCell<InlineListData>>),
    /// `Linqerizer<T>` (`docs/DATASTRUCTURES.md` §6) — a lazy query
    /// pipeline. `source` is a snapshot taken once, at `.query()` time;
    /// `ops` accumulates as the chain grows (`.where()`/`.select()`/
    /// `.order_by()`/`.order_by_desc()` each return a *new* `Linqerizer`
    /// with one more op appended, never mutating the one they were
    /// called on — matching how every other LINQ-shaped thing works,
    /// C#/Rust iterators/JS included; `let a = q.where(f); let b =
    /// q.select(g);` would be surprising if `b` silently inherited `a`'s
    /// filter). Nothing in `ops` actually runs until a terminal method
    /// (`.to_list()`/`.first()`/`.count()`/`.group_by()`) walks `source`
    /// applying every pending op in order — that deferred-until-terminal
    /// property is the actual point, and the thing the old `eval_linq`
    /// (fully eager, one-pass) never had.
    Linqerizer(Rc<LinqPipeline>),
}

/// Backing storage for `Value::Linqerizer`. Immutable once built (see
/// the `Value::Linqerizer` doc comment on why chaining allocates a new
/// one rather than mutating) — plain `Rc`, no `RefCell`, nothing here
/// is ever mutated in place.
#[derive(Debug, Clone)]
pub struct LinqPipeline {
    /// Snapshot of the source, taken once at `.query()` time. `Rc` so
    /// every step in a chain shares it instead of re-cloning the whole
    /// `Vec` on every `.where()`/`.select()` call — only `ops` grows.
    pub source: Rc<Vec<Value>>,
    pub ops:    Vec<LinqOp>,
}

#[derive(Debug, Clone)]
pub enum LinqOp {
    Where(FunctionId),
    Select(FunctionId),
    /// `bool` = descending.
    OrderBy(FunctionId, bool),
}

/// Backing storage for `Value::InlineList`.
#[derive(Debug, Clone)]
pub struct InlineListData {
    pub items:    Vec<Value>,
    pub capacity: usize,
}

/// Backing storage for `Value::Pool`. Block-chained (DATASTRUCTURES.md —
/// Pool absorbs Hive's growability rather than a separate `Hive<T>` type):
/// `blocks[i]`/`generations[i]` are always the same length (`block_size`,
/// fixed at construction, from the enclosing `with pool<T>(count) { }`).
/// A full pool with `growable == true` appends a brand-new block rather
/// than reallocating/copying the existing ones — existing `Handle`s
/// (flat, cross-block indices) stay valid, and no live slot's data ever
/// moves. `free_list` is a `VecDeque` so both LIFO (default, pop from the
/// back — matches §10 item 3's "most-recently-freed reused first") and
/// FIFO (`.fifo()` opt-in, pop from the front) are O(1); `.push_back` on
/// release either way, only the acquire-side pop direction differs.
#[derive(Debug, Clone)]
pub struct PoolData {
    pub blocks:      Vec<Vec<Option<Value>>>,
    pub generations: Vec<Vec<u64>>,
    pub free_list:   std::collections::VecDeque<usize>,
    pub block_size:  usize,
    pub growable:    bool,
    pub fifo:        bool,
}

impl PoolData {
    pub fn with_capacity(capacity: usize) -> Self {
        PoolData {
            blocks:      vec![vec![None; capacity]],
            generations: vec![vec![0; capacity]],
            free_list:   (0..capacity).collect(),
            block_size:  capacity,
            growable:    false,
            fifo:        false,
        }
    }

    #[inline]
    fn locate(&self, flat_index: usize) -> (usize, usize) {
        (flat_index / self.block_size, flat_index % self.block_size)
    }

    pub fn total_capacity(&self) -> usize {
        self.blocks.len() * self.block_size
    }

    pub fn slot(&self, flat_index: usize) -> Option<&Value> {
        let (b, o) = self.locate(flat_index);
        self.blocks.get(b)?.get(o)?.as_ref()
    }

    pub fn slot_mut(&mut self, flat_index: usize) -> Option<&mut Option<Value>> {
        let (b, o) = self.locate(flat_index);
        self.blocks.get_mut(b)?.get_mut(o)
    }

    pub fn generation(&self, flat_index: usize) -> Option<u64> {
        let (b, o) = self.locate(flat_index);
        self.generations.get(b)?.get(o).copied()
    }

    pub fn generation_mut(&mut self, flat_index: usize) -> Option<&mut u64> {
        let (b, o) = self.locate(flat_index);
        self.generations.get_mut(b)?.get_mut(o)
    }

    /// Append one new block (`block_size` fresh slots) if `growable`,
    /// returning whether it did. Existing blocks are untouched — this is
    /// the actual "no reallocation-copy" property that makes it Hive-
    /// shaped rather than just `Vec::resize` wearing a new name; the
    /// latter would've been just as `Handle`-safe (indices, not raw
    /// pointers, so nothing would dangle) but would pay an O(n) copy on
    /// every grow and rule out a raw-pointer-stable FFI escape hatch
    /// later (DATASTRUCTURES.md §1).
    pub fn try_grow(&mut self) -> bool {
        if !self.growable { return false; }
        let start = self.blocks.len() * self.block_size;
        self.blocks.push(vec![None; self.block_size]);
        self.generations.push(vec![0; self.block_size]);
        for i in start..start + self.block_size {
            self.free_list.push_back(i);
        }
        true
    }

    /// LIFO (default) pops the back — most-recently-freed reused first.
    /// FIFO (`.fifo()`) pops the front. Release always pushes to the
    /// back either way; only the acquire-side end differs.
    pub fn free_pop(&mut self) -> Option<usize> {
        if self.fifo { self.free_list.pop_front() } else { self.free_list.pop_back() }
    }

    pub fn free_push(&mut self, index: usize) {
        self.free_list.push_back(index);
    }

    /// Every currently-occupied slot, in index order, holes skipped —
    /// the actual skipfield behavior (`for x in pool { }`,
    /// DATASTRUCTURES.md §1's ".iter()"). Bare values only, not paired
    /// with their `Handle` — see `value_to_iter_vec`'s own doc comment
    /// on why that pairing is deliberately not done yet.
    pub fn iter_occupied(&self) -> impl Iterator<Item = &Value> + '_ {
        self.blocks.iter().flatten().filter_map(|s| s.as_ref())
    }
}

/// Payload carried by an enum variant at runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum EnumPayload {
    None,
    Tuple(Vec<Value>),
    Struct(HashMap<String, Value>),
}

/// Control-flow signal. Returned as `Err(Signal)` to propagate non-local
/// exits through the evaluation call stack without unwinding.
#[derive(Debug, Clone)]
pub enum Signal {
    /// `return expr` — propagates to enclosing function call.
    Return(Value),
    /// `break expr?` — exits the enclosing loop.
    Break(Option<Value>),
    /// `continue` — skips to the next loop iteration.
    Continue,
    /// `fail expr` — propagates until caught by `try { } catch`.
    Fail(Value),
    /// Unrecoverable error (`panic()`, failed assertion, interpreter bug).
    Panic(String),
}

/// Every eval function returns this. `Ok(Value)` is the normal path;
/// `Err(Signal)` carries control flow or errors.
pub type EvalResult = Result<Value, Signal>;

// ── Value methods ─────────────────────────────────────────────────

impl Value {
    /// Short human-readable type name for errors and `typeof()`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null          => "null",
            Value::Void          => "void",
            Value::Bool(_)       => "bool",
            Value::Int(_)        => "int",
            Value::Float(_)      => "float",
            Value::Double(_)     => "double",
            Value::Char(_)       => "char",
            Value::Str(_)        => "string",
            Value::List(_)       => "List",
            Value::Dict(_)       => "Dictionary",
            Value::Queue(_)      => "Queue",
            Value::Stack(_)      => "Stack",
            Value::Tuple(_)      => "tuple",
            Value::Struct { .. } => "struct",
            Value::Enum { .. }   => "enum",
            Value::Function(_)   => "function",
            Value::Pool(_)       => "Pool",
            Value::Handle { .. } => "Handle",
            Value::InlineList(_) => "InlineList",
            Value::Linqerizer(_) => "Linqerizer",
        }
    }

    /// Ubel conditions must be `bool`; anything else is a runtime panic.
    pub fn is_truthy(&self) -> Result<bool, Signal> {
        match self {
            Value::Bool(b) => Ok(*b),
            other => Err(Signal::Panic(format!(
                "condition must be bool, got {}",
                other.type_name()
            ))),
        }
    }

    /// Structural equality that doesn't require `Hash`.
    /// Heap types (`List`, `Dict`, `Struct`) use referential equality
    /// (same `Rc` pointer) which is consistent with HIGH-tier shared semantics.
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null,  Value::Null)  => true,
            (Value::Void,  Value::Void)  => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a),  Value::Int(b))  => a == b,
            (Value::Float(a),  Value::Float(b))  => a == b,
            (Value::Double(a), Value::Double(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a),  Value::Str(b))  => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equals(y))
            }
            (Value::Enum { type_name: ta, variant: va, payload: pa },
             Value::Enum { type_name: tb, variant: vb, payload: pb }) => {
                ta == tb && va == vb && pa == pb
            }
            // Heap types: pointer equality.
            (Value::List(a),   Value::List(b))   => Rc::ptr_eq(a, b),
            (Value::Dict(a),   Value::Dict(b))   => Rc::ptr_eq(a, b),
            (Value::Queue(a),  Value::Queue(b))  => Rc::ptr_eq(a, b),
            (Value::Stack(a),  Value::Stack(b))  => Rc::ptr_eq(a, b),
            (Value::Struct { fields: a, .. },
             Value::Struct { fields: b, .. })    => Rc::ptr_eq(a, b),
            (Value::Function(a), Value::Function(b)) => a == b,
            (Value::Pool(a), Value::Pool(b)) => Rc::ptr_eq(a, b),
            (Value::Handle { index: ia, generation: ga },
             Value::Handle { index: ib, generation: gb }) => ia == ib && ga == gb,
            (Value::InlineList(a), Value::InlineList(b)) => Rc::ptr_eq(a, b),
            (Value::Linqerizer(a), Value::Linqerizer(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Convenience: make an empty `Value::Pool` with the given capacity.
    pub fn new_pool(capacity: usize) -> Self {
        Value::Pool(Rc::new(RefCell::new(PoolData::with_capacity(capacity))))
    }

    /// Convenience: make an empty `Value::InlineList` with the given
    /// (sema-checked-literal) capacity.
    pub fn new_inline_list(capacity: usize) -> Self {
        Value::InlineList(Rc::new(RefCell::new(InlineListData {
            items: Vec::with_capacity(capacity),
            capacity,
        })))
    }

    /// Convenience: make a `Value::Str` from a `&str`.
    pub fn str_from(s: impl Into<String>) -> Self {
        Value::Str(Rc::new(s.into()))
    }

    /// Convenience: make an empty `Value::List`.
    pub fn new_list() -> Self {
        Value::List(Rc::new(RefCell::new(Vec::new())))
    }

    /// Convenience: make an empty `Value::Dict`.
    pub fn new_dict() -> Self {
        Value::Dict(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn new_queue() -> Self {
        Value::Queue(Rc::new(RefCell::new(VecDeque::new())))
    }

    pub fn new_stack() -> Self {
        Value::Stack(Rc::new(RefCell::new(Vec::new())))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool { self.equals(other) }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null        => write!(f, "null"),
            Value::Void        => Ok(()),
            Value::Bool(b)     => write!(f, "{}", b),
            Value::Int(n)      => write!(f, "{}", n),
            Value::Float(v)    => write!(f, "{}", v),
            Value::Double(v)   => write!(f, "{}", v),
            Value::Char(c)     => write!(f, "{}", c),
            Value::Str(s)      => write!(f, "{}", s),
            Value::Function(i) => write!(f, "<fn #{}>", i),
            Value::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            Value::List(rc) => {
                let items = rc.borrow();
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Dict(rc) => {
                let entries = rc.borrow();
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Queue(rc) => {
                let items = rc.borrow();
                write!(f, "Queue[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Stack(rc) => {
                let items = rc.borrow();
                write!(f, "Stack[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::InlineList(rc) => {
                let data = rc.borrow();
                write!(f, "InlineList[")?;
                for (i, v) in data.items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "] (len={}, capacity={})", data.items.len(), data.capacity)
            }
            Value::Linqerizer(pipeline) => {
                // Lazy and not yet materialized — showing the pending
                // pipeline shape rather than pretending to show results
                // that haven't been computed (no `Interpreter` access
                // here to actually run the ops even if we wanted to).
                write!(f, "Linqerizer(source_len={}, pending_ops={})",
                    pipeline.source.len(), pipeline.ops.len())
            }
            Value::Struct { type_name, fields } => {
                let fields = fields.borrow();
                write!(f, "{} {{", type_name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Pool(rc) => {
                let pool = rc.borrow();
                write!(f, "Pool(capacity={}, free={})", pool.total_capacity(), pool.free_list.len())
            }
            Value::Handle { index, generation } => write!(f, "Handle(#{}/{})", index, generation),
            Value::Enum { type_name, variant, payload } => {
                match payload.as_ref() {
                    EnumPayload::None => write!(f, "{}.{}", type_name, variant),
                    EnumPayload::Tuple(items) => {
                        write!(f, "{}.{}(", type_name, variant)?;
                        for (i, v) in items.iter().enumerate() {
                            if i > 0 { write!(f, ", ")?; }
                            write!(f, "{}", v)?;
                        }
                        write!(f, ")")
                    }
                    EnumPayload::Struct(fields) => {
                        write!(f, "{}.{} {{", type_name, variant)?;
                        for (i, (k, v)) in fields.iter().enumerate() {
                            if i > 0 { write!(f, ", ")?; }
                            write!(f, "{}: {}", k, v)?;
                        }
                        write!(f, "}}")
                    }
                }
            }
        }
    }
        }
