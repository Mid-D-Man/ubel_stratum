// src/interpreter/eval/mod.rs
//! `Interpreter` struct, function table, and call dispatch.

#![allow(dead_code)]

pub mod expr;
pub mod stmt;
pub mod pattern;

use std::collections::HashMap;

use crate::ast::arena::AstArena;
use crate::ast::common::TierAnnotation;
use crate::ast::declarations::{FunctionDecl, MethodDecl, ParamKind, StructMember};
use crate::ast::root::{Item, Program};
use crate::ast::statements::Block;
use crate::interpreter::builtins::{all_builtins, BuiltinFn};
use crate::interpreter::env::Environment;
use crate::interpreter::value::{EvalResult, FunctionId, Signal, Value};

// ── FunctionDef ───────────────────────────────────────────────────

/// One entry in the interpreter's function table.
pub struct FunctionDef<'ast> {
    pub name:     Option<String>,
    /// Parameter names in declaration order.
    /// `self` variants are excluded here — they are handled by `call_method`.
    pub params:   Vec<String>,
    pub body:     FunctionBody<'ast>,
    /// Environment captured at definition time (closure snapshot).
    pub closure:  Environment,
    pub tier:     TierAnnotation,
    pub is_async: bool,
}

pub enum FunctionBody<'ast> {
    /// User-defined function body. `Block<'ast>` is Copy so we own a copy.
    Ast { block: Block<'ast> },
    /// Native Rust built-in.
    Builtin(BuiltinFn),
}

// ── Interpreter ───────────────────────────────────────────────────

/// Tree-walking interpreter. Parameterised over `'ast` because `FunctionDef`
/// stores arena-allocated `Block<'ast>` nodes.
///
/// `Value` and `Environment` are both lifetime-free, so closures and heap
/// values can be passed around freely.
pub struct Interpreter<'ast> {
    pub(crate) env:          Environment,
    pub(crate) functions:    Vec<FunctionDef<'ast>>,
    /// struct_type_name → method_name → FunctionId.
    /// Built during the pre-declare pass of `run_program`.
    pub(crate) method_table: HashMap<String, HashMap<String, FunctionId>>,
    /// Reference to the original AST arena — used to re-parse interpolated
    /// string expressions at runtime via `parser::parse_expr`.
    pub(crate) arena:        &'ast AstArena,
}

impl<'ast> Interpreter<'ast> {
    /// Create a fresh interpreter. `arena` must be the same arena used to
    /// parse the program that will be run.
    pub fn new(arena: &'ast AstArena) -> Self {
        let mut interp = Interpreter {
            env:          Environment::new(),
            functions:    Vec::new(),
            method_table: HashMap::new(),
            arena,
        };
        interp.register_builtins();
        interp
    }

    // ── Registration ─────────────────────────────────────────────

    fn register_builtins(&mut self) {
        for (name, func) in all_builtins() {
            let id = self.functions.len();
            self.functions.push(FunctionDef {
                name:     Some(name.to_string()),
                params:   vec![],
                body:     FunctionBody::Builtin(func),
                closure:  Environment::new(),
                tier:     TierAnnotation::High,
                is_async: false,
            });
            self.env.define(name, Value::Function(id));
        }
    }

    /// Append a FunctionDef and return its FunctionId.
    pub fn alloc_function(&mut self, def: FunctionDef<'ast>) -> FunctionId {
        let id = self.functions.len();
        self.functions.push(def);
        id
    }

    /// Register a top-level function declaration and return its FunctionId.
    /// Extracts all necessary data before the mutable call to avoid
    /// borrow-checker conflicts.
    pub fn register_fn(&mut self, f: &'ast FunctionDecl<'ast>) -> FunctionId {
        let params: Vec<String> = f.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        let block    = f.body;           // Copy
        let closure  = self.env.snapshot();
        let name     = f.name.to_string();
        let tier     = f.tier;
        let is_async = f.is_async;

        self.alloc_function(FunctionDef {
            name:    Some(name),
            params,
            body:    FunctionBody::Ast { block },
            closure,
            tier,
            is_async,
        })
    }

