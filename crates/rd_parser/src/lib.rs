// crates/rd_parser/src/lib.rs
//! Recursive-descent parser for Ubel Stratum.
//!
//! Produces the same `ubel_stratum::ast` types as the LALRPOP parser in
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
//! let arena  = AstArena::new();
//! let tokens = tokenize(source).unwrap();
//! let program = parse(&arena, &tokens, source.to_string()).unwrap();
//! ```
//!
//! # Module layout
//!
//! | Module           | Contents                                              |
//! |------------------|-------------------------------------------------------|
//! | `cursor`         | `Cursor` — peek / advance / expect over a token slice |
//! | `error`          | Error constructor helpers → `ParseError`              |
//! | `parser`         | `Parser<'ast,'tok>` struct + entry points             |
//! | `parse_attr`     | `@tier`, `@cfg`, `@core`, `@tag`, custom attributes   |
//! | `parse_type`     | Type expression parsing                               |
//! | `parse_pattern`  | Destructure pattern parsing                           |
//! | `parse_expr`     | Expression parsing (Pratt precedence climbing)        |
//! | `parse_stmt`     | Statement + block parsing                             |
//! | `parse_decl`     | fn / struct / enum / trait / impl / extend / const    |
//! | `parse_program`  | Package declaration, imports, top-level item list     |

pub mod cursor;
pub mod error;

mod parser;

// Each module adds `impl Parser { ... }` methods for its grammar category.
// Declared `pub(crate)` — not exposed publicly; callers go through `parse()`.
pub(crate) mod parse_attr;
pub(crate) mod parse_type;
pub(crate) mod parse_pattern;
pub(crate) mod parse_expr;
pub(crate) mod parse_stmt;
pub(crate) mod parse_decl;
pub(crate) mod parse_program;

pub use parser::Parser;

use ubel_stratum::{
    ast::{arena::AstArena, expressions::Expr, root::Program},
    error_management::ErrorManager,
    lexer::Token,
};

// ── Public entry points ───────────────────────────────────────────────────────

/// Parse a full `.strat` source file into a `Program<'ast>`.
///
/// `tokens` must end with a `TokenType::Eof` token; `ubel_stratum::lexer::tokenize`
/// guarantees this.
///
/// Returns `Ok(program)` if parsing succeeded without errors, or
/// `Err(errors)` containing every diagnostic collected during parsing.
pub fn parse<'ast>(
    arena:  &'ast AstArena,
    tokens: &[Token],
    source: String,
) -> Result<Program<'ast>, ErrorManager> {
    Parser::new(arena, tokens, source).parse_program()
}

/// Parse a single expression from a raw source string.
///
/// Used by the interpreter to evaluate interpolated-string segments.
/// Returns `None` if the source is empty, invalid, or contains only errors.
pub fn parse_expr_str<'ast>(
    arena:  &'ast AstArena,
    source: &str,
) -> Option<&'ast Expr<'ast>> {
    let tokens = ubel_stratum::lexer::tokenize(source).ok()?;
    Parser::new(arena, &tokens, source.to_string()).parse_single_expr()
}
