// src/interpreter/eval/expr.rs
//! Expression evaluation.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::common::{AssignOp, BinOp, UnaryOp};
use crate::ast::expressions::{
    ArgKind, Expr, ExprKind, LambdaBody, LinqClause, MatchArmBody, OrElseFallback,
};
use crate::ast::literals::{InterpolationPart, Literal};
use crate::ast::types::{Type, TypeKind};
use crate::interpreter::eval::{stmt, pattern, FunctionBody, FunctionDef, Interpreter};
use crate::interpreter::value::{EvalResult, Signal, Value};

// ── Main entry ────────────────────────────────────────────────────

pub fn eval_expr<'ast>(interp: &mut Interpreter<'ast>, expr: &Expr<'ast>) -> EvalResult {
    match &expr.kind {

        // ── Literals ──────────────────────────────────────────────
        ExprKind::Lit(lit) => eval_literal(interp, lit),

        // ── Identifier lookup ─────────────────────────────────────
        ExprKind::Ident(name) => interp.lookup(name),

        // ── self ─────────────────────────────────────────────────
        ExprKind::SelfExpr => interp.lookup("self"),

        // ── Short declaration: x := expr ──────────────────────────
        // Defines in the current scope and returns the value.
        ExprKind::ShortDecl { name, value } => {
            let val = eval_expr(interp, value)?;
            interp.env.define(name, val.clone());
            Ok(val)
        }

        // ── Assignment ────────────────────────────────────────────
        ExprKind::Assign { op, target, value } => {
            eval_assign(interp, *op, target, value)
        }

        // ── Pipe: left |> right ───────────────────────────────────
        ExprKind::Pipe { left, right } => {
            let left_val = eval_expr(interp, left)?;
            let right_val = eval_expr(interp, right)?;
            match right_val {
                Value::Function(id) => interp.call_function(id, &[left_val]),
                other => Err(Signal::Panic(format!(
                    "right side of |> must be a function, got {}", other.type_name()
                ))),
            }
        }

        // ── Binary operators ─────────────────────────────────────
        ExprKind::BinOp { op, lhs, rhs } => {
            // Short-circuit logical operators.
            match op {
                BinOp::And => {
                    let l = eval_expr(interp, lhs)?;
                    if !l.is_truthy()? { return Ok(Value::Bool(false)); }
                    let r = eval_expr(interp, rhs)?;
                    Ok(Value::Bool(r.is_truthy()?))
                }
                BinOp::Or => {
                    let l = eval_expr(interp, lhs)?;
                    if l.is_truthy()? { return Ok(Value::Bool(true)); }
                    let r = eval_expr(interp, rhs)?;
                    Ok(Value::Bool(r.is_truthy()?))
                }
                _ => {
                    let lv = eval_expr(interp, lhs)?;
                    let rv = eval_expr(interp, rhs)?;
                    eval_binop(*op, lv, rv)
                }
            }
        }

        // ── Unary operators ───────────────────────────────────────
        ExprKind::UnaryOp { op, operand } => {
            let v = eval_expr(interp, operand)?;
            match op {
                UnaryOp::Neg => match v {
                    Value::Int(n)    => Ok(Value::Int(-n)),
                    Value::Float(f)  => Ok(Value::Float(-f)),
                    Value::Double(d) => Ok(Value::Double(-d)),
                    other => Err(Signal::Panic(format!("cannot negate {}", other.type_name()))),
                },
                UnaryOp::Not => {
                    let b = v.is_truthy()?;
                    Ok(Value::Bool(!b))
                }
                UnaryOp::BitNot => match v {
                    Value::Int(n) => Ok(Value::Int(!n)),
                    other => Err(Signal::Panic(format!("~ not supported on {}", other.type_name()))),
                },
                // Tree-walker doesn't implement real async — await is a no-op here.
                UnaryOp::Await => Ok(v),
            }
        }

        // ── Function / method call ────────────────────────────────
        ExprKind::Call { callee, args } => {
            // Check for method or static call: receiver.method(args)
            if let ExprKind::Field { target: recv_expr, field: method_name } = &callee.kind {
                return eval_call_with_receiver(interp, recv_expr, method_name, args);
            }

            // Regular function call.
            let callee_val = eval_expr(interp, callee)?;
            let eval_args  = eval_args(interp, args)?;
            match callee_val {
                Value::Function(id) => interp.call_function(id, &eval_args),
                other => Err(Signal::Panic(format!(
                    "cannot call value of type '{}'", other.type_name()
                ))),
            }
        }

        // ── Field access: obj.field ───────────────────────────────
        ExprKind::Field { target, field } => {
            // `EnumName.Variant` constructs an enum value — this has to be
            // checked before evaluating `target`, since `EnumName` is a
            // type name, not something bound in the environment.
            if let ExprKind::Ident(name) = &target.kind {
                if let Some(variants) = interp.enum_table.get(*name) {
                    return if variants.contains(*field) {
                        Ok(Value::Enum {
                            type_name: name.to_string(),
                            variant:   field.to_string(),
                            payload:   Box::new(crate::interpreter::value::EnumPayload::None),
                        })
                    } else {
                        Err(Signal::Panic(format!(
                            "enum '{}' has no variant '{}'", name, field
                        )))
                    };
                }
            }
            let obj = eval_expr(interp, target)?;
            get_field(obj, field)
        }

        // ── Index: collection[index] ──────────────────────────────
        ExprKind::Index { target, index } => {
            let coll = eval_expr(interp, target)?;
            let idx  = eval_expr(interp, index)?;
            eval_index(coll, idx)
        }

        // ── Optional chain: obj?.field or obj?.method() ───────────
        ExprKind::OptionalChain { target, access } => {
            let obj = eval_expr(interp, target)?;
            if matches!(obj, Value::Null) {
                return Ok(Value::Null);
            }
            use crate::ast::expressions::OptionalAccess;
            match access {
                OptionalAccess::Field(field) => get_field(obj, field),
                OptionalAccess::Method { name, args } => {
                    let eval_args = eval_args(interp, args)?;
                    eval_method_call(interp, obj, name, &eval_args)
                }
            }
        }

        // ── Error propagation: expr? ──────────────────────────────
        // Signal::Fail propagates naturally up the call stack.
        // On Ok(val), strip Optional / Fallible wrapper and continue.
        ExprKind::Try(inner) => {
            let val = eval_expr(interp, inner)?;
            // In the tree-walker, fallible results are just values —
            // Signal::Fail would already have propagated above.
            Ok(val)
        }

        // ── Await: async is a no-op in the tree-walker ────────────
        ExprKind::Await(inner) => eval_expr(interp, inner),

        // ── Type cast: expr as Type ───────────────────────────────
        ExprKind::As { expr: inner, ty } => {
            let val = eval_expr(interp, inner)?;
            eval_cast(val, ty)
        }

        // ── Array literal: [a, b, c] ──────────────────────────────
        ExprKind::Array(elems) => {
            let items: Result<Vec<Value>, Signal> = elems.iter()
                .map(|e| eval_expr(interp, e))
                .collect();
            Ok(Value::List(Rc::new(RefCell::new(items?))))
        }

        // ── Tuple literal: (a, b, c) ─────────────────────────────
        ExprKind::Tuple(elems) => {
            let items: Result<Vec<Value>, Signal> = elems.iter()
                .map(|e| eval_expr(interp, e))
                .collect();
            Ok(Value::Tuple(items?))
        }

        // ── Dictionary literal: { key = value, ... } ─────────────
        ExprKind::Dict(entries) => {
            let mut pairs: Vec<(Value, Value)> = Vec::with_capacity(entries.len());
            for entry in entries.iter() {
                let k = eval_expr(interp, entry.key)?;
                let v = eval_expr(interp, entry.value)?;
                pairs.push((k, v));
            }
            Ok(Value::Dict(Rc::new(RefCell::new(pairs))))
        }

        // ── Anonymous object: { x = 1, y = 2 } ───────────────────
        ExprKind::AnonObject(fields) => {
            let mut map = HashMap::new();
            for f in fields.iter() {
                let v = eval_expr(interp, f.value)?;
                map.insert(f.name.to_string(), v);
            }
            Ok(Value::Struct {
                type_name: "<anon>".to_string(),
                fields:    Rc::new(RefCell::new(map)),
            })
        }

        // ── Struct literal: Point { x = 1, y = 2 } ───────────────
        ExprKind::StructLit { path, fields } => {
            let type_name = path.last().copied().unwrap_or("").to_string();
            let mut map = HashMap::new();
            for f in fields.iter() {
                let v = eval_expr(interp, f.value)?;
                map.insert(f.name.to_string(), v);
            }
            Ok(Value::Struct {
                type_name,
                fields: Rc::new(RefCell::new(map)),
            })
        }

        // ── Lambda: fn(params) body ───────────────────────────────
        ExprKind::Lambda(lambda) => {
            let closure: crate::interpreter::env::Environment = interp.env.snapshot();
            let params: Vec<String> = lambda.params.iter()
                .map(|p| p.name.to_string())
                .collect();
            let body = match &lambda.body {
                LambdaBody::Block(b) => FunctionBody::Ast { block: *b },
                LambdaBody::Expr(e)  => FunctionBody::ExprBody { expr: e },
            };
            let id = interp.alloc_function(FunctionDef {
                name:     None,
                params,
                body,
                closure,
                tier:     crate::ast::common::TierAnnotation::High,
                is_async: false,
            });
            Ok(Value::Function(id))
        }

        // ── Block expression: { stmts } ───────────────────────────
        ExprKind::Block(b) => stmt::eval_block(interp, b),

        // ── If expression ─────────────────────────────────────────
        ExprKind::If(if_node) => {
            let cond = eval_expr(interp, if_node.condition)?;
            if cond.is_truthy()? {
                return stmt::eval_block(interp, &if_node.then_block);
            }
            for elif in if_node.elif_branches {
                let c = eval_expr(interp, elif.condition)?;
                if c.is_truthy()? {
                    return stmt::eval_block(interp, &elif.block);
                }
            }
            match &if_node.else_block {
                Some(b) => stmt::eval_block(interp, b),
                None    => Ok(Value::Void),
            }
        }

        // ── Match expression ─────────────────────────────────────
        ExprKind::Match(m) => {
            let scrutinee = eval_expr(interp, m.scrutinee)?;
            for arm in m.arms.iter() {
                interp.env.push();
                let matched = pattern::match_pattern(&arm.pattern, &scrutinee, &mut interp.env);
                if matched {
                    let guard_ok = match arm.guard {
                        Some(g) => eval_expr(interp, g)?.is_truthy()?,
                        None    => true,
                    };
                    if guard_ok {
                        let result = match &arm.body {
                            MatchArmBody::Expr(e)  => eval_expr(interp, e),
                            MatchArmBody::Block(b) => stmt::eval_block(interp, b),
                        };
                        interp.env.pop();
                        return result;
                    }
                }
                interp.env.pop();
            }
            Ok(Value::Void)
        }

        // ── LINQ: from x in source where ... select projection ────
        ExprKind::Linq(linq) => eval_linq(interp, linq),

        // ── Or-else: expr or fallback ─────────────────────────────
        ExprKind::OrElse { expr: inner, fallback } => {
            let val = eval_expr(interp, inner)?;
            match &val {
                Value::Null => match fallback {
                    OrElseFallback::Expr(fb) => eval_expr(interp, fb),
                    OrElseFallback::Continue => Err(Signal::Continue),
                    OrElseFallback::Break    => Err(Signal::Break(None)),
                    OrElseFallback::Return(maybe_e) => {
                        let v = match maybe_e {
                            Some(e) => eval_expr(interp, e)?,
                            None    => Value::Void,
                        };
                        Err(Signal::Return(v))
                    }
                },
                _ => Ok(val),
            }
        }
    }
}

