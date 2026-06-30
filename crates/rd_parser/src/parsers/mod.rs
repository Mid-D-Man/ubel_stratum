// crates/rd_parser/src/parsers/mod.rs
//! Grammar-category parse modules.
//!
//! Each sub-module adds `impl Parser<'ast, 'tok>` methods for one part of the
//! grammar. They share the `Parser` struct from `crate::parser` — lifetimes,
//! arena, error accumulator, memo cache, and all hot helpers.
//!
//! Parsing order during a full file parse:
//!   parse_program → parse_decl → parse_stmt → parse_expr
//!                                           ↘ parse_type
//!                                           ↘ parse_pattern
//!   (attributes are parsed by parse_attr before every declaration)

pub(crate) mod parse_attr;
pub(crate) mod parse_type;
pub(crate) mod parse_pattern;
pub(crate) mod parse_expr;
pub(crate) mod parse_stmt;
pub(crate) mod parse_decl;
pub(crate) mod parse_program;
