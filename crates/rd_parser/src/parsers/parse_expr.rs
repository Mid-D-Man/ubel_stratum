// crates/rd_parser/src/parsers/parse_expr.rs
//! Expression parser — Pratt / Top-Down Operator Precedence.
//!
//! Binding power table (L = left bp, R = right bp):
//!   Assignment  = += -= …      L=1  R=2   right-assoc
//!   Range .. ..=               L=3  R=4
//!   OrElse `or return/break`   handled before Or
//!   Or / ||                    L=5  R=6
//!   And / &&                   L=7  R=8
//!   Equality == !=             L=9  R=10
//!   Comparison < > <= >=       L=11 R=12
//!   Pipe |>                    L=13 R=14
//!   BitOr |                    L=15 R=16
//!   BitXor ^                   L=17 R=18
//!   BitAnd &                   L=19 R=20
//!   Shift << >>                L=21 R=22
//!   Add/Sub + -                L=23 R=24
//!   Mul/Div/Mod * / %          L=25 R=26
//!   As (type coercion)         L=27 R=— (RHS is a Type, not an Expr)
//!   Unary prefix               R=28
//!   Postfix . () [] ? ?.       L=29 R=30

use ubel_stratum::{
    ast::{
        common::{AssignOp, BinOp, TierAnnotation, UnaryOp},
        expressions::{
            Arg, ArgKind, DictEntry, ElifBranch, Expr, ExprKind,
            FieldInit, IfBranchBody, IfExpr, Lambda, LambdaBody, LambdaParam,
            LinqClause, LinqExpr, MatchArm, MatchExpr,
            ObjectField, OptionalAccess, OrElseFallback,
        },
        literals::{InterpolationPart, Literal},
    },
    error_management::errors::ParseContext,
    lexer::{InterpolationPart as LexPart, Span as LSpan, TokenType},
};

use crate::parser::Parser;

// ── Public entry point ────────────────────────────────────────────────────────

pub(crate) fn parse_expr<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<&'ast Expr<'ast>> {
    parse_bp(p, 0)
}

// ── Pratt core ────────────────────────────────────────────────────────────────

fn parse_bp<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
    min_bp: u8,
) -> Option<&'ast Expr<'ast>> {
    let lo = p.span();
    let mut lhs = parse_prefix(p)?;

    loop {
        let op = p.cursor.peek().clone();

        // ── Special: `or return/break/continue` → OrElse ─────────────────────
        if matches!(op, TokenType::Or) {
            let nxt = p.cursor.peek_nth(1).clone();
            if matches!(nxt, TokenType::Return | TokenType::Break | TokenType::Continue) {
                if 5 < min_bp { break; }
                p.cursor.advance(); // `or`
                let fallback = parse_or_else_rhs(p)?;
                let span = lo.merge(&p.span());
                lhs = p.alloc(Expr { kind: ExprKind::OrElse { expr: lhs, fallback }, span });
                continue;
            }
        }

        // ── Struct literal: `TypeName { field = val }` ────────────────────────
        if matches!(op, TokenType::LeftBrace) && is_struct_prefix(lhs) && is_struct_open(p) {
            lhs = parse_struct_lit(p, lhs, lo)?;
            continue;
        }

        let (l_bp, r_bp) = match infix_bp(&op) {
            Some(v) => v,
            None    => break,
        };
        if l_bp < min_bp { break; }

        let op_span = p.cursor.current_span();
        p.cursor.advance();

        // ── Postfix (no RHS recursion) ────────────────────────────────────────
        match &op {
            TokenType::Dot => { lhs = parse_dot(p, lhs, op_span, lo)?; continue; }
            TokenType::LeftParen => { lhs = parse_call(p, lhs, op_span, lo)?; continue; }
            TokenType::LeftBracket => { lhs = parse_index(p, lhs, op_span, lo)?; continue; }
            TokenType::Question => {
                let span = lo.merge(&op_span);
                lhs = p.alloc(Expr { kind: ExprKind::Try(lhs), span });
                continue;
            }
            TokenType::QuestionDot => { lhs = parse_opt_chain(p, lhs, op_span, lo)?; continue; }
            _ => {}
        }

        // ── `as` type-coercion: RHS is a Type ────────────────────────────────
        if matches!(op, TokenType::As) {
            let ty   = p.parse_type_expr()?;
            let span = lo.merge(&ty.span);
            lhs = p.alloc(Expr { kind: ExprKind::As { expr: lhs, ty }, span });
            continue;
        }

        // ── Assignment ────────────────────────────────────────────────────────
        if let Some(aop) = to_assign_op(&op) {
            let value = parse_bp(p, r_bp)?;
            let span  = lo.merge(&value.span);
            lhs = p.alloc(Expr { kind: ExprKind::Assign { op: aop, target: lhs, value }, span });
            continue;
        }

        // ── Pipe |> ───────────────────────────────────────────────────────────
        if matches!(op, TokenType::PipeArrow) {
            let right = parse_bp(p, r_bp)?;
            let span  = lo.merge(&right.span);
            lhs = p.alloc(Expr { kind: ExprKind::Pipe { left: lhs, right }, span });
            continue;
        }

        // ── Binary operators ──────────────────────────────────────────────────
        if let Some(bop) = to_bin_op(&op) {
            let rhs  = parse_bp(p, r_bp)?;
            let span = lo.merge(&rhs.span);
            lhs = p.alloc(Expr { kind: ExprKind::BinOp { op: bop, lhs, rhs }, span });
            continue;
        }

        break;
    }
    Some(lhs)
}

