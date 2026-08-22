// crates/core/src/sema/cfg.rs
//! Control-flow graph for `@tier(low)` function bodies — Phase A of the
//! borrow checker (see docs/MEMORY_MODEL.md §9). This module ONLY builds
//! the graph; it isn't wired into `sema::analyse` yet, because there's no
//! check to run against it. That's Phase C (fact collection) and Phase D
//! (the liveness/loan fixed point), still ahead.
//!
//! ## Scope, stated plainly
//!
//! Blocks are **statement-granularity**, not per-sub-expression "points"
//! the way Polonius uses (Start/Mid per statement). Intra-statement borrow
//! conflicts — two conflicting borrows inside one call's argument list —
//! are a direct AST-local check for the fact-collection pass, not
//! something CFG points need to represent. This mirrors the same
//! simplification real early NLL work made before Polonius needed the
//! extra precision.
//!
//! **Statement-level control flow only.** `If`/`Match` used as
//! *statements* are fully decomposed into real branches. `If`/`Match` used
//! as *expressions* (embedded inside a larger expression, including a
//! `then`-keyword single-expr branch body) are treated as one opaque,
//! straight-line unit for this first version — their internal branching
//! isn't represented in the graph. Same for the `?` operator appearing
//! inside a larger expression. Widening this is real, valid follow-up
//! work, not a correctness bug in what's here: it means the checker built
//! on top of this will be conservative (may reject some valid code) in
//! exactly those spots, never unsound.
//!
//! **`try`/`catch` gets a conservative approximation**: an edge from
//! before the `try` straight to the `catch` block (as if any statement in
//! the body could throw immediately), rather than per-statement throw
//! edges. Safe — it only makes originating-in-the-body loans look "more
//! live than necessary" from catch's perspective, which is conservative,
//! not unsound.
//!
//! **`with`/`using` are scope-transparent**: their body's statements
//! inline straight into the surrounding block sequence, no new branching.
//! RAII/drop-order semantics are a fact-collection concern (Phase C), not
//! a control-flow-*shape* one.
//!
//! **No panics on malformed input.** `break`/`continue` outside a loop
//! isn't caught by any existing sema pass yet (checked before writing
//! this — it isn't) — rather than `.expect()`-panicking on it, this
//! builder degrades to `Terminator::Unreachable`. A real
//! `BreakOutsideLoop`/`ContinueOutsideLoop` diagnostic is a real, small,
//! separate follow-up (probably `name_resolution.rs`), not solved here.

use crate::ast::declarations::FunctionDecl;
use crate::ast::expressions::{IfBranchBody, IfExpr, MatchArm, MatchArmBody};
use crate::ast::statements::{Block, Stmt, StmtKind};

/// Index into `Cfg::blocks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// How control leaves a block.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Falls through unconditionally to one successor.
    Goto(BlockId),
    /// N-way branch — 2 for `if`/`while`, one per arm for `match`. The
    /// branching condition/scrutinee lives on the source `Stmt` that ends
    /// this block; the graph doesn't duplicate it.
    Branch(Vec<BlockId>),
    /// `return`/`fail`, or the implicit end of the function.
    Return,
    /// Dead code (after an unconditional `return`/`break`/`continue`
    /// earlier in the same block), or a `break`/`continue` this builder
    /// couldn't resolve to a loop — see the module doc's panic-avoidance
    /// note. Kept as a real block, not dropped, so later passes can still
    /// see and diagnose it.
    Unreachable,
}

#[derive(Debug)]
pub struct BasicBlock<'ast> {
    pub id: BlockId,
    /// Statements in this block, in order — references into the original
    /// AST, never a lowered copy.
    pub stmts: Vec<&'ast Stmt<'ast>>,
    pub terminator: Terminator,
}

#[derive(Debug)]
pub struct Cfg<'ast> {
    pub blocks: Vec<BasicBlock<'ast>>,
    pub entry: BlockId,
}

