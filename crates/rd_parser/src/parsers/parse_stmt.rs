// crates/rd_parser/src/parsers/parse_stmt.rs
//
// Fixed to match the actual statements.rs AST types:
//   StmtKind::Let { mutable, binding: BindingTarget, ty, value }
//   StmtKind::If(&'ast IfExpr)  — pointer, not inline
//   StmtKind::For { binding, iter, body: &'ast Block }  — body is pointer
//   StmtKind::With { allocator: AllocatorKind, body: &'ast Block }
//   StmtKind::Try { body, catch_binding, catch_body }
//   SizeUnit::Bytes / KB / MB / GB  (not B)

use ubel_stratum::{
    ast::{
        common::AssignOp,
        expressions::{ElifBranch, IfExpr, MatchArm, MatchArmBody},
        statements::{
            AllocatorKind, BindingTarget, Block, SizeExpr, SizeUnit,
            Stmt, StmtKind, UsingBinding,
        },
        patterns::DestructurePattern,
    },
    error_management::error_types::ParseContext,
    lexer::TokenType,
};

use crate::parser::Parser;
use crate::parsers::parse_expr;

impl<'ast, 'tok> Parser<'ast, 'tok> {

    // ── Block (public — called from parse_decl) ───────────────────────────────

    pub(crate) fn parse_block(&mut self) -> Option<Block<'ast>> {
        parse_block_inner(self)
    }
}

/// Free function so parse_expr.rs can call it without going through `impl Parser`.
pub(crate) fn parse_block_inner<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<Block<'ast>> {
    let prev = p.enter(ParseContext::Block);
    let lo   = p.span();

    let open_span = lo;
    if let Err(e) = p.cursor.expect(&TokenType::LeftBrace) {
        p.emit(crate::error::from_cursor(e, ParseContext::Block));
        p.leave(prev);
        return None;
    }

    let mut stmts: Vec<Stmt<'ast>> = Vec::with_capacity(p.estimates.block_stmts);

    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        while p.cursor.eat(&TokenType::Semicolon) {}
        if p.cursor.is_at(&TokenType::RightBrace) { break; }

        if let Some(stmt) = parse_stmt(p) {
            stmts.push(stmt);
        } else {
            p.recover_to_stmt();
        }
    }

    let close_span = p.span();
    if !p.cursor.eat(&TokenType::RightBrace) {
        p.emit(crate::error::unclosed('{', open_span, None, close_span));
    }

    p.leave(prev);
    let span  = lo.merge(&close_span);
    let stmts = p.arena.alloc_slice_clone(&stmts);
    Some(Block { stmts, span })
}

// ── Statement dispatcher ──────────────────────────────────────────────────────

pub(crate) fn parse_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<Stmt<'ast>> {
    let lo   = p.span();
    let prev = p.enter(ParseContext::Statement);

    let kind = match p.cursor.peek().clone() {
        TokenType::Let      => parse_let_stmt(p)?,
        TokenType::Return   => parse_return_stmt(p)?,
        TokenType::Fail     => parse_fail_stmt(p)?,
        TokenType::If       => parse_if_stmt(p)?,
        TokenType::Match    => parse_match_stmt(p)?,
        TokenType::For      => parse_for_stmt(p)?,
        TokenType::While    => parse_while_stmt(p)?,
        TokenType::Loop     => parse_loop_stmt(p)?,
        TokenType::Break    => parse_break_stmt(p)?,
        TokenType::Continue => { p.cursor.advance(); StmtKind::Continue }
        TokenType::With     => parse_with_stmt(p)?,
        TokenType::Using    => parse_using_stmt(p)?,
        TokenType::Extract  => parse_extract_stmt(p)?,
        TokenType::Defer    => parse_defer_stmt(p)?,
        TokenType::Try      => parse_try_stmt(p)?,
        TokenType::Unsafe   => parse_unsafe_stmt(p)?,
        // Short declaration: `name := expr`
        TokenType::Ident(_) if matches!(p.cursor.peek_nth(1), TokenType::ColonEqual) =>
            parse_short_decl(p)?,
        // Assign or expression statement
        _ => {
            let expr = parse_expr::parse_expr(p)?;
            // Check for assignment after expr
            if let Some(op) = try_eat_assign_op(p) {
                let value = parse_expr::parse_expr(p)?;
                StmtKind::Expr(p.alloc(ubel_stratum::ast::expressions::Expr {
                    kind: ubel_stratum::ast::expressions::ExprKind::Assign {
                        op, target: expr, value,
                    },
                    span: lo.merge(&value.span),
                }))
            } else {
                StmtKind::Expr(expr)
            }
        }
    };

    p.eat_sep();
    p.leave(prev);
    Some(Stmt { kind, span: lo.merge(&p.span()) })
}