// ── Prefix ────────────────────────────────────────────────────────────────────

fn parse_prefix<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<&'ast Expr<'ast>> {
    let lo = p.span();
    match p.cursor.peek().clone() {
        TokenType::IntLit(n)    => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Int(n)),    span: lo })) }
        TokenType::FloatLit(f)  => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Float(f)),  span: lo })) }
        TokenType::DoubleLit(d) => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Double(d)), span: lo })) }
        TokenType::StringLit(s) => { let s = p.intern(&s); p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Str(s)), span: lo })) }
        TokenType::VerbatimString(s) => { let s = p.intern(&s); p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::VerbatimStr(s)), span: lo })) }
        TokenType::CharLit(c)   => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Char(c)),   span: lo })) }
        TokenType::True         => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(true)),  span: lo })) }
        TokenType::False        => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(false)), span: lo })) }
        TokenType::Null         => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Null),        span: lo })) }
        TokenType::InterpolatedString(parts) => parse_interp(p, parts, lo),
        TokenType::SelfKw       => { p.cursor.advance(); Some(p.alloc(Expr { kind: ExprKind::SelfExpr, span: lo })) }
        TokenType::Ident(name)  => parse_ident_expr(p, name, lo),
        // Built-in collection type names (List, Dictionary, Set, Queue,
        // Stack) are dedicated keyword tokens for parse_type.rs's benefit,
        // but they're also ordinary names in expression position —
        // `List.new()`, `Dictionary.new()`. Route through the same path
        // as a plain identifier, using the canonical spelling.
        TokenType::KwList | TokenType::KwDictionary | TokenType::KwSet
        | TokenType::KwQueue | TokenType::KwStack | TokenType::KwInlineList => {
            let name = p.cursor.peek().to_string();
            parse_ident_expr(p, name, lo)
        }
        TokenType::Minus => { p.cursor.advance(); let o = parse_bp(p, 28)?; let span = lo.merge(&o.span); Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Neg,    operand: o }, span })) }
        TokenType::Bang  => { p.cursor.advance(); let o = parse_bp(p, 28)?; let span = lo.merge(&o.span); Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Not,    operand: o }, span })) }
        TokenType::Not   => { p.cursor.advance(); let o = parse_bp(p, 28)?; let span = lo.merge(&o.span); Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Not,    operand: o }, span })) }
        TokenType::Tilde => { p.cursor.advance(); let o = parse_bp(p, 28)?; let span = lo.merge(&o.span); Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::BitNot, operand: o }, span })) }
        TokenType::Await => {
            if p.tier != TierAnnotation::High {
                p.emit(crate::error::illegal_here("await", "await is only valid in @tier(high)", lo, Some("use callback pattern for MID/LOW")));
            }
            p.cursor.advance();
            let o = parse_bp(p, 28)?; let span = lo.merge(&o.span);
            Some(p.alloc(Expr { kind: ExprKind::Await(o), span }))
        }
        TokenType::LeftParen    => parse_paren_or_tuple(p, lo),
        TokenType::LeftBracket  => parse_array_lit(p, lo),
        TokenType::LeftBrace    => parse_brace_expr(p, lo),
        TokenType::If           => parse_if_expr(p, lo),
        TokenType::Match        => parse_match_expr(p, lo),
        TokenType::From         => {
            // LINQ: from Ident in Expr ...
            if matches!(p.cursor.peek_nth(1), TokenType::Ident(_)) && matches!(p.cursor.peek_nth(2), TokenType::In) {
                parse_linq(p, lo)
            } else {
                p.expected(&["identifier after 'from' (LINQ query)"]); None
            }
        }
        TokenType::Fn           => parse_lambda(p, lo),
        TokenType::Async        => {
            // async block (not async fn — that's a declaration)
            p.cursor.advance();
            let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
            let block = p.alloc(block);
            let span  = lo.merge(&block.span);
            Some(p.alloc(Expr { kind: ExprKind::Block(block), span }))
        }
        TokenType::Unsafe       => {
            p.cursor.advance();
            let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
            let block = p.alloc(block);
            let span  = lo.merge(&block.span);
            Some(p.alloc(Expr { kind: ExprKind::Block(block), span }))
        }
        _ => { p.expected(&["expression"]); None }
    }
}

