// crates/rd_parser/src/parsers/parse_expr.rs
//
// Expression parser — Pratt / Top-Down Operator Precedence.
//
// BINDING POWER TABLE (from PARSER_RULES.md):
//   Postfix  . () [] ? ?.     (29, 30)   left-assoc, handled specially
//   Assignment = += ...       ( 2,  1)   right-assoc
//   Range ..  ..=             ( 3,  4)   left-assoc
//   Or  or ||                 ( 5,  6)   left-assoc
//   And and &&                ( 7,  8)   left-assoc
//   Equality  == !=           ( 9, 10)   left-assoc (sema catches chaining)
//   Comparison < > <= >=      (11, 12)   left-assoc
//   Pipe |>                   (13, 14)   left-assoc
//   BitOr |                   (15, 16)   left-assoc
//   BitXor ^                  (17, 18)   left-assoc
//   BitAnd &                  (19, 20)   left-assoc
//   Shift << >>               (21, 22)   left-assoc
//   Add/Sub + -               (23, 24)   left-assoc
//   Mul/Div/Mod * / %         (25, 26)   left-assoc
//   Unary prefix (right-only) ( —, 27)

use ubel_stratum::{
    ast::{
        common::{AssignOp, BinOp, Span, UnaryOp},
        expressions::{
            Arg, ArgKind, DictEntry, ElifBranch, Expr, ExprKind, FieldInit,
            IfExpr, Lambda, LambdaBody, LambdaParam, LinqClause, LinqExpr,
            MatchArm, MatchArmBody, MatchExpr, ObjectField, OptionalAccess,
            OrElseFallback,
        },
        literals::Literal,
    },
    error_management::error_types::ParseContext,
    lexer::{Span as LexSpan, TokenType},
};

use crate::{
    keywords::{is_linq_keyword, is_primitive_type, is_collection_type},
    parser::Parser,
};

// ── Public entry points ───────────────────────────────────────────────────────

pub(crate) fn parse_expr<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<&'ast Expr<'ast>> {
    parse_expr_bp(p, 0)
}

pub(crate) fn parse_expr_entry<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<&'ast Expr<'ast>> {
    parse_expr(p)
}

// ── Pratt core ────────────────────────────────────────────────────────────────

fn parse_expr_bp<'ast, 'tok>(
    p:      &mut Parser<'ast, 'tok>,
    min_bp: u8,
) -> Option<&'ast Expr<'ast>> {
    let lo = p.span();

    // ── Prefix ───────────────────────────────────────────────────────────────
    let mut lhs = parse_prefix(p)?;

    // ── Infix / postfix loop ──────────────────────────────────────────────────
    loop {
        let op_tt = p.cursor.peek().clone();

        // ── `or return` / `or continue` / `or break` (OrElse) ─────────────
        // Distinct from logical `or Expr`. Detected by peeking at what follows.
        if matches!(op_tt, TokenType::Or) {
            let next = p.cursor.peek_nth(1);
            if matches!(next, TokenType::Return | TokenType::Break | TokenType::Continue) {
                if 5 < min_bp { break; }   // or BP = 5
                p.cursor.advance(); // consume `or`
                let fallback = parse_or_else_fallback(p)?;
                let span = lo.merge(&p.span());
                lhs = p.alloc(Expr { kind: ExprKind::OrElse { expr: lhs, fallback }, span });
                continue;
            }
        }

        // ── Struct literal postfix: `Name { field = val }` ─────────────────
        // Only valid after an Ident or dotted path (not after an arbitrary expr).
        if matches!(op_tt, TokenType::LeftBrace)
            && is_struct_lit_prefix(lhs)
            && is_struct_lit_open(p)
        {
            lhs = parse_struct_lit_body(p, lhs, lo)?;
            continue;
        }

        // ── Standard infix / postfix via BP table ───────────────────────────
        let (l_bp, r_bp) = match infix_bp(&op_tt) {
            Some(bp) => bp,
            None     => break,
        };
        if l_bp < min_bp { break; }

        let op_span = p.cursor.current_span();
        p.cursor.advance(); // consume the operator

        // ── Postfix-style (no RHS recursion) ───────────────────────────────
        match &op_tt {
            TokenType::Dot => {
                lhs = parse_field_or_method(p, lhs, op_span, lo)?;
                continue;
            }
            TokenType::LeftParen => {
                lhs = parse_call(p, lhs, op_span, lo)?;
                continue;
            }
            TokenType::LeftBracket => {
                lhs = parse_index(p, lhs, op_span, lo)?;
                continue;
            }
            TokenType::Question => {
                let span = lo.merge(&op_span);
                lhs = p.alloc(Expr { kind: ExprKind::Try(lhs), span });
                continue;
            }
            TokenType::QuestionDot => {
                lhs = parse_optional_chain(p, lhs, op_span, lo)?;
                continue;
            }
            _ => {}
        }

        // ── Assignment (right-assoc) ────────────────────────────────────────
        if let Some(assign_op) = to_assign_op(&op_tt) {
            let value = parse_expr_bp(p, r_bp)?;
            let span  = lo.merge(&value.span);
            lhs = p.alloc(Expr {
                kind: ExprKind::Assign { op: assign_op, target: lhs, value },
                span,
            });
            continue;
        }

        // ── Pipe operator ────────────────────────────────────────────────────
        if matches!(op_tt, TokenType::PipeArrow) {
            let right = parse_expr_bp(p, r_bp)?;
            let span  = lo.merge(&right.span);
            lhs = p.alloc(Expr { kind: ExprKind::Pipe { left: lhs, right }, span });
            continue;
        }

        // ── Standard binary operators ────────────────────────────────────────
        if let Some(bin_op) = to_bin_op(&op_tt) {
            let rhs  = parse_expr_bp(p, r_bp)?;
            let span = lo.merge(&rhs.span);
            lhs = p.alloc(Expr { kind: ExprKind::BinOp { op: bin_op, lhs, rhs }, span });
            continue;
        }

        break; // unhandled op — shouldn't reach here if infix_bp is correct
    }

    Some(lhs)
}

