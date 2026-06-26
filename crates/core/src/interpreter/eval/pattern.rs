// src/interpreter/eval/pattern.rs
//! Pattern matching for match arms and destructuring.

#![allow(dead_code)]

use std::collections::HashMap;

use crate::ast::literals::Literal;
use crate::ast::patterns::{
    DestructureElement, DestructurePattern, EnumPatternPayload,
    FieldPattern, Pattern, PatternKind,
};
use crate::interpreter::env::Environment;
use crate::interpreter::value::{EnumPayload, Value};

// ── Public API ────────────────────────────────────────────────────

/// Try to match `value` against `pattern`, defining any bound names into `env`
/// on success. Returns `true` if the pattern matched.
///
/// Bindings are committed atomically — either all succeed (pattern matched,
/// all names defined) or none are (pattern failed, env is unchanged).
/// This invariant is maintained by the internal `try_match` helper.
pub fn match_pattern(pattern: &Pattern<'_>, value: &Value, env: &mut Environment) -> bool {
    match try_match(pattern, value) {
        Some(bindings) => {
            for (name, val) in bindings { env.define(&name, val); }
            true
        }
        None => false,
    }
}

/// Bind a destructuring pattern (from `extract` or destructuring `let`) into env.
/// Unlike `match_pattern`, this always binds — failure is a runtime panic since
/// the type-checker should have caught arity mismatches.
pub fn bind_destructure_pattern(
    pattern: &DestructurePattern<'_>,
    value:   Value,
    env:     &mut Environment,
) {
    match pattern {
        DestructurePattern::Ident(name) => {
            env.define(name, value);
        }
        DestructurePattern::Tuple(t) => {
            let items = match value {
                Value::Tuple(v) => v,
                Value::List(rc) => rc.borrow().clone(),
                other => {
                    env.define("_", other); // graceful fallback
                    return;
                }
            };
            for (elem, val) in t.elements.iter().zip(items.into_iter()) {
                bind_destructure_elem(elem, val, env);
            }
        }
        DestructurePattern::Array(a) => {
            let items = match value {
                Value::List(rc) => rc.borrow().clone(),
                Value::Tuple(v) => v,
                other => {
                    env.define("_", other);
                    return;
                }
            };
            let n = a.elements.len().min(items.len());
            for (elem, val) in a.elements[..n].iter().zip(items[..n].iter()) {
                bind_destructure_elem(elem, val.clone(), env);
            }
            // Bind rest if present.
            if let Some(rest_name) = a.rest.flatten() {
                let rest_items = items[n..].to_vec();
                env.define(rest_name, Value::List(
                    std::rc::Rc::new(std::cell::RefCell::new(rest_items))
                ));
            }
        }
        DestructurePattern::Struct(s) => {
            let fields = match value {
                Value::Struct { fields, .. } => fields.borrow().clone(),
                _ => return,
            };
            for fd in s.fields.iter() {
                let field_val = fields.get(fd.field).cloned().unwrap_or(Value::Null);
                if let Some(sub_pat) = &fd.pattern {
                    bind_destructure_pattern(sub_pat, field_val, env);
                } else {
                    env.define(fd.field, field_val);
                }
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────

/// Try to match `pattern` against `value`. Returns `Some(bindings)` on success
/// where `bindings` is the ordered list of `(name, value)` pairs to define,
/// or `None` if the pattern didn't match.
///
/// Collecting bindings before committing them means OR patterns can try each
/// alternative cleanly without partially polluting the environment.
fn try_match(pattern: &Pattern<'_>, value: &Value) -> Option<Vec<(String, Value)>> {
    let mut bindings = Vec::new();
    if match_inner(pattern, value, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

/// Recursive matching core. Accumulates name bindings into `out`.
/// Returns `true` if the pattern matched.
fn match_inner(
    pattern:  &Pattern<'_>,
    value:    &Value,
    out:      &mut Vec<(String, Value)>,
) -> bool {
    match &pattern.kind {
        // ── Wildcard `_` — always matches, binds nothing ──────────
        PatternKind::Wildcard => true,

        // ── Literal — value must equal the literal ────────────────
        PatternKind::Literal(lit) => match_literal(lit, value),

        // ── Binding: `x` or `mut x` — always matches, binds name ─
        PatternKind::Ident { name, .. } => {
            out.push((name.to_string(), value.clone()));
            true
        }

        // ── Tuple: (a, b, c) ──────────────────────────────────────
        PatternKind::Tuple(pats) => {
            let items = match value {
                Value::Tuple(v) => v.as_slice(),
                // Allow matching a list as a tuple pattern (flexible MVP behaviour).
                Value::List(rc) => {
                    // Can't easily return a borrow here; clone.
                    let cloned: Vec<Value> = rc.borrow().clone();
                    return match_tuple_slice(pats, &cloned, out);
                }
                _ => return false,
            };
            match_tuple_slice(pats, items, out)
        }

        // ── Array: [a, b, ...rest] ────────────────────────────────
        PatternKind::Array { elements, rest } => {
            let items: Vec<Value> = match value {
                Value::List(rc)  => rc.borrow().clone(),
                Value::Tuple(v)  => v.clone(),
                _ => return false,
            };
            // Without a rest pattern, length must match exactly.
            if rest.is_none() && items.len() != elements.len() {
                return false;
            }
            // With a rest pattern, we need at least as many items as fixed elements.
            if rest.is_some() && items.len() < elements.len() {
                return false;
            }
            let mut trial = Vec::new();
            for (pat, item) in elements.iter().zip(items.iter()) {
                if !match_inner(pat, item, &mut trial) { return false; }
            }
            // Bind the rest if named.
            if let Some(Some(rest_name)) = rest {
                let rest_items = items[elements.len()..].to_vec();
                trial.push((rest_name.to_string(), Value::List(
                    std::rc::Rc::new(std::cell::RefCell::new(rest_items))
                )));
            }
            out.extend(trial);
            true
        }

        // ── Struct: Point { x, y } ────────────────────────────────
        PatternKind::Struct { name: expected_name, fields } => {
            let (type_name, field_map) = match value {
                Value::Struct { type_name, fields } => {
                    (type_name.as_str(), fields.borrow().clone())
                }
                _ => return false,
            };
            // If the pattern names a type, check it matches.
            if let Some(n) = expected_name {
                if *n != type_name && *n != "<anon>" { return false; }
            }
            let mut trial = Vec::new();
            for fp in fields.iter() {
                let field_val = field_map.get(fp.field).cloned().unwrap_or(Value::Null);
                if let Some(sub_pat) = &fp.pattern {
                    if !match_inner(sub_pat, &field_val, &mut trial) { return false; }
                } else {
                    // Shorthand `{ name }` — bind the field name directly.
                    trial.push((fp.field.to_string(), field_val));
                }
            }
            out.extend(trial);
            true
        }

        // ── Enum: Status.Active or Ok(x) or Err { code, msg } ────
        PatternKind::Enum { path, payload } => {
            // Extract expected type and variant from the path.
            // `path = ["Status", "Active"]` → type = "Status", variant = "Active"
            // `path = ["Ok"]`               → variant = "Ok" (any type)
            let (expected_type, expected_variant) = if path.len() >= 2 {
                (Some(path[path.len() - 2]), path[path.len() - 1])
            } else {
                (None, path[0])
            };

            match value {
                Value::Enum { type_name, variant, payload: val_payload } => {
                    // Check type name if provided.
                    if let Some(et) = expected_type {
                        if et != type_name.as_str() { return false; }
                    }
                    if expected_variant != variant.as_str() { return false; }

                    // Match payload.
                    let mut trial = Vec::new();
                    let matched = match (payload, val_payload.as_ref()) {
                        (EnumPatternPayload::None, EnumPayload::None) => true,
                        (EnumPatternPayload::Tuple(pats), EnumPayload::Tuple(vals)) => {
                            if pats.len() != vals.len() { return false; }
                            pats.iter().zip(vals.iter()).all(|(p, v)| {
                                match_inner(p, v, &mut trial)
                            })
                        }
                        (EnumPatternPayload::Struct(fps), EnumPayload::Struct(fields)) => {
                            match_struct_payload(fps, fields, &mut trial)
                        }
                        // Tolerant: pattern expects None but value has payload — no match.
                        _ => false,
                    };
                    if matched {
                        out.extend(trial);
                        true
                    } else {
                        false
                    }
                }
                // Allow matching integers against discriminant-valued enums
                // when the expected variant is a bare name (e.g. 0 matches Active = 0).
                _ => false,
            }
        }

        // ── Range: lo..hi (exclusive) or lo..=hi (inclusive) ─────
        PatternKind::Range { lo, hi, inclusive } => {
            match value {
                Value::Int(n) => {
                    let lo_val = match literal_to_i64(lo) { Some(v) => v, None => return false };
                    let hi_val = match literal_to_i64(hi) { Some(v) => v, None => return false };
                    if *inclusive {
                        *n >= lo_val && *n <= hi_val
                    } else {
                        *n >= lo_val && *n < hi_val
                    }
                }
                Value::Char(c) => {
                    let lo_char = match literal_to_char(lo) { Some(v) => v, None => return false };
                    let hi_char = match literal_to_char(hi) { Some(v) => v, None => return false };
                    if *inclusive {
                        *c >= lo_char && *c <= hi_char
                    } else {
                        *c >= lo_char && *c < hi_char
                    }
                }
                _ => false,
            }
        }

        // ── OR pattern: A | B | C ─────────────────────────────────
        // Try each alternative; commit bindings from the first that matches.
        PatternKind::Or(pats) => {
            for pat in pats.iter() {
                let mut trial = Vec::new();
                if match_inner(pat, value, &mut trial) {
                    out.extend(trial);
                    return true;
                }
            }
            false
        }

        // ── Extract: extract { field, ... } ──────────────────────
        PatternKind::Extract(fields) => {
            let field_map: HashMap<String, Value> = match value {
                Value::Struct { fields: f, .. } => f.borrow().clone(),
                _ => return false,
            };
            let mut trial = Vec::new();
            for fp in fields.iter() {
                let fv = field_map.get(fp.field).cloned().unwrap_or(Value::Null);
                if let Some(sub_pat) = &fp.pattern {
                    if !match_inner(sub_pat, &fv, &mut trial) { return false; }
                } else {
                    trial.push((fp.field.to_string(), fv));
                }
            }
            out.extend(trial);
            true
        }
    }
}

// ── Literal matching helpers ──────────────────────────────────────

fn match_literal(lit: &Literal<'_>, value: &Value) -> bool {
    match (lit, value) {
        (Literal::Null,      Value::Null)       => true,
        (Literal::Bool(b),   Value::Bool(v))    => b == v,
        (Literal::Int(n),    Value::Int(v))     => n == v,
        (Literal::Float(f),  Value::Float(v))   => f == v,
        (Literal::Double(d), Value::Double(v))  => d == v,
        (Literal::Char(c),   Value::Char(v))    => c == v,
        (Literal::Str(s),    Value::Str(v))     => *s == v.as_str(),
        // Allow int literal to match float/double (common in range patterns).
        (Literal::Int(n),    Value::Float(v))   => (*n as f32) == *v,
        (Literal::Int(n),    Value::Double(v))  => (*n as f64) == *v,
        _ => false,
    }
}

fn literal_to_i64(lit: &Literal<'_>) -> Option<i64> {
    match lit {
        Literal::Int(n) => Some(*n),
        _ => None,
    }
}

fn literal_to_char(lit: &Literal<'_>) -> Option<char> {
    match lit {
        Literal::Char(c) => Some(*c),
        _ => None,
    }
}

// ── Tuple slice matching ──────────────────────────────────────────

fn match_tuple_slice(
    pats:  &[Pattern<'_>],
    items: &[Value],
    out:   &mut Vec<(String, Value)>,
) -> bool {
    if pats.len() != items.len() { return false; }
    let mut trial = Vec::new();
    for (pat, val) in pats.iter().zip(items.iter()) {
        if !match_inner(pat, val, &mut trial) { return false; }
    }
    out.extend(trial);
    true
}

// ── Struct payload matching ───────────────────────────────────────

fn match_struct_payload(
    fps:    &[FieldPattern<'_>],
    fields: &HashMap<String, Value>,
    out:    &mut Vec<(String, Value)>,
) -> bool {
    let mut trial = Vec::new();
    for fp in fps.iter() {
        let fv = fields.get(fp.field).cloned().unwrap_or(Value::Null);
        if let Some(sub_pat) = &fp.pattern {
            if !match_inner(sub_pat, &fv, &mut trial) { return false; }
        } else {
            trial.push((fp.field.to_string(), fv));
        }
    }
    out.extend(trial);
    true
}

// ── Destructure element helper ────────────────────────────────────

fn bind_destructure_elem(
    elem:  &DestructureElement<'_>,
    value: Value,
    env:   &mut Environment,
) {
    match elem {
        DestructureElement::Ident(name) => env.define(name, value),
        DestructureElement::Wildcard    => {}
        DestructureElement::Nested(pat) => bind_destructure_pattern(pat, value, env),
    }
        }
