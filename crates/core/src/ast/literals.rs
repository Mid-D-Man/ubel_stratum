// src/ast/literals.rs
//! Literal value nodes.
//!
//! `Literal<'ast>` is `Copy` — all variants are either primitive values
//! or fat pointers into the arena, so copying is always cheap.


#![allow(dead_code)]




use crate::ast::expressions::Expr;

/// A parsed `{expr:spec}` format specifier — the part after the `:` in an
/// interpolation hole. First slice, deliberately not full parity with
/// Rust's `format!` mini-language (see `docs/PRINT_FORMAT_RULES.md`):
/// width, precision, and alignment are covered; fill character (beyond
/// the implicit space), sign forcing (`+`), the alternate form (`#`),
/// zero-padding, and numeric bases (`x`/`o`/`b`) are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatSpec {
    pub align:     Option<Align>,
    pub width:     Option<u32>,
    pub precision: Option<u32>,
    /// Trailing `?` — selects `Value::debug_string()` over `Display` as
    /// the base formatter (see `interpreter::eval::expr::apply_format_spec`
    /// and `Value::debug_string` doc comments for exactly what differs:
    /// quoted/escaped `Str`/`Char`, and live `Rc::strong_count` on
    /// `Shared`/`SyncShared`).
    pub debug:     bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// One segment of an interpolated string `$"...{...}..."`.
#[derive(Debug, Clone, Copy)]
pub enum InterpolationPart<'ast> {
    /// A plain text run: `"Hello, "` in `$"Hello, {name}"`.
    Text(&'ast str),
    /// A fully parsed expression hole: `name` in `{name}`, with an
    /// optional format spec: `{name:>10}`. Parsed by rd_parser at the
    /// same time as everything else -- no re-parsing happens later, in
    /// sema or the interpreter.
    Expr {
        expr: &'ast Expr<'ast>,
        spec: Option<FormatSpec>,
    },
}


/// Every form of literal that Ubel Stratum supports.
#[derive(Debug, Clone, Copy)]
pub enum Literal<'ast> {
    /// Integer literal: `42`, `0xFF`, `0b1010`, `1_000_000`
    Int(i64),
    /// 32-bit float: `3.14f`
    Float(f32),
    /// 64-bit float (default when no suffix): `3.14`
    Double(f64),
    /// Plain string: `"hello\nworld"`
    Str(&'ast str),
    /// Interpolated string: `$"Hello, {name}!"`
    InterpolatedStr(&'ast [InterpolationPart<'ast>]),
    /// Verbatim string (no escapes, `""` to embed a quote): `@"C:\path"`
    VerbatimStr(&'ast str),
    /// Interpolated verbatim: `$@"C:\Users\{name}"`
    InterpolatedVerbatimStr(&'ast [InterpolationPart<'ast>]),
    /// Character: `'a'`, `'\n'`
    Char(char),
    /// Boolean: `true` / `false`
    Bool(bool),
    /// The `null` literal
    Null,
}
