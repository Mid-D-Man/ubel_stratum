// crates/rd_parser/src/parser.rs

use rustc_hash::FxHashMap;

use ubel_stratum::{
    ast::{
        arena::{AstArena, BumpVec},
        common::TierAnnotation,
        expressions::Expr,
        root::Program,
    },
    error_management::{
        error_types::{ParseContext, ParseError},
        ErrorManager,
    },
    lexer::{Span, Token, TokenType},
};

use crate::cursor::Cursor;

// ── Memoisation ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MemoRule {
    GenericArgs   = 0,
    TypeExpr      = 1,
    ClosureParams = 2,
}

pub(crate) enum MemoEntry {
    Hit { end_pos: usize },
    Miss,
}

#[inline(always)]
fn memo_key(pos: usize, rule: MemoRule) -> u64 {
    ((pos as u64) << 8) | (rule as u64)
}

// ── Parser struct ─────────────────────────────────────────────────────────────

pub struct Parser<'ast, 'tok> {
    pub(crate) cursor:  Cursor<'tok>,
    pub(crate) arena:   &'ast AstArena,
    pub(crate) errors:  ErrorManager,
    pub(crate) context: ParseContext,
    pub(crate) tier:    TierAnnotation,
    pub(crate) memo:    FxHashMap<u64, MemoEntry>,
}

impl<'ast, 'tok> Parser<'ast, 'tok> {
    pub fn new(arena: &'ast AstArena, tokens: &'tok [Token], source: String) -> Self {
        Parser {
            cursor:  Cursor::new(tokens),
            arena,
            errors:  ErrorManager::new(source),
            context: ParseContext::TopLevel,
            tier:    TierAnnotation::High,
            memo:    FxHashMap::default(),
        }
    }

