// crates/core/src/builtins/validate.rs
//! Sema-time validation for calls to builtin functions — the "compile_time_validator"
//! piece that was missing before this module existed. Catches wrong argument
//! counts/kinds against a `BuiltinSignature`, the same way user function
//! calls get checked against their declared signature.
//!
//! NOTE: written and unit-tested here, but not yet called from
//! `sema::type_infer`'s `ExprKind::Call` handling — that requires converting
//! the real `SemaType` (from `sema::type_table`) into the coarser
//! `ParamType` this module works with, at the call site. Left as a
//! deliberate next step rather than rushed alongside the module split.

use crate::builtins::{BuiltinSignature, ParamType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinCallError {
    ArityMismatch { name: &'static str, expected: usize, at_least: bool, found: usize },
    ArgTypeMismatch { name: &'static str, arg_index: usize, expected: ParamType, found: ParamType },
}

impl std::fmt::Display for BuiltinCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinCallError::ArityMismatch { name, expected, at_least, found } => write!(
                f, "{}() expects {}{} argument{}, got {}",
                name, if *at_least { "at least " } else { "" }, expected,
                if *expected == 1 { "" } else { "s" }, found
            ),
            BuiltinCallError::ArgTypeMismatch { name, arg_index, expected, found } => write!(
                f, "{}() argument {} expected {:?}, got {:?}",
                name, arg_index + 1, expected, found
            ),
        }
    }
}

/// Check a call's argument count and (coarse) argument types against a
/// builtin's declared signature.
pub fn validate_call(
    sig:       &BuiltinSignature,
    arg_types: &[ParamType],
) -> Result<(), BuiltinCallError> {
    let found = arg_types.len();
    let expected = sig.params.len();

    if sig.variadic {
        if found < expected {
            return Err(BuiltinCallError::ArityMismatch {
                name: sig.name, expected, at_least: true, found,
            });
        }
    } else if found != expected {
        return Err(BuiltinCallError::ArityMismatch {
            name: sig.name, expected, at_least: false, found,
        });
    }

    for (i, (param, arg)) in sig.params.iter().zip(arg_types.iter()).enumerate() {
        if !param_accepts(*param, *arg) {
            return Err(BuiltinCallError::ArgTypeMismatch {
                name: sig.name, arg_index: i, expected: *param, found: *arg,
            });
        }
    }
    Ok(())
}

fn param_accepts(expected: ParamType, actual: ParamType) -> bool {
    match expected {
        ParamType::Any     => true,
        ParamType::Numeric => matches!(actual, ParamType::Int | ParamType::Float | ParamType::Double),
        _                  => expected == actual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::Lowering;

    const SQRT_SIG: BuiltinSignature = BuiltinSignature {
        name: "sqrt", params: &[ParamType::Numeric], variadic: false,
        return_type: ParamType::Numeric, lowering: Lowering::Intrinsic("llvm.sqrt"),
        run: crate::builtins::global::math::sqrt,
    };

    #[test]
    fn sqrt_accepts_one_numeric_arg() {
        assert!(validate_call(&SQRT_SIG, &[ParamType::Int]).is_ok());
        assert!(validate_call(&SQRT_SIG, &[ParamType::Double]).is_ok());
    }

    #[test]
    fn sqrt_rejects_wrong_arity() {
        assert!(validate_call(&SQRT_SIG, &[]).is_err());
        assert!(validate_call(&SQRT_SIG, &[ParamType::Int, ParamType::Int]).is_err());
    }

    #[test]
    fn sqrt_rejects_non_numeric_arg() {
        assert!(validate_call(&SQRT_SIG, &[ParamType::Str]).is_err());
    }

    #[test]
    fn variadic_println_accepts_any_count() {
        const PRINTLN_SIG: BuiltinSignature = BuiltinSignature {
            name: "println", params: &[], variadic: true,
            return_type: ParamType::Any, lowering: Lowering::RuntimeCall("ubel_rt_println"),
            run: crate::builtins::global::io::println,
        };
        assert!(validate_call(&PRINTLN_SIG, &[]).is_ok());
        assert!(validate_call(&PRINTLN_SIG, &[ParamType::Str, ParamType::Int]).is_ok());
    }
}