// ── Literal evaluation ────────────────────────────────────────────

fn eval_literal<'ast>(interp: &mut Interpreter<'ast>, lit: &Literal<'ast>) -> EvalResult {
    match lit {
        Literal::Int(n)    => Ok(Value::Int(*n)),
        Literal::Float(f)  => Ok(Value::Float(*f)),
        Literal::Double(d) => Ok(Value::Double(*d)),
        Literal::Bool(b)   => Ok(Value::Bool(*b)),
        Literal::Char(c)   => Ok(Value::Char(*c)),
        Literal::Null      => Ok(Value::Null),

        Literal::Str(s)        => Ok(Value::str_from(*s)),
        Literal::VerbatimStr(s) => Ok(Value::str_from(*s)),

        // Interpolation holes are fully parsed Expr nodes by the time the
        // interpreter sees them (parsed by rd_parser, alongside everything
        // else) — evaluating one is no different from evaluating any other
        // expression in the language.
        Literal::InterpolatedStr(parts)
        | Literal::InterpolatedVerbatimStr(parts) => {
            let mut result = String::new();
            for part in parts.iter() {
                match part {
                    InterpolationPart::Text(t) => result.push_str(t),
                    InterpolationPart::Expr(expr) => {
                        let val = eval_expr(interp, expr)?;
                        result.push_str(&val.to_string());
                    }
                }
            }
            Ok(Value::str_from(result))
        }
    }
}

