// crates/core/src/sema/facts.rs
//! Phase C of the LOW-tier borrow checker: fact collection over the CFG
//! `cfg.rs` builds. Produces the raw, AST-driven inputs Phase D's
//! liveness/loan fixed point will consume. Like `cfg.rs`, this module
//! does no fixed-point computation itself and isn't wired into
//! `sema::analyse` yet — there's still no check to run its output
//! against. That's Phase D.
//!
//! ## Scope, stated plainly
//!
//! **Loans only — no move tracking.** Move/ownership checking
//! (`Unique<T>` semantics) is real, separate, deliberately-deferred work.
//! `Unique<T>`/`Shared<T>`/`SyncShared<T>` are locked in by name but have
//! no concrete wiring yet (see `MEMORY_MODEL.md` §9), so there's no
//! settled notion yet of exactly which bindings are move-only — building
//! move tracking against an unsettled ownership-type story would mean
//! building it twice. Loans (`&`/`ref`, `&mut`/`ref mut`) have a settled
//! story — real syntax, real structural types (see §5.6) — so that's
//! what this collects.
//!
//! **Places: local bindings only.** `&x` is tracked precisely
//! (`Place::Local`). `&x.field`, `&arr[i]` are still recorded as real
//! loans — nothing silently vanishes — but as `Place::Unknown`, with no
//! invalidation-conflict detection. Widening `Place` to real field/index
//! paths is valid, real follow-up work, not a soundness bug in what's
//! here: `Place::Unknown` is an *under*-approximation (fewer conflicts
//! found than a precise analysis would), never an over-approximation.
//! Phase D must treat `Place::Unknown` loans as "insufficient
//! information," not "no conflict."
//!
//! **Expression walking stops at the same boundary `cfg.rs` draws.**
//! Control-flow-as-*expression* (`if`/`match` used as a value, a `{ }`
//! block expression, a lambda body) is opaque — not descended into, for
//! the same reason `cfg.rs` doesn't decompose it: doing so needs a
//! statement walker layered on an expression walker, not just the
//! expression walker this module builds. A borrow hiding inside one of
//! these is simply never found — again, under-approximation, not unsound.
//!
//! **`loan_invalidated_at` is a pure syntactic scan, not CFG-reachability-
//! aware.** For each loan, every *other* statement in the whole function
//! gets checked for a conflicting access to the same place, regardless of
//! whether that statement is actually reachable from the loan's issue
//! point. Deliberately over-approximate — Phase D, which already needs
//! full CFG liveness for its own propagation, is where reachability
//! belongs. A candidate invalidation Phase D's liveness later shows is
//! unreachable from the loan just never gets combined into a real error;
//! over-approximating here costs a few discarded candidates, nothing more.

use std::collections::HashMap;

use crate::ast::expressions::{ArgKind, Expr, ExprKind, OptionalAccess};
use crate::ast::statements::{Stmt, StmtKind};
use crate::lexer::Span;
use crate::sema::cfg::{BlockId, Cfg};

// ── Points and places ───────────────────────────────────────────────────

/// A point in the CFG — one specific statement within one specific block.
/// Matches `cfg.rs`'s own statement granularity; there's no finer
/// "start"/"mid" split the way Polonius uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub block: BlockId,
    pub stmt_index: usize,
}

/// What a loan borrows from. See the module doc's scope note — `Local`
/// is precise, everything else is `Unknown` for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place<'ast> {
    Local(&'ast str),
    Unknown,
}

fn expr_as_place<'ast>(expr: &'ast Expr<'ast>) -> Place<'ast> {
    match &expr.kind {
        ExprKind::Ident(name) => Place::Local(name),
        _ => Place::Unknown,
    }
}

// ── Loans ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoanId(pub u32);

#[derive(Debug, Clone, Copy)]
pub struct Loan<'ast> {
    pub id: LoanId,
    pub place: Place<'ast>,
    pub mutable: bool,
    pub issued_at: Point,
    pub span: Span,
}

