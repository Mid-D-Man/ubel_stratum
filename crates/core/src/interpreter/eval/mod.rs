// src/interpreter/eval/mod.rs
//! Interpreter struct, function table, and call dispatch.

#![allow(dead_code)]

pub mod expr;
pub mod stmt;
pub mod pattern;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::ast::arena::AstArena;
use crate::ast::common::TierAnnotation;
use crate::ast::declarations::{FunctionDecl, MethodDecl, ParamKind, StructMember};
use crate::ast::expressions::Expr;
use crate::ast::root::{Item, Program};
use crate::ast::statements::Block;
use crate::builtins::BuiltinFn;
use crate::interpreter::env::Environment;
use crate::interpreter::value::{EvalResult, FunctionId, Signal, Value};

// ── FunctionBody ──────────────────────────────────────────────────

pub enum FunctionBody<'ast> {
    /// User-defined function with a block body.
    Ast { block: Block<'ast> },
    /// Lambda with an expression body: `fn(x) x * 2`
    /// Stored as a reference into the arena so no allocation is needed.
    ExprBody { expr: &'ast Expr<'ast> },
    /// Native Rust built-in.
    Builtin(BuiltinFn),
}

// ── FunctionDef ───────────────────────────────────────────────────

pub struct FunctionDef<'ast> {
    pub name:     Option<String>,
    /// Parameter names in declaration order.
    /// `self` / `&self` / `mut self` params are excluded — they are bound
    /// by `call_method` directly.
    pub params:   Vec<String>,
    pub body:     FunctionBody<'ast>,
    /// Environment captured at definition time (closure snapshot).
    pub closure:  Environment,
    pub tier:     TierAnnotation,
    pub is_async: bool,
}

// ── Interpreter ───────────────────────────────────────────────────

/// Tree-walking interpreter.
///
/// The `'ast` lifetime is needed because `FunctionDef` stores arena-allocated
/// `Block<'ast>` and `&'ast Expr<'ast>` references.  `Value` and `Environment`
/// are both lifetime-free, so closures and runtime values can be passed around
/// freely without the compiler tracking AST provenance through them.
pub struct Interpreter<'ast> {
    pub(crate) env:          Environment,
    pub(crate) functions:    Vec<FunctionDef<'ast>>,
    /// struct_type_name → method_name → FunctionId.
    /// Static and instance methods both live here; the call site determines
    /// whether a receiver is passed.
    pub(crate) method_table: HashMap<String, HashMap<String, FunctionId>>,
    /// enum_type_name → set of its declared (bare, payload-less) variant
    /// names. Consulted by `EnumName.Variant` field-access evaluation to
    /// tell "construct an enum value" apart from an ordinary field lookup —
    /// see `eval::expr::eval_expr`'s `ExprKind::Field` case.
    pub(crate) enum_table:   HashMap<String, std::collections::HashSet<String>>,
    /// The arena used to parse the program.  Kept here so interpolated-string
    /// expression holes (`$"Hello {expr}"`) can be parsed at runtime via
    /// `crate::parser::parse_expr`.
    pub(crate) arena:        &'ast AstArena,
}

impl<'ast> Interpreter<'ast> {
    pub fn new(arena: &'ast AstArena) -> Self {
        let mut interp = Interpreter {
            env:          Environment::new(),
            functions:    Vec::new(),
            method_table: HashMap::new(),
            enum_table:   HashMap::new(),
            arena,
        };
        interp.register_builtins();
        interp
    }

    // ── Registration ─────────────────────────────────────────────

    fn register_builtins(&mut self) {
        for sig in crate::builtins::global::GLOBAL_BUILTINS {
            let id = self.functions.len();
            self.functions.push(FunctionDef {
                name:     Some(sig.name.to_string()),
                params:   vec![],
                body:     FunctionBody::Builtin(sig.run),
                closure:  Environment::new(),
                tier:     TierAnnotation::High,
                is_async: false,
            });
            self.env.define(sig.name, Value::Function(id));
        }
    }

    pub fn alloc_function(&mut self, def: FunctionDef<'ast>) -> FunctionId {
        let id = self.functions.len();
        self.functions.push(def);
        id
    }

