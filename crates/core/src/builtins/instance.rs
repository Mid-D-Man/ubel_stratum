// crates/core/src/builtins/instance.rs
//! Instance methods — `receiver.method(args)` — grouped by the runtime
//! `Value` kind they apply to. Actual dispatch still lives in
//! `interpreter::eval::expr::eval_method_call` (it needs the receiver's
//! `Rc`/inner data, which doesn't fit the same `fn(&[Value]) -> EvalResult`
//! shape as global builtins) — these modules are the single implementation
//! that dispatch calls into, and the name lists below are what sema
//! consults to know a method name is valid for a given receiver kind.

pub mod list_methods;
pub mod string_methods;
pub mod dict_methods;
pub mod tuple_methods;
pub mod queue_methods;
pub mod stack_methods;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    List,
    Str,
    Dict,
    Tuple,
    Queue,
    Stack,
}

/// Returns the valid method names for a given receiver kind — used by sema
/// (once wired in) to flag `myList.frobnicate()` as an error before runtime,
/// the same way undefined global names are caught today.
pub fn method_names(kind: ReceiverKind) -> &'static [&'static str] {
    match kind {
        ReceiverKind::List  => list_methods::METHOD_NAMES,
        ReceiverKind::Str   => string_methods::METHOD_NAMES,
        ReceiverKind::Dict  => dict_methods::METHOD_NAMES,
        ReceiverKind::Tuple => tuple_methods::METHOD_NAMES,
        ReceiverKind::Queue => queue_methods::METHOD_NAMES,
        ReceiverKind::Stack => stack_methods::METHOD_NAMES,
    }
}