// ── Ident / path / short-decl ─────────────────────────────────────────────────

fn parse_ident_expr<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>, name: String, lo: LSpan,
) -> Option<&'ast Expr<'ast>> {
    let name = p.intern(&name);
    p.cursor.advance();

    // Short declaration: `name := expr`
    if p.cursor.is_at(&TokenType::ColonEqual) {
        p.cursor.advance();
        let value = parse_expr(p)?;
        let span  = lo.merge(&value.span);
        return Some(p.alloc(Expr { kind: ExprKind::ShortDecl { name, value }, span }));
    }

    // Build dotted path: `std.io.File`
    let mut segs: Vec<&'ast str> = Vec::with_capacity(p.estimates.path_segs);
    segs.push(name);
    let mut hi = lo;

    while p.cursor.eat(&TokenType::Dot) {
        // Peek: if Ident (or a built-in collection keyword, which is also
        // valid as an ordinary name here) followed by `(` it's a method
        // call — break path, let postfix handle it.
        let seg_name = match p.cursor.peek() {
            TokenType::Ident(s) => Some(s.clone()),
            TokenType::KwList | TokenType::KwDictionary | TokenType::KwSet
            | TokenType::KwQueue | TokenType::KwStack | TokenType::KwInlineList => Some(p.cursor.peek().to_string()),
            _ => None,
        };
        if let Some(seg) = seg_name {
            let seg = p.intern(&seg);
            if matches!(p.cursor.peek_nth(1), TokenType::LeftParen) {
                // Field access result; postfix loop will handle `(`
                p.cursor.advance();
                hi = p.span();
                let base = build_path(p, &segs, lo);
                let span = lo.merge(&hi);
                return Some(p.alloc(Expr { kind: ExprKind::Field { target: base, field: seg }, span }));
            }
            p.cursor.advance();
            hi = p.span();
            segs.push(seg);
        } else {
            p.expected(&["identifier after '.'"]);
            break;
        }
    }

    Some(build_path(p, &segs, lo.merge(&hi)))
}

fn build_path<'ast>(p: &Parser<'ast, '_>, segs: &[&'ast str], span: LSpan) -> &'ast Expr<'ast> {
    if segs.len() == 1 {
        p.alloc(Expr { kind: ExprKind::Ident(segs[0]), span })
    } else {
        let mut e = p.alloc(Expr { kind: ExprKind::Ident(segs[0]), span });
        for seg in &segs[1..] {
            e = p.alloc(Expr { kind: ExprKind::Field { target: e, field: seg }, span });
        }
        e
    }
}

// ── Struct literal ────────────────────────────────────────────────────────────

fn is_struct_prefix(lhs: &Expr<'_>) -> bool {
    matches!(lhs.kind, ExprKind::Ident(_) | ExprKind::Field { .. })
}

fn is_struct_open(p: &Parser<'_, '_>) -> bool {
    debug_assert!(matches!(p.cursor.peek(), TokenType::LeftBrace));
    matches!(
        (p.cursor.peek_nth(1), p.cursor.peek_nth(2)),
        (TokenType::RightBrace, _) | (TokenType::Ident(_), TokenType::Equal)
    )
}

fn extract_path_from_expr<'ast>(_p: &Parser<'ast, '_>, e: &'ast Expr<'ast>) -> Vec<&'ast str> {
    let mut segs = Vec::new();
    fn collect<'a>(e: &'a Expr<'a>, segs: &mut Vec<&'a str>) {
        match e.kind {
            ExprKind::Ident(n) => segs.push(n),
            ExprKind::Field { target, field } => { collect(target, segs); segs.push(field); }
            _ => {}
        }
    }
    collect(e, &mut segs);
    segs
}