#[derive(Debug, Default)]
pub struct Facts<'ast> {
    pub loans: Vec<Loan<'ast>>,
    /// Points where a loan's place is reassigned — its "natural" kill,
    /// before any liveness-based shrinking Phase D does.
    pub loan_killed_at: HashMap<LoanId, Vec<Point>>,
    /// Points where some access conflicts with a loan's terms. See the
    /// module doc's over-approximation note.
    pub loan_invalidated_at: HashMap<LoanId, Vec<Point>>,
}

/// Collects every loan/kill/invalidation-candidate fact for one
/// function's CFG. Doesn't check tier — callers decide which functions
/// are worth running this on (LOW-tier ones; see `cfg::build`'s own
/// doc note, same division of responsibility).
pub fn collect<'ast>(cfg: &Cfg<'ast>) -> Facts<'ast> {
    let mut facts = Facts::default();
    let mut next_id = 0u32;

    for block in &cfg.blocks {
        for (stmt_index, stmt) in block.stmts.iter().enumerate() {
            let point = Point { block: block.id, stmt_index };
            for_each_top_expr(stmt, &mut |e| collect_loans_in_expr(e, point, &mut facts.loans, &mut next_id));
        }
    }

    for loan in &facts.loans {
        for block in &cfg.blocks {
            for (stmt_index, stmt) in block.stmts.iter().enumerate() {
                let point = Point { block: block.id, stmt_index };
                if point == loan.issued_at { continue; }
                match classify_access(stmt, loan.place) {
                    Access::None => {}
                    Access::Reassign => {
                        facts.loan_killed_at.entry(loan.id).or_default().push(point);
                    }
                    Access::Conflict => {
                        facts.loan_invalidated_at.entry(loan.id).or_default().push(point);
                    }
                }
            }
        }
    }

    facts
}

fn collect_loans_in_expr<'ast>(
    expr: &'ast Expr<'ast>, point: Point, loans: &mut Vec<Loan<'ast>>, next_id: &mut u32,
) {
    walk_expr(expr, &mut |e| {
        if let ExprKind::Borrow { mutable, place } = &e.kind {
            let id = LoanId(*next_id);
            *next_id += 1;
            loans.push(Loan {
                id, place: expr_as_place(place), mutable: *mutable,
                issued_at: point, span: e.span,
            });
        }
    });
}

// ── Access classification ───────────────────────────────────────────────

enum Access {
    None,
    /// The place was reassigned (the loan's "natural" kill).
    Reassign,
    /// The place was read, or re-borrowed, conflicting with an
    /// outstanding loan's terms.
    Conflict,
}

/// Whether `stmt` reassigns or conflictingly-accesses `place`.
/// `Place::Unknown` never matches anything — see the module doc.
fn classify_access<'ast>(stmt: &'ast Stmt<'ast>, place: Place<'ast>) -> Access {
    if matches!(place, Place::Unknown) { return Access::None; }

    // Reassignment first: `place = value` kills any outstanding loan of
    // `place`, and doesn't ALSO count as a conflicting read of the old
    // value (the assignment's own RHS is checked separately, in case it
    // reads `place` on its way to overwriting it — e.g. `x = x + 1`,
    // which IS a real conflicting read of the old value if `x` is
    // mutably borrowed elsewhere).
    if let StmtKind::Expr(e) = &stmt.kind {
        if let ExprKind::Assign { target, value, .. } = &e.kind {
            if expr_as_place(target) == place {
                if expr_reads_place(value, place) {
                    return Access::Conflict;
                }
                return Access::Reassign;
            }
        }
    }

    let mut found = false;
    for_each_top_expr(stmt, &mut |e| {
        if expr_reads_place(e, place) { found = true; }
    });
    if found { Access::Conflict } else { Access::None }
}

/// Does `expr` (or anything it directly contains, per `walk_expr`'s scope)
/// read `place` — as a plain use, or by borrowing it again?
fn expr_reads_place<'ast>(expr: &'ast Expr<'ast>, place: Place<'ast>) -> bool {
    let mut found = false;
    walk_expr(expr, &mut |e| {
        match &e.kind {
            ExprKind::Ident(name) if Place::Local(name) == place => found = true,
            ExprKind::Borrow { place: inner, .. } if expr_as_place(inner) == place => found = true,
            _ => {}
        }
    });
    found
}