// ── Prefix parser ─────────────────────────────────────────────────────────────

fn parse_prefix<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<&'ast Expr<'ast>> {
    let lo = p.span();

    match p.cursor.peek().clone() {

        // ── Literals ──────────────────────────────────────────────────────────
        TokenType::IntLit(n) => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Int(n)), span: lo }))
        }
        TokenType::FloatLit(f) => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Float(f)), span: lo }))
        }
        TokenType::StringLit(s) => {
            let s = p.intern(&s);
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Str(s)), span: lo }))
        }
        TokenType::VerbatimString(s) => {
            let s = p.intern(&s);
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::VerbatimStr(s)), span: lo }))
        }
        TokenType::CharLit(c) => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Char(c)), span: lo }))
        }
        TokenType::True => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(true)), span: lo }))
        }
        TokenType::False => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(false)), span: lo }))
        }
        TokenType::Null => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::Null), span: lo }))
        }
        TokenType::InterpolatedString(parts) => {
            parse_interpolated_string(p, parts, lo)
        }

        // ── `self` ────────────────────────────────────────────────────────────
        TokenType::SelfKw => {
            p.cursor.advance();
            Some(p.alloc(Expr { kind: ExprKind::SelfExpr, span: lo }))
        }

        // ── Identifier / path / struct literal / LINQ ─────────────────────────
        TokenType::Ident(name) => {
            parse_ident_expr(p, name, lo)
        }

        // ── Unary prefix operators ────────────────────────────────────────────
        TokenType::Minus => {
            p.cursor.advance();
            let operand = parse_expr_bp(p, 27)?;
            let span    = lo.merge(&operand.span);
            Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Neg, operand }, span }))
        }
        TokenType::Bang => {
            p.cursor.advance();
            let operand = parse_expr_bp(p, 27)?;
            let span    = lo.merge(&operand.span);
            Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Not, operand }, span }))
        }
        TokenType::Not => {
            p.cursor.advance();
            let operand = parse_expr_bp(p, 27)?;
            let span    = lo.merge(&operand.span);
            Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::Not, operand }, span }))
        }
        TokenType::Tilde => {
            p.cursor.advance();
            let operand = parse_expr_bp(p, 27)?;
            let span    = lo.merge(&operand.span);
            Some(p.alloc(Expr { kind: ExprKind::UnaryOp { op: UnaryOp::BitNot, operand }, span }))
        }

        // ── `await expr` ──────────────────────────────────────────────────────
        TokenType::Await => {
            if p.tier != ubel_stratum::ast::common::TierAnnotation::High {
                p.emit(crate::error::illegal_here(
                    "await",
                    "await is only valid in @tier(high) functions",
                    lo,
                    Some("remove @tier(mid)/@tier(low), or use a callback pattern"),
                ));
            }
            p.cursor.advance();
            let operand = parse_expr_bp(p, 27)?;
            let span    = lo.merge(&operand.span);
            Some(p.alloc(Expr { kind: ExprKind::Await(operand), span }))
        }

        // ── Grouping or tuple: `(...)` ────────────────────────────────────────
        TokenType::LeftParen => {
            parse_paren_or_tuple(p, lo)
        }

        // ── Array literal: `[...]` ────────────────────────────────────────────
        TokenType::LeftBracket => {
            parse_array_literal(p, lo)
        }

        // ── Block / anonymous object: `{...}` ─────────────────────────────────
        TokenType::LeftBrace => {
            if is_anon_object_open(p) {
                parse_anon_object(p, lo)
            } else {
                let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
                let block_ref = p.alloc(block);
                let span = block.span;
                Some(p.alloc(Expr { kind: ExprKind::Block(block_ref), span }))
            }
        }

        // ── If expression ─────────────────────────────────────────────────────
        TokenType::If => {
            parse_if_expr(p, lo)
        }

        // ── Match expression ──────────────────────────────────────────────────
        TokenType::Match => {
            parse_match_expr(p, lo)
        }

        // ── LINQ query: `from Ident in Expr ...` ──────────────────────────────
        TokenType::From => {
            if matches!(p.cursor.peek_nth(1), TokenType::Ident(_))
               && matches!(p.cursor.peek_nth(2), TokenType::In)
            {
                parse_linq_query(p, lo)
            } else {
                p.expected(&["identifier after 'from' for LINQ query"]);
                None
            }
        }

        // ── Lambda: `fn(params) body` ─────────────────────────────────────────
        TokenType::Fn => {
            parse_lambda(p, lo)
        }

        // ── Unsafe block ──────────────────────────────────────────────────────
        TokenType::Unsafe => {
            p.cursor.advance();
            let block    = crate::parsers::parse_stmt::parse_block_inner(p)?;
            let block_ref = p.alloc(block);
            let span     = lo.merge(&block.span);
            Some(p.alloc(Expr { kind: ExprKind::Block(block_ref), span }))
        }

        _ => {
            p.expected(&["expression"]);
            None
        }
    }
}

