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
        errors::{ParseContext, ParseError},
        ErrorManager,
    },
    lexer::{Span, Token, TokenType},
};

use crate::cursor::Cursor;
use crate::estimates::ParseEstimates;

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
    pub(crate) cursor:     Cursor<'tok>,
    pub(crate) arena:      &'ast AstArena,
    pub(crate) errors:     ErrorManager,
    pub(crate) context:    ParseContext,
    pub(crate) tier:       TierAnnotation,
    pub(crate) memo:       FxHashMap<u64, MemoEntry>,
    /// Dynamic estimates derived from total token count.
    /// Use these instead of the static `cap::*` constants wherever possible.
    pub(crate) estimates:  ParseEstimates,
}

impl<'ast, 'tok> Parser<'ast, 'tok> {
    pub fn new(arena: &'ast AstArena, tokens: &'tok [Token], source: String) -> Self {
        let estimates = ParseEstimates::from_token_count(tokens.len());
        Parser {
            cursor:    Cursor::new(tokens),
            arena,
            errors:    ErrorManager::new(source),
            context:   ParseContext::TopLevel,
            tier:      TierAnnotation::High,
            memo:      FxHashMap::default(),
            estimates,
        }
    }

    pub fn parse_program(mut self) -> Result<Program<'ast>, ErrorManager> {
        let prog = crate::parsers::parse_program::parse_program(&mut self);
        if self.errors.has_errors() { Err(self.errors) } else { Ok(prog) }
    }

    pub fn parse_single_expr(mut self) -> Option<&'ast Expr<'ast>> {
        crate::parsers::parse_expr::parse_expr(&mut self)
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

impl<'ast, 'tok> Parser<'ast, 'tok> {

    #[inline(always)]
    pub(crate) fn emit(&mut self, err: ParseError) {
        self.errors.add_parse_error(err);
    }

    #[inline(always)]
    pub(crate) fn expected(&mut self, what: &[&str]) {
        let tok = self.cursor.peek_token();
        self.emit(crate::error::unexpected(tok, what, self.context.clone()));
    }

    #[inline(always)]
    pub(crate) fn enter(&mut self, ctx: ParseContext) -> ParseContext {
        std::mem::replace(&mut self.context, ctx)
    }

    #[inline(always)]
    pub(crate) fn leave(&mut self, ctx: ParseContext) {
        self.context = ctx;
    }

    #[inline(always)]
    pub(crate) fn enter_tier(&mut self, tier: TierAnnotation) -> TierAnnotation {
        std::mem::replace(&mut self.tier, tier)
    }

    #[inline(always)]
    pub(crate) fn leave_tier(&mut self, tier: TierAnnotation) {
        self.tier = tier;
    }

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

    #[inline(always)]
    pub(crate) fn bump_vec<T>(&self) -> BumpVec<'ast, T> {
        self.arena.vec()
    }

    /// Pre-allocated bump Vec. Use `self.estimates.*` for `cap`.
    #[inline(always)]
    pub(crate) fn bump_vec_cap<T>(&self, cap: usize) -> BumpVec<'ast, T> {
        self.arena.vec_with_capacity(cap)
    }

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

    #[inline(always)]
    pub(crate) fn eat_ident(&mut self) -> Option<(&'ast str, Span)> {
        match self.cursor.peek() {
            TokenType::Ident(name) => {
                let name = name.clone();
                let span = self.cursor.current_span();
                self.cursor.advance();
                Some((self.intern(&name), span))
            }
            // Built-in collection type names are dedicated keyword tokens
            // (so `parse_type.rs` can special-case them for generic syntax
            // like `List<int>`), but they're also valid ordinary names
            // anywhere an identifier is expected — `List.new()`, `summon
            // std.collections.List`, etc. Accept them here as identifier-
            // like, using the same canonical spelling as their Display impl.
            TokenType::KwList | TokenType::KwDictionary | TokenType::KwSet
            | TokenType::KwQueue | TokenType::KwStack => {
                let name = self.cursor.peek().to_string();
                let span = self.cursor.current_span();
                self.cursor.advance();
                Some((self.intern(&name), span))
            }
            // Same reasoning as above, for the tier-system keywords. This
            // is what makes `@tier(mid)` parseable at all: `tier` lexes as
            // TokenType::Tier (needed as the attribute name), and
            // `high`/`mid`/`low` lex as TokenType::High/Mid/Low (needed as
            // the attribute's bare-identifier argument in
            // parse_attr.rs::parse_generic_attr_arg). Neither is
            // TokenType::Ident, so without this, `@tier(...)` cannot be
            // written in source at all — see docs/MEMORY_MODEL.md.
            TokenType::Tier | TokenType::High | TokenType::Mid | TokenType::Low => {
                let name = self.cursor.peek().to_string();
                let span = self.cursor.current_span();
                self.cursor.advance();
                Some((self.intern(&name), span))
            }
            // Same reasoning again, for the `with`-statement allocator
            // keywords. parse_stmt.rs::parse_allocator_kind resolves
            // `arena` / `pool` / `gc` / `heap` via eat_ident() too — all
            // four lex as dedicated tokens, not TokenType::Ident, so
            // without this, no `with arena(...)` / `with pool<T>(n)` /
            // `with gc` / `with heap` statement can be written in source
            // at all, regardless of tier.
            TokenType::Arena | TokenType::Pool | TokenType::Gc | TokenType::Heap => {
                let name = self.cursor.peek().to_string();
                let span = self.cursor.current_span();
                self.cursor.advance();
                Some((self.intern(&name), span))
            }
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn expect_ident(&mut self) -> Option<(&'ast str, Span)> {
        if let Some(p) = self.eat_ident() { Some(p) } else {
            self.expected(&["identifier"]);
            None
        }
    }

    /// Eat an optional comma or semicolon separator.
    /// Commas and semicolons are interchangeable optional separators in Ubel.
    #[inline(always)]
    pub(crate) fn eat_sep(&mut self) -> bool {
        self.cursor.eat(&TokenType::Comma) || self.cursor.eat(&TokenType::Semicolon)
    }

    #[inline(always)]
    pub(crate) fn is_sep(&self) -> bool {
        matches!(self.peek(), TokenType::Comma | TokenType::Semicolon)
    }

    #[inline(always)]
    pub(crate) fn memo_get(&self, pos: usize, rule: MemoRule) -> Option<&MemoEntry> {
        self.memo.get(&memo_key(pos, rule))
    }

    #[inline(always)]
    pub(crate) fn memo_set(&mut self, pos: usize, rule: MemoRule, entry: MemoEntry) {
        self.memo.insert(memo_key(pos, rule), entry);
    }

    pub(crate) const DECL_SYNC: &'static [TokenType] = &[
        TokenType::Fn,    TokenType::Struct, TokenType::Enum,
        TokenType::Trait, TokenType::Impl,   TokenType::Extend,
        TokenType::Const, TokenType::TypeKw, TokenType::Pub,
        TokenType::At,    TokenType::Edge,   TokenType::Eof,
    ];

    pub(crate) const STMT_SYNC: &'static [TokenType] = &[
        TokenType::Semicolon, TokenType::RightBrace,
        TokenType::Fn,        TokenType::Eof,
    ];

    #[cold]
    pub(crate) fn recover_to_decl(&mut self) {
        self.cursor.skip_until_any(Self::DECL_SYNC);
    }

    #[cold]
    pub(crate) fn recover_to_stmt(&mut self) {
        self.cursor.skip_until_any(Self::STMT_SYNC);
        self.cursor.eat(&TokenType::Semicolon);
    }

    /// Call at the end of every iteration of a "while not at closing
    /// delimiter" list-parsing loop. `pos_before` must be the cursor
    /// position recorded at the *top* of that same iteration.
    ///
    /// `skip_until_any` (used by `recover_to_decl` / `recover_to_stmt` /
    /// bespoke recovery functions) can legitimately perform zero
    /// iterations when the cursor already sits on a sync token — that is
    /// correct for *finding* a sync point, but it does not by itself
    /// guarantee a list loop advances. If a failed inner parse consumed
    /// no tokens and recovery also consumed none, this forces exactly
    /// one token of progress so the loop is guaranteed to terminate on
    /// malformed input instead of hanging forever.
    #[cold]
    pub(crate) fn guard_progress(&mut self, pos_before: usize) {
        if self.cursor.position() == pos_before && !self.cursor.is_eof() {
            self.cursor.advance();
        }
    }

    /// Returns true if the current token can start an expression.
    pub(crate) fn can_start_expr(&self) -> bool {
        matches!(self.cursor.peek(),
            TokenType::Ident(_)  | TokenType::IntLit(_)   | TokenType::FloatLit(_) |
            TokenType::StringLit(_) | TokenType::True     | TokenType::False        |
            TokenType::Null      | TokenType::SelfKw      | TokenType::Minus        |
            TokenType::Bang      | TokenType::Not         | TokenType::Tilde        |
            TokenType::Await     | TokenType::LeftParen   | TokenType::LeftBracket  |
            TokenType::LeftBrace | TokenType::If          | TokenType::Match        |
            TokenType::From      | TokenType::Unsafe      | TokenType::Async        |
            TokenType::Fn       | TokenType::InterpolatedString(_) |
            TokenType::VerbatimString(_) | TokenType::CharLit(_)  |
            // Borrow/Deref prefix operators — `&`/`ref`/`&mut`/`ref mut`
            // and `*`/`deref` (docs/PARSER_RULES.md §5.6). Missing this
            // meant `return *x`/`return &x` misparsed as a bare `return`
            // (void) followed by a dangling expression statement — caught
            // by the first real fixture run, same failure shape as the
            // `where` collision that hit Linqerizer's `.query()` earlier.
            TokenType::Amp       | TokenType::Ref         | TokenType::Star        |
            TokenType::Deref
        )
    }
}

// ── Static capacity module (kept for backward compat in parse_attr.rs) ───────

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
    pub const PATH_SEGS:      usize = 3;
}