    /// Register a top-level function declaration.
    /// Takes `FunctionDecl<'ast>` by value — it is `Copy` so fields
    /// (`body: Block<'ast>`, etc.) are copied off the stack while the
    /// underlying arena data they point to lives as long as `'ast`.
    pub fn register_fn(&mut self, f: FunctionDecl<'ast>) -> FunctionId {
        let params: Vec<String> = f.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        let block    = f.body;  // Block<'ast> is Copy
        let closure  = self.env.snapshot();
        let name_str = f.name.to_string();
        let tier     = f.tier;
        let is_async = f.is_async;
        self.alloc_function(FunctionDef {
            name:    Some(name_str),
            params,
            body:    FunctionBody::Ast { block },
            closure,
            tier,
            is_async,
        })
    }

    /// Register a struct method. The FunctionId is stored in `method_table`.
    pub fn register_method(&mut self, struct_name: &str, m: MethodDecl<'ast>) -> FunctionId {
        // Only Named params — `self` variants are handled at call sites.
        let params: Vec<String> = m.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        let block    = m.body;
        let closure  = self.env.snapshot();
        let name_str = format!("{}::{}", struct_name, m.name);
        let tier     = m.tier;
        let is_async = m.is_async;
        self.alloc_function(FunctionDef {
            name:    Some(name_str),
            params,
            body:    FunctionBody::Ast { block },
            closure,
            tier,
            is_async,
        })
    }

    // ── Program entry point ───────────────────────────────────────

    pub fn run_program(&mut self, program: &'ast Program<'ast>) -> Result<(), String> {
        // Pre-declare pass: register everything before running any body so
        // forward references and mutual recursion work.
        let mut top_level_fns: Vec<FunctionId> = Vec::new();
        for item in program.items.iter().copied() {
            match item {
                Item::Function(f) => {
                    // f: FunctionDecl<'ast> — Copy value whose fields point into 'ast arena
                    let id = self.register_fn(f);
                    self.env.define(f.name, Value::Function(id));
                    top_level_fns.push(id);
                }
                Item::Struct(s) => {
                    for member in s.members.iter().copied() {
                        if let StructMember::Method(m) = member {
                            let id = self.register_method(s.name, m);
                            self.method_table
                                .entry(s.name.to_string())
                                .or_default()
                                .insert(m.name.to_string(), id);
                            top_level_fns.push(id);
                        }
                    }
                }
                Item::Enum(e) => {
                    let variants: std::collections::HashSet<String> = e.variants.iter()
                        .filter(|v| matches!(v.payload, crate::ast::declarations::EnumVariantPayload::None))
                        .map(|v| v.name.to_string())
                        .collect();
                    self.enum_table.insert(e.name.to_string(), variants);
                    // TODO: EnumVariantPayload::Discriminant/Tuple/Struct aren't
                    // constructible at runtime yet — payload-carrying variants
                    // are silently excluded from enum_table rather than handled.
                }
                _ => {}
            }
        }

        // `register_fn`/`register_method` snapshot `self.env` for the
        // closure at the moment each item is registered — which, mid-loop,
        // is *before* later top-level items have been declared. A function
        // declared early in the file (e.g. `main`) would otherwise end up
        // with a closure that can't see a function declared later in the
        // same file (e.g. `greet`), even though both are top-level and
        // should share one module scope. Backfill every top-level
        // fn/method's closure with a single snapshot taken now, after the
        // whole pre-declare pass has finished.
        let module_scope = self.env.snapshot();
        for id in top_level_fns {
            self.functions[id].closure = module_scope.clone();
        }

        let main_val = self.env.get("main").cloned()
            .ok_or_else(|| "no `main` function found".to_string())?;

        match main_val {
            Value::Function(id) => match self.call_function(id, &[]) {
                Ok(_) | Err(Signal::Return(_)) => Ok(()),
                Err(Signal::Panic(msg)) => Err(format!("panic: {}", msg)),
                Err(Signal::Fail(v))    => Err(format!("unhandled fail: {}", v)),
                Err(Signal::Break(_))   => Err("break outside loop".to_string()),
                Err(Signal::Continue)   => Err("continue outside loop".to_string()),
            },
            _ => Err("`main` is not a function".to_string()),
        }
    }