// ── Binary operator evaluation ────────────────────────────────────

fn eval_binop(op: BinOp, lhs: Value, rhs: Value) -> EvalResult {
    // String concatenation.
    if let BinOp::Add = op {
        if let (Value::Str(a), Value::Str(b)) = (&lhs, &rhs) {
            return Ok(Value::str_from(format!("{}{}", a, b)));
        }
    }

    // Range operators → produce a list of integers.
    match op {
        BinOp::Range => {
            if let (Value::Int(lo), Value::Int(hi)) = (&lhs, &rhs) {
                let items: Vec<Value> = (*lo..*hi).map(Value::Int).collect();
                return Ok(Value::List(Rc::new(RefCell::new(items))));
            }
            return Err(Signal::Panic(".. requires integer operands".into()));
        }
        BinOp::RangeIncl => {
            if let (Value::Int(lo), Value::Int(hi)) = (&lhs, &rhs) {
                let items: Vec<Value> = (*lo..=*hi).map(Value::Int).collect();
                return Ok(Value::List(Rc::new(RefCell::new(items))));
            }
            return Err(Signal::Panic("..= requires integer operands".into()));
        }
        _ => {}
    }

    // Equality — works on any type.
    match op {
        BinOp::Eq => return Ok(Value::Bool(lhs.equals(&rhs))),
        BinOp::Ne => return Ok(Value::Bool(!lhs.equals(&rhs))),
        _ => {}
    }

    // Numeric operations with implicit promotion.
    // Promotion ladder: Int → Float → Double.
    let (lv, rv, is_double, is_float) = promote_numeric(&lhs, &rhs)?;

    let result = match op {
        BinOp::Add  => lv + rv,
        BinOp::Sub  => lv - rv,
        BinOp::Mul  => lv * rv,
        BinOp::Div  => {
            if rv == 0.0 { return Err(Signal::Panic("division by zero".into())); }
            lv / rv
        }
        BinOp::Rem  => {
            if rv == 0.0 { return Err(Signal::Panic("modulo by zero".into())); }
            lv % rv
        }
        BinOp::Lt   => return Ok(Value::Bool(lv < rv)),
        BinOp::Le   => return Ok(Value::Bool(lv <= rv)),
        BinOp::Gt   => return Ok(Value::Bool(lv > rv)),
        BinOp::Ge   => return Ok(Value::Bool(lv >= rv)),

        BinOp::BitAnd => return bitwise_op(op, &lhs, &rhs),
        BinOp::BitOr  => return bitwise_op(op, &lhs, &rhs),
        BinOp::BitXor => return bitwise_op(op, &lhs, &rhs),
        BinOp::Shl    => return bitwise_op(op, &lhs, &rhs),
        BinOp::Shr    => return bitwise_op(op, &lhs, &rhs),

        _ => return Err(Signal::Panic(format!("unsupported binary op: {:?}", op))),
    };

    if is_double {
        Ok(Value::Double(result))
    } else if is_float {
        Ok(Value::Float(result as f32))
    } else {
        Ok(Value::Int(result as i64))
    }
}