fn parse_struct_lit<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>, lhs: &'ast Expr<'ast>, lo: LSpan,
) -> Option<&'ast Expr<'ast>> {
    let path_vec = extract_path_from_expr(p, lhs);
    let path     = p.arena.alloc_slice_copy(path_vec.as_slice());
    let open     = p.span();
    p.cursor.advance(); // `{`
    let mut fields: Vec<FieldInit<'ast>> = Vec::with_capacity(p.estimates.struct_fields);
    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let flo = p.span();
        let (name, _) = p.expect_ident()?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) { p.emit(crate::error::from_cursor(e, ParseContext::Expr)); break; }
        let value = parse_expr(p)?;
        fields.push(FieldInit { name, value, span: flo.merge(&value.span) });
        p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightBrace) { p.emit(crate::error::unclosed('{', open, None, p.span())); }
    let span   = lo.merge(&p.span());
    let fields = p.arena.alloc_slice_copy(fields.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::StructLit { path, fields }, span }))
}

// ── Brace expr: Dict / AnonObject / Block ─────────────────────────────────────

fn parse_brace_expr<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    debug_assert!(matches!(p.cursor.peek(), TokenType::LeftBrace));
    let t1 = p.cursor.peek_nth(1).clone();
    let t2 = p.cursor.peek_nth(2).clone();
    match (&t1, &t2) {
        // `{ Ident = ...}` → AnonObject
        (TokenType::Ident(_), TokenType::Equal) => parse_anon_object(p, lo),
        // `{ }` → empty AnonObject
        (TokenType::RightBrace, _) => {
            p.cursor.advance(); p.cursor.advance(); // { }
            let span = lo.merge(&p.span());
            Some(p.alloc(Expr { kind: ExprKind::AnonObject(&[]), span }))
        }
        // `{ StringLit = ...}` or `{ IntLit = ...}` → Dict
        (TokenType::StringLit(_), TokenType::Equal) |
        (TokenType::IntLit(_),    TokenType::Equal) |
        (TokenType::DoubleLit(_), TokenType::Equal) |
        (TokenType::FloatLit(_),  TokenType::Equal) |
        (TokenType::True,         TokenType::Equal) |
        (TokenType::False,        TokenType::Equal) => parse_dict(p, lo),
        // Everything else → block expression
        _ => {
            let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
            let block = p.alloc(block);
            let span  = lo.merge(&block.span);
            Some(p.alloc(Expr { kind: ExprKind::Block(block), span }))
        }
    }
}

fn parse_dict<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let open = p.span(); p.cursor.advance(); // `{`
    let mut entries: Vec<DictEntry<'ast>> = Vec::with_capacity(p.estimates.call_args);
    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let elo = p.span();
        // min_bp = 3, not the usual 0 (i.e. not the public parse_expr(p)).
        // Assignment `=` is a valid infix operator in the general
        // expression grammar — parsing the key with min_bp=0 would let it
        // greedily consume `key = value` as one Assign expression before
        // control ever reached the explicit expect(Equal) below, which is
        // exactly what made single- and multi-entry dict literals fail to
        // parse before this fix (see docs/MEMORY_MODEL.md). Note this
        // file's own infix_bp table gives assignment (l_bp=2, r_bp=1) —
        // reversed from the header comment's documented "L=1 R=2" for
        // right-associativity — so min_bp needs to be 3, one above
        // assignment's *actual* l_bp of 2, not 2. Range (l_bp=3) and
        // everything looser still works fine in key position.
        let key = parse_bp(p, 3)?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) { p.emit(crate::error::from_cursor(e, ParseContext::Expr)); break; }
        let value = parse_expr(p)?;
        entries.push(DictEntry { key, value, span: elo.merge(&value.span) });
        p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightBrace) { p.emit(crate::error::unclosed('{', open, None, p.span())); }
    let span    = lo.merge(&p.span());
    let entries = p.arena.alloc_slice_copy(entries.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::Dict(entries), span }))
}

fn parse_anon_object<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let open = p.span(); p.cursor.advance(); // `{`
    let mut fields: Vec<ObjectField<'ast>> = Vec::with_capacity(p.estimates.struct_fields);
    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let flo = p.span();
        let (name, _) = p.expect_ident()?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) { p.emit(crate::error::from_cursor(e, ParseContext::Expr)); break; }
        let value = parse_expr(p)?;
        fields.push(ObjectField { name, value, span: flo.merge(&value.span) });
        p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightBrace) { p.emit(crate::error::unclosed('{', open, None, p.span())); }
    let span   = lo.merge(&p.span());
    let fields = p.arena.alloc_slice_copy(fields.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::AnonObject(fields), span }))
}

// ── Paren / tuple ─────────────────────────────────────────────────────────────

