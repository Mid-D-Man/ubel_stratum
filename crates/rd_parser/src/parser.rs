// crates/rd_parser/src/parser.rs
//! The `Parser<'ast, 'tok>` struct and its two public entry points.
//!
//! All parse logic lives in the `parse_*` modules via `impl Parser` blocks.
//! This file only defines the struct, constructs it, drives the top-level
//! orchestration, and provides shared helpers that every parse module uses.
//!
//! # Lifetime parameters
//!
//! - `'ast`  — lifetime of the AST arena; every allocated node lives this long.
//! - `'tok`  — lifetime of the token slice; no tokens are copied into the arena,
//!             but string lexemes from identifiers ARE interned via `arena.alloc_str`.

use ubel_stratum::{
    ast::{arena::AstArena, common::TierAnnotation, expressions::Expr, root::Program},
    error_management::{
        error_types::{ParseContext, ParseError},
        ErrorManager,
    },
    lexer::{Span, Token, TokenType},
};

use crate::cursor::Cursor;

// ── Parser struct ─────────────────────────────────────────────────────────────

pub struct Parser<'ast, 'tok> {
    /// Token stream — advanced by every `parse_*` method.
    pub(crate) cursor:   Cursor<'tok>,
    /// Arena that owns all AST node memory for this parse.
    pub(crate) arena:    &'ast AstArena,
    /// Accumulates every diagnostic found during parsing.
    /// Parsing always tries to continue past errors (error-recovery mode).
    pub(crate) errors:   ErrorManager,
    /// The grammar context we are currently in — appears in error messages.
    pub(crate) context:  ParseContext,
    /// Tier of the *enclosing* function/method.
    /// Used by statement parsers to emit early "await in @tier(low)" errors
    /// without waiting for the full semantic analysis pass.
    pub(crate) tier:     TierAnnotation,
}

// ── Construction + entry points ───────────────────────────────────────────────

impl<'ast, 'tok> Parser<'ast, 'tok> {
    /// Build a parser from a pre-lexed token slice.
    ///
    /// `tokens` must end with `TokenType::Eof` (guaranteed by `tokenize()`).
    pub fn new(arena: &'ast AstArena, tokens: &'tok [Token], source: String) -> Self {
        Parser {
            cursor:  Cursor::new(tokens),
            arena,
            errors:  ErrorManager::new(source),
            context: ParseContext::TopLevel,
            tier:    TierAnnotation::High, // default — functions opt down
        }
    }

