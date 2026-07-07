// crates/core/src/builtins/global.rs
//! Global (bare-call) builtins: `println(x)`, `sqrt(x)`, etc.
//!
//! `GLOBAL_BUILTINS` is the single source of truth, consumed by:
//!   - `sema::name_resolution::Resolver::declare_builtins` (name resolution)
//!   - `builtins::validate` (arity/type checking, once wired into type_infer)
//!   - `interpreter::eval::Interpreter::register_builtins` (runtime dispatch)
//!   - (future) LLVM codegen's Call-lowering decision, via `lowering`

pub mod io;
pub mod diagnostics;
pub mod math;
pub mod conversions;

use crate::builtins::{BuiltinSignature, Lowering, ParamType};

pub static GLOBAL_BUILTINS: &[BuiltinSignature] = &[
    // ── I/O — needs real runtime support, no LLVM intrinsic exists ────────
    BuiltinSignature {
        name: "println", params: &[], variadic: true, return_type: ParamType::Any,
        lowering: Lowering::RuntimeCall("ubel_rt_println"), run: io::println,
    },
    BuiltinSignature {
        name: "print", params: &[], variadic: true, return_type: ParamType::Any,
        lowering: Lowering::RuntimeCall("ubel_rt_print"), run: io::print,
    },
    BuiltinSignature {
        name: "log", params: &[], variadic: true, return_type: ParamType::Any,
        lowering: Lowering::RuntimeCall("ubel_rt_log"), run: io::log,
    },

    // ── Diagnostics ────────────────────────────────────────────────────────
    BuiltinSignature {
        name: "assert", params: &[ParamType::Bool], variadic: true, return_type: ParamType::Any,
        lowering: Lowering::RuntimeCall("ubel_rt_assert"), run: diagnostics::assert,
    },
    BuiltinSignature {
        name: "panic", params: &[], variadic: true, return_type: ParamType::Any,
        lowering: Lowering::RuntimeCall("ubel_rt_panic"), run: diagnostics::panic,
    },
    BuiltinSignature {
        name: "typeof", params: &[ParamType::Any], variadic: false, return_type: ParamType::Str,
        // Statically known once sema tracks the arg's type — fold to a
        // string literal at compile time instead of a runtime call.
        lowering: Lowering::ConstFoldable, run: diagnostics::type_of,
    },

    // ── Math — these map to native LLVM intrinsics, not a linked call ──────
    BuiltinSignature {
        name: "sqrt", params: &[ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.sqrt"), run: math::sqrt,
    },
    BuiltinSignature {
        name: "abs", params: &[ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.fabs"), run: math::abs,
    },
    BuiltinSignature {
        name: "min", params: &[ParamType::Numeric, ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.minnum"), run: math::min,
    },
    BuiltinSignature {
        name: "max", params: &[ParamType::Numeric, ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.maxnum"), run: math::max,
    },
    BuiltinSignature {
        name: "floor", params: &[ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.floor"), run: math::floor,
    },
    BuiltinSignature {
        name: "ceil", params: &[ParamType::Numeric], variadic: false, return_type: ParamType::Numeric,
        lowering: Lowering::Intrinsic("llvm.ceil"), run: math::ceil,
    },
    BuiltinSignature {
        name: "range", params: &[ParamType::Int, ParamType::Int], variadic: false, return_type: ParamType::List,
        lowering: Lowering::RuntimeCall("ubel_rt_range"), run: math::range,
    },

    // ── Conversions — formatting/parsing needs runtime support ─────────────
    BuiltinSignature {
        name: "to_string", params: &[ParamType::Any], variadic: false, return_type: ParamType::Str,
        lowering: Lowering::RuntimeCall("ubel_rt_to_string"), run: conversions::to_string,
    },
    BuiltinSignature {
        name: "to_int", params: &[ParamType::Any], variadic: false, return_type: ParamType::Int,
        lowering: Lowering::RuntimeCall("ubel_rt_to_int"), run: conversions::to_int,
    },
    BuiltinSignature {
        name: "to_float", params: &[ParamType::Any], variadic: false, return_type: ParamType::Float,
        lowering: Lowering::RuntimeCall("ubel_rt_to_float"), run: conversions::to_float,
    },
    BuiltinSignature {
        name: "to_double", params: &[ParamType::Any], variadic: false, return_type: ParamType::Double,
        lowering: Lowering::RuntimeCall("ubel_rt_to_double"), run: conversions::to_double,
    },
];