// ── let binding ───────────────────────────────────────────────────────────────

fn parse_let_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `let`
    let mutable = p.cursor.eat(&TokenType::Mut);
    let (name, _) = p.expect_ident()?;
    let ty    = crate::parsers::parse_decl::parse_type_annotation_opt(p);
    if let Err(e) = p.cursor.expect(&TokenType::Equal) {
        p.emit(crate::error::from_cursor(e, ParseContext::Statement));
        return None;
    }
    let value = parse_expr::parse_expr(p)?;
    Some(StmtKind::Let {
        mutable,
        binding: BindingTarget::Ident(name),
        ty,
        value,
    })
}

fn parse_short_decl<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    let (name, _) = p.eat_ident()?;
    p.cursor.advance(); // `:=`
    let value = parse_expr::parse_expr(p)?;
    Some(StmtKind::Let {
        mutable: false,
        binding: BindingTarget::Ident(name),
        ty: None,
        value,
    })
}

// ── return / fail ─────────────────────────────────────────────────────────────

fn parse_return_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `return`
    let value = if p.can_start_expr() {
        Some(parse_expr::parse_expr(p)?)
    } else { None };
    Some(StmtKind::Return(value))
}

fn parse_fail_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `fail`
    let value = parse_expr::parse_expr(p)?;
    Some(StmtKind::Fail(value))
}

// ── if / elif / else ──────────────────────────────────────────────────────────

fn parse_if_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    let lo = p.span();
    p.cursor.advance(); // `if`

    let condition = parse_expr::parse_expr(p)?;
    let then_block = parse_block_inner(p)?;

    let mut elif_branches: Vec<ElifBranch<'ast>> = Vec::with_capacity(2);
    while p.cursor.eat(&TokenType::Elif) {
        let elif_lo  = p.span();
        let cond     = parse_expr::parse_expr(p)?;
        let block    = parse_block_inner(p)?;
        let span     = elif_lo.merge(&block.span);
        elif_branches.push(ElifBranch { condition: cond, block, span });
    }
    let elif_branches = p.arena.alloc_slice_clone(&elif_branches);

    let else_block = if p.cursor.eat(&TokenType::Else) {
        Some(parse_block_inner(p)?)
    } else { None };

    let span    = lo.merge(&p.span());
    let if_node = p.alloc(IfExpr { condition, then_block, elif_branches, else_block, span });
    Some(StmtKind::If(if_node))
}

// ── match ─────────────────────────────────────────────────────────────────────

fn parse_match_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `match`
    let scrutinee = parse_expr::parse_expr(p)?;

    let open_span = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftBrace) {
        p.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
        return None;
    }

    let mut arms: Vec<MatchArm<'ast>> = Vec::with_capacity(p.estimates.match_arms);
    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        if let Some(arm) = parse_match_arm(p) { arms.push(arm); }
        p.eat_sep();
    }

    if !p.cursor.eat(&TokenType::RightBrace) {
        p.emit(crate::error::unclosed('{', open_span, None, p.span()));
    }
    let arms = p.arena.alloc_slice_clone(&arms);
    Some(StmtKind::Match { scrutinee, arms })
}