fn parse_paren_or_tuple<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let open = p.span(); p.cursor.advance(); // `(`
    if p.cursor.eat(&TokenType::RightParen) {
        let span = lo.merge(&p.span());
        return Some(p.alloc(Expr { kind: ExprKind::Tuple(&[]), span }));
    }
    let first = parse_expr(p)?;
    if p.cursor.eat(&TokenType::RightParen) { return Some(first); } // grouped
    let mut elems: Vec<&'ast Expr<'ast>> = Vec::with_capacity(4);
    elems.push(first);
    while p.cursor.eat(&TokenType::Comma) {
        if p.cursor.is_at(&TokenType::RightParen) { break; }
        elems.push(parse_expr(p)?);
    }
    if !p.cursor.eat(&TokenType::RightParen) { p.emit(crate::error::unclosed('(', open, None, p.span())); }
    let span  = lo.merge(&p.span());
    let elems = p.arena.alloc_slice_copy(elems.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::Tuple(elems), span }))
}

// ── Array literal ─────────────────────────────────────────────────────────────

fn parse_array_lit<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let open = p.span(); p.cursor.advance(); // `[`
    let mut elems: Vec<&'ast Expr<'ast>> = Vec::with_capacity(p.estimates.call_args);
    while !p.cursor.is_at(&TokenType::RightBracket) && !p.cursor.is_eof() {
        elems.push(parse_expr(p)?); p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightBracket) { p.emit(crate::error::unclosed('[', open, None, p.span())); }
    let span  = lo.merge(&p.span());
    let elems = p.arena.alloc_slice_copy(elems.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::Array(elems), span }))
}

// ── If expression ─────────────────────────────────────────────────────────────

fn parse_if_expr<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    p.cursor.advance(); // `if`
    let condition = parse_expr(p)?;
    let then_body = crate::parsers::parse_stmt::parse_if_branch_body(p)?;
    let mut elif_branches: Vec<ElifBranch<'ast>> = Vec::with_capacity(2);
    while p.cursor.eat(&TokenType::Elif) {
        let blo  = p.span();
        let cond = parse_expr(p)?;
        let body = crate::parsers::parse_stmt::parse_if_branch_body(p)?;
        let bspan = match &body {
            IfBranchBody::Block(b) => b.span,
            IfBranchBody::Expr(e)  => e.span,
        };
        elif_branches.push(ElifBranch { condition: cond, body, span: blo.merge(&bspan) });
    }
    let else_body = if p.cursor.eat(&TokenType::Else) {
        Some(crate::parsers::parse_stmt::parse_if_branch_body(p)?)
    } else { None };
    let span          = lo.merge(&p.span());
    let elif_branches = p.arena.alloc_slice_copy(elif_branches.as_slice());
    let node          = p.alloc(IfExpr { condition, then_body, elif_branches, else_body, span });
    Some(p.alloc(Expr { kind: ExprKind::If(node), span }))
}

// ── Match expression ──────────────────────────────────────────────────────────

fn parse_match_expr<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let prev = p.enter(ParseContext::MatchArm);
    p.cursor.advance(); // `match`
    let scrutinee = parse_expr(p)?;
    let open = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftBrace) { p.emit(crate::error::from_cursor(e, ParseContext::MatchArm)); p.leave(prev); return None; }
    let mut arms: Vec<MatchArm<'ast>> = Vec::with_capacity(p.estimates.match_arms);
    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let pos_before = p.cursor.position();
        if let Some(arm) = parse_match_arm(p) { arms.push(arm); } else { p.recover_to_stmt(); }
        p.eat_sep();
        p.guard_progress(pos_before);
    }
    if !p.cursor.eat(&TokenType::RightBrace) { p.emit(crate::error::unclosed('{', open, None, p.span())); }
    let span = lo.merge(&p.span());
    let arms = p.arena.alloc_slice_copy(arms.as_slice());
    let node = p.alloc(MatchExpr { scrutinee, arms, span });
    p.leave(prev);
    Some(p.alloc(Expr { kind: ExprKind::Match(node), span }))
}

fn parse_match_arm<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<MatchArm<'ast>> {
    let lo      = p.span();
    let pattern = crate::parsers::parse_pattern::parse_pattern(p)?;
    let guard   = if p.cursor.eat(&TokenType::Where) { Some(parse_expr(p)?) } else { None };
    // `then` is an accepted alternate spelling for `=>` here — purely
    // stylistic, since match arms already support brace-free single
    // expressions via `=>` (`Some(x) => x`); `then` doesn't change the
    // body-parsing rules below, just the separator token.
    if !p.cursor.eat(&TokenType::FatArrow) && !p.cursor.eat(&TokenType::Then) {
        let found = p.cursor.peek_token();
        p.emit(crate::error::unexpected(found, &["'=>'", "'then'"], ParseContext::MatchArm));
        return None;
    }
    let body = crate::parsers::parse_stmt::parse_match_arm_body(p)?;
    Some(MatchArm { pattern, guard, body, span: lo.merge(&p.span()) })
}