// ── Expression walking ──────────────────────────────────────────────────

/// Extracts the direct, non-block-body expression(s) a statement carries
/// — e.g. an `if`'s condition, not its branch bodies. Branch/loop bodies
/// are already separate blocks in `cfg.rs`'s output, visited by
/// `collect`'s own outer loop over every block — this only needs the
/// expressions living directly ON the statement itself.
fn for_each_top_expr<'ast>(stmt: &'ast Stmt<'ast>, f: &mut impl FnMut(&'ast Expr<'ast>)) {
    match &stmt.kind {
        StmtKind::Let { value, .. } => f(value),
        StmtKind::Expr(e) => f(e),
        StmtKind::Return(e) | StmtKind::Break(e) => { if let Some(e) = e { f(e); } }
        StmtKind::Fail(e) | StmtKind::Defer(e) => f(e),
        StmtKind::If(if_expr) => {
            f(if_expr.condition);
            for elif in if_expr.elif_branches { f(elif.condition); }
        }
        StmtKind::Match { scrutinee, .. } => f(scrutinee),
        StmtKind::For { iter, .. } => f(iter),
        StmtKind::While { condition, .. } => f(condition),
        StmtKind::Using { bindings, .. } => { for b in *bindings { f(b.value); } }
        StmtKind::Extract { value, .. } => f(value),
        // Loop/Continue carry no expression of their own. With's
        // allocator size, Try's catch binding, and Unsafe's body are
        // deliberately not walked here — none plausibly carries a loan
        // worth tracking in v1, and body statements are separate blocks
        // already, same as If/Match/loop bodies.
        StmtKind::Loop(_) | StmtKind::Continue
        | StmtKind::With { .. } | StmtKind::Try { .. } | StmtKind::Unsafe(_) => {}
    }
}

/// Calls `visit` on `expr` and recursively on every sub-expression it
/// directly contains. Scope matches `cfg.rs`'s own precedent: control-
/// flow constructs used as *expressions* (`if`, `match`, a `{ }` block,
/// a lambda body) are opaque — not descended into. See the module doc.
fn walk_expr<'ast>(expr: &'ast Expr<'ast>, visit: &mut impl FnMut(&'ast Expr<'ast>)) {
    visit(expr);
    match &expr.kind {
        ExprKind::Lit(_) | ExprKind::Ident(_) | ExprKind::SelfExpr => {}
        ExprKind::BinOp { lhs, rhs, .. } => { walk_expr(lhs, visit); walk_expr(rhs, visit); }
        ExprKind::UnaryOp { operand, .. } => walk_expr(operand, visit),
        ExprKind::Assign { target, value, .. } => { walk_expr(target, visit); walk_expr(value, visit); }
        ExprKind::Pipe { left, right } => { walk_expr(left, visit); walk_expr(right, visit); }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, visit);
            for a in *args { walk_arg(a, visit); }
        }
        ExprKind::Field { target, .. } => walk_expr(target, visit),
        ExprKind::Index { target, index } => { walk_expr(target, visit); walk_expr(index, visit); }
        ExprKind::OptionalChain { target, access } => {
            walk_expr(target, visit);
            if let OptionalAccess::Method { args, .. } = access {
                for a in *args { walk_arg(a, visit); }
            }
        }
        ExprKind::Try(inner) | ExprKind::Await(inner) | ExprKind::Deref(inner) => walk_expr(inner, visit),
        ExprKind::Borrow { place, .. } => walk_expr(place, visit),
        ExprKind::Tuple(items) | ExprKind::Array(items) => { for e in *items { walk_expr(e, visit); } }
        ExprKind::Dict(entries) => { for e in *entries { walk_expr(e.key, visit); walk_expr(e.value, visit); } }
        ExprKind::AnonObject(fields) => { for f in *fields { walk_expr(f.value, visit); } }
        ExprKind::StructLit { fields, .. } => { for f in *fields { walk_expr(f.value, visit); } }
        ExprKind::OrElse { expr, .. } => walk_expr(expr, visit),
        ExprKind::As { expr, .. } => walk_expr(expr, visit),
        ExprKind::ShortDecl { value, .. } => walk_expr(value, visit),
        // Opaque — see module doc.
        ExprKind::Lambda(_) | ExprKind::Block(_) | ExprKind::If(_) | ExprKind::Match(_) => {}
    }
}

