// src/interpreter/eval/mod.rs
//! `Interpreter` struct, function table, and call dispatch.

#![allow(dead_code)]

pub mod expr;
pub mod stmt;
pub mod pattern;

use crate::ast::common::TierAnnotation;
use crate::ast::declarations::{FunctionDecl, ParamKind};
use crate::ast::root::{Item, Program};
use crate::ast::statements::Block;
use crate::interpreter::builtins::{all_builtins, BuiltinFn};
use crate::interpreter::env::Environment;
use crate::interpreter::value::{EvalResult, FunctionId, Signal, Value};

// ── FunctionDef ───────────────────────────────────────────────────

/// One entry in the interpreter's function table.
pub struct FunctionDef<'ast> {
    pub name:     Option<String>,
    /// Parameter names in declaration order — used to bind args in the
    /// call frame. Separate from the AST params so we don't need a borrow
    /// on the function table when building the environment.
    pub params:   Vec<String>,
    pub body:     FunctionBody<'ast>,
    /// Environment captured at definition time (closure snapshot).
    pub closure:  Environment,
    pub tier:     TierAnnotation,
    pub is_async: bool,
}

/// The implementation of a callable.
pub enum FunctionBody<'ast> {
    /// User-defined function — Block<'ast> is Copy so we own a copy.
    Ast {
        block: Block<'ast>,
    },
    /// Native Rust implementation.
    Builtin(BuiltinFn),
}

// ── Interpreter ───────────────────────────────────────────────────

/// The tree-walking interpreter. Carries the `'ast` lifetime only because
/// `FunctionDef<'ast>` stores `Block<'ast>` references.
/// `Value` and `Environment` are both lifetime-free.
pub struct Interpreter<'ast> {
    pub(crate) env:       Environment,
    pub(crate) functions: Vec<FunctionDef<'ast>>,
}

impl<'ast> Interpreter<'ast> {
    /// Create a fresh interpreter with all built-ins registered.
    pub fn new() -> Self {
        let mut interp = Interpreter {
            env:       Environment::new(),
            functions: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    /// Register every built-in into the function table and global env.
    fn register_builtins(&mut self) {
        for (name, func) in all_builtins() {
            let id = self.alloc_function(FunctionDef {
                name:     Some(name.to_string()),
                params:   vec![], // builtins receive the full args slice directly
                body:     FunctionBody::Builtin(func),
                closure:  Environment::new(),
                tier:     TierAnnotation::High,
                is_async: false,
            });
            self.env.define(name, Value::Function(id));
        }
    }

    /// Append a `FunctionDef` to the table and return its `FunctionId`.
    pub fn alloc_function(&mut self, def: FunctionDef<'ast>) -> FunctionId {
        let id = self.functions.len();
        self.functions.push(def);
        id
    }

    /// Register a named function declaration and return its `FunctionId`.
    /// Called during `run_program`'s pre-declare pass so mutual recursion works.
    pub fn register_fn(&mut self, f: &'ast FunctionDecl<'ast>) -> FunctionId {
        let param_names: Vec<String> = f.params.iter()
            .filter_map(|p| match p.kind {
                ParamKind::Named { name, .. } => Some(name.to_string()),
                _ => None, // self params handled by method-call dispatch
            })
            .collect();

        self.alloc_function(FunctionDef {
            name:    Some(f.name.to_string()),
            params:  param_names,
            // f.body (Block<'ast>) is Copy — safe to store by value.
            body:    FunctionBody::Ast { block: f.body },
            closure: self.env.snapshot(),
            tier:    f.tier,
            is_async: f.is_async,
        })
    }

    /// Run a full program: pre-declare top-level functions, then call `main`.
    pub fn run_program(&mut self, program: &'ast Program<'ast>) -> Result<(), String> {
        // Pre-declare every top-level function so mutual recursion works.
        let mut top_level: Vec<(&'ast FunctionDecl<'ast>, FunctionId)> = Vec::new();
        for item in program.items {
            if let Item::Function(f) = item {
                let id = self.register_fn(f);
                self.env.define(f.name, Value::Function(id));
                top_level.push((f, id));
            }
        }

        // Find and call main.
        let main_val = self.env.get("main")
            .cloned()
            .ok_or_else(|| "no `main` function found".to_string())?;

        match main_val {
            Value::Function(id) => match self.call_function(id, &[]) {
                Ok(_)                   => Ok(()),
                Err(Signal::Return(_))  => Ok(()),
                Err(Signal::Panic(msg)) => Err(format!("panic: {}", msg)),
                Err(Signal::Fail(v))    => Err(format!("unhandled fail: {}", v)),
                Err(Signal::Break(_))   => Err("break outside loop".to_string()),
                Err(Signal::Continue)   => Err("continue outside loop".to_string()),
            },
            _ => Err("`main` is not a function".to_string()),
        }
    }

    /// Call a function by id with already-evaluated argument values.
    ///
    /// We extract everything we need from `self.functions[id]` in one borrow,
    /// then drop the borrow before calling `stmt::eval_block` which needs
    /// `&mut self`. This avoids the classic "borrow while mutably borrowed" issue.
    pub fn call_function(&mut self, id: FunctionId, args: &[Value]) -> EvalResult {
        // Extract the data we need — releasing the immutable borrow — before
        // any mutable operations.
        let (is_builtin, builtin_fn, block_opt, param_names, closure_snap) = {
            let def = &self.functions[id];
            match &def.body {
                FunctionBody::Builtin(f) => {
                    // f is a fn pointer (Copy), extract before releasing borrow.
                    (true, Some(*f), None, vec![], Environment::new())
                }
                FunctionBody::Ast { block } => {
                    // Block<'ast> is Copy — clone gives us an owned copy.
                    (false, None, Some(*block), def.params.clone(), def.closure.clone())
                }
            }
        }; // ← immutable borrow on self.functions released here

        if is_builtin {
            // Builtins get the args slice directly; no frame setup needed.
            return builtin_fn.unwrap()(args);
        }

        let block = block_opt.unwrap();

        // Set up the call frame: replace current env with the closure snapshot,
        // push a fresh scope, bind parameters, run the body, restore the env.
        let caller_env = std::mem::replace(&mut self.env, closure_snap);
        self.env.push();
        for (name, val) in param_names.iter().zip(args.iter()) {
            self.env.define(name, val.clone());
        }

        let result = stmt::eval_block(self, &block);

        // Always restore the caller's environment, even on signal.
        self.env = caller_env;

        // `return v` unboxed to Ok(v); other signals propagate.
        match result {
            Ok(v) | Err(Signal::Return(v)) => Ok(v),
            Err(other)                     => Err(other),
        }
    }

    /// Resolve a name in the current environment, turning a miss into a Panic.
    pub fn lookup(&self, name: &str) -> EvalResult {
        self.env.get(name)
            .cloned()
            .ok_or_else(|| Signal::Panic(format!("undefined name '{}'", name)))
    }
                                }