// ── Lambda ────────────────────────────────────────────────────────────────────

fn parse_lambda<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    p.cursor.advance(); // `fn`
    let open = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftParen) { p.emit(crate::error::from_cursor(e, ParseContext::FunctionParam)); return None; }
    let mut params: Vec<LambdaParam<'ast>> = Vec::with_capacity(p.estimates.fn_params);
    while !p.cursor.is_at(&TokenType::RightParen) && !p.cursor.is_eof() {
        let plo = p.span();
        let _   = p.cursor.eat(&TokenType::Mut);
        let (name, _) = p.expect_ident()?;
        let ty  = crate::parsers::parse_decl::parse_type_annotation_opt(p);
        params.push(LambdaParam { name, ty, span: plo.merge(&p.span()) }); p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightParen) { p.emit(crate::error::unclosed('(', open, None, p.span())); }
    let body = if p.cursor.is_at(&TokenType::LeftBrace) {
        LambdaBody::Block(crate::parsers::parse_stmt::parse_block_inner(p)?)
    } else {
        LambdaBody::Expr(parse_expr(p)?)
    };
    let span   = lo.merge(&p.span());
    let params = p.arena.alloc_slice_copy(params.as_slice());
    let node   = p.alloc(Lambda { params, body, span });
    Some(p.alloc(Expr { kind: ExprKind::Lambda(node), span }))
}

// ── LINQ query ────────────────────────────────────────────────────────────────

fn parse_linq<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    if p.tier != TierAnnotation::High {
        p.emit(crate::error::illegal_here("LINQ", "LINQ is only valid in @tier(high)", lo, Some("use method chains in MID/LOW tier")));
    }
    let prev = p.enter(ParseContext::LinqQuery);
    p.cursor.advance(); // `from`
    let (binding, _) = p.expect_ident()?;
    if let Err(e) = p.cursor.expect(&TokenType::In) { p.emit(crate::error::from_cursor(e, ParseContext::LinqQuery)); p.leave(prev); return None; }
    let source = parse_expr(p)?;
    let mut clauses: Vec<LinqClause<'ast>> = Vec::with_capacity(p.estimates.linq_clauses);
    loop {
        match p.cursor.peek().clone() {
            TokenType::Where => { p.cursor.advance(); clauses.push(LinqClause::Where(parse_expr(p)?)); }
            TokenType::Let   => { p.cursor.advance(); let (n, _) = p.expect_ident()?; p.cursor.eat(&TokenType::Equal); clauses.push(LinqClause::Let { name: n, value: parse_expr(p)? }); }
            TokenType::Ident(ref kw) => match kw.as_str() {
                "orderby"  => { p.cursor.advance(); let e = parse_expr(p)?; let desc = if let TokenType::Ident(ref d) = p.cursor.peek().clone() { if d == "descending" { p.cursor.advance(); true } else if d == "ascending" { p.cursor.advance(); false } else { false } } else { false }; clauses.push(LinqClause::OrderBy { expr: e, descending: desc }); }
                "groupby"  => { p.cursor.advance(); clauses.push(LinqClause::GroupBy(parse_expr(p)?)); }
                "select"   => break,
                _          => break,
            },
            _ => break,
        }
    }
    let select = if let TokenType::Ident(kw) = p.cursor.peek().clone() {
        if kw == "select" { p.cursor.advance(); parse_expr(p)? } else { p.expected(&["'select'"]); p.leave(prev); return None; }
    } else { p.expected(&["'select'"]); p.leave(prev); return None; };
    let span    = lo.merge(&select.span);
    let clauses = p.arena.alloc_slice_copy(clauses.as_slice());
    let node    = p.alloc(LinqExpr { binding, source, clauses, select, span });
    p.leave(prev);
    Some(p.alloc(Expr { kind: ExprKind::Linq(node), span }))
}

// ── Interpolated string ───────────────────────────────────────────────────────