// ── Identifier / path expression ──────────────────────────────────────────────

fn parse_ident_expr<'ast, 'tok>(
    p:    &mut Parser<'ast, 'tok>,
    name: String,
    lo:   LexSpan,
) -> Option<&'ast Expr<'ast>> {
    // Fast-path: is this name a LINQ keyword used as an identifier? Unusual,
    // but emit a warning and treat as ident.
    let name = p.intern(&name);
    p.cursor.advance();

    // Build path: `Ident` or `Ident.Ident.Ident`
    let mut path: Vec<&'ast str> = p.bump_vec_cap(p.estimates.path_segs);
    path.push(name);
    let mut hi = lo;

    while p.cursor.eat(&TokenType::Dot) {
        // Peek ahead: if next is `Ident` AND what follows is NOT `(`, it might
        // be a field/method chain that the postfix loop should handle.
        // But here we only collect a STATIC path for struct literals.
        // Method call chains are left to the postfix loop.
        // So: consume identifiers that look like module/type path segments,
        // stopping if the ident is followed by `(` (method call).
        if let TokenType::Ident(seg) = p.cursor.peek().clone() {
            let next_next = p.cursor.peek_nth(1);
            if matches!(next_next, TokenType::LeftParen) {
                // `seg(` → this is a method call; let postfix handle it.
                // But we already consumed `.`! Restore that `.` by going back.
                // Since we already advanced past `.`, put the path through the postfix handler:
                // Build a Field expr and let the main loop handle the `(`.
                let seg = p.intern(&seg);
                p.cursor.advance();
                hi = p.span();
                let current_path = path.into_bump_slice();
                let base = if current_path.len() == 1 {
                    p.alloc(Expr { kind: ExprKind::Ident(current_path[0]), span: lo })
                } else {
                    build_path_expr(p, current_path, lo)
                };
                // Build field access then return (postfix loop handles `(` next)
                let span = lo.merge(&hi);
                return Some(p.alloc(Expr {
                    kind: ExprKind::Field { target: base, field: seg },
                    span,
                }));
            }
            let seg = p.intern(&seg);
            p.cursor.advance();
            hi = p.span();
            path.push(seg);
        } else {
            // `.` not followed by ident — error, but produce what we have
            p.expected(&["identifier after '.'"]);
            break;
        }
    }

    hi = p.cursor.current_span();
    let path_slice = path.into_bump_slice();
    Some(build_path_expr(p, path_slice, lo.merge(&hi)))
}

fn build_path_expr<'ast>(
    p:    &Parser<'ast, '_>,
    path: &'ast [&'ast str],
    span: LexSpan,
) -> &'ast Expr<'ast> {
    if path.len() == 1 {
        p.alloc(Expr { kind: ExprKind::Ident(path[0]), span })
    } else {
        // Multi-segment: build a chain of Field accesses
        // `a.b.c` → Field(Field(Ident("a"), "b"), "c")
        let mut expr = p.alloc(Expr { kind: ExprKind::Ident(path[0]), span });
        for seg in &path[1..] {
            expr = p.alloc(Expr { kind: ExprKind::Field { target: expr, field: seg }, span });
        }
        expr
    }
}

// ── Struct literal ────────────────────────────────────────────────────────────

/// Returns true if `lhs` looks like a type name path (Ident or Field chain).
fn is_struct_lit_prefix(lhs: &Expr<'_>) -> bool {
    match lhs.kind {
        ExprKind::Ident(_) => true,
        ExprKind::Field { .. } => true,
        _ => false,
    }
}

/// 2-token lookahead: `{ Ident =` or `{ }` → struct literal.
/// Otherwise → block expression or something else.
fn is_struct_lit_open(p: &Parser<'_, '_>) -> bool {
    debug_assert!(matches!(p.cursor.peek(), TokenType::LeftBrace));
    // Peek inside: `{` `Ident` `=` → struct literal
    // `{` `}` → empty struct literal
    let t1 = p.cursor.peek_nth(1);
    let t2 = p.cursor.peek_nth(2);
    match (t1, t2) {
        (TokenType::RightBrace, _)           => true, // `Name {}` empty struct
        (TokenType::Ident(_), TokenType::Equal) => true, // `Name { field = ...`
        _ => false,
    }
}