fn walk_arg<'ast>(arg: &'ast crate::ast::expressions::Arg<'ast>, visit: &mut impl FnMut(&'ast Expr<'ast>)) {
    match &arg.kind {
        ArgKind::Positional(e) => walk_expr(e, visit),
        ArgKind::Named { value, .. } => walk_expr(value, visit),
    }
}

// ── Tests ────────────────────────────────────────────────────────────
//
// Same self-contained, hand-built-AST approach as cfg.rs's own tests —
// builds real function bodies via the arena, runs them through
// cfg::build then facts::collect, and asserts on the real output.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::arena::AstArena;
    use crate::ast::common::{AssignOp, Span, TierAnnotation, Visibility};
    use crate::ast::declarations::FunctionDecl;
    use crate::ast::expressions::{Arg, ArgKind};
    use crate::ast::literals::Literal;
    use crate::sema::cfg;

    const Z: Span = Span { start: 0, end: 0, line: 0, column: 0 };

    fn ident<'a>(arena: &'a AstArena, name: &'a str) -> &'a Expr<'a> {
        arena.alloc(Expr { kind: ExprKind::Ident(arena.alloc_str(name)), span: Z })
    }

    fn lit_int<'a>(arena: &'a AstArena, n: i64) -> &'a Expr<'a> {
        arena.alloc(Expr { kind: ExprKind::Lit(Literal::Int(n)), span: Z })
    }

    fn borrow<'a>(arena: &'a AstArena, mutable: bool, place: &'a Expr<'a>) -> &'a Expr<'a> {
        arena.alloc(Expr { kind: ExprKind::Borrow { mutable, place }, span: Z })
    }

    fn let_stmt<'a>(arena: &'a AstArena, name: &'a str, value: &'a Expr<'a>) -> Stmt<'a> {
        Stmt {
            kind: StmtKind::Let {
                mutable: false,
                binding: crate::ast::statements::BindingTarget::Ident(arena.alloc_str(name)),
                ty: None,
                value,
            },
            span: Z,
        }
    }

    fn reassign_stmt<'a>(arena: &'a AstArena, name: &'a str, value: &'a Expr<'a>) -> Stmt<'a> {
        let target = ident(arena, name);
        let assign = arena.alloc(Expr {
            kind: ExprKind::Assign { op: AssignOp::Assign, target, value },
            span: Z,
        });
        Stmt { kind: StmtKind::Expr(assign), span: Z }
    }

    fn func<'a>(arena: &'a AstArena, stmts: &[Stmt<'a>]) -> FunctionDecl<'a> {
        FunctionDecl {
            tier: TierAnnotation::default(), attributes: &[], visibility: Visibility::default(),
            is_async: false, name: arena.alloc_str("f"), lifetime_params: &[], generic_params: &[],
            params: &[], return_type: None,
            body: crate::ast::statements::Block { stmts: arena.alloc_slice_copy(stmts), span: Z },
            span: Z,
        }
    }

    fn facts_for<'a>(decl: &'a FunctionDecl<'a>) -> Facts<'a> {
        let graph = cfg::build(decl);
        collect(&graph)
    }

    #[test]
    fn shared_borrow_is_one_loan() {
        let arena = AstArena::new();
        let x = ident(&arena, "x");
        let stmts = [let_stmt(&arena, "p", borrow(&arena, false, x))];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        assert_eq!(facts.loans[0].place, Place::Local("x"));
        assert!(!facts.loans[0].mutable);
    }

    #[test]
    fn mutable_borrow_is_flagged_mutable() {
        let arena = AstArena::new();
        let x = ident(&arena, "x");
        let stmts = [let_stmt(&arena, "p", borrow(&arena, true, x))];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        assert!(facts.loans[0].mutable);
    }

    #[test]
    fn reassignment_kills_the_loan() {
        let arena = AstArena::new();
        let x1 = ident(&arena, "x");
        let stmts = [
            let_stmt(&arena, "p", borrow(&arena, false, x1)),
            reassign_stmt(&arena, "x", lit_int(&arena, 5)),
        ];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        let killed = facts.loan_killed_at.get(&facts.loans[0].id);
        assert_eq!(killed.map(|v| v.len()), Some(1), "x = 5 should kill the loan on x");
        assert!(facts.loan_invalidated_at.get(&facts.loans[0].id).is_none(),
            "a pure reassignment is a kill, not a conflicting read");
    }

    #[test]
    fn read_while_mutably_borrowed_is_invalidation() {
        let arena = AstArena::new();
        let x1 = ident(&arena, "x");
        let x2 = ident(&arena, "x");
        let stmts = [
            let_stmt(&arena, "p", borrow(&arena, true, x1)),
            let_stmt(&arena, "y", x2), // reads x while &mut x is outstanding
        ];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        let invalidated = facts.loan_invalidated_at.get(&facts.loans[0].id);
        assert_eq!(invalidated.map(|v| v.len()), Some(1), "reading x should invalidate the outstanding &mut x");
    }

    #[test]
    fn unrelated_statement_produces_no_facts() {
        let arena = AstArena::new();
        let x = ident(&arena, "x");
        let stmts = [
            let_stmt(&arena, "p", borrow(&arena, false, x)),
            let_stmt(&arena, "y", lit_int(&arena, 10)),
        ];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        assert!(facts.loan_killed_at.is_empty());
        assert!(facts.loan_invalidated_at.is_empty());
    }

    #[test]
    fn two_borrows_in_one_call_are_two_loans() {
        let arena = AstArena::new();
        let a = ident(&arena, "a");
        let b = ident(&arena, "b");
        let callee = ident(&arena, "combine");
        let args: &[Arg] = arena.alloc_slice_copy(&[
            Arg { kind: ArgKind::Positional(borrow(&arena, false, a)), span: Z },
            Arg { kind: ArgKind::Positional(borrow(&arena, true, b)), span: Z },
        ]);
        let call = arena.alloc(Expr { kind: ExprKind::Call { callee, args }, span: Z });
        let stmts = [let_stmt(&arena, "z", call)];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        // Proves the expression walker correctly descends into Call args
        // — a real gap class this session already learned to check for
        // (can_start_expr, the where/.query() collision).
        assert_eq!(facts.loans.len(), 2, "one borrow per call argument");
    }

    #[test]
    fn assigning_through_a_read_of_the_old_value_is_a_conflict_not_a_kill() {
        // x = x + 1 — this DOES read x's old value on its way to
        // overwriting it, and if x is borrowed elsewhere, that read is a
        // real conflict, not just a clean reassignment.
        let arena = AstArena::new();
        let x1 = ident(&arena, "x");
        let x2 = ident(&arena, "x");
        let one = lit_int(&arena, 1);
        let sum = arena.alloc(Expr {
            kind: ExprKind::BinOp { op: crate::ast::common::BinOp::Add, lhs: x2, rhs: one },
            span: Z,
        });
        let stmts = [
            let_stmt(&arena, "p", borrow(&arena, true, x1)),
            reassign_stmt(&arena, "x", sum),
        ];
        let decl = arena.alloc(func(&arena, &stmts));
        let facts = facts_for(decl);

        assert_eq!(facts.loans.len(), 1);
        assert!(facts.loan_invalidated_at.get(&facts.loans[0].id).is_some(),
            "x = x + 1 reads the old x — a real conflict against &mut x");
        assert!(facts.loan_killed_at.get(&facts.loans[0].id).is_none(),
            "the conflict path is exclusive of the plain-kill path — see classify_access");
    }
}
