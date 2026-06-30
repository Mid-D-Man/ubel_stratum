// crates/rd_parser/src/parsers/parse_stmt.rs
//
// Statement and block parser.
// Full statement implementations come next batch; parse_block is needed now
// so parse_decl.rs can compile and be tested end-to-end.
//
// Grammar:
//   Block     ::= "{" Statement* "}"
//   Statement ::= LetStmt | AssignStmt | ReturnStmt | IfStmt | MatchStmt |
//                 ForStmt | WhileStmt | LoopStmt | BreakStmt | ContinueStmt |
//                 WithStmt | UsingStmt | ExtractStmt | DeferStmt | TryBlock |
//                 UnsafeBlock | ExprStmt

use ubel_stratum::{
    ast::statements::{Block, Stmt, StmtKind},
    error_management::error_types::ParseContext,
    lexer::{Span, TokenType},
};

use crate::parser::{cap, Parser};

impl<'ast, 'tok> Parser<'ast, 'tok> {

    // ── Block ─────────────────────────────────────────────────────────────────

    /// Parse `{ Statement* }`. Called from every declaration parser.
    pub(crate) fn parse_block(&mut self) -> Option<Block<'ast>> {
        let prev = self.enter(ParseContext::Block);
        let lo   = self.span();

        let open_span = lo;
        if let Err(e) = self.cursor.expect(&TokenType::LeftBrace) {
            self.emit(crate::error::from_cursor(e, ParseContext::Block));
            self.leave(prev);
            return None;
        }

        let mut stmts: Vec<Stmt<'ast>> = Vec::with_capacity(cap::BLOCK_STMTS);