fn parse_struct_lit_body<'ast, 'tok>(
    p:   &mut Parser<'ast, 'tok>,
    lhs: &'ast Expr<'ast>,
    lo:  LexSpan,
) -> Option<&'ast Expr<'ast>> {
    // Extract path from lhs (Ident or Field chain)
    let path: &'ast [&'ast str] = extract_path(p, lhs);

    let open_span = p.span();
    p.cursor.advance(); // consume `{`

    let mut fields: Vec<FieldInit<'ast>> =
        Vec::with_capacity(p.estimates.struct_fields);

    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let flo = p.span();
        let (name, _) = p.expect_ident()?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) {
            p.emit(crate::error::from_cursor(e, ParseContext::Expr));
            break;
        }
        let value = parse_expr(p)?;
        let span  = flo.merge(&value.span);
        fields.push(FieldInit { name, value, span });
        p.eat_sep();
    }

    if !p.cursor.eat(&TokenType::RightBrace) {
        let at = p.span();
        p.emit(crate::error::unclosed('{', open_span, None, at));
    }

    let span   = lo.merge(&p.span());
    let fields = p.arena.alloc_slice_clone(&fields);
    Some(p.alloc(Expr { kind: ExprKind::StructLit { path, fields }, span }))
}

fn extract_path<'ast>(p: &Parser<'ast, '_>, expr: &'ast Expr<'ast>) -> &'ast [&'ast str] {
    let mut segs: Vec<&'ast str> = Vec::with_capacity(p.estimates.path_segs);
    collect_path_segs(expr, &mut segs);
    p.arena.alloc_slice_clone(&segs)
}

fn collect_path_segs<'ast>(expr: &'ast Expr<'ast>, segs: &mut Vec<&'ast str>) {
    match expr.kind {
        ExprKind::Ident(name) => segs.push(name),
        ExprKind::Field { target, field } => {
            collect_path_segs(target, segs);
            segs.push(field);
        }
        _ => {} // shouldn't happen if is_struct_lit_prefix was correct
    }
}

// ── Anonymous object `{ field = value }` ─────────────────────────────────────

/// 2-token lookahead: `{` `Ident` `=` → anonymous object (no preceding name).
fn is_anon_object_open(p: &Parser<'_, '_>) -> bool {
    debug_assert!(matches!(p.cursor.peek(), TokenType::LeftBrace));
    let t1 = p.cursor.peek_nth(1);
    let t2 = p.cursor.peek_nth(2);
    match (t1, t2) {
        (TokenType::Ident(_), TokenType::Equal) => true,
        _ => false,
    }
}

fn parse_anon_object<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let open_span = p.span();
    p.cursor.advance(); // `{`
    let mut fields: Vec<ObjectField<'ast>> =
        Vec::with_capacity(p.estimates.struct_fields);

    while !p.cursor.is_at(&TokenType::RightBrace) && !p.cursor.is_eof() {
        let flo  = p.span();
        let (name, _) = p.expect_ident()?;
        if let Err(e) = p.cursor.expect(&TokenType::Equal) {
            p.emit(crate::error::from_cursor(e, ParseContext::Expr));
            break;
        }
        let value = parse_expr(p)?;
        let span  = flo.merge(&value.span);
        fields.push(ObjectField { name, value, span });
        p.eat_sep();
    }

    if !p.cursor.eat(&TokenType::RightBrace) {
        let at = p.span();
        p.emit(crate::error::unclosed('{', open_span, None, at));
    }
    let span   = lo.merge(&p.span());
    let fields = p.arena.alloc_slice_clone(&fields);
    Some(p.alloc(Expr { kind: ExprKind::AnonObject(fields), span }))
}

// ── Parenthesised expression or tuple ────────────────────────────────────────

fn parse_paren_or_tuple<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let open_span = p.span();
    p.cursor.advance(); // `(`

    if p.cursor.eat(&TokenType::RightParen) {
        // `()` — empty tuple
        let span  = lo.merge(&p.span());
        let empty: &'ast [&'ast Expr<'ast>] = &[];
        return Some(p.alloc(Expr { kind: ExprKind::Tuple(empty), span }));
    }

    let first = parse_expr(p)?;

    if p.cursor.eat(&TokenType::RightParen) {
        // Single element `(expr)` → grouped expression, return inner
        return Some(first);
    }

    // Must be a tuple: at least one comma required
    let mut elems: Vec<&'ast Expr<'ast>> =
        Vec::with_capacity(p.estimates.fn_params);
    elems.push(first);

    while p.cursor.eat(&TokenType::Comma) {
        if p.cursor.is_at(&TokenType::RightParen) { break; }
        elems.push(parse_expr(p)?);
    }

    if !p.cursor.eat(&TokenType::RightParen) {
        let at = p.span();
        p.emit(crate::error::unclosed('(', open_span, None, at));
    }

    let span  = lo.merge(&p.span());
    let elems = p.arena.alloc_slice_clone(&elems);
    Some(p.alloc(Expr { kind: ExprKind::Tuple(elems), span }))
}

// ── Array literal ─────────────────────────────────────────────────────────────