/// Promote both values to f64 for arithmetic.
/// Returns (lv, rv, is_double, is_float).
fn promote_numeric(lhs: &Value, rhs: &Value) -> Result<(f64, f64, bool, bool), Signal> {
    let to_f64 = |v: &Value| -> Option<(f64, bool, bool)> {
        match v {
            Value::Int(n)    => Some((*n as f64, false, false)),
            Value::Float(f)  => Some((*f as f64, false, true)),
            Value::Double(d) => Some((*d, true, false)),
            _ => None,
        }
    };
    let (lv, ld, lf) = to_f64(lhs).ok_or_else(|| Signal::Panic(format!(
        "arithmetic not supported on {}", lhs.type_name()
    )))?;
    let (rv, rd, rf) = to_f64(rhs).ok_or_else(|| Signal::Panic(format!(
        "arithmetic not supported on {}", rhs.type_name()
    )))?;
    Ok((lv, rv, ld || rd, lf || rf))
}

fn bitwise_op(op: BinOp, lhs: &Value, rhs: &Value) -> EvalResult {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(match op {
            BinOp::BitAnd => a & b,
            BinOp::BitOr  => a | b,
            BinOp::BitXor => a ^ b,
            BinOp::Shl    => a << (b & 63),
            BinOp::Shr    => a >> (b & 63),
            _ => unreachable!(),
        })),
        _ => Err(Signal::Panic(format!(
            "bitwise op requires int operands, got {} and {}",
            lhs.type_name(), rhs.type_name()
        ))),
    }
}

// ── Assignment ────────────────────────────────────────────────────

fn eval_assign<'ast>(
    interp: &mut Interpreter<'ast>,
    op:     AssignOp,
    target: &'ast Expr<'ast>,
    value:  &'ast Expr<'ast>,
) -> EvalResult {
    let rhs = eval_expr(interp, value)?;

    if let AssignOp::Assign = op {
        return write_lvalue(interp, target, rhs);
    }

    // Compound assignment: read → binop → write.
    let current = read_lvalue(interp, target)?;
    let binop   = assign_op_to_binop(op);
    let new_val = eval_binop(binop, current, rhs)?;
    write_lvalue(interp, target, new_val)
}