fn parse_interp<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, parts: Vec<LexPart>, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let mut ast_parts: Vec<InterpolationPart<'ast>> = Vec::with_capacity(parts.len());
    for part in &parts {
        match part {
            LexPart::Text(t) => ast_parts.push(InterpolationPart::Text(p.intern(t))),
            LexPart::Expr(tokens) => {
                // The lexer already tokenized this hole's own source range
                // (see string_parser.rs::parse_interpolation_expr) — sub-parse
                // those tokens into a real expression right now, the same way
                // everything else in the file gets parsed. Shares `p.arena` so
                // the result is valid for 'ast; the sub-parser's own cursor
                // and error manager are local to just this one hole.
                let mut sub = Parser::new(p.arena, tokens, String::new());
                match parse_expr(&mut sub) {
                    Some(expr) => ast_parts.push(InterpolationPart::Expr(expr)),
                    None => {
                        let sub_errors = sub.errors.take_parse_errors();
                        if sub_errors.is_empty() {
                            p.emit(crate::error::illegal_here(
                                "interpolation hole",
                                "expression could not be parsed",
                                lo,
                                None,
                            ));
                        } else {
                            for err in sub_errors {
                                p.emit(err);
                            }
                        }
                        return None;
                    }
                }
            }
        }
    }
    p.cursor.advance();
    let span  = lo.merge(&p.span());
    let parts = p.arena.alloc_slice_copy(ast_parts.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::InterpolatedStr(parts)), span }))
}

// ── Postfix helpers ───────────────────────────────────────────────────────────

fn parse_dot<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, target: &'ast Expr<'ast>, _op: LSpan, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let (field, fspan) = p.expect_ident()?;
    if p.cursor.is_at(&TokenType::LeftParen) {
        let callee = p.alloc(Expr { kind: ExprKind::Field { target, field }, span: lo.merge(&fspan) });
        parse_call(p, callee, p.span(), lo)
    } else {
        Some(p.alloc(Expr { kind: ExprKind::Field { target, field }, span: lo.merge(&fspan) }))
    }
}

fn parse_call<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, callee: &'ast Expr<'ast>, open_sp: LSpan, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    // `(` already consumed by caller OR we need to consume it here
    let open = if p.cursor.is_at(&TokenType::LeftParen) { p.cursor.advance().span } else { open_sp };
    let mut args: Vec<Arg<'ast>> = Vec::with_capacity(p.estimates.call_args);
    while !p.cursor.is_at(&TokenType::RightParen) && !p.cursor.is_eof() {
        args.push(parse_arg(p)?); p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightParen) { p.emit(crate::error::unclosed('(', open, None, p.span())); }
    let span = lo.merge(&p.span());
    let args = p.arena.alloc_slice_copy(args.as_slice());
    Some(p.alloc(Expr { kind: ExprKind::Call { callee, args }, span }))
}

fn parse_arg<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<Arg<'ast>> {
    let lo = p.span();
    if let TokenType::Ident(name) = p.cursor.peek().clone() {
        if matches!(p.cursor.peek_nth(1), TokenType::Equal) {
            let name = p.intern(&name); p.cursor.advance(); p.cursor.advance();
            let value = parse_expr(p)?;
            return Some(Arg { kind: ArgKind::Named { name, value }, span: lo.merge(&value.span) });
        }
    }
    let e = parse_expr(p)?;
    Some(Arg { kind: ArgKind::Positional(e), span: lo.merge(&e.span) })
}

fn parse_index<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, target: &'ast Expr<'ast>, open_sp: LSpan, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let index = parse_expr(p)?;
    if !p.cursor.eat(&TokenType::RightBracket) { p.emit(crate::error::unclosed('[', open_sp, None, p.span())); }
    let span = lo.merge(&p.span());
    Some(p.alloc(Expr { kind: ExprKind::Index { target, index }, span }))
}

fn parse_opt_chain<'ast, 'tok>(p: &mut Parser<'ast, 'tok>, target: &'ast Expr<'ast>, _op: LSpan, lo: LSpan) -> Option<&'ast Expr<'ast>> {
    let (name, _) = p.expect_ident()?;
    let access = if p.cursor.is_at(&TokenType::LeftParen) {
        let open = p.cursor.advance().span;
        let mut args: Vec<Arg<'ast>> = Vec::with_capacity(p.estimates.call_args);
        while !p.cursor.is_at(&TokenType::RightParen) && !p.cursor.is_eof() { args.push(parse_arg(p)?); p.eat_sep(); }
        if !p.cursor.eat(&TokenType::RightParen) { p.emit(crate::error::unclosed('(', open, None, p.span())); }
        OptionalAccess::Method { name, args: p.arena.alloc_slice_copy(args.as_slice()) }
    } else {
        OptionalAccess::Field(name)
    };
    let span = lo.merge(&p.span());
    Some(p.alloc(Expr { kind: ExprKind::OptionalChain { target, access }, span }))
}

// ── OrElse RHS ────────────────────────────────────────────────────────────────