fn parse_match_arm<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<MatchArm<'ast>> {
    let lo      = p.span();
    let pattern = crate::parsers::parse_pattern::parse_pattern(p)?;
    let guard   = if p.cursor.eat(&TokenType::Where) {
        Some(parse_expr::parse_expr(p)?)
    } else { None };
    if let Err(e) = p.cursor.expect(&TokenType::FatArrow) {
        p.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
        return None;
    }
    let body = if p.cursor.is_at(&TokenType::LeftBrace) {
        MatchArmBody::Block(parse_block_inner(p)?)
    } else {
        MatchArmBody::Expr(parse_expr::parse_expr(p)?)
    };
    let span = lo.merge(&p.span());
    Some(MatchArm { pattern, guard, body, span })
}

// ── for / while / loop ────────────────────────────────────────────────────────

fn parse_for_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `for`
    let (name, _) = p.expect_ident()?;
    if let Err(e) = p.cursor.expect(&TokenType::In) {
        p.emit(crate::error::from_cursor(e, ParseContext::Statement));
        return None;
    }
    let iter  = parse_expr::parse_expr(p)?;
    let body  = parse_block_inner(p)?;
    let body  = p.alloc(body); // body is &'ast Block in StmtKind::For
    Some(StmtKind::For { binding: BindingTarget::Ident(name), iter, body })
}

fn parse_while_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `while`
    let condition = parse_expr::parse_expr(p)?;
    let body      = p.alloc(parse_block_inner(p)?);
    Some(StmtKind::While { condition, body })
}

fn parse_loop_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `loop`
    let body = p.alloc(parse_block_inner(p)?);
    Some(StmtKind::Loop(body))
}

fn parse_break_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `break`
    let value = if p.can_start_expr() { Some(parse_expr::parse_expr(p)?) } else { None };
    Some(StmtKind::Break(value))
}

// ── with arena(...) ───────────────────────────────────────────────────────────

fn parse_with_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    let prev = p.enter(ParseContext::ArenaBlock);
    p.cursor.advance(); // `with`

    let allocator = parse_allocator_kind(p)?;
    let body      = p.alloc(parse_block_inner(p)?);

    p.leave(prev);
    Some(StmtKind::With { allocator, body })
}

fn parse_allocator_kind<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<AllocatorKind<'ast>> {
    let (kw, span) = p.eat_ident()?;
    match kw {
        "arena" => {
            let open = p.span();
            if let Err(e) = p.cursor.expect(&TokenType::LeftParen) {
                p.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                return None;
            }
            let size = parse_size_expr(p)?;
            if !p.cursor.eat(&TokenType::RightParen) {
                p.emit(crate::error::unclosed('(', open, None, p.span()));
            }
            Some(AllocatorKind::Arena(size))
        }
        "pool" => {
            // pool<T>(count)
            let _ = crate::parsers::parse_type::parse_type_annotation_opt_inner(p);
            let open = p.span();
            if let Err(e) = p.cursor.expect(&TokenType::LeftParen) {
                p.emit(crate::error::from_cursor(e, ParseContext::ArenaBlock));
                return None;
            }
            let ty_ref = p.parse_type_expr();
            let count  = parse_expr::parse_expr(p)?;
            if !p.cursor.eat(&TokenType::RightParen) {
                p.emit(crate::error::unclosed('(', open, None, p.span()));
            }
            let ty = ty_ref.unwrap_or_else(|| {
                let void_ty = ubel_stratum::ast::types::TypeKind::Void;
                p.alloc(ubel_stratum::ast::types::Type {
                    kind: void_ty, span,
                })
            });
            Some(AllocatorKind::Pool { ty, count })
        }
        "gc"   => Some(AllocatorKind::Gc),
        "heap" => Some(AllocatorKind::Heap),
        _ => {
            p.emit(crate::error::raw(
                format!("unknown allocator '{}'; expected arena, pool, gc, or heap", kw),
                span,
            ));
            None
        }
    }
}

fn parse_size_expr<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<SizeExpr<'ast>> {
    // `Integer SizeUnit` or general expression
    if let TokenType::IntLit(n) = p.cursor.peek().clone() {
        let saved = p.cursor.position();
        p.cursor.advance();
        if let TokenType::Ident(unit_str) = p.cursor.peek().clone() {
            if let Some(unit) = crate::keywords::parse_size_unit(&unit_str) {
                p.cursor.advance();
                return Some(SizeExpr::WithUnit { value: n as u64, unit });
            }
        }
        // Not a unit — restore and fall through to expression parse
        p.cursor.restore(saved);
    }
    let expr = parse_expr::parse_expr(p)?;
    Some(SizeExpr::Expr(expr))
}