    /// Parse a full source file, consuming `self`.
    ///
    /// Returns `Ok(program)` even if non-fatal errors were collected — callers
    /// should decide whether to proceed based on `errors.has_errors()`.
    /// Returns `Err(errors)` if errors were fatal (e.g. hit EOF with no progress).
    pub fn parse_program(mut self) -> Result<Program<'ast>, ErrorManager> {
        let program = self.parse_program_inner();
        if self.errors.has_errors() {
            Err(self.errors)
        } else {
            Ok(program)
        }
    }

    /// Parse a single expression from an already-lexed token slice.
    ///
    /// Returns `None` if parsing failed or the token stream was empty.
    pub fn parse_single_expr(mut self) -> Option<&'ast Expr<'ast>> {
        self.parse_expr_entry()
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────
//
// These are used by every `parse_*` module and must be in `parser.rs` (or a
// file imported here) so they can access `self.cursor`, `self.errors`, etc.

impl<'ast, 'tok> Parser<'ast, 'tok> {
    // ── Error emission ────────────────────────────────────────────────────────

    /// Push a parse error onto the error accumulator.
    #[inline]
    pub(crate) fn emit(&mut self, err: ParseError) {
        self.errors.add_parse_error(err);
    }

    /// Emit an "unexpected token" error for the current token, with a list of
    /// human-readable alternatives.
    pub(crate) fn expected(&mut self, what: &[&str]) {
        let tok = self.cursor.peek_token();
        self.emit(crate::error::unexpected(tok, what, self.context.clone()));
    }

    // ── Context management ────────────────────────────────────────────────────

    /// Temporarily set the parse context (for error messages) and return the
    /// old context so the caller can restore it later.
    #[inline]
    pub(crate) fn enter(&mut self, ctx: ParseContext) -> ParseContext {
        std::mem::replace(&mut self.context, ctx)
    }

    /// Restore a previously saved context (see `enter`).
    #[inline]
    pub(crate) fn leave(&mut self, ctx: ParseContext) {
        self.context = ctx;
    }

    // ── Tier tracking ─────────────────────────────────────────────────────────

    /// Set the current function tier and return the old value.
    #[inline]
    pub(crate) fn enter_tier(&mut self, tier: TierAnnotation) -> TierAnnotation {
        std::mem::replace(&mut self.tier, tier)
    }

    /// Restore the tier to a saved value.
    #[inline]
    pub(crate) fn leave_tier(&mut self, tier: TierAnnotation) {
        self.tier = tier;
    }

    // ── Arena helpers ─────────────────────────────────────────────────────────

    /// Intern a string into the AST arena.
    /// Returns `&'ast str` — the string lives as long as the arena.
    #[inline]
    pub(crate) fn intern(&self, s: &str) -> &'ast str {
        self.arena.alloc_str(s)
    }

    /// Intern a `Vec<T: Copy>` into an arena slice.
    #[inline]
    pub(crate) fn alloc_vec<T: Copy>(&self, v: Vec<T>) -> &'ast [T] {
        self.arena.alloc_vec_copy(v)
    }

    /// Allocate a single AST node into the arena.
    #[inline]
    pub(crate) fn alloc<T>(&self, val: T) -> &'ast T {
        self.arena.alloc(val)
    }

    // ── Token helpers ─────────────────────────────────────────────────────────

    /// Consume the current token and return its span.
    /// Useful when we need to record where something started.
    #[inline]
    pub(crate) fn advance_span(&mut self) -> Span {
        self.cursor.advance().span
    }

    /// The span of the current (not-yet-consumed) token.
    #[inline]
    pub(crate) fn span(&self) -> Span {
        self.cursor.current_span()
    }

    /// Peek at the current token type.
    #[inline]
    pub(crate) fn peek(&self) -> &TokenType {
        self.cursor.peek()
    }

    /// Try to eat a `TokenType::Ident` and intern its text.
    /// Returns the interned `&'ast str` or `None` without advancing.
    pub(crate) fn eat_ident(&mut self) -> Option<(&'ast str, Span)> {
        if let TokenType::Ident(name) = self.cursor.peek() {
            let span = self.cursor.current_span();
            // Clone the name before advancing (borrow-checker: can't hold
            // a borrow on cursor.peek() while calling cursor.advance()).
            let name = name.clone();
            self.cursor.advance();
            Some((self.intern(&name), span))
        } else {
            None
        }
    }

    /// Eat an identifier or emit an error and return `None`.
    pub(crate) fn expect_ident(&mut self) -> Option<(&'ast str, Span)> {
        if let Some(pair) = self.eat_ident() {
            Some(pair)
        } else {
            self.expected(&["identifier"]);
            None
        }
    }

    // ── Error recovery ────────────────────────────────────────────────────────

    /// Declaration-level sync set — used after a hard parse error to find the
    /// next safe starting point for a top-level item.
    pub(crate) const DECL_SYNC: &'static [TokenType] = &[
        TokenType::Fn,
        TokenType::Struct,
        TokenType::Enum,
        TokenType::Trait,
        TokenType::Impl,
        TokenType::Extend,
        TokenType::Const,
        TokenType::TypeKw,
        TokenType::Pub,
        TokenType::At,
        TokenType::Eof,
    ];

    /// Statement-level sync set.
    pub(crate) const STMT_SYNC: &'static [TokenType] = &[
        TokenType::Semicolon,
        TokenType::RightBrace,
        TokenType::Fn,
        TokenType::Struct,
        TokenType::Eof,
    ];

    /// Skip to the nearest declaration boundary and emit an error.
    /// Used when we encounter something completely unexpected at item level.
    pub(crate) fn recover_to_decl(&mut self) {
        self.cursor.skip_until_any(Self::DECL_SYNC);
    }

    /// Skip to the nearest statement boundary.
    pub(crate) fn recover_to_stmt(&mut self) {
        self.cursor.skip_until_any(Self::STMT_SYNC);
        // Eat the semicolon if that's what we synced to.
        self.cursor.eat(&TokenType::Semicolon);
    }
}

// ── Stub entry points (filled in by parse_program.rs / parse_expr.rs) ────────

impl<'ast, 'tok> Parser<'ast, 'tok> {
    /// Called by `parse_program()` — implemented in `parse_program.rs`.
    #[inline]
    pub(crate) fn parse_program_inner(&mut self) -> Program<'ast> {
        crate::parse_program::parse_program(self)
    }

    /// Called by `parse_single_expr()` — implemented in `parse_expr.rs`.
    #[inline]
    pub(crate) fn parse_expr_entry(&mut self) -> Option<&'ast Expr<'ast>> {
        crate::parse_expr::parse_expr(self)
    }
          }