fn assign_op_to_binop(op: AssignOp) -> BinOp {
    match op {
        AssignOp::Assign     => BinOp::Add, // unreachable in compound path
        AssignOp::AddAssign  => BinOp::Add,
        AssignOp::SubAssign  => BinOp::Sub,
        AssignOp::MulAssign  => BinOp::Mul,
        AssignOp::DivAssign  => BinOp::Div,
        AssignOp::RemAssign  => BinOp::Rem,
        AssignOp::BitAndAssign => BinOp::BitAnd,
        AssignOp::BitOrAssign  => BinOp::BitOr,
        AssignOp::BitXorAssign => BinOp::BitXor,
        AssignOp::ShlAssign    => BinOp::Shl,
        AssignOp::ShrAssign    => BinOp::Shr,
    }
}

/// Read the current value of an lvalue expression without consuming it.
fn read_lvalue<'ast>(interp: &mut Interpreter<'ast>, target: &'ast Expr<'ast>) -> EvalResult {
    eval_expr(interp, target)
}

/// Write a value to an lvalue expression.
fn write_lvalue<'ast>(
    interp: &mut Interpreter<'ast>,
    target: &'ast Expr<'ast>,
    value:  Value,
) -> EvalResult {
    match &target.kind {
        ExprKind::Ident(name) => {
            // Try to update existing binding; define if it doesn't exist.
            if !interp.env.set(name, value.clone()) {
                interp.env.define(name, value);
            }
            Ok(Value::Void)
        }
        ExprKind::Field { target: obj_expr, field } => {
            let obj = eval_expr(interp, obj_expr)?;
            match obj {
                Value::Struct { fields, .. } => {
                    fields.borrow_mut().insert(field.to_string(), value);
                    Ok(Value::Void)
                }
                other => Err(Signal::Panic(format!(
                    "cannot assign to field '{}' on {}", field, other.type_name()
                ))),
            }
        }
        ExprKind::Index { target: coll_expr, index: idx_expr } => {
            let coll = eval_expr(interp, coll_expr)?;
            let idx  = eval_expr(interp, idx_expr)?;
            match (coll, idx) {
                (Value::List(rc), Value::Int(i)) => {
                    let mut list = rc.borrow_mut();
                    let i = i as usize;
                    if i < list.len() {
                        list[i] = value;
                        Ok(Value::Void)
                    } else {
                        Err(Signal::Panic(format!(
                            "index {} out of bounds (len {})", i, list.len()
                        )))
                    }
                }
                (Value::Dict(rc), key) => {
                    let mut dict = rc.borrow_mut();
                    if let Some(entry) = dict.iter_mut().find(|(k, _)| k.equals(&key)) {
                        entry.1 = value;
                    } else {
                        dict.push((key, value));
                    }
                    Ok(Value::Void)
                }
                _ => Err(Signal::Panic("invalid index assignment target".into())),
            }
        }
        _ => Err(Signal::Panic("invalid assignment target".into())),
    }
}

// ── Field access ──────────────────────────────────────────────────

fn get_field(obj: Value, field: &str) -> EvalResult {
    match obj {
        Value::Struct { ref fields, .. } => {
            fields.borrow().get(field)
                .cloned()
                .ok_or_else(|| Signal::Panic(format!(
                    "no field '{}' on {}", field, obj.type_name()
                )))
        }
        Value::Enum { ref type_name, ref variant, payload: _ } => {
            // Allow accessing discriminant metadata fields.
            match field {
                "type_name" => Ok(Value::str_from(type_name.as_str())),
                "variant"   => Ok(Value::str_from(variant.as_str())),
                _ => Err(Signal::Panic(format!("no field '{}' on enum", field))),
            }
        }
        other => Err(Signal::Panic(format!(
            "cannot access field '{}' on {}", field, other.type_name()
        ))),
    }
}

// ── Index access ──────────────────────────────────────────────────

fn eval_index(coll: Value, idx: Value) -> EvalResult {
    match (coll, idx) {
        (Value::List(rc), Value::Int(i)) => {
            let list = rc.borrow();
            let i = if i < 0 {
                // Negative indexing: -1 = last element.
                let len = list.len() as i64;
                (len + i) as usize
            } else {
                i as usize
            };
            list.get(i)
                .cloned()
                .ok_or_else(|| Signal::Panic(format!("list index {} out of bounds", i)))
        }
        (Value::Tuple(elems), Value::Int(i)) => {
            let i = i as usize;
            elems.get(i)
                .cloned()
                .ok_or_else(|| Signal::Panic(format!("tuple index {} out of bounds", i)))
        }
        (Value::Dict(rc), key) => {
            let dict = rc.borrow();
            dict.iter()
                .find(|(k, _)| k.equals(&key))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Signal::Panic("key not found in dictionary".into()))
        }
        (Value::Str(s), Value::Int(i)) => {
            let i = i as usize;
            s.chars().nth(i)
                .map(Value::Char)
                .ok_or_else(|| Signal::Panic(format!("string index {} out of bounds", i)))
        }
        (coll, idx) => Err(Signal::Panic(format!(
            "cannot index {} with {}", coll.type_name(), idx.type_name()
        ))),
    }
}