fn parse_or_else_rhs<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<OrElseFallback<'ast>> {
    match p.cursor.peek().clone() {
        TokenType::Return   => { p.cursor.advance(); let v = if p.can_start_expr() { Some(parse_expr(p)?) } else { None }; Some(OrElseFallback::Return(v)) }
        TokenType::Break    => { p.cursor.advance(); Some(OrElseFallback::Break) }
        TokenType::Continue => { p.cursor.advance(); Some(OrElseFallback::Continue) }
        _                   => { Some(OrElseFallback::Expr(parse_expr(p)?)) }
    }
}

// ── Binding power / operator tables ──────────────────────────────────────────

#[inline]
fn infix_bp(tt: &TokenType) -> Option<(u8, u8)> {
    match tt {
        // Postfix
        TokenType::Dot | TokenType::LeftParen | TokenType::LeftBracket |
        TokenType::Question | TokenType::QuestionDot => Some((29, 30)),
        // Assignment — right-assoc (r_bp < l_bp)
        TokenType::Equal | TokenType::PlusEqual | TokenType::MinusEqual |
        TokenType::StarEqual | TokenType::SlashEqual | TokenType::PercentEqual |
        TokenType::AmpEqual | TokenType::PipeEqual | TokenType::CaretEqual |
        TokenType::LeftShiftEqual | TokenType::RightShiftEqual => Some((2, 1)),
        TokenType::DotDot | TokenType::DotDotEqual => Some((3, 4)),
        TokenType::Or  | TokenType::PipePipe  => Some((5, 6)),
        TokenType::And | TokenType::AmpAmp    => Some((7, 8)),
        TokenType::EqualEqual | TokenType::BangEqual => Some((9, 10)),
        TokenType::Less | TokenType::Greater | TokenType::LessEqual | TokenType::GreaterEqual => Some((11, 12)),
        TokenType::PipeArrow => Some((13, 14)),
        TokenType::Pipe      => Some((15, 16)),
        TokenType::Caret     => Some((17, 18)),
        TokenType::Amp       => Some((19, 20)),
        TokenType::LeftShift | TokenType::RightShift => Some((21, 22)),
        TokenType::Plus | TokenType::Minus => Some((23, 24)),
        TokenType::Star | TokenType::Slash | TokenType::Percent => Some((25, 26)),
        TokenType::As => Some((27, 28)),
        _ => None,
    }
}

fn to_bin_op(tt: &TokenType) -> Option<BinOp> {
    match tt {
        TokenType::Plus          => Some(BinOp::Add),   TokenType::Minus     => Some(BinOp::Sub),
        TokenType::Star          => Some(BinOp::Mul),   TokenType::Slash     => Some(BinOp::Div),
        TokenType::Percent       => Some(BinOp::Rem),   TokenType::Amp       => Some(BinOp::BitAnd),
        TokenType::Pipe          => Some(BinOp::BitOr), TokenType::Caret     => Some(BinOp::BitXor),
        TokenType::LeftShift     => Some(BinOp::Shl),   TokenType::RightShift => Some(BinOp::Shr),
        TokenType::EqualEqual    => Some(BinOp::Eq),    TokenType::BangEqual => Some(BinOp::Ne),
        TokenType::Less          => Some(BinOp::Lt),    TokenType::LessEqual => Some(BinOp::Le),
        TokenType::Greater       => Some(BinOp::Gt),    TokenType::GreaterEqual => Some(BinOp::Ge),
        TokenType::And | TokenType::AmpAmp  => Some(BinOp::And),
        TokenType::Or  | TokenType::PipePipe => Some(BinOp::Or),
        TokenType::DotDot        => Some(BinOp::Range), TokenType::DotDotEqual => Some(BinOp::RangeIncl),
        _ => None,
    }
}

fn to_assign_op(tt: &TokenType) -> Option<AssignOp> {
    match tt {
        TokenType::Equal           => Some(AssignOp::Assign),
        TokenType::PlusEqual       => Some(AssignOp::AddAssign),
        TokenType::MinusEqual      => Some(AssignOp::SubAssign),
        TokenType::StarEqual       => Some(AssignOp::MulAssign),
        TokenType::SlashEqual      => Some(AssignOp::DivAssign),
        TokenType::PercentEqual    => Some(AssignOp::RemAssign),
        TokenType::AmpEqual        => Some(AssignOp::BitAndAssign),
        TokenType::PipeEqual       => Some(AssignOp::BitOrAssign),
        TokenType::CaretEqual      => Some(AssignOp::BitXorAssign),
        TokenType::LeftShiftEqual  => Some(AssignOp::ShlAssign),
        TokenType::RightShiftEqual => Some(AssignOp::ShrAssign),
        _ => None,
    }
            }