fn parse_array_literal<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let open_span = p.span();
    p.cursor.advance(); // `[`
    let mut elems: Vec<&'ast Expr<'ast>> =
        Vec::with_capacity(p.estimates.call_args);

    while !p.cursor.is_at(&TokenType::RightBracket) && !p.cursor.is_eof() {
        elems.push(parse_expr(p)?);
        p.eat_sep();
    }

    if !p.cursor.eat(&TokenType::RightBracket) {
        let at = p.span();
        p.emit(crate::error::unclosed('[', open_span, None, at));
    }
    let span  = lo.merge(&p.span());
    let elems = p.arena.alloc_slice_clone(&elems);
    Some(p.alloc(Expr { kind: ExprKind::Array(elems), span }))
}

// ── Postfix: field access / method call ───────────────────────────────────────

fn parse_field_or_method<'ast, 'tok>(
    p:       &mut Parser<'ast, 'tok>,
    target:  &'ast Expr<'ast>,
    op_span: LexSpan,
    lo:      LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let (field, field_span) = p.expect_ident()?;

    if p.cursor.is_at(&TokenType::LeftParen) {
        // Method call: `target.method(args)`
        let args     = parse_arg_list(p)?;
        let span     = lo.merge(&p.span());
        let callee   = p.alloc(Expr {
            kind: ExprKind::Field { target, field },
            span: lo.merge(&field_span),
        });
        Some(p.alloc(Expr { kind: ExprKind::Call { callee, args }, span }))
    } else {
        // Field access: `target.field`
        let span = lo.merge(&field_span);
        Some(p.alloc(Expr { kind: ExprKind::Field { target, field }, span }))
    }
}

// ── Postfix: call ─────────────────────────────────────────────────────────────

fn parse_call<'ast, 'tok>(
    p:       &mut Parser<'ast, 'tok>,
    callee:  &'ast Expr<'ast>,
    op_span: LexSpan,
    lo:      LexSpan,
) -> Option<&'ast Expr<'ast>> {
    // `(` was already consumed by the caller
    let args = parse_arg_list_after_paren(p, op_span)?;
    let span = lo.merge(&p.span());
    Some(p.alloc(Expr { kind: ExprKind::Call { callee, args }, span }))
}

/// Parse `( arg, arg )` — the `(` has already been consumed.
fn parse_arg_list_after_paren<'ast, 'tok>(
    p:         &mut Parser<'ast, 'tok>,
    open_span: LexSpan,
) -> Option<&'ast [Arg<'ast>]> {
    let mut args: Vec<Arg<'ast>> = Vec::with_capacity(p.estimates.call_args);

    while !p.cursor.is_at(&TokenType::RightParen) && !p.cursor.is_eof() {
        args.push(parse_arg(p)?);
        p.eat_sep();
    }

    if !p.cursor.eat(&TokenType::RightParen) {
        let at = p.span();
        p.emit(crate::error::unclosed('(', open_span, None, at));
    }
    Some(p.arena.alloc_slice_clone(&args))
}

/// Parse `( arg, arg )` — the `(` has NOT been consumed.
fn parse_arg_list<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<&'ast [Arg<'ast>]> {
    let open_span = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftParen) {
        p.emit(crate::error::from_cursor(e, ParseContext::Expr));
        return None;
    }
    parse_arg_list_after_paren(p, open_span)
}

fn parse_arg<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<Arg<'ast>> {
    let lo = p.span();
    // Check for named arg: `name = expr`
    if let TokenType::Ident(name) = p.cursor.peek().clone() {
        if matches!(p.cursor.peek_nth(1), TokenType::Equal) {
            let name = p.intern(&name);
            p.cursor.advance(); // ident
            p.cursor.advance(); // =
            let value = parse_expr(p)?;
            let span  = lo.merge(&value.span);
            return Some(Arg { kind: ArgKind::Named { name, value }, span });
        }
    }
    let expr = parse_expr(p)?;
    let span = lo.merge(&expr.span);
    Some(Arg { kind: ArgKind::Positional(expr), span })
}

// ── Postfix: index ────────────────────────────────────────────────────────────

fn parse_index<'ast, 'tok>(
    p:       &mut Parser<'ast, 'tok>,
    target:  &'ast Expr<'ast>,
    op_span: LexSpan,
    lo:      LexSpan,
) -> Option<&'ast Expr<'ast>> {
    // `[` already consumed
    let index = parse_expr(p)?;
    if !p.cursor.eat(&TokenType::RightBracket) {
        let at = p.span();
        p.emit(crate::error::unclosed('[', op_span, None, at));
    }
    let span = lo.merge(&p.span());
    Some(p.alloc(Expr { kind: ExprKind::Index { target, index }, span }))
}

// ── Postfix: optional chain `?.` ─────────────────────────────────────────────

fn parse_optional_chain<'ast, 'tok>(
    p:       &mut Parser<'ast, 'tok>,
    target:  &'ast Expr<'ast>,
    op_span: LexSpan,
    lo:      LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let (name, name_span) = p.expect_ident()?;
    let access = if p.cursor.is_at(&TokenType::LeftParen) {
        let args = parse_arg_list(p)?;
        OptionalAccess::Method { name, args }
    } else {
        OptionalAccess::Field(name)
    };
    let span = lo.merge(&p.span());
    Some(p.alloc(Expr { kind: ExprKind::OptionalChain { target, access }, span }))
}