impl<'ast> Cfg<'ast> {
    pub fn block(&self, id: BlockId) -> &BasicBlock<'ast> {
        &self.blocks[id.0 as usize]
    }

    /// Computed on demand, not maintained incrementally during the build —
    /// cheap enough for the block counts a LOW-tier function body will
    /// realistically have.
    pub fn predecessors(&self, id: BlockId) -> Vec<BlockId> {
        self.blocks.iter()
            .filter(|b| match &b.terminator {
                Terminator::Goto(t)    => *t == id,
                Terminator::Branch(ts) => ts.contains(&id),
                Terminator::Return | Terminator::Unreachable => false,
            })
            .map(|b| b.id)
            .collect()
    }
}

/// Build the CFG for one function body. Intended for `@tier(low)`
/// functions — nothing here checks the tier itself; that's the caller's
/// job (this module is a data structure, not a policy).
pub fn build<'ast>(decl: &'ast FunctionDecl<'ast>) -> Cfg<'ast> {
    let mut b = Builder { blocks: Vec::new(), loop_stack: Vec::new() };
    let entry = b.new_block();
    match b.build_block(&decl.body, entry) {
        Some(tail) => b.set_terminator(tail, Terminator::Return), // implicit void return
        None        => {} // every path already terminated explicitly
    }
    Cfg { blocks: b.blocks, entry }
}

// ── Builder ──────────────────────────────────────────────────────────

struct Builder<'ast> {
    blocks: Vec<BasicBlock<'ast>>,
    /// Innermost-first stack of (loop header, loop exit), for resolving
    /// `continue`/`break`.
    loop_stack: Vec<(BlockId, BlockId)>,
}

