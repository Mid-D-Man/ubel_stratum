// crates/rd_parser/src/lib.rs
//! Recursive-descent parser for Ubel Stratum.
//!
//! Produces the same [`ubel_stratum::ast`] types as the LALRPOP parser in
//! `crates/parser`, but requires no code-generation step and compiles as fast
//! as any ordinary Rust crate.
//!
//! # Public API
//!
//! ```rust,ignore
//! use ubel_stratum_rd::{parse, parse_expr_str};
//! use ubel_stratum::ast::arena::AstArena;
//! use ubel_stratum::lexer::tokenize;
//!
//! let src    = r#"fn main() { println("hi") }"#;
//! let arena  = AstArena::new();
//! let tokens = tokenize(src).unwrap();
//! let program = parse(&arena, &tokens, src.to_string()).unwrap();
//! ```
//!
//! # Crate layout
//!
//! | Module / File        | Contents                                           |
//! |----------------------|----------------------------------------------------|
//! | `cursor`             | `Cursor<'tok>` — zero-copy token stream view       |
//! | `error`              | Error constructor helpers → `core::ParseError`     |
//! | `parser`             | `Parser<'ast,'tok>` struct + shared hot helpers    |
//! | `parsers/`           | `impl Parser` blocks, one file per grammar category|

pub mod cursor;
pub mod error;

pub(crate) mod parser;

/// All grammar-category parse modules live under this sub-module.
/// Each file adds `impl Parser<'ast, 'tok> { ... }` for its category.
pub(crate) mod parsers;

pub use parser::Parser;

use ubel_stratum::{
    ast::{arena::AstArena, expressions::Expr, root::Program},
    error_management::ErrorManager,
    lexer::Token,
};

// ── Public entry points ───────────────────────────────────────────────────────

/// Parse a complete `.strat` source file into a `Program<'ast>`.
///
/// `tokens` must end with `TokenType::Eof`; `ubel_stratum::lexer::tokenize`
/// guarantees this.
///
/// Returns `Ok(program)` on success or `Err(diagnostics)` on failure.
/// The parser is error-recovering — it always attempts to continue past the
/// first error, so `Err` may contain multiple diagnostics.
pub fn parse<'ast>(
    arena:  &'ast AstArena,
    tokens: &[Token],
    source: String,
) -> Result<Program<'ast>, ErrorManager> {
    Parser::new(arena, tokens, source).parse_program()
}

/// Parse a single expression from raw source text.
///
/// Lexes `source` internally. Used by the interpreter to evaluate
/// interpolated-string segments without a full file parse.
/// Returns `None` if the source is empty, invalid, or produces only errors.
pub fn parse_expr_str<'ast>(
    arena:  &'ast AstArena,
    source: &str,
) -> Option<&'ast Expr<'ast>> {
    let tokens = ubel_stratum::lexer::tokenize(source).ok()?;
    Parser::new(arena, &tokens, source.to_string()).parse_single_expr()
}