// ── If expression ─────────────────────────────────────────────────────────────

fn parse_if_expr<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let prev = p.enter(ParseContext::Statement);
    p.cursor.advance(); // `if`

    let condition  = parse_expr(p)?;
    let then_block = crate::parsers::parse_stmt::parse_block_inner(p)?;

    let mut elif_branches: Vec<ElifBranch<'ast>> = Vec::with_capacity(2);
    while p.cursor.eat(&TokenType::Elif) {
        let elif_lo  = p.span();
        let cond     = parse_expr(p)?;
        let block    = crate::parsers::parse_stmt::parse_block_inner(p)?;
        let span     = elif_lo.merge(&block.span);
        elif_branches.push(ElifBranch { condition: cond, block, span });
    }

    let else_block = if p.cursor.eat(&TokenType::Else) {
        Some(crate::parsers::parse_stmt::parse_block_inner(p)?)
    } else { None };

    let span       = lo.merge(&p.span());
    let elif_branches = p.arena.alloc_slice_clone(&elif_branches);
    let if_node    = p.alloc(IfExpr { condition, then_block, elif_branches, else_block, span });

    p.leave(prev);
    Some(p.alloc(Expr { kind: ExprKind::If(if_node), span }))
}

// ── Match expression ──────────────────────────────────────────────────────────

fn parse_match_expr<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    let prev = p.enter(ParseContext::MatchArm);
    p.cursor.advance(); // `match`

    let scrutinee = parse_expr(p)?;
    let open_span = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftBrace) {
        p.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
        p.leave(prev);
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

    let span      = lo.merge(&p.span());
    let arms      = p.arena.alloc_slice_clone(&arms);
    let match_node = p.alloc(MatchExpr { scrutinee, arms, span });
    p.leave(prev);
    Some(p.alloc(Expr { kind: ExprKind::Match(match_node), span }))
}

fn parse_match_arm<'ast, 'tok>(p: &mut Parser<'ast, 'tok>) -> Option<MatchArm<'ast>> {
    let lo      = p.span();
    let pattern = crate::parsers::parse_pattern::parse_pattern(p)?;

    let guard = if p.cursor.eat(&TokenType::Where) {
        Some(parse_expr(p)?)
    } else { None };

    if let Err(e) = p.cursor.expect(&TokenType::FatArrow) {
        p.emit(crate::error::from_cursor(e, ParseContext::MatchArm));
        return None;
    }

    let body = if p.cursor.is_at(&TokenType::LeftBrace) {
        let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
        MatchArmBody::Block(block)
    } else {
        MatchArmBody::Expr(parse_expr(p)?)
    };

    let span = lo.merge(&p.span());
    Some(MatchArm { pattern, guard, body, span })
}

// ── Lambda ────────────────────────────────────────────────────────────────────

fn parse_lambda<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    p.cursor.advance(); // `fn`

    // Single-param shorthand: `fn x expr`
    if let TokenType::Ident(name) = p.cursor.peek().clone() {
        if !matches!(p.cursor.peek_nth(1), TokenType::LeftParen | TokenType::Colon) {
            let name  = p.intern(&name);
            let pspan = p.span();
            p.cursor.advance();
            let param = LambdaParam { name, ty: None, span: pspan };
            let body_expr = parse_expr(p)?;
            let span  = lo.merge(&body_expr.span);
            let params = p.arena.alloc_slice_clone(&[param]);
            let node  = p.alloc(Lambda {
                params,
                body: LambdaBody::Expr(body_expr),
                span,
            });
            return Some(p.alloc(Expr { kind: ExprKind::Lambda(node), span }));
        }
    }

    // Full form: `fn(params) body`
    let open_span = p.span();
    if let Err(e) = p.cursor.expect(&TokenType::LeftParen) {
        p.emit(crate::error::from_cursor(e, ParseContext::FunctionParam));
        return None;
    }

    let mut params: Vec<LambdaParam<'ast>> = Vec::with_capacity(p.estimates.fn_params);
    while !p.cursor.is_at(&TokenType::RightParen) && !p.cursor.is_eof() {
        let plo   = p.span();
        let _mut  = p.cursor.eat(&TokenType::Mut);
        let (name, _) = p.expect_ident()?;
        let ty    = crate::parsers::parse_decl::parse_type_annotation_opt(p);
        params.push(LambdaParam { name, ty, span: plo.merge(&p.span()) });
        p.eat_sep();
    }
    if !p.cursor.eat(&TokenType::RightParen) {
        p.emit(crate::error::unclosed('(', open_span, None, p.span()));
    }

    let body = if p.cursor.is_at(&TokenType::LeftBrace) {
        let block = crate::parsers::parse_stmt::parse_block_inner(p)?;
        LambdaBody::Block(block)
    } else {
        LambdaBody::Expr(parse_expr(p)?)
    };

    let span   = lo.merge(&p.span());
    let params = p.arena.alloc_slice_clone(&params);
    let node   = p.alloc(Lambda { params, body, span });
    Some(p.alloc(Expr { kind: ExprKind::Lambda(node), span }))
}

