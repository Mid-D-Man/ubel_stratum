
// src/ast/literals.rs
//! Literal value nodes.
//!
//! `Literal<'ast>` is `Copy` — all variants are either primitive values
//! or fat pointers into the arena, so copying is always cheap.


#![allow(dead_code)]




/// One segment of an interpolated string `$"..."`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationPart<'ast> {
    /// A plain text run: `"Hello, "` in `$"Hello, {name}"`.
    Text(&'ast str),
    /// The raw source text of an expression hole: `"name"` in `{name}`.
    /// The parser will re-tokenize this during expression lowering.
    Expr(&'ast str),
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