// ── using ─────────────────────────────────────────────────────────────────────

fn parse_using_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `using`
    let mut bindings: Vec<UsingBinding<'ast>> = Vec::with_capacity(2);

    loop {
        let blo = p.span();
        if let Err(e) = p.cursor.expect(&TokenType::Let) {
            p.emit(crate::error::from_cursor(e, ParseContext::Statement));
            break;
        }
        let mutable = p.cursor.eat(&TokenType::Mut);
        let (name, _) = p.expect_ident()?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) {
            p.emit(crate::error::from_cursor(e, ParseContext::Statement));
            break;
        }
        let value = parse_expr::parse_expr(p)?;
        let span  = blo.merge(&value.span);
        bindings.push(UsingBinding { mutable, name, value, span });
        if !p.cursor.eat(&TokenType::Comma) { break; }
    }

    let body     = p.alloc(parse_block_inner(p)?);
    let bindings = p.arena.alloc_slice_clone(&bindings);
    Some(StmtKind::Using { bindings, body })
}

// ── extract / defer / try / unsafe ───────────────────────────────────────────

fn parse_extract_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `extract`
    // Simple ident for now — full destructure pattern in parse_pattern.rs next batch
    let (name, span) = p.expect_ident()?;
    let pattern = DestructurePattern::Ident(name, span);
    if let Err(e) = p.cursor.expect(&TokenType::Equal) {
        p.emit(crate::error::from_cursor(e, ParseContext::Statement));
        return None;
    }
    let value = parse_expr::parse_expr(p)?;
    Some(StmtKind::Extract { pattern, value })
}

fn parse_defer_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `defer`
    let expr = parse_expr::parse_expr(p)?;
    Some(StmtKind::Defer(expr))
}

fn parse_try_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `try`
    let body = p.alloc(parse_block_inner(p)?);

    let (catch_binding, catch_body) = if p.cursor.eat(&TokenType::Catch) {
        let open = p.span();
        if let Err(e) = p.cursor.expect(&TokenType::LeftParen) {
            p.emit(crate::error::from_cursor(e, ParseContext::Statement));
            return Some(StmtKind::Try { body, catch_binding: None, catch_body: None });
        }
        let (name, _) = p.expect_ident()?;
        if !p.cursor.eat(&TokenType::RightParen) {
            p.emit(crate::error::unclosed('(', open, None, p.span()));
        }
        let cb = p.alloc(parse_block_inner(p)?);
        (Some(name), Some(cb))
    } else {
        (None, None)
    };

    Some(StmtKind::Try { body, catch_binding, catch_body })
}

fn parse_unsafe_stmt<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<StmtKind<'ast>> {
    p.cursor.advance(); // `unsafe`
    let body = p.alloc(parse_block_inner(p)?);
    Some(StmtKind::Unsafe(body))
}

// ── Assignment op helper ──────────────────────────────────────────────────────

fn try_eat_assign_op(p: &mut Parser<'_, '_>) -> Option<AssignOp> {
    let op = match p.cursor.peek() {
        TokenType::Equal           => AssignOp::Assign,
        TokenType::PlusEqual       => AssignOp::AddAssign,
        TokenType::MinusEqual      => AssignOp::SubAssign,
        TokenType::StarEqual       => AssignOp::MulAssign,
        TokenType::SlashEqual      => AssignOp::DivAssign,
        TokenType::PercentEqual    => AssignOp::RemAssign,
        TokenType::AmpEqual        => AssignOp::BitAndAssign,
        TokenType::PipeEqual       => AssignOp::BitOrAssign,
        TokenType::CaretEqual      => AssignOp::BitXorAssign,
        TokenType::LeftShiftEqual  => AssignOp::ShlAssign,
        TokenType::RightShiftEqual => AssignOp::ShrAssign,
        _                          => return None,
    };
    p.cursor.advance();
    Some(op)
        }