        while !self.cursor.is_at(&TokenType::RightBrace) && !self.cursor.is_eof() {
            // Eat stray separators between statements (optional semicolons)
            while self.cursor.eat(&TokenType::Semicolon) {}

            if self.cursor.is_at(&TokenType::RightBrace) { break; }

            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            } else {
                self.recover_to_stmt();
            }
        }

        let close_span = self.span();
        if !self.cursor.eat(&TokenType::RightBrace) {
            self.emit(crate::error::unclosed('{', open_span, None, close_span));
        }

        self.leave(prev);
        let span  = lo.merge(&close_span);
        let stmts = self.arena.alloc_slice_clone(&stmts);
        Some(Block { stmts, span })
    }

    // ── Statement dispatcher ──────────────────────────────────────────────────

    pub(crate) fn parse_stmt(&mut self) -> Option<Stmt<'ast>> {
        let lo = self.span();
        let prev = self.enter(ParseContext::Statement);

        let kind = match self.cursor.peek().clone() {
            TokenType::Let     => self.parse_let_stmt()?,
            TokenType::Return  => self.parse_return_stmt()?,
            TokenType::Fail    => self.parse_fail_stmt()?,
            TokenType::If      => self.parse_if_stmt()?,
            TokenType::Match   => self.parse_match_stmt()?,
            TokenType::For     => self.parse_for_stmt()?,
            TokenType::While   => self.parse_while_stmt()?,
            TokenType::Loop    => self.parse_loop_stmt()?,
            TokenType::Break   => self.parse_break_stmt()?,
            TokenType::Continue => { self.cursor.advance(); StmtKind::Continue }
            TokenType::With    => self.parse_with_stmt()?,
            TokenType::Using   => self.parse_using_stmt()?,
            TokenType::Extract => self.parse_extract_stmt()?,
            TokenType::Defer   => self.parse_defer_stmt()?,
            TokenType::Try     => self.parse_try_block_stmt()?,
            TokenType::Unsafe  => self.parse_unsafe_block_stmt()?,
            // Short declaration: `name := expr`
            TokenType::Ident(_) if matches!(self.cursor.peek_nth(1), TokenType::ColonEqual) =>
                self.parse_short_decl()?,
            // Assignment or expression statement
            _ => {
                let expr = crate::parsers::parse_expr::parse_expr(self)?;
                // Check for assignment op following the expression
                if let Some(op) = self.try_eat_assign_op() {
                    let value = crate::parsers::parse_expr::parse_expr(self)?;
                    StmtKind::Assign { op, target: expr, value }
                } else {
                    StmtKind::Expr(expr)
                }
            }
        };

        self.eat_sep(); // optional trailing semicolon
        self.leave(prev);
        Some(Stmt { kind, span: lo.merge(&self.span()) })
    }

    // ── let stmt ─────────────────────────────────────────────────────────────

    fn parse_let_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `let`
        let mutable = self.cursor.eat(&TokenType::Mut);
        // Binding target: simple ident for now (destructure: next batch)
        let (name, _) = self.expect_ident()?;
        let ty        = self.parse_type_annotation();
        if let Err(e) = self.cursor.expect(&TokenType::Equal) {
            self.emit(crate::error::from_cursor(e, ParseContext::Statement));
            return None;
        }
        let init = crate::parsers::parse_expr::parse_expr(self)?;
        Some(StmtKind::Let { mutable, name, ty, init })
    }

    // ── Short declaration: `name := expr` ────────────────────────────────────

    fn parse_short_decl(&mut self) -> Option<StmtKind<'ast>> {
        let (name, _) = self.eat_ident()?;
        self.cursor.advance(); // consume `:=`
        let init = crate::parsers::parse_expr::parse_expr(self)?;
        Some(StmtKind::Let { mutable: false, name, ty: None, init })
    }

    // ── return / fail ─────────────────────────────────────────────────────────

    fn parse_return_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `return`
        // Value is optional — if next token can't start an expression, treat as void
        let value = if self.can_start_expr() {
            crate::parsers::parse_expr::parse_expr(self)
        } else { None };
        Some(StmtKind::Return(value))
    }

    fn parse_fail_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `fail`
        let value = crate::parsers::parse_expr::parse_expr(self)?;
        Some(StmtKind::Fail(value))
    }

    // ── if / elif / else ─────────────────────────────────────────────────────

    fn parse_if_stmt(&mut self) -> Option<StmtKind<'ast>> {
        let prev = self.enter(ParseContext::Statement);
        self.cursor.advance(); // consume `if`
        let cond      = crate::parsers::parse_expr::parse_expr(self)?;
        let then_body = self.parse_block()?;

        let mut elif_branches = Vec::with_capacity(2);
        while self.cursor.eat(&TokenType::Elif) {
            let elif_cond = crate::parsers::parse_expr::parse_expr(self)?;
            let elif_body = self.parse_block()?;
            elif_branches.push((elif_cond, elif_body));
        }

        let else_body = if self.cursor.eat(&TokenType::Else) {
            Some(self.parse_block()?)
        } else { None };

        self.leave(prev);
        Some(StmtKind::If {
            cond,
            then_body,
            elif_branches: self.arena.alloc_slice_clone(&elif_branches),
            else_body,
        })
    }

    // ── match ─────────────────────────────────────────────────────────────────

    fn parse_match_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `match`
        let subject = crate::parsers::parse_expr::parse_expr(self)?;
        let open    = self.span();
        if let Err(e) = self.cursor.expect(&TokenType::LeftBrace) {
            self.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
            return None;
        }
        let mut arms = Vec::with_capacity(cap::MATCH_ARMS);
        while !self.cursor.is_at(&TokenType::RightBrace) && !self.cursor.is_eof() {
            if let Some(arm) = self.parse_match_arm() { arms.push(arm); }
            self.eat_sep();
        }
        if !self.cursor.eat(&TokenType::RightBrace) {
            self.emit(crate::error::unclosed('{', open, None, self.span()));
        }
        Some(StmtKind::Match {
            subject,
            arms: self.arena.alloc_slice_clone(&arms),
        })
    }

    fn parse_match_arm(
        &mut self,
    ) -> Option<ubel_stratum::ast::statements::MatchArm<'ast>> {
        let prev    = self.enter(ParseContext::MatchArm);
        let lo      = self.span();
        let pattern = crate::parsers::parse_pattern::parse_pattern(self)?;

        // Optional guard: `where expr`
        let guard = if self.cursor.eat(&TokenType::Where) {
            Some(crate::parsers::parse_expr::parse_expr(self)?)
        } else { None };

        if let Err(e) = self.cursor.expect(&TokenType::FatArrow) {
            self.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
            self.leave(prev);
            return None;
        }

        // Body: either a block or an expression
        let body = if self.cursor.is_at(&TokenType::LeftBrace) {
            ubel_stratum::ast::statements::MatchBody::Block(self.parse_block()?)
        } else {
            ubel_stratum::ast::statements::MatchBody::Expr(
                crate::parsers::parse_expr::parse_expr(self)?
            )
        };

        self.leave(prev);
        Some(ubel_stratum::ast::statements::MatchArm {
            pattern,
            guard,
            body,
            span: lo.merge(&self.span()),
        })
    }

    // ── for / while / loop ────────────────────────────────────────────────────

    fn parse_for_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `for`
        let (binding, _) = self.expect_ident()?;
        if let Err(e) = self.cursor.expect(&TokenType::In) {
            self.emit(crate::error::from_cursor(e, ParseContext::Statement));
            return None;
        }
        let iter = crate::parsers::parse_expr::parse_expr(self)?;
        let body = self.parse_block()?;
        Some(StmtKind::For { binding, iter, body })
    }

    fn parse_while_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let cond = crate::parsers::parse_expr::parse_expr(self)?;
        let body = self.parse_block()?;
        Some(StmtKind::While { cond, body })
    }

    fn parse_loop_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let body = self.parse_block()?;
        Some(StmtKind::Loop(body))
    }

    fn parse_break_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let value = if self.can_start_expr() {
            crate::parsers::parse_expr::parse_expr(self)
        } else { None };
        Some(StmtKind::Break(value))
    }

    // ── with arena / pool / gc / heap ─────────────────────────────────────────

    fn parse_with_stmt(&mut self) -> Option<StmtKind<'ast>> {
        let prev = self.enter(ParseContext::ArenaBlock);
        self.cursor.advance(); // consume `with`

        let alloc = self.parse_allocator_expr()?;
        let body  = self.parse_block()?;

        self.leave(prev);
        Some(StmtKind::With { alloc, body })
    }

    fn parse_allocator_expr(
        &mut self,
    ) -> Option<ubel_stratum::ast::statements::AllocatorExpr<'ast>> {
        use ubel_stratum::ast::statements::{AllocatorExpr, AllocatorKind, SizeExpr, SizeUnit};

        let (kw, span) = self.eat_ident()?;
        match kw {
            "arena" => {
                if let Err(e) = self.cursor.expect(&TokenType::LeftParen) {
                    self.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                    return None;
                }
                let size = self.parse_size_expr()?;
                if let Err(e) = self.cursor.expect(&TokenType::RightParen) {
                    self.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                }
                Some(AllocatorExpr { kind: AllocatorKind::Arena, size: Some(size), span })
            }
            "pool" => {
                // pool<T>(size)
                let _ = self.try_parse_generic_args(); // eat <T>
                if let Err(e) = self.cursor.expect(&TokenType::LeftParen) {
                    self.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                    return None;
                }
                let size = self.parse_size_expr()?;
                if let Err(e) = self.cursor.expect(&TokenType::RightParen) {
                    self.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                }
                Some(AllocatorExpr { kind: AllocatorKind::Pool, size: Some(size), span })
            }
            "gc"   => Some(AllocatorExpr { kind: AllocatorKind::Gc,   size: None, span }),
            "heap" => Some(AllocatorExpr { kind: AllocatorKind::Heap, size: None, span }),
            _ => {
                self.emit(crate::error::raw(
                    format!("unknown allocator '{}'; expected 'arena', 'pool', 'gc', or 'heap'", kw),
                    span,
                ));
                None
            }
        }
    }

    fn parse_size_expr(
        &mut self,
    ) -> Option<ubel_stratum::ast::statements::SizeExpr<'ast>> {
        use ubel_stratum::ast::statements::{SizeExpr, SizeUnit};

        // Either `Expr` or `Integer SizeUnit`
        if let TokenType::IntLit(n) = self.cursor.peek().clone() {
            let n_val = n;
            let pos   = self.cursor.position();
            self.cursor.advance();

            // Check for size unit: B KB MB GB
            if let TokenType::Ident(unit) = self.cursor.peek().clone() {
                let unit_parsed = match unit.as_str() {
                    "B"  => Some(SizeUnit::B),
                    "KB" => Some(SizeUnit::KB),
                    "MB" => Some(SizeUnit::MB),
                    "GB" => Some(SizeUnit::GB),
                    _    => None,
                };
                if let Some(unit) = unit_parsed {
                    self.cursor.advance();
                    return Some(SizeExpr::Literal { value: n_val as u64, unit });
                }
            }
            // Not a unit — restore and parse as expression
            self.cursor.restore(pos);
        }

        let expr = crate::parsers::parse_expr::parse_expr(self)?;
        Some(SizeExpr::Expr(expr))
    }

    // ── using ─────────────────────────────────────────────────────────────────

    fn parse_using_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance(); // consume `using`
        let mut bindings = Vec::with_capacity(2);

        loop {
            if let Err(e) = self.cursor.expect(&TokenType::Let) {
                self.emit(crate::error::from_cursor(e, ParseContext::Statement));
                break;
            }
            let mutable = self.cursor.eat(&TokenType::Mut);
            let (name, _) = self.expect_ident()?;
            if let Err(e) = self.cursor.expect(&TokenType::Equal) {
                self.emit(crate::error::from_cursor(e, ParseContext::Statement));
                break;
            }
            let init = crate::parsers::parse_expr::parse_expr(self)?;
            bindings.push((mutable, name, init));
            if !self.cursor.eat(&TokenType::Comma) { break; }
        }

        let body = self.parse_block()?;
        Some(StmtKind::Using {
            bindings: self.arena.alloc_slice_clone(&bindings),
            body,
        })
    }

    // ── extract / defer / try / unsafe ────────────────────────────────────────

    fn parse_extract_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        // Pattern + = + expr — pattern parser next batch, stub for now
        let (name, _) = self.expect_ident()?;
        if let Err(e) = self.cursor.expect(&TokenType::Equal) {
            self.emit(crate::error::from_cursor(e, ParseContext::Statement));
            return None;
        }
        let value = crate::parsers::parse_expr::parse_expr(self)?;
        Some(StmtKind::Extract { name, value })
    }

    fn parse_defer_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let expr = crate::parsers::parse_expr::parse_expr(self)?;
        Some(StmtKind::Defer(expr))
    }

    fn parse_try_block_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let body = self.parse_block()?;
        let catch_body = if self.cursor.eat(&TokenType::Catch) {
            if let Err(e) = self.cursor.expect(&TokenType::LeftParen) {
                self.emit(crate::error::from_cursor(e, ParseContext::Statement));
            }
            let (err_name, _) = self.expect_ident()?;
            if let Err(e) = self.cursor.expect(&TokenType::RightParen) {
                self.emit(crate::error::from_cursor(e, ParseContext::Statement));
            }
            let cb = self.parse_block()?;
            Some((err_name, cb))
        } else { None };
        Some(StmtKind::Try { body, catch_body })
    }

    fn parse_unsafe_block_stmt(&mut self) -> Option<StmtKind<'ast>> {
        self.cursor.advance();
        let body = self.parse_block()?;
        Some(StmtKind::Unsafe(body))
    }

    // ── Assignment op helper ──────────────────────────────────────────────────

    fn try_eat_assign_op(
        &mut self,
    ) -> Option<ubel_stratum::ast::common::AssignOp> {
        use ubel_stratum::ast::common::AssignOp;
        let op = match self.cursor.peek() {
            TokenType::Equal          => AssignOp::Assign,
            TokenType::PlusEqual      => AssignOp::AddAssign,
            TokenType::MinusEqual     => AssignOp::SubAssign,
            TokenType::StarEqual      => AssignOp::MulAssign,
            TokenType::SlashEqual     => AssignOp::DivAssign,
            TokenType::PercentEqual   => AssignOp::RemAssign,
            TokenType::AmpEqual       => AssignOp::BitAndAssign,
            TokenType::PipeEqual      => AssignOp::BitOrAssign,
            TokenType::CaretEqual     => AssignOp::BitXorAssign,
            TokenType::LeftShiftEqual => AssignOp::ShlAssign,
            TokenType::RightShiftEqual=> AssignOp::ShrAssign,
            _                         => return None,
        };
        self.cursor.advance();
        Some(op)
    }

    // ── Can-start-expression check ────────────────────────────────────────────

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
            TokenType::VerbatimString(_) | TokenType::CharLit(_)
        )
    }
          }