    // ── Call dispatch ─────────────────────────────────────────────

    /// Call a function by FunctionId with already-evaluated arguments.
    ///
    /// Uses a local enum to carry extracted body data so the immutable borrow
    /// on `self.functions` is fully released before any mutation of `self.env`.
    pub fn call_function(&mut self, id: FunctionId, args: &[Value]) -> EvalResult {
        // Phase 1: extract body data — immutable borrow on self.functions.
        enum BodyData<'a> {
            Builtin(BuiltinFn),
            Block(Block<'a>),
            Expr(&'a Expr<'a>),
        }

        let (body, param_names, closure) = {
            let def = &self.functions[id];
            let body = match &def.body {
                FunctionBody::Builtin(f)        => BodyData::Builtin(*f),
                FunctionBody::Ast { block }     => BodyData::Block(*block),
                FunctionBody::ExprBody { expr } => BodyData::Expr(expr),
            };
            (body, def.params.clone(), def.closure.clone())
        }; // ← immutable borrow released here

        // Phase 2: run the body — may mutate self freely.
        match body {
            BodyData::Builtin(f) => f(args),

            BodyData::Block(block) => {
                let caller_env = std::mem::replace(&mut self.env, closure);
                self.env.push();
                for (name, val) in param_names.iter().zip(args.iter()) {
                    self.env.define(name, val.clone());
                }
                let result = stmt::eval_block(self, &block);
                self.env = caller_env;
                match result {
                    Ok(v) | Err(Signal::Return(v)) => Ok(v),
                    Err(other)                     => Err(other),
                }
            }

            BodyData::Expr(e) => {
                let caller_env = std::mem::replace(&mut self.env, closure);
                self.env.push();
                for (name, val) in param_names.iter().zip(args.iter()) {
                    self.env.define(name, val.clone());
                }
                let result = expr::eval_expr(self, e);
                self.env = caller_env;
                match result {
                    Ok(v) | Err(Signal::Return(v)) => Ok(v),
                    Err(other)                     => Err(other),
                }
            }
        }
    }

    /// Call a struct method with an explicit `self` receiver.
    pub fn call_method(
        &mut self,
        id:       FunctionId,
        receiver: Value,
        args:     &[Value],
    ) -> EvalResult {
        enum BodyData<'a> {
            Builtin(BuiltinFn),
            Block(Block<'a>),
            Expr(&'a Expr<'a>),
        }

        let (body, param_names, closure) = {
            let def = &self.functions[id];
            let body = match &def.body {
                FunctionBody::Builtin(f)        => BodyData::Builtin(*f),
                FunctionBody::Ast { block }     => BodyData::Block(*block),
                FunctionBody::ExprBody { expr } => BodyData::Expr(expr),
            };
            (body, def.params.clone(), def.closure.clone())
        };

        match body {
            BodyData::Builtin(f) => {
                let mut all = vec![receiver];
                all.extend_from_slice(args);
                f(&all)
            }
            BodyData::Block(block) => {
                let caller_env = std::mem::replace(&mut self.env, closure);
                self.env.push();
                self.env.define("self", receiver);
                for (name, val) in param_names.iter().zip(args.iter()) {
                    self.env.define(name, val.clone());
                }
                let result = stmt::eval_block(self, &block);
                self.env = caller_env;
                match result {
                    Ok(v) | Err(Signal::Return(v)) => Ok(v),
                    Err(other)                     => Err(other),
                }
            }
            BodyData::Expr(e) => {
                let caller_env = std::mem::replace(&mut self.env, closure);
                self.env.push();
                self.env.define("self", receiver);
                for (name, val) in param_names.iter().zip(args.iter()) {
                    self.env.define(name, val.clone());
                }
                let result = expr::eval_expr(self, e);
                self.env = caller_env;
                match result {
                    Ok(v) | Err(Signal::Return(v)) => Ok(v),
                    Err(other)                     => Err(other),
                }
            }
        }
    }

    pub fn lookup(&self, name: &str) -> EvalResult {
        self.env.get(name)
            .cloned()
            .ok_or_else(|| Signal::Panic(format!("undefined name '{}'", name)))
    }
    }