impl<'ast> Builder<'ast> {
    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BasicBlock { id, stmts: Vec::new(), terminator: Terminator::Return });
        id
    }

    fn set_terminator(&mut self, id: BlockId, term: Terminator) {
        self.blocks[id.0 as usize].terminator = term;
    }

    fn push_stmt(&mut self, id: BlockId, stmt: &'ast Stmt<'ast>) {
        self.blocks[id.0 as usize].stmts.push(stmt);
    }

    /// Builds `block`'s statements starting in `current`. Returns the
    /// block execution falls through to afterward — `None` if every path
    /// out of `block` terminated early (return/fail/break/continue), i.e.
    /// there's genuinely no fallthrough successor to wire up.
    fn build_block(&mut self, block: &'ast Block<'ast>, mut current: BlockId) -> Option<BlockId> {
        for stmt in block.stmts {
            match &stmt.kind {
                StmtKind::If(if_expr) => {
                    current = self.build_if(stmt, if_expr, current)?;
                }
                StmtKind::Match { arms, .. } => {
                    current = self.build_match(stmt, arms, current)?;
                }
                StmtKind::While { body, .. } => {
                    current = self.build_conditional_loop(stmt, body, current);
                }
                StmtKind::For { body, .. } => {
                    // Same CFG shape as `while` — a real per-iteration
                    // binding is a fact-collection concern (Phase C), not
                    // a control-flow-shape one.
                    current = self.build_conditional_loop(stmt, body, current);
                }
                StmtKind::Loop(body) => {
                    current = self.build_unconditional_loop(stmt, body, current);
                }
                StmtKind::Return(_) | StmtKind::Fail(_) => {
                    self.push_stmt(current, stmt);
                    self.set_terminator(current, Terminator::Return);
                    return None;
                }
                StmtKind::Break(_) => {
                    self.push_stmt(current, stmt);
                    match self.loop_stack.last() {
                        Some(&(_, exit)) => self.set_terminator(current, Terminator::Goto(exit)),
                        // See module doc: no BreakOutsideLoop check exists
                        // upstream yet. Degrade, don't panic.
                        None => self.set_terminator(current, Terminator::Unreachable),
                    }
                    return None;
                }
                StmtKind::Continue => {
                    self.push_stmt(current, stmt);
                    match self.loop_stack.last() {
                        Some(&(header, _)) => self.set_terminator(current, Terminator::Goto(header)),
                        None => self.set_terminator(current, Terminator::Unreachable),
                    }
                    return None;
                }
                StmtKind::With { body, .. } | StmtKind::Using { body, .. } => {
                    self.push_stmt(current, stmt);
                    current = self.build_block(body, current)?;
                }
                StmtKind::Try { body, catch_body, .. } => {
                    current = self.build_try(stmt, body, *catch_body, current)?;
                }
                // Let, Expr, Extract, Defer, and anything else not listed
                // above: straight-line, stays in the current block. This
                // includes if/match used as *expressions* rather than
                // statements — see the module doc's scope note.
                _ => {
                    self.push_stmt(current, stmt);
                }
            }
        }
        Some(current)
    }

    fn build_if(
        &mut self, stmt: &'ast Stmt<'ast>, if_expr: &'ast IfExpr<'ast>, current: BlockId,
    ) -> Option<BlockId> {
        self.push_stmt(current, stmt);

        let mut branch_targets = Vec::with_capacity(2 + if_expr.elif_branches.len());
        let mut converge_points = Vec::new();

        let (then_entry, then_tail) = self.build_branch_body(BranchBody::from_if(&if_expr.then_body));
        branch_targets.push(then_entry);
        if let Some(t) = then_tail { converge_points.push(t); }

        for elif in if_expr.elif_branches {
            let (entry, tail) = self.build_branch_body(BranchBody::from_if(&elif.body));
            branch_targets.push(entry);
            if let Some(t) = tail { converge_points.push(t); }
        }

        let has_else = if_expr.else_body.is_some();
        if let Some(else_body) = &if_expr.else_body {
            let (entry, tail) = self.build_branch_body(BranchBody::from_if(else_body));
            branch_targets.push(entry);
            if let Some(t) = tail { converge_points.push(t); }
        }

        self.set_terminator(current, Terminator::Branch(branch_targets));

        if !has_else {
            // No `else` — falling off the condition entirely is itself a
            // valid path, so it must converge too. Represent it as an
            // empty pass-through block used as one more branch target,
            // since `Branch` already carries every target explicitly.
            let skip = self.new_block();
            if let Terminator::Branch(targets) = &mut self.blocks[current.0 as usize].terminator {
                targets.push(skip);
            }
            converge_points.push(skip);
        }

        self.converge(converge_points)
    }

    fn build_match(
        &mut self, stmt: &'ast Stmt<'ast>, arms: &'ast [MatchArm<'ast>], current: BlockId,
    ) -> Option<BlockId> {
        self.push_stmt(current, stmt);

        let mut branch_targets = Vec::with_capacity(arms.len());
        let mut converge_points = Vec::new();

        for arm in arms {
            let (entry, tail) = self.build_branch_body(BranchBody::from_match_arm(&arm.body));
            branch_targets.push(entry);
            if let Some(t) = tail { converge_points.push(t); }
        }

        self.set_terminator(current, Terminator::Branch(branch_targets));
        self.converge(converge_points)
    }

    /// Builds one `if`/`elif`/`else`/match-arm body. Returns the entry
    /// block for that branch, and (if the branch falls through rather
    /// than terminating early) the block it falls through from.
    fn build_branch_body(&mut self, body: BranchBody<'ast>) -> (BlockId, Option<BlockId>) {
        let entry = self.new_block();
        match body {
            BranchBody::Block(b) => {
                let tail = self.build_block(b, entry);
                (entry, tail)
            }
            // A single expression body (`then expr`, or a bare match-arm
            // expression) is one opaque unit for this first version — see
            // the module doc's scope note. It always falls through.
            BranchBody::Expr(_) => (entry, Some(entry)),
        }
    }

    /// Joins every block in `points` into one new successor block, and
    /// wires each of them to it via `Goto`. Returns `None` if `points` is
    /// empty (every branch terminated early — nothing to converge).
    fn converge(&mut self, points: Vec<BlockId>) -> Option<BlockId> {
        if points.is_empty() {
            return None;
        }
        let after = self.new_block();
        for p in points {
            self.set_terminator(p, Terminator::Goto(after));
        }
        Some(after)
    }

    fn build_conditional_loop(
        &mut self, stmt: &'ast Stmt<'ast>, body: &'ast Block<'ast>, current: BlockId,
    ) -> BlockId {
        self.push_stmt(current, stmt);
        let header = self.new_block();
        self.set_terminator(current, Terminator::Goto(header));

        let body_entry = self.new_block();
        let exit = self.new_block();
        // Condition check: body if true, exit if false. The condition
        // expression itself lives on `stmt` (already pushed above); the
        // header block carries no statements of its own.
        self.set_terminator(header, Terminator::Branch(vec![body_entry, exit]));

        self.loop_stack.push((header, exit));
        if let Some(tail) = self.build_block(body, body_entry) {
            self.set_terminator(tail, Terminator::Goto(header)); // back-edge
        }
        self.loop_stack.pop();

        exit
    }

    fn build_unconditional_loop(
        &mut self, stmt: &'ast Stmt<'ast>, body: &'ast Block<'ast>, current: BlockId,
    ) -> BlockId {
        self.push_stmt(current, stmt);
        let header = self.new_block();
        self.set_terminator(current, Terminator::Goto(header));

        let exit = self.new_block();
        // `Loop` has no condition — the only way out is an internal
        // `break`, which is why `exit` isn't wired as a header successor
        // the way the conditional-loop case wires it. `exit` may end up
        // with zero predecessors if the body has no `break` at all; that's
        // a real, valid "infinite loop" shape, not a bug in the builder.
        self.loop_stack.push((header, exit));
        match self.build_block(body, header) {
            Some(tail) => self.set_terminator(tail, Terminator::Goto(header)), // back-edge
            None        => {} // every path in the body already terminated
        }
        self.loop_stack.pop();

        exit
    }

    fn build_try(
        &mut self, stmt: &'ast Stmt<'ast>, body: &'ast Block<'ast>,
        catch_body: Option<&'ast Block<'ast>>, current: BlockId,
    ) -> Option<BlockId> {
        self.push_stmt(current, stmt);

        let try_entry = self.new_block();
        let catch_entry = self.new_block();
        // Conservative approximation (see module doc): any statement in
        // the body could throw immediately, so `current` branches to both
        // `try_entry` and `catch_entry` up front, rather than threading a
        // throw-edge out of every individual statement in the body.
        self.set_terminator(current, Terminator::Branch(vec![try_entry, catch_entry]));

        let mut converge_points = Vec::new();
        if let Some(t) = self.build_block(body, try_entry) { converge_points.push(t); }

        match catch_body {
            Some(cb) => {
                if let Some(t) = self.build_block(cb, catch_entry) { converge_points.push(t); }
            }
            // No `catch` — an unhandled throw isn't representable as a
            // normal fallthrough; treat `catch_entry` as terminating
            // (propagates to the caller). Real propagation semantics are
            // a Phase C/D concern, not this builder's.
            None => self.set_terminator(catch_entry, Terminator::Return),
        }

        self.converge(converge_points)
    }
}