// ── Call dispatch helpers ─────────────────────────────────────────

/// Evaluate a call where the callee is `recv_expr.method_name(args)`.
///
/// Three cases in priority order:
///   1. `TypeName.method(args)` — static method (type name in method_table).
///   2. `obj.method(args)` — instance method on a struct.
///   3. `obj.method(args)` — built-in method on List, Str, Dict, Tuple.
fn eval_call_with_receiver<'ast>(
    interp:       &mut Interpreter<'ast>,
    recv_expr:    &'ast Expr<'ast>,
    method_name:  &str,
    raw_args:     &'ast [crate::ast::expressions::Arg<'ast>],
) -> EvalResult {
    // Case 1: static call — receiver is a bare identifier naming a type.
    if let ExprKind::Ident(type_name) = &recv_expr.kind {
        // `Pool.new()` needs the interpreter's own ambient
        // `pool_capacity_stack` (MEMORY_MODEL.md §11) — it can't be a
        // normal `BuiltinFn` (`fn(&[Value]) -> EvalResult`), since that
        // signature has no way to reach interpreter state. Checked
        // before `is_builtin_namespace` for that reason, not folded
        // into `constructors.rs`/`resolve_namespace_member` like
        // `List.new()` etc.
        if *type_name == "Pool" && method_name == "new" {
            let cap = interp.pool_capacity_stack.last().copied().ok_or_else(|| {
                Signal::Panic(
                    "Pool.new() requires an enclosing with pool<T>(count) block".into(),
                )
            })?;
            return Ok(Value::new_pool(cap));
        }
        if crate::builtins::is_builtin_namespace(type_name) {
            let func = crate::builtins::resolve_namespace_member(type_name, method_name)
                .ok_or_else(|| Signal::Panic(format!(
                    "'{}' has no member '{}'", type_name, method_name
                )))?;
            let args = eval_args(interp, raw_args)?;
            return func(&args);
        }
        if interp.method_table.contains_key(*type_name) {
            let fn_id = interp.method_table
                .get(*type_name)
                .and_then(|m| m.get(method_name))
                .copied()
                .ok_or_else(|| Signal::Panic(format!(
                    "no static method '{}' on '{}'", method_name, type_name
                )))?;
            let args = eval_args(interp, raw_args)?;
            return interp.call_function(fn_id, &args);
        }
    }

    // Case 2 & 3: evaluate the receiver, then dispatch.
    let receiver = eval_expr(interp, recv_expr)?;
    let args     = eval_args(interp, raw_args)?;
    eval_method_call(interp, receiver, method_name, &args)
}