    pub fn parse_program(mut self) -> Result<Program<'ast>, ErrorManager> {
        let prog = crate::parsers::parse_program::parse_program(&mut self);
        if self.errors.has_errors() { Err(self.errors) } else { Ok(prog) }
    }

    pub fn parse_single_expr(mut self) -> Option<&'ast Expr<'ast>> {
        crate::parsers::parse_expr::parse_expr_entry(&mut self)
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

impl<'ast, 'tok> Parser<'ast, 'tok> {

    // ── Error ─────────────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn emit(&mut self, err: ParseError) {
        self.errors.add_parse_error(err);
    }

    #[inline(always)]
    pub(crate) fn expected(&mut self, what: &[&str]) {
        let tok = self.cursor.peek_token();
        self.emit(crate::error::unexpected(tok, what, self.context.clone()));
    }

    // ── Context ───────────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn enter(&mut self, ctx: ParseContext) -> ParseContext {
        std::mem::replace(&mut self.context, ctx)
    }

    #[inline(always)]
    pub(crate) fn leave(&mut self, ctx: ParseContext) {
        self.context = ctx;
    }

    // ── Tier ──────────────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn enter_tier(&mut self, tier: TierAnnotation) -> TierAnnotation {
        std::mem::replace(&mut self.tier, tier)
    }

    #[inline(always)]
    pub(crate) fn leave_tier(&mut self, tier: TierAnnotation) {
        self.tier = tier;
    }

    // ── Arena ─────────────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn intern(&self, s: &str) -> &'ast str {
        self.arena.alloc_str(s)
    }

    #[inline(always)]
    pub(crate) fn alloc<T>(&self, val: T) -> &'ast T {
        self.arena.alloc(val)
    }

    #[inline(always)]
    pub(crate) fn alloc_slice_copy<T: Copy>(&self, v: &[T]) -> &'ast [T] {
        self.arena.alloc_slice_copy(v)
    }

    /// Bump-backed Vec — no pre-allocation. Use `bump_vec_cap` when size is known.
    #[inline(always)]
    pub(crate) fn bump_vec<T>(&self) -> BumpVec<'ast, T> {
        self.arena.vec()
    }

    /// Bump-backed Vec with pre-allocated capacity. Always prefer this.
    /// The `'ast` lifetime means allocations survive past this call.
    #[inline(always)]
    pub(crate) fn bump_vec_cap<T>(&self, cap: usize) -> BumpVec<'ast, T> {
        self.arena.vec_with_capacity(cap)
    }

    // ── Token helpers ─────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn peek(&self) -> &TokenType {
        self.cursor.peek()
    }

    #[inline(always)]
    pub(crate) fn span(&self) -> Span {
        self.cursor.current_span()
    }

    #[inline(always)]
    pub(crate) fn advance_span(&mut self) -> Span {
        self.cursor.advance().span
    }

    /// Try to consume an identifier; return `(interned_name, span)` or `None`.
    #[inline(always)]
    pub(crate) fn eat_ident(&mut self) -> Option<(&'ast str, Span)> {
        if let TokenType::Ident(name) = self.cursor.peek() {
            let name = name.clone();
            let span = self.cursor.current_span();
            self.cursor.advance();
            Some((self.intern(&name), span))
        } else {
            None
        }
    }

    /// Consume an identifier or emit an error and return `None`.
    #[inline]
    pub(crate) fn expect_ident(&mut self) -> Option<(&'ast str, Span)> {
        if let Some(p) = self.eat_ident() { Some(p) } else {
            self.expected(&["identifier"]);
            None
        }
    }

    /// Eat an optional separator: `,` or `;`.
    /// Commas and semicolons are both valid separators in Ubel list contexts.
    /// Returns `true` if one was consumed.
    #[inline(always)]
    pub(crate) fn eat_sep(&mut self) -> bool {
        self.cursor.eat(&TokenType::Comma) || self.cursor.eat(&TokenType::Semicolon)
    }

    /// Check if the current token is a separator without consuming it.
    #[inline(always)]
    pub(crate) fn is_sep(&self) -> bool {
        matches!(self.peek(), TokenType::Comma | TokenType::Semicolon)
    }

    // ── Memoisation ───────────────────────────────────────────────────────────

    #[inline(always)]
    pub(crate) fn memo_get(&self, pos: usize, rule: MemoRule) -> Option<&MemoEntry> {
        self.memo.get(&memo_key(pos, rule))
    }

    #[inline(always)]
    pub(crate) fn memo_set(&mut self, pos: usize, rule: MemoRule, entry: MemoEntry) {
        self.memo.insert(memo_key(pos, rule), entry);
    }

    // ── Sync sets ─────────────────────────────────────────────────────────────

    pub(crate) const DECL_SYNC: &'static [TokenType] = &[
        TokenType::Fn,     TokenType::Struct,  TokenType::Enum,
        TokenType::Trait,  TokenType::Impl,    TokenType::Extend,
        TokenType::Const,  TokenType::TypeKw,  TokenType::Pub,
        TokenType::At,     TokenType::Edge,    TokenType::Eof,
    ];

    pub(crate) const STMT_SYNC: &'static [TokenType] = &[
        TokenType::Semicolon, TokenType::RightBrace,
        TokenType::Fn,        TokenType::Eof,
    ];

    // ── Recovery ──────────────────────────────────────────────────────────────

    #[cold]
    pub(crate) fn recover_to_decl(&mut self) {
        self.cursor.skip_until_any(Self::DECL_SYNC);
    }

    #[cold]
    pub(crate) fn recover_to_stmt(&mut self) {
        self.cursor.skip_until_any(Self::STMT_SYNC);
        self.cursor.eat(&TokenType::Semicolon);
    }
}

// ── Capacity constants ────────────────────────────────────────────────────────

pub(crate) mod cap {
    pub const FN_PARAMS:      usize = 4;
    pub const CALL_ARGS:      usize = 4;
    pub const STRUCT_FIELDS:  usize = 8;
    pub const BLOCK_STMTS:    usize = 16;
    pub const MATCH_ARMS:     usize = 8;
    pub const GENERIC_PARAMS: usize = 2;
    pub const GENERIC_ARGS:   usize = 2;
    pub const IMPORT_LIST:    usize = 4;
    pub const ATTR_ARGS:      usize = 2;
    pub const IMPL_ITEMS:     usize = 8;
    pub const TRAIT_ITEMS:    usize = 8;
    pub const ENUM_VARIANTS:  usize = 8;
    pub const LINQ_CLAUSES:   usize = 4;
    pub const PATH_SEGS:      usize = 3;
    }