// ── LINQ query ────────────────────────────────────────────────────────────────

fn parse_linq_query<'ast, 'tok>(
    p:  &mut Parser<'ast, 'tok>,
    lo: LexSpan,
) -> Option<&'ast Expr<'ast>> {
    if p.tier != ubel_stratum::ast::common::TierAnnotation::High {
        p.emit(crate::error::illegal_here(
            "LINQ query",
            "LINQ is only valid in @tier(high) functions",
            lo,
            Some("use method chains instead in MID/LOW tier"),
        ));
    }

    let prev = p.enter(ParseContext::LinqQuery);
    p.cursor.advance(); // `from`

    let (binding, _) = p.expect_ident()?;
    if let Err(e) = p.cursor.expect(&TokenType::In) {
        p.emit(crate::error::from_cursor(e, ParseContext::LinqQuery));
        p.leave(prev);
        return None;
    }
    let source = parse_expr(p)?;

    let mut clauses: Vec<LinqClause<'ast>> = Vec::with_capacity(p.estimates.linq_clauses);

    // Parse clauses: where, orderby, groupby, let
    loop {
        match p.cursor.peek().clone() {
            TokenType::Where => {
                p.cursor.advance();
                let cond = parse_expr(p)?;
                clauses.push(LinqClause::Where(cond));
            }
            TokenType::Let => {
                p.cursor.advance();
                let (name, _) = p.expect_ident()?;
                if let Err(e) = p.cursor.expect(&TokenType::Equal) {
                    p.emit(crate::error::from_cursor(e, ParseContext::LinqQuery));
                    break;
                }
                let value = parse_expr(p)?;
                clauses.push(LinqClause::Let { name, value });
            }
            TokenType::Ident(kw) => {
                match kw.as_str() {
                    "orderby" => {
                        p.cursor.advance();
                        let expr = parse_expr(p)?;
                        let descending = if let TokenType::Ident(dir) = p.cursor.peek() {
                            if dir == "descending" { p.cursor.advance(); true }
                            else if dir == "ascending" { p.cursor.advance(); false }
                            else { false }
                        } else { false };
                        clauses.push(LinqClause::OrderBy { expr, descending });
                    }
                    "groupby" => {
                        p.cursor.advance();
                        let expr = parse_expr(p)?;
                        clauses.push(LinqClause::GroupBy(expr));
                    }
                    "select" => break, // handled below
                    _ => break,        // unknown keyword, stop clause parsing
                }
            }
            _ => break,
        }
    }

    // `select expr` — required
    let select = if let TokenType::Ident(kw) = p.cursor.peek().clone() {
        if kw == "select" {
            p.cursor.advance();
            parse_expr(p)?
        } else {
            p.emit(crate::error::raw(
                "LINQ query must end with 'select expr'",
                p.span(),
            ));
            p.leave(prev);
            return None;
        }
    } else {
        p.expected(&["'select'"]);
        p.leave(prev);
        return None;
    };

    let span    = lo.merge(&select.span);
    let clauses = p.arena.alloc_slice_clone(&clauses);
    let node    = p.alloc(LinqExpr { binding, source, clauses, select, span });
    p.leave(prev);
    Some(p.alloc(Expr { kind: ExprKind::Linq(node), span }))
}

// ── OrElse fallback ───────────────────────────────────────────────────────────

fn parse_or_else_fallback<'ast, 'tok>(
    p: &mut Parser<'ast, 'tok>,
) -> Option<OrElseFallback<'ast>> {
    match p.cursor.peek().clone() {
        TokenType::Return => {
            p.cursor.advance();
            let val = if p.can_start_expr() { Some(parse_expr(p)?) } else { None };
            Some(OrElseFallback::Return(val))
        }
        TokenType::Break => {
            p.cursor.advance();
            Some(OrElseFallback::Break)
        }
        TokenType::Continue => {
            p.cursor.advance();
            Some(OrElseFallback::Continue)
        }
        _ => {
            let val = parse_expr(p)?;
            Some(OrElseFallback::Expr(val))
        }
    }
}

// ── Interpolated string ───────────────────────────────────────────────────────

fn parse_interpolated_string<'ast, 'tok>(
    p:     &mut Parser<'ast, 'tok>,
    parts: Vec<ubel_stratum::lexer::InterpolationPart>,
    lo:    LexSpan,
) -> Option<&'ast Expr<'ast>> {
    use ubel_stratum::ast::literals::InterpolationPart as AstPart;
    use ubel_stratum::lexer::InterpolationPart as LexPart;

    let mut ast_parts: Vec<AstPart<'ast>> = Vec::with_capacity(parts.len());
    for part in &parts {
        match part {
            LexPart::Text(t)     => ast_parts.push(AstPart::Text(p.intern(t))),
            LexPart::Expr(e_src) => ast_parts.push(AstPart::Expr(p.intern(e_src))),
        }
    }
    p.cursor.advance();
    let span   = lo.merge(&p.span());
    let parts  = p.arena.alloc_slice_clone(&ast_parts);
    Some(p.alloc(Expr { kind: ExprKind::Lit(Literal::InterpolatedStr(parts)), span }))
}