    /// Register a struct method. The FunctionId is stored in `method_table`
    /// and looked up during method-call dispatch.
    pub fn register_method(
        &mut self,
        struct_name: &str,
        m: &'ast MethodDecl<'ast>,
    ) -> FunctionId {
        // Non-`self` params only — `self` is bound by `call_method`.
        let params: Vec<String> = m.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        let block    = m.body;
        let closure  = self.env.snapshot();
        let name     = format!("{}::{}", struct_name, m.name);
        let tier     = m.tier;
        let is_async = m.is_async;

        self.alloc_function(FunctionDef {
            name:    Some(name),
            params,
            body:    FunctionBody::Ast { block },
            closure,
            tier,
            is_async,
        })
    }

    // ── Program entry point ───────────────────────────────────────

    /// Pre-declare all top-level items (enables mutual recursion), then call `main`.
    pub fn run_program(&mut self, program: &'ast Program<'ast>) -> Result<(), String> {
        // Pre-declare pass: register functions and struct methods before executing
        // any body, so forward references and mutual recursion work correctly.
        for item in program.items.iter().copied() {
            match item {
                Item::Function(f) => {
                    let id = self.register_fn(f);
                    self.env.define(f.name, Value::Function(id));
                }
                Item::Struct(s) => {
                    for member in s.members.iter().copied() {
                        if let StructMember::Method(m) = member {
                            let id = self.register_method(s.name, m);
                            self.method_table
                                .entry(s.name.to_string())
                                .or_default()
                                .insert(m.name.to_string(), id);
                        }
                    }
                }
                _ => {}
            }
        }

        // Find `main` and run it.
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

    /// Call a function by FunctionId with already-evaluated argument values.
    ///
    /// All data is extracted from the function table before any mutation
    /// of `self`, avoiding borrow-checker conflicts.
    pub fn call_function(&mut self, id: FunctionId, args: &[Value]) -> EvalResult {
        // Extract everything we need, releasing the immutable borrow on
        // `self.functions` before any mutable operations below.
        let (is_builtin, builtin_fn, block_opt, param_names, closure) = {
            let def = &self.functions[id];
            match &def.body {
                FunctionBody::Builtin(f) => (true, Some(*f), None, vec![], Environment::new()),
                FunctionBody::Ast { block } => {
                    (false, None, Some(*block), def.params.clone(), def.closure.clone())
                }
            }
        }; // ← immutable borrow released here

        if is_builtin {
            return builtin_fn.unwrap()(args);
        }

        let block = block_opt.unwrap();

        // Swap in the closure as the new environment, push a frame, bind params.
        let caller_env = std::mem::replace(&mut self.env, closure);
        self.env.push();
        for (name, val) in param_names.iter().zip(args.iter()) {
            self.env.define(name, val.clone());
        }

        let result = stmt::eval_block(self, &block);

        // Always restore the caller's environment.
        self.env = caller_env;

        match result {
            Ok(v) | Err(Signal::Return(v)) => Ok(v),
            Err(other)                     => Err(other),
        }
    }

    /// Call a struct method with an explicit receiver.
    /// Defines `self` in the call frame before binding regular parameters.
    pub fn call_method(
        &mut self,
        id:       FunctionId,
        receiver: Value,
        args:     &[Value],
    ) -> EvalResult {
        let (is_builtin, builtin_fn, block_opt, param_names, closure) = {
            let def = &self.functions[id];
            match &def.body {
                FunctionBody::Builtin(f) => (true, Some(*f), None, vec![], Environment::new()),
                FunctionBody::Ast { block } => {
                    (false, None, Some(*block), def.params.clone(), def.closure.clone())
                }
            }
        };

        if is_builtin {
            // Builtins called as methods get receiver prepended to args.
            let mut all_args = vec![receiver];
            all_args.extend_from_slice(args);
            return builtin_fn.unwrap()(&all_args);
        }

        let block = block_opt.unwrap();
        let caller_env = std::mem::replace(&mut self.env, closure);
        self.env.push();
        self.env.define("self", receiver);   // ← receiver visible as `self`
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

    /// Resolve a name in the current environment, panicking on miss.
    pub fn lookup(&self, name: &str) -> EvalResult {
        self.env.get(name)
            .cloned()
            .ok_or_else(|| Signal::Panic(format!("undefined name '{}'", name)))
    }
}