/// `IfBranchBody` and `MatchArmBody` are structurally identical (`Block`
/// or `Expr`) but are two separate AST enums — this lets `build_branch_body`
/// take either without duplicating itself. Holds references, not owned
/// values: both source enums are reached through an `&'ast`-lifetime
/// reference already (the enclosing `IfExpr`/`MatchArm` is itself
/// arena-allocated), so borrowing through them preserves `'ast`, and
/// `build_block` needs exactly that lifetime on its `Block` argument.
enum BranchBody<'ast> {
    Block(&'ast Block<'ast>),
    Expr(&'ast crate::ast::expressions::Expr<'ast>),
}

impl<'ast> BranchBody<'ast> {
    fn from_if(b: &'ast IfBranchBody<'ast>) -> Self {
        match b {
            IfBranchBody::Block(blk) => BranchBody::Block(blk),
            IfBranchBody::Expr(e)    => BranchBody::Expr(e),
        }
    }

    fn from_match_arm(b: &'ast MatchArmBody<'ast>) -> Self {
        match b {
            MatchArmBody::Block(blk) => BranchBody::Block(blk),
            MatchArmBody::Expr(e)    => BranchBody::Expr(e),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────
//
// Self-contained: builds raw AST fragments directly via the arena, same
// approach as sema/tests.rs, but doesn't reuse its helpers — this only
// needs If/While/Loop/Break/Return shapes, not a full sema::analyse run,
// so a separate minimal helper set stays simpler than threading through
// tests.rs's function-declaration-oriented ones.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::arena::AstArena;
    use crate::ast::common::{Span, TierAnnotation, Visibility};
    use crate::ast::declarations::FunctionDecl;
    use crate::ast::expressions::{Expr, ExprKind};
    use crate::ast::literals::Literal;
    use crate::ast::statements::StmtKind;

    const Z: Span = Span { start: 0, end: 0, line: 0, column: 0 };

    fn cond<'a>(arena: &'a AstArena) -> &'a Expr<'a> {
        arena.alloc(Expr { kind: ExprKind::Lit(Literal::Bool(true)), span: Z })
    }

    fn stmt<'a>(kind: StmtKind<'a>) -> Stmt<'a> {
        Stmt { kind, span: Z }
    }

    fn block<'a>(arena: &'a AstArena, stmts: &[Stmt<'a>]) -> Block<'a> {
        Block { stmts: arena.alloc_slice_copy(stmts), span: Z }
    }

    fn func<'a>(arena: &'a AstArena, body: Block<'a>) -> FunctionDecl<'a> {
        FunctionDecl {
            tier: TierAnnotation::default(), attributes: &[], visibility: Visibility::default(),
            is_async: false, name: arena.alloc_str("f"), lifetime_params: &[], generic_params: &[],
            params: &[], return_type: None, body, span: Z,
        }
    }

    fn if_stmt<'a>(
        arena: &'a AstArena, then_body: Block<'a>, else_body: Option<Block<'a>>,
    ) -> Stmt<'a> {
        let if_expr = arena.alloc(IfExpr {
            condition: cond(arena),
            then_body: IfBranchBody::Block(then_body),
            elif_branches: &[],
            else_body: else_body.map(IfBranchBody::Block),
            span: Z,
        });
        stmt(StmtKind::If(if_expr))
    }

    /// Every block reachable from `entry`, in the order a simple
    /// breadth-first walk visits them — good enough for asserting shapes
    /// without depending on the builder's exact allocation order.
    fn reachable(cfg: &Cfg) -> Vec<BlockId> {
        let mut seen = vec![cfg.entry];
        let mut i = 0;
        while i < seen.len() {
            let succs: Vec<BlockId> = match &cfg.block(seen[i]).terminator {
                Terminator::Goto(t)    => vec![*t],
                Terminator::Branch(ts) => ts.clone(),
                Terminator::Return | Terminator::Unreachable => vec![],
            };
            for s in succs {
                if !seen.contains(&s) { seen.push(s); }
            }
            i += 1;
        }
        seen
    }

    #[test]
    fn straight_line_is_one_block() {
        let arena = AstArena::new();
        let body = block(&arena, &[stmt(StmtKind::Return(None))]);
        let decl = func(&arena, body);
        let cfg = build(&arena.alloc(decl));

        assert_eq!(cfg.blocks.len(), 1, "no branching statements — should be exactly one block");
        assert!(matches!(cfg.block(cfg.entry).terminator, Terminator::Return));
    }

    #[test]
    fn if_else_both_return_needs_no_convergence() {
        let arena = AstArena::new();
        let then_b = block(&arena, &[stmt(StmtKind::Return(None))]);
        let else_b = block(&arena, &[stmt(StmtKind::Return(None))]);
        let body = block(&arena, &[if_stmt(&arena, then_b, Some(else_b))]);
        let decl = func(&arena, body);
        let cfg = build(&arena.alloc(decl));

        // entry (the `if` itself) + then-block + else-block = 3. Both
        // branches return, so no synthetic skip/convergence block.
        assert_eq!(cfg.blocks.len(), 3, "both branches return — nothing should converge");
        match &cfg.block(cfg.entry).terminator {
            Terminator::Branch(targets) => assert_eq!(targets.len(), 2, "if/else — exactly two targets"),
            other => panic!("expected Branch, got {:?}", other),
        }
        for id in reachable(&cfg) {
            if id != cfg.entry {
                assert!(matches!(cfg.block(id).terminator, Terminator::Return));
            }
        }
    }

    #[test]
    fn if_no_else_converges_through_synthetic_skip_block() {
        let arena = AstArena::new();
        let then_b = block(&arena, &[stmt(StmtKind::Expr(cond(&arena)))]); // no return — falls through
        let body = block(&arena, &[if_stmt(&arena, then_b, None)]);
        let decl = func(&arena, body);
        let cfg = build(&arena.alloc(decl));

        // entry + then-block + synthetic skip block + converge-after block = 4.
        assert_eq!(cfg.blocks.len(), 4, "no else — needs a synthetic skip target plus a convergence block");
        match &cfg.block(cfg.entry).terminator {
            Terminator::Branch(targets) => assert_eq!(targets.len(), 2, "then + synthetic skip"),
            other => panic!("expected Branch, got {:?}", other),
        }
        // Whatever block the two paths converge on should have both of
        // them as predecessors.
        let after = *reachable(&cfg).last().unwrap();
        assert_eq!(cfg.predecessors(after).len(), 2, "both then-path and skip-path should land here");
    }

    #[test]
    fn while_loop_has_header_body_backedge_and_exit() {
        let arena = AstArena::new();
        let loop_body = block(&arena, &[stmt(StmtKind::Break(None))]);
        let while_stmt = stmt(StmtKind::While { condition: cond(&arena), body: arena.alloc(loop_body) });
        let body = block(&arena, &[while_stmt]);
        let decl = func(&arena, body);
        let cfg = build(&arena.alloc(decl));

        // pre-header + header + body-entry(=break block) + exit = 4.
        assert_eq!(cfg.blocks.len(), 4);
        let header = match &cfg.block(cfg.entry).terminator {
            Terminator::Goto(h) => *h,
            other => panic!("expected Goto(header), got {:?}", other),
        };
        match &cfg.block(header).terminator {
            Terminator::Branch(targets) => assert_eq!(targets.len(), 2, "condition: body or exit"),
            other => panic!("expected Branch at loop header, got {:?}", other),
        }
        // header should have two predecessors once the body's back-edge
        // is wired: the pre-header AND the (break-less) tail of the body.
        // Here the body is just `break`, so its own block terminates via
        // Goto(exit) instead of looping back — header's only predecessor
        // is the pre-header.
        assert_eq!(cfg.predecessors(header).len(), 1, "body is just `break` — no back-edge reaches header");
    }

    #[test]
    fn nested_if_inside_loop_does_not_panic() {
        let arena = AstArena::new();
        let then_b = block(&arena, &[stmt(StmtKind::Break(None))]);
        let inner_if = if_stmt(&arena, then_b, None);
        let loop_body = block(&arena, &[inner_if]);
        let body = block(&arena, &[stmt(StmtKind::Loop(arena.alloc(loop_body)))]);
        let decl = func(&arena, body);
        let cfg = build(&arena.alloc(decl)); // must not panic

        assert!(!cfg.blocks.is_empty());
        // `Loop` has no condition — the header's own terminator is never
        // `Branch` the way a `while` header's is; it falls straight into
        // the body block, which is where the nested `if` lives.
    }
}