// ── Binding power table ───────────────────────────────────────────────────────

/// Returns `(left_bp, right_bp)` for an infix/postfix operator.
/// Left-assoc: r_bp = l_bp + 1.  Right-assoc: l_bp > r_bp (e.g. assignment).
/// Returns `None` for tokens that are not infix/postfix operators.
#[inline]
fn infix_bp(tt: &TokenType) -> Option<(u8, u8)> {
    match tt {
        // Postfix — handled specially above but need BPs so loop doesn't exit
        TokenType::Dot
        | TokenType::LeftParen
        | TokenType::LeftBracket
        | TokenType::Question
        | TokenType::QuestionDot     => Some((29, 30)),

        // Assignment — right-associative: l_bp > r_bp
        TokenType::Equal
        | TokenType::PlusEqual
        | TokenType::MinusEqual
        | TokenType::StarEqual
        | TokenType::SlashEqual
        | TokenType::PercentEqual
        | TokenType::AmpEqual
        | TokenType::PipeEqual
        | TokenType::CaretEqual
        | TokenType::LeftShiftEqual
        | TokenType::RightShiftEqual  => Some((2, 1)),

        // Range — left-assoc
        TokenType::DotDot
        | TokenType::DotDotEqual      => Some((3, 4)),

        // Logical Or — left-assoc
        TokenType::Or
        | TokenType::PipePipe         => Some((5, 6)),

        // Logical And — left-assoc
        TokenType::And
        | TokenType::AmpAmp           => Some((7, 8)),

        // Equality — left-assoc (sema enforces non-chaining)
        TokenType::EqualEqual
        | TokenType::BangEqual        => Some((9, 10)),

        // Comparison — left-assoc
        TokenType::Less
        | TokenType::Greater
        | TokenType::LessEqual
        | TokenType::GreaterEqual     => Some((11, 12)),

        // Pipe operator `|>`
        TokenType::PipeArrow          => Some((13, 14)),

        // Bitwise Or `|`
        TokenType::Pipe               => Some((15, 16)),

        // Bitwise Xor `^`
        TokenType::Caret              => Some((17, 18)),

        // Bitwise And `&`
        TokenType::Amp                => Some((19, 20)),

        // Shift
        TokenType::LeftShift
        | TokenType::RightShift       => Some((21, 22)),

        // Additive
        TokenType::Plus
        | TokenType::Minus            => Some((23, 24)),

        // Multiplicative
        TokenType::Star
        | TokenType::Slash
        | TokenType::Percent          => Some((25, 26)),

        _ => None,
    }
}

// ── Operator conversion helpers ───────────────────────────────────────────────

fn to_bin_op(tt: &TokenType) -> Option<BinOp> {
    match tt {
        TokenType::Plus           => Some(BinOp::Add),
        TokenType::Minus          => Some(BinOp::Sub),
        TokenType::Star           => Some(BinOp::Mul),
        TokenType::Slash          => Some(BinOp::Div),
        TokenType::Percent        => Some(BinOp::Rem),
        TokenType::Amp            => Some(BinOp::BitAnd),
        TokenType::Pipe           => Some(BinOp::BitOr),
        TokenType::Caret          => Some(BinOp::BitXor),
        TokenType::LeftShift      => Some(BinOp::Shl),
        TokenType::RightShift     => Some(BinOp::Shr),
        TokenType::EqualEqual     => Some(BinOp::Eq),
        TokenType::BangEqual      => Some(BinOp::Ne),
        TokenType::Less           => Some(BinOp::Lt),
        TokenType::LessEqual      => Some(BinOp::Le),
        TokenType::Greater        => Some(BinOp::Gt),
        TokenType::GreaterEqual   => Some(BinOp::Ge),
        TokenType::And | TokenType::AmpAmp  => Some(BinOp::And),
        TokenType::Or  | TokenType::PipePipe => Some(BinOp::Or),
        TokenType::DotDot         => Some(BinOp::Range),
        TokenType::DotDotEqual    => Some(BinOp::RangeIncl),
        _                         => None,
    }
}

fn to_assign_op(tt: &TokenType) -> Option<AssignOp> {
    match tt {
        TokenType::Equal          => Some(AssignOp::Assign),
        TokenType::PlusEqual      => Some(AssignOp::AddAssign),
        TokenType::MinusEqual     => Some(AssignOp::SubAssign),
        TokenType::StarEqual      => Some(AssignOp::MulAssign),
        TokenType::SlashEqual     => Some(AssignOp::DivAssign),
        TokenType::PercentEqual   => Some(AssignOp::RemAssign),
        TokenType::AmpEqual       => Some(AssignOp::BitAndAssign),
        TokenType::PipeEqual      => Some(AssignOp::BitOrAssign),
        TokenType::CaretEqual     => Some(AssignOp::BitXorAssign),
        TokenType::LeftShiftEqual => Some(AssignOp::ShlAssign),
        TokenType::RightShiftEqual => Some(AssignOp::ShrAssign),
        _ => None,
    }
  }