/// Dispatch a method call given an already-evaluated receiver.
fn eval_method_call(
    interp:      &mut Interpreter<'_>,
    receiver:    Value,
    method_name: &str,
    args:        &[Value],
) -> EvalResult {
    match &receiver {
        // ── Built-in List methods ──────────────────────────────────
        Value::List(rc) => {
            use crate::builtins::instance::list_methods as m;
            match method_name {
                "len"      => return Ok(m::len(rc)),
                "push"     => return m::push(rc, args),
                "pop"      => return Ok(m::pop(rc)),
                "contains" => return m::contains(rc, args),
                "first"    => return Ok(m::first(rc)),
                "last"     => return Ok(m::last(rc)),
                "is_empty" => return Ok(m::is_empty(rc)),
                "reverse"  => return Ok(m::reverse(rc)),
                _ => {}
            }
        }

        // ── Built-in Queue methods ──────────────────────────────────
        Value::Queue(rc) => {
            use crate::builtins::instance::queue_methods as m;
            match method_name {
                "len"      => return Ok(m::len(rc)),
                "is_empty" => return Ok(m::is_empty(rc)),
                "enqueue"  => return m::enqueue(rc, args),
                "dequeue"  => return Ok(m::dequeue(rc)),
                "peek"     => return Ok(m::peek(rc)),
                "contains" => return m::contains(rc, args),
                "clear"    => return Ok(m::clear(rc)),
                _ => {}
            }
        }

        // ── Built-in Stack methods ──────────────────────────────────
        Value::Stack(rc) => {
            use crate::builtins::instance::stack_methods as m;
            match method_name {
                "len"      => return Ok(m::len(rc)),
                "is_empty" => return Ok(m::is_empty(rc)),
                "push"     => return m::push(rc, args),
                "pop"      => return Ok(m::pop(rc)),
                "peek"     => return Ok(m::peek(rc)),
                "contains" => return m::contains(rc, args),
                "clear"    => return Ok(m::clear(rc)),
                _ => {}
            }
        }

        // ── Built-in Pool methods (MEMORY_MODEL.md §11) ──────────────
        Value::Pool(rc) => {
            use crate::builtins::instance::pool_methods as m;
            match method_name {
                "acquire" => return m::acquire(rc, args),
                "release" => return m::release(rc, args),
                "at"      => return m::at(rc, args),
                _ => {}
            }
        }

        // ── Built-in String methods ────────────────────────────────
        Value::Str(s) => {
            let s = s.clone(); // Rc clone so we don't hold borrow across returns
            match method_name {
                "len"        => return Ok(Value::Int(s.len() as i64)),
                "is_empty"   => return Ok(Value::Bool(s.is_empty())),
                "to_upper"   => return Ok(Value::str_from(s.to_uppercase())),
                "to_lower"   => return Ok(Value::str_from(s.to_lowercase())),
                "trim"       => return Ok(Value::str_from(s.trim())),
                "trim_start" => return Ok(Value::str_from(s.trim_start())),
                "trim_end"   => return Ok(Value::str_from(s.trim_end())),
                "chars"      => {
                    let chars: Vec<Value> = s.chars().map(Value::Char).collect();
                    return Ok(Value::List(Rc::new(RefCell::new(chars))));
                }
                "contains" => {
                    let sub = args.first().ok_or_else(|| Signal::Panic("contains() needs 1 arg".into()))?;
                    if let Value::Str(sub_str) = sub {
                        return Ok(Value::Bool(s.contains(sub_str.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "starts_with" => {
                    let sub = args.first().ok_or_else(|| Signal::Panic("starts_with() needs 1 arg".into()))?;
                    if let Value::Str(sub_str) = sub {
                        return Ok(Value::Bool(s.starts_with(sub_str.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "ends_with" => {
                    let sub = args.first().ok_or_else(|| Signal::Panic("ends_with() needs 1 arg".into()))?;
                    if let Value::Str(sub_str) = sub {
                        return Ok(Value::Bool(s.ends_with(sub_str.as_str())));
                    }
                    return Ok(Value::Bool(false));
                }
                "split" => {
                    let delim = args.first().ok_or_else(|| Signal::Panic("split() needs 1 arg".into()))?;
                    if let Value::Str(d) = delim {
                        let parts: Vec<Value> = s.split(d.as_str())
                            .map(Value::str_from)
                            .collect();
                        return Ok(Value::List(Rc::new(RefCell::new(parts))));
                    }
                    return Err(Signal::Panic("split() delimiter must be a string".into()));
                }
                "replace" => {
                    if args.len() < 2 { return Err(Signal::Panic("replace() needs 2 arguments".into())); }
                    if let (Value::Str(from), Value::Str(to)) = (&args[0], &args[1]) {
                        return Ok(Value::str_from(s.replace(from.as_str(), to.as_str())));
                    }
                    return Err(Signal::Panic("replace() arguments must be strings".into()));
                }
                _ => {}
            }
        }

        // ── Built-in Dict methods ──────────────────────────────────
        Value::Dict(rc) => {
            use crate::builtins::instance::dict_methods as m;
            match method_name {
                "len"          => return Ok(m::len(rc)),
                "is_empty"     => return Ok(m::is_empty(rc)),
                "contains_key" => return m::contains_key(rc, args),
                "keys"         => return Ok(m::keys(rc)),
                "values"       => return Ok(m::values(rc)),
                "insert"       => return m::insert(rc, args),
                "at"           => return m::at(rc, args),
                _ => {}
            }
        }

        // ── Built-in Tuple methods ─────────────────────────────────
        Value::Tuple(elems) => match method_name {
            "len" => return Ok(Value::Int(elems.len() as i64)),
            _ => {}
        },

        _ => {}
    }

    // User-defined instance method on a struct.
    let type_name = match &receiver {
        Value::Struct { type_name, .. } => type_name.clone(),
        other => return Err(Signal::Panic(format!(
            "no method '{}' on {}", method_name, other.type_name()
        ))),
    };
    let fn_id = interp.method_table
        .get(&type_name)
        .and_then(|m| m.get(method_name))
        .copied()
        .ok_or_else(|| Signal::Panic(format!(
            "no method '{}' on '{}'", method_name, type_name
        )))?;
    interp.call_method(fn_id, receiver, args)
}

// ── Evaluate argument list ────────────────────────────────────────

fn eval_args<'ast>(
    interp: &mut Interpreter<'ast>,
    args:   &'ast [crate::ast::expressions::Arg<'ast>],
) -> Result<Vec<Value>, Signal> {
    args.iter()
        .map(|a| match &a.kind {
            ArgKind::Positional(e)       => eval_expr(interp, e),
            ArgKind::Named { value, .. } => eval_expr(interp, value),
        })
        .collect()
}

// ── Type cast ─────────────────────────────────────────────────────

fn eval_cast<'ast>(val: Value, ty: &'ast Type<'ast>) -> EvalResult {
    match ty.kind {
        TypeKind::Int | TypeKind::I64 | TypeKind::I32 => match val {
            Value::Int(n)    => Ok(Value::Int(n)),
            Value::Float(f)  => Ok(Value::Int(f as i64)),
            Value::Double(d) => Ok(Value::Int(d as i64)),
            Value::Bool(b)   => Ok(Value::Int(if b { 1 } else { 0 })),
            Value::Str(s)    => s.parse::<i64>()
                .map(Value::Int)
                .map_err(|_| Signal::Panic(format!("cannot cast '{}' to int", s))),
            other => Err(Signal::Panic(format!("cannot cast {} to int", other.type_name()))),
        },
        TypeKind::Float | TypeKind::F32 => match val {
            Value::Float(f)  => Ok(Value::Float(f)),
            Value::Int(n)    => Ok(Value::Float(n as f32)),
            Value::Double(d) => Ok(Value::Float(d as f32)),
            other => Err(Signal::Panic(format!("cannot cast {} to float", other.type_name()))),
        },
        TypeKind::Double | TypeKind::F64 => match val {
            Value::Double(d) => Ok(Value::Double(d)),
            Value::Int(n)    => Ok(Value::Double(n as f64)),
            Value::Float(f)  => Ok(Value::Double(f as f64)),
            other => Err(Signal::Panic(format!("cannot cast {} to double", other.type_name()))),
        },
        TypeKind::Str => Ok(Value::str_from(val.to_string())),
        TypeKind::Bool => {
            let b = val.is_truthy()?;
            Ok(Value::Bool(b))
        }
        _ => Ok(val), // Unknown cast — pass through for now.
    }
}

// ── LINQ evaluation ───────────────────────────────────────────────

fn eval_linq<'ast>(
    interp: &mut Interpreter<'ast>,
    linq:   &'ast crate::ast::expressions::LinqExpr<'ast>,
) -> EvalResult {
    let source = eval_expr(interp, linq.source)?;
    let items: Vec<Value> = match source {
        Value::List(rc) => rc.borrow().clone(),
        other           => return Err(Signal::Panic(format!(
            "LINQ source must be a List, got {}", other.type_name()
        ))),
    };

    let mut result: Vec<Value> = Vec::new();
    let mut order_by: Option<Vec<(Value, Value)>> = None; // (key, item) pairs for sort

    'each_item: for item in items {
        interp.env.push();
        interp.env.define(linq.binding, item.clone());

        // Process each clause in declaration order.
        for clause in linq.clauses.iter() {
            match clause {
                LinqClause::Where(cond) => {
                    match eval_expr(interp, cond)?.is_truthy() {
                        Ok(true)  => {}
                        Ok(false) => { interp.env.pop(); continue 'each_item; }
                        Err(s)    => { interp.env.pop(); return Err(s); }
                    }
                }
                LinqClause::Let { name, value } => {
                    let v = eval_expr(interp, value)?;
                    interp.env.define(name, v);
                }
                LinqClause::OrderBy { expr, descending: _ } => {
                    let key = eval_expr(interp, expr)?;
                    let projected = eval_expr(interp, linq.select)?;
                    let entry = (key, projected);
                    order_by.get_or_insert_with(Vec::new).push(entry);
                    interp.env.pop();
                    continue 'each_item;
                }
                LinqClause::GroupBy(_) => {
                    // TODO: full groupby support. For MVP, treat as a no-op.
                }
            }
        }

        let projected = eval_expr(interp, linq.select)?;
        interp.env.pop();
        result.push(projected);
    }

    // If we collected orderby pairs, sort and use those as the result.
    if let Some(mut pairs) = order_by {
        pairs.sort_by(|(ka, _), (kb, _)| {
            compare_values(ka, kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        // Check if any clause was descending — simplified: use last orderby directive.
        let descending = linq.clauses.iter().any(|c| {
            matches!(c, LinqClause::OrderBy { descending: true, .. })
        });
        if descending { pairs.reverse(); }
        result = pairs.into_iter().map(|(_, v)| v).collect();
    }

    Ok(Value::List(Rc::new(RefCell::new(result))))
}

/// Compare two values for ordering. Returns None if incomparable.
fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x),    Value::Int(y))    => Some(x.cmp(y)),
        (Value::Float(x),  Value::Float(y))  => x.partial_cmp(y),
        (Value::Double(x), Value::Double(y)) => x.partial_cmp(y),
        (Value::Str(x),    Value::Str(y))    => Some(x.cmp(y)),
        (Value::Bool(x),   Value::Bool(y))   => Some(x.cmp(y)),
        _ => None,
    }
    }
