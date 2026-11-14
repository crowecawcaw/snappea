//! Mutants of a P model: small changes to its code, each of which a good test suite
//! should notice. `snappea check` runs the test cases against each mutant; a mutant that
//! every test case still passes marks code whose effect no spec or assertion checks.
//!
//! A mutant changes one place in the code of the machines under test (never a spec
//! monitor or a test case's main machine, which are the tests themselves):
//!
//! - removes a statement with an effect: an assignment, collection update, `send`,
//!   `raise`, `announce`, `goto`, `new` or function call, or a whole `if`, `while` or
//!   `foreach`;
//! - empties a function or handler body (keeping its top-level `return`s);
//! - forces the condition of an `if` to `true` or to `false`;
//! - swaps an operator: `<` and `<=`, `>` and `>=`, `==` and `!=`, `+` and `-` (numbers),
//!   `&&` and `||`;
//! - drops either side of `&&` or `||`;
//! - removes a state's `defer` or `ignore` policy for an event, or turns `defer` into `ignore`;
//! - removes a `!`;
//! - flips a `true` or `false` literal.
//!
//! Conditions on a nondeterministic choice (`$`, `$$`, `choose`) are not forced, swapped
//! or negated: that only changes which choices take which branch, and the checker
//! explores every choice anyway.
//!
//! Removing a whole `if`, `while` or `foreach`, or emptying a body, is a *coarse* mutant:
//! the other mutants in it (see [`parents`]) change part of what it removes. When no test
//! case detects the coarse mutant, nothing checks any of that code, so the mutants in it
//! need not run one by one.
//!
//! Mutants are found by walking function bodies in a fixed order; [`apply`] walks the
//! same way to find the place again, so a [`Mutant`] needs no pointer into the IR.
//! Assertions, `print` and format strings are never changed.

use rustc_hash::FxHashMap;

use crate::checker::bytecode::{Probe, ProbeKind};
use crate::checker::code_coverage::{fun_context, shorten, snippet};
use crate::sema::{
    BinOp, Block, EventId, Expr, ExprKind, FunId, HandlerAction, MachineId, MachineKind, PType, Program, StateId, Stmt,
    StmtKind, TestCase, UnOp,
};
use crate::syntax::{SourceMap, Span};

/// What a mutant changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The statement is removed.
    Remove,
    /// Remove one event from a state's deferral declaration.
    RemoveDefer,
    /// Discard a deferred event instead of keeping it for a later state.
    DeferToIgnore,
    /// Remove one event from a state's ignore declaration.
    RemoveIgnore,
    /// The `if` condition is replaced by a constant.
    Force(bool),
    /// The binary operator is replaced.
    Operator(BinOp),
    /// `!e` becomes `e`.
    DropNot,
    /// A boolean literal is negated.
    FlipBool,
    /// Every top-level statement of the body but `return` is removed.
    EmptyBody,
    /// Drop the left (true) or right (false) clause of a conjunction.
    DropConjunct { left: bool },
    /// Drop the left (true) or right (false) clause of a disjunction.
    DropDisjunct { left: bool },
}

impl Change {
    /// The mutation's ID, as JSON results and `mutation:<id>` suppressions name it (see
    /// [`snappea_lint::MUTATIONS`]).
    pub fn name(self) -> &'static str {
        match self {
            Change::Remove => "remove-statement",
            Change::RemoveDefer => "remove-defer",
            Change::DeferToIgnore => "defer-to-ignore",
            Change::RemoveIgnore => "remove-ignore",
            Change::Force(true) => "condition-true",
            Change::Force(false) => "condition-false",
            Change::Operator(_) => "swap-operator",
            Change::DropNot => "remove-not",
            Change::FlipBool => "flip-boolean",
            Change::EmptyBody => "empty-body",
            Change::DropConjunct { .. } => "drop-conjunct",
            Change::DropDisjunct { .. } => "drop-disjunct",
        }
    }
}

/// One mutant.
#[derive(Debug, Clone)]
pub struct Mutant {
    /// The function whose body is changed; absent for state queue-policy changes.
    pub fun: Option<FunId>,
    /// State and event whose queue policy changes.
    queue: Option<(StateId, EventId)>,
    /// The place in the body, in [`walk`] order.
    site: u32,
    pub change: Change,
    /// The code coverage probe (see [`Probe`]) that runs whenever the changed code does:
    /// only a test case that ran it can notice the change.
    pub probe: Option<u32>,
    /// The changed code.
    pub span: Span,
    pub file: String,
    pub line: u32,
    /// Where the code is, e.g. `Server.Serving, on eRequest`.
    pub context: String,
    /// The change, e.g. ``removed `send client, eResp;` ``.
    pub description: String,
    /// It removes a whole `if`, `while`, `foreach` or body (see [`parents`]).
    pub coarse: bool,
    /// For a change that does nothing in some states (swapping `<` for `<=` when the
    /// operands are equal, removing an assignment of the value already there), the
    /// [`ProbeKind::Changes`] probes that run when it would change a value: only a test
    /// case that ran one can notice the change. Empty for other changes.
    pub changes: Vec<u32>,
    /// Why the change would not change a value when no test case ran a `changes` probe,
    /// e.g. `the operands are never equal`.
    pub unchanged: &'static str,
    /// The variable the changed statement only writes, when no check depends on it (see
    /// [`crate::checker::checked`]): no test case can notice the change, so it is not run.
    pub unchecked: Option<String>,
    /// Why no test case can detect the change, found from the code (see
    /// [`crate::checker::equivalent`]): it is not run.
    pub equivalent: Option<String>,
    /// The change is in, or is, an `if` or `while` on a nondeterministic choice: often
    /// fault injection in a machine that stands in for the environment.
    pub under_choice: bool,
}

impl Mutant {
    /// Whether a passing run exercised the statement or examined this queue policy.
    pub fn reached(&self, hits: &crate::checker::code_coverage::Hits) -> bool {
        self.probe.is_some_and(|p| hits.ran(p)) || self.queue.is_some_and(|(s, e)| hits.queue_policy(s, e))
    }

    /// Whether the change would have changed a value in a run with code coverage `hits`.
    pub fn changed(&self, hits: &crate::checker::code_coverage::Hits) -> bool {
        self.unchecked.is_none()
            && self.reached(hits)
            && (self.changes.is_empty() || self.changes.iter().any(|p| hits.ran(*p)))
    }
}

/// For each of `mutants`, the smallest coarse mutant ([`Mutant::coarse`]) in the same
/// function that contains its change, if any. When no test case detects that one, nothing
/// checks the code it removes, this mutant's change included.
pub fn parents(mutants: &[Mutant]) -> Vec<Option<usize>> {
    let contains = |outer: &Mutant, inner: &Mutant| {
        outer.fun == inner.fun
            && outer.span.file == inner.span.file
            && outer.span.start <= inner.span.start
            && inner.span.end <= outer.span.end
    };
    let len = |m: &Mutant| m.span.end - m.span.start;
    mutants
        .iter()
        .enumerate()
        .map(|(i, m)| {
            mutants
                .iter()
                .enumerate()
                .filter(|(j, c)| {
                    // a coarse mutant's parent is strictly larger, so there are no cycles
                    *j != i && c.coarse && contains(c, m) && (!m.coarse || len(c) > len(m))
                })
                .min_by_key(|(j, c)| (len(c), *j))
                .map(|(j, _)| j)
        })
        .collect()
}

/// The functions a mutant may change: those of the machines the test cases check (not
/// spec monitors, a test case's main machine, nor the other machines in `drivers`, see
/// [`crate::checker::driver_facts::driver_machines`]), and the functions they call.
/// Functions declared under a `PTst` directory are test scaffolding, even when a
/// design function calls them. Their callees outside `PTst` are still considered.
pub fn mutable_funs(prog: &Program, tests: &[TestCase], drivers: &[bool]) -> Vec<bool> {
    let mut machines = vec![false; prog.machines.len()];
    let mut mains = vec![false; prog.machines.len()];
    for tc in tests {
        let (module, main) = prog.test_case_module(tc);
        for (_, m) in &module.interface_def {
            machines[m.idx()] = true;
        }
        if let Some(m) = main {
            mains[m.idx()] = true;
        }
    }
    let is_model = |m: MachineId| {
        machines[m.idx()]
            && !mains[m.idx()]
            && !drivers.get(m.idx()).copied().unwrap_or(false)
            && prog.machine(m).kind == MachineKind::Impl
    };
    let mut funs = vec![false; prog.funs.len()];
    let mut work = Vec::new();
    for (i, f) in prog.funs.iter().enumerate() {
        if f.owner.is_some_and(is_model) {
            funs[i] = true;
            work.push(FunId(i as u32));
        }
    }
    while let Some(f) = work.pop() {
        for c in &prog.fun(f).callees {
            let owner = prog.fun(*c).owner;
            // a method of a spec or test driver called from the model is still a test's code
            if !funs[c.idx()] && owner.is_none_or(is_model) {
                funs[c.idx()] = true;
                work.push(*c);
            }
        }
    }
    // receive cases are functions of their own
    for (i, f) in prog.funs.iter().enumerate() {
        if f.parent.is_some_and(|p| funs[p.idx()]) {
            funs[i] = true;
        }
    }
    for (i, f) in prog.funs.iter().enumerate() {
        if prog
            .sources
            .files()
            .get(f.span.file.0 as usize)
            .is_some_and(|source| source.name.split(['/', '\\']).any(|part| part.eq_ignore_ascii_case("PTst")))
        {
            funs[i] = false;
        }
    }
    funs
}

/// The mutants of the functions in `funs` (see [`mutable_funs`]), with `probes`, the code
/// coverage probes of the program's code
/// ([`compile_program_with_probes`](crate::checker::compile::compile_program_with_probes)).
pub fn mutants(prog: &Program, funs: &[bool], probes: &[Probe]) -> Vec<Mutant> {
    let probe_of: FxHashMap<(ProbeKind, FunId, Span), u32> =
        probes.iter().enumerate().rev().map(|(id, p)| ((p.kind, p.fun, p.span), id as u32)).collect();
    let mut changes_at: FxHashMap<(FunId, Span), Vec<u32>> = FxHashMap::default();
    for (id, p) in probes.iter().enumerate().filter(|(_, p)| p.kind == ProbeKind::Changes) {
        changes_at.entry((p.fun, p.span)).or_default().push(id as u32);
    }
    let sm: &SourceMap = &prog.sources;
    let checked = crate::checker::checked::analyze(prog);
    let mut effectful = Effects { prog, funs: FxHashMap::default() };
    let mut freshness = crate::checker::equivalent::Freshness::new(prog);
    let mut out = Vec::new();
    for (i, f) in prog.funs.iter().enumerate() {
        let fun = FunId(i as u32);
        let Some(body) = f.body.clone().filter(|_| funs.get(i).copied().unwrap_or(false)) else { continue };
        let mut body = body;
        let context = fun_context(prog, fun);
        // statements that only write a value no check depends on
        let mut unchecked: FxHashMap<Span, String> = FxHashMap::default();
        crate::checker::checked::visit_stmts(&body, &mut |s| {
            if checked.removable(s) {
                if let Some(v) = crate::checker::checked::written(s) {
                    unchecked.insert(s.span, prog.var(v).name.clone());
                }
            }
        });
        // emptying a body of one statement is the same as removing it
        let effects = body.stmts.iter().filter(|s| !matches!(s.kind, StmtKind::Return(_) | StmtKind::Nop)).count();
        if effects >= 2 && !f.span.is_dummy() {
            if let Some(&probe) = probe_of.get(&(ProbeKind::Entry, fun, f.span)) {
                let loc = sm.location(f.span);
                out.push(Mutant {
                    fun: Some(fun),
                    queue: None,
                    site: u32::MAX,
                    change: Change::EmptyBody,
                    probe: Some(probe),
                    span: f.span,
                    file: loc.file.unwrap_or_default(),
                    line: loc.line.max(0) as u32,
                    context: context.clone(),
                    description: format!("removed the whole body ({} statements)", effects),
                    coarse: true,
                    changes: Vec::new(),
                    unchanged: "",
                    unchecked: None,
                    equivalent: None,
                    under_choice: false,
                });
            }
        }
        // changes that cannot be detected
        let guards = crate::checker::equivalent::self_assigning_guards(&body);
        let sole_choice = crate::checker::equivalent::sole_choice_statements(&body);
        let mut choices: Vec<Span> = Vec::new();
        crate::checker::checked::visit_stmts(&body, &mut |s| match &s.kind {
            StmtKind::If { cond, .. } | StmtKind::While { cond, .. } if has_choice(cond) => choices.push(s.span),
            _ => {}
        });
        let mut counters: FxHashMap<Span, String> = FxHashMap::default();
        crate::checker::checked::visit_stmts(&body, &mut |s| {
            if let Some((at, why)) = freshness.counter(s) {
                counters.insert(at, why);
            }
        });
        let mut site = 0u32;
        walk(prog, &mut body, &mut |stmt, target| {
            let this = site;
            site += 1;
            let (kind, span) = match target {
                Target::Stmt(_, Change::Force(true)) => (ProbeKind::Else, stmt),
                Target::Stmt(_, Change::Force(false)) => (ProbeKind::Then, stmt),
                _ => (ProbeKind::Stmt, stmt),
            };
            let Some(&probe) = probe_of.get(&(kind, fun, span)) else { return };
            let coarse = matches!(&target, Target::Stmt(s, Change::Remove) if compound(s));
            let watched = match &target {
                // removing what the statement computes besides the value (a call that
                // sends, a `new`, a choice) changes more than the value
                Target::Stmt(s, Change::Remove) if effectful.computes_with_effect(s) => None,
                Target::Stmt(s, Change::Remove) => match s.kind {
                    StmtKind::Assign { .. } => Some((stmt, "it only assigns the value already there")),
                    StmtKind::Add { .. } => Some((stmt, "the value is always there already")),
                    StmtKind::Remove { .. } => Some((stmt, "the value is never there")),
                    _ => None,
                },
                Target::Expr(e, Change::Operator(BinOp::Add | BinOp::Sub)) => {
                    Some((e.span, "the right operand is always 0"))
                }
                Target::Expr(e, Change::Operator(BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)) => {
                    Some((e.span, "the operands are never equal"))
                }
                _ => None,
            };
            let (watched, unchanged) = watched.map_or((None, ""), |(at, why)| (Some(at), why));
            let equivalent = match &target {
                Target::Stmt(s, Change::Remove) => crate::checker::equivalent::removed_choice_branch(s)
                    .or_else(|| {
                        sole_choice.contains(&s.span).then_some(crate::checker::equivalent::SOLE_CHOICE_STATEMENT)
                    })
                    .map(str::to_string),
                Target::Expr(e, Change::Operator(BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)) => {
                    guards.get(&e.span).map(|why| why.to_string())
                }
                Target::Expr(e, Change::Operator(BinOp::Add | BinOp::Sub)) => counters.get(&e.span).cloned(),
                _ => None,
            };
            let changes = watched.and_then(|at| changes_at.get(&(fun, at))).cloned().unwrap_or_default();
            let (change, at, description) = describe(sm, &target);
            if at.is_dummy() {
                return;
            }
            let loc = sm.location(at);
            out.push(Mutant {
                fun: Some(fun),
                queue: None,
                site: this,
                change,
                probe: Some(probe),
                span: at,
                file: loc.file.unwrap_or_default(),
                line: loc.line.max(0) as u32,
                context: context.clone(),
                description,
                coarse,
                changes,
                unchanged,
                unchecked: unchecked.get(&stmt).cloned(),
                equivalent,
                under_choice: choices.iter().any(|c| c.file == at.file && c.start <= at.start && at.end <= c.end),
            });
        });
    }
    out
}

/// Queue-policy mutations in selected design machines, one per declared event.
/// Drivers and monitors are excluded even when they have no function bodies.
pub fn queue_mutants(prog: &Program, tests: &[TestCase], drivers: &[bool]) -> Vec<Mutant> {
    let mut selected = vec![false; prog.machines.len()];
    let mut mains = vec![false; prog.machines.len()];
    for tc in tests {
        let (module, main) = prog.test_case_module(tc);
        for (_, m) in &module.interface_def {
            selected[m.idx()] = true;
        }
        if let Some(m) = main {
            mains[m.idx()] = true;
        }
    }
    let mut out = Vec::new();
    for (i, machine) in prog.machines.iter().enumerate() {
        let in_test_dir = prog
            .sources
            .files()
            .get(machine.span.file.0 as usize)
            .is_some_and(|s| s.name.split(['/', '\\']).any(|p| p.eq_ignore_ascii_case("PTst")));
        if !selected[i]
            || mains[i]
            || drivers.get(i).copied().unwrap_or(false)
            || in_test_dir
            || machine.kind != MachineKind::Impl
        {
            continue;
        }
        for state in &machine.states {
            let st = prog.state(*state);
            for h in &st.handlers {
                let changes: &[Change] = match h.action {
                    HandlerAction::Defer => &[Change::RemoveDefer, Change::DeferToIgnore],
                    HandlerAction::Ignore => &[Change::RemoveIgnore],
                    _ => continue,
                };
                if h.span.is_dummy() {
                    continue;
                }
                let loc = prog.sources.location(h.span);
                let event = &prog.event(h.event).name;
                for change in changes {
                    let description = match change {
                        Change::RemoveDefer => format!("removed `defer {event}`"),
                        Change::DeferToIgnore => format!("`defer {event}` replaced by `ignore {event}`"),
                        Change::RemoveIgnore => format!("removed `ignore {event}`"),
                        _ => unreachable!(),
                    };
                    out.push(Mutant {
                        fun: None,
                        queue: Some((*state, h.event)),
                        site: 0,
                        change: *change,
                        probe: None,
                        span: h.span,
                        file: loc.file.clone().unwrap_or_default(),
                        line: loc.line.max(0) as u32,
                        context: format!("{}.{}, on {}", machine.name, st.name, event),
                        description,
                        coarse: false,
                        changes: Vec::new(),
                        unchanged: "",
                        unchecked: None,
                        equivalent: None,
                        under_choice: false,
                    });
                }
            }
        }
    }
    out
}

/// Which code does more than compute values, for the mutants that change no value (see
/// [`Mutant::changes`]): removing such code changes the program even when the value it
/// assigns is already there.
struct Effects<'p> {
    prog: &'p Program,
    /// Functions known to have an effect, or not.
    funs: FxHashMap<FunId, bool>,
}

impl Effects<'_> {
    /// Whether evaluating `e` creates a machine, makes a choice, or calls a function with
    /// an effect.
    fn expr(&mut self, e: &Expr) -> bool {
        let mut calls = Vec::new();
        let mut found = false;
        visit_expr(e, &mut |e| match &e.kind {
            ExprKind::Ctor { .. } | ExprKind::Nondet | ExprKind::FairNondet | ExprKind::Choose(_) => found = true,
            ExprKind::FunCall { fun, .. } => calls.push(*fun),
            _ => {}
        });
        found || calls.into_iter().any(|f| self.fun(f))
    }

    /// Whether computing what the assignment (or `+=`, `-=`) `s` writes, and where, has
    /// an effect.
    fn computes_with_effect(&mut self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Assign { lhs, rhs: value } | StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                self.expr(lhs) || self.expr(value)
            }
            _ => false,
        }
    }

    /// Whether running `s` (its nested statements included) does more than write locals
    /// and parameters: sends, raises, announces, moves to a state, receives, creates a
    /// machine, writes a field or makes a choice, or calls a function that does.
    fn stmt_has_effect(&mut self, s: &Stmt) -> bool {
        self.own_effect(s)
            || match &s.kind {
                StmtKind::Block(b) => b.stmts.iter().any(|s| self.stmt_has_effect(s)),
                StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => self.stmt_has_effect(body),
                StmtKind::If { then_branch, else_branch, .. } => {
                    self.stmt_has_effect(then_branch) || self.stmt_has_effect(else_branch)
                }
                _ => false,
            }
    }

    fn own_effect(&mut self, s: &Stmt) -> bool {
        let prog = self.prog;
        let field = |lhs: &Expr| {
            crate::checker::checked::root(lhs).is_none_or(|v| prog.var(v).role == crate::sema::VarRole::Field)
        };
        match &s.kind {
            StmtKind::Send { .. }
            | StmtKind::Raise { .. }
            | StmtKind::Announce { .. }
            | StmtKind::Goto { .. }
            | StmtKind::Receive { .. }
            | StmtKind::Ctor { .. } => true,
            StmtKind::FunCall { fun, args } => self.fun(*fun) || args.iter().any(|a| self.expr(a)),
            StmtKind::Assign { lhs, rhs } => field(lhs) || self.expr(lhs) || self.expr(rhs),
            StmtKind::Insert { lhs, index, value } => {
                field(lhs) || self.expr(lhs) || self.expr(index) || self.expr(value)
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                field(lhs) || self.expr(lhs) || self.expr(value)
            }
            StmtKind::Return(Some(e)) | StmtKind::Print(e) => self.expr(e),
            StmtKind::While { cond, .. } | StmtKind::If { cond, .. } => self.expr(cond),
            StmtKind::Foreach { collection, .. } => self.expr(collection),
            StmtKind::Assert { cond, .. } | StmtKind::Assume { cond, .. } => self.expr(cond),
            _ => false,
        }
    }

    /// Whether calling `f` has an effect (see [`Effects::stmt_has_effect`]): a foreign
    /// function always may.
    fn fun(&mut self, f: FunId) -> bool {
        if let Some(&known) = self.funs.get(&f) {
            return known;
        }
        let prog: &Program = self.prog;
        let decl = prog.fun(f);
        let Some(body) = decl.body.as_ref().filter(|_| !decl.is_foreign()) else {
            self.funs.insert(f, true);
            return true;
        };
        // a recursive call adds no effect of its own
        self.funs.insert(f, false);
        let effect = body.stmts.iter().any(|s| self.stmt_has_effect(s));
        self.funs.insert(f, effect);
        effect
    }
}

/// `prog` with mutant `m` applied.
pub fn apply(prog: &Program, m: &Mutant) -> Program {
    let mut out = prog.clone();
    if let Some((state, event)) = m.queue {
        let handlers = &mut out.states[state.idx()].handlers;
        let i = handlers.iter().position(|h| h.event == event).expect("queue mutant's event exists");
        match m.change {
            Change::RemoveDefer | Change::RemoveIgnore => {
                handlers.remove(i);
            }
            Change::DeferToIgnore => handlers[i].action = HandlerAction::Ignore,
            _ => unreachable!("not a queue-policy mutant"),
        }
        return out;
    }
    let fun = m.fun.expect("function mutant has a function");
    let mut body = out.funs[fun.idx()].body.take().expect("a mutant changes a function with a body");
    if m.change == Change::EmptyBody {
        for s in &mut body.stmts {
            if !matches!(s.kind, StmtKind::Return(_)) {
                s.kind = StmtKind::Nop;
            }
        }
        out.funs[fun.idx()].body = Some(body);
        return out;
    }
    let mut site = 0u32;
    walk(prog, &mut body, &mut |_, target| {
        if site == m.site {
            change(target);
        }
        site += 1;
    });
    out.funs[fun.idx()].body = Some(body);
    out
}

/// A place a mutant can change, and the change.
enum Target<'a> {
    Stmt(&'a mut Stmt, Change),
    Expr(&'a mut Expr, Change),
}

fn change(target: Target<'_>) {
    match target {
        Target::Stmt(s, Change::Remove) => s.kind = StmtKind::Nop,
        Target::Stmt(s, Change::Force(v)) => {
            if let StmtKind::If { cond, .. } = &mut s.kind {
                cond.kind = ExprKind::BoolLit(v);
            }
        }
        Target::Expr(e, Change::Operator(new)) => {
            if let ExprKind::Binary { op, .. } = &mut e.kind {
                *op = new;
            }
        }
        Target::Expr(e, Change::DropConjunct { left } | Change::DropDisjunct { left }) => {
            if let ExprKind::Binary { lhs, rhs, .. } = &e.kind {
                *e = if left { (**rhs).clone() } else { (**lhs).clone() };
            }
        }
        Target::Expr(e, Change::DropNot) => {
            if let ExprKind::Unary { operand, .. } = &mut e.kind {
                let inner =
                    std::mem::replace(&mut **operand, Expr { kind: ExprKind::NullLit, ty: PType::Null, span: e.span });
                *e = inner;
            }
        }
        Target::Expr(e, Change::FlipBool) => {
            if let ExprKind::BoolLit(b) = &mut e.kind {
                *b = !*b;
            }
        }
        _ => unreachable!("no such mutant"),
    }
}

/// Calls `visit` with each place in `body` a mutant can change, in a fixed order, and the
/// span of the statement it is in.
fn walk(prog: &Program, body: &mut Block, visit: &mut dyn FnMut(Span, Target<'_>)) {
    for s in &mut body.stmts {
        walk_stmt(prog, s, visit);
    }
}

fn walk_stmt(prog: &Program, s: &mut Stmt, visit: &mut dyn FnMut(Span, Target<'_>)) {
    let span = s.span;
    let removable = matches!(
        s.kind,
        StmtKind::Assign { .. }
            | StmtKind::Insert { .. }
            | StmtKind::Add { .. }
            | StmtKind::Remove { .. }
            | StmtKind::Send { .. }
            | StmtKind::Raise { .. }
            | StmtKind::Announce { .. }
            | StmtKind::Goto { .. }
            | StmtKind::FunCall { .. }
            | StmtKind::Ctor { .. }
    ) || compound(s);
    if removable {
        visit(span, Target::Stmt(s, Change::Remove));
    }
    if matches!(&s.kind, StmtKind::If { cond, .. } if !matches!(cond.kind, ExprKind::BoolLit(_)) && !has_choice(cond)) {
        visit(span, Target::Stmt(s, Change::Force(true)));
        visit(span, Target::Stmt(s, Change::Force(false)));
    }
    macro_rules! exprs {
        ($($e:expr),*) => {{ $(walk_expr(prog, $e, span, visit);)* }};
    }
    match &mut s.kind {
        StmtKind::Block(b) => walk(prog, b, visit),
        // the checks themselves, and what only they read
        StmtKind::Assert { .. } | StmtKind::Assume { .. } | StmtKind::Print(_) => {}
        StmtKind::Return(e) => {
            if let Some(e) = e {
                exprs!(e)
            }
        }
        StmtKind::Assign { lhs, rhs } => exprs!(lhs, rhs),
        StmtKind::Insert { lhs, index, value } => exprs!(lhs, index, value),
        StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => exprs!(lhs, value),
        StmtKind::While { cond, body } => {
            exprs!(cond);
            walk_stmt(prog, body, visit);
        }
        StmtKind::Foreach { collection, body, .. } => {
            exprs!(collection);
            walk_stmt(prog, body, visit);
        }
        StmtKind::If { cond, then_branch, else_branch } => {
            exprs!(cond);
            walk_stmt(prog, then_branch, visit);
            walk_stmt(prog, else_branch, visit);
        }
        StmtKind::FunCall { args, .. } => args.iter_mut().for_each(|e| exprs!(e)),
        StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
            exprs!(event);
            payload.iter_mut().for_each(|e| exprs!(e));
        }
        StmtKind::Send { target, event, payload } => {
            exprs!(target, event);
            payload.iter_mut().for_each(|e| exprs!(e));
        }
        StmtKind::Ctor { payload, .. } | StmtKind::Goto { payload, .. } => payload.iter_mut().for_each(|e| exprs!(e)),
        // receive cases are functions of their own
        StmtKind::Receive { .. } | StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
    }
}

/// Whether removing `s` removes other statements: an `if`, `while` or `foreach`.
fn compound(s: &Stmt) -> bool {
    matches!(s.kind, StmtKind::If { .. } | StmtKind::While { .. } | StmtKind::Foreach { .. })
}

/// The operator a mutant puts in place of `op` in `e`, if any.
pub(crate) fn swapped(prog: &Program, op: BinOp, e: &Expr) -> Option<BinOp> {
    let numeric = || matches!(e.ty.canonicalize(prog), PType::Int | PType::Float);
    Some(match op {
        BinOp::Lt => BinOp::Le,
        BinOp::Le => BinOp::Lt,
        BinOp::Gt => BinOp::Ge,
        BinOp::Ge => BinOp::Gt,
        BinOp::Eq => BinOp::Neq,
        BinOp::Neq => BinOp::Eq,
        BinOp::And => BinOp::Or,
        BinOp::Or => BinOp::And,
        BinOp::Add if numeric() => BinOp::Sub,
        BinOp::Sub if numeric() => BinOp::Add,
        _ => return None,
    })
}

/// Whether `e` contains a nondeterministic choice (`$`, `$$` or `choose`).
fn has_choice(e: &Expr) -> bool {
    let mut found = false;
    visit_expr(e, &mut |e| found |= matches!(e.kind, ExprKind::Nondet | ExprKind::FairNondet | ExprKind::Choose(_)));
    found
}

/// Calls `f` with `e` and each expression in it.
pub(crate) fn visit_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    visit_children(e, &mut |x| visit_expr(x, f));
}

/// Calls `f` with each expression directly in `e`.
pub(crate) fn visit_children(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    match &e.kind {
        ExprKind::Tuple(es) | ExprKind::NamedTuple(es) | ExprKind::FunCall { args: es, .. } => es.iter().for_each(f),
        ExprKind::Str { args, .. } => args.iter().for_each(f),
        ExprKind::TupleAccess { base: x, .. }
        | ExprKind::NamedTupleAccess { base: x, .. }
        | ExprKind::Keys(x)
        | ExprKind::Values(x)
        | ExprKind::Sizeof(x)
        | ExprKind::Cast(x)
        | ExprKind::Coerce(x)
        | ExprKind::Unary { operand: x, .. } => f(x),
        ExprKind::SeqAccess { base: a, index: b }
        | ExprKind::SetAccess { base: a, index: b }
        | ExprKind::MapAccess { base: a, key: b }
        | ExprKind::Binary { lhs: a, rhs: b, .. }
        | ExprKind::Contains { item: a, collection: b } => {
            f(a);
            f(b);
        }
        ExprKind::Ctor { payload, .. } | ExprKind::Choose(payload) => payload.iter().for_each(|x| f(x)),
        _ => {}
    }
}

fn walk_expr(prog: &Program, e: &mut Expr, stmt: Span, visit: &mut dyn FnMut(Span, Target<'_>)) {
    // swapping a comparison or a logical operator on a choice, or removing a `!` from one,
    // only changes which choices take which branch: the checker explores them all anyway
    let logic = |op: BinOp| !matches!(op, BinOp::Add | BinOp::Sub);
    match &e.kind {
        ExprKind::Binary { op, .. } if logic(*op) && has_choice(e) => {}
        ExprKind::Binary { op, .. } => {
            let op = *op;
            if let Some(new) = swapped(prog, op, e) {
                visit(stmt, Target::Expr(e, Change::Operator(new)));
            }
            for left in [true, false] {
                let change = match op {
                    BinOp::And => Change::DropConjunct { left },
                    BinOp::Or => Change::DropDisjunct { left },
                    _ => continue,
                };
                visit(stmt, Target::Expr(e, change));
            }
        }
        ExprKind::Unary { op: UnOp::Not, .. } if has_choice(e) => {}
        ExprKind::Unary { op: UnOp::Not, .. } => visit(stmt, Target::Expr(e, Change::DropNot)),
        ExprKind::BoolLit(_) => visit(stmt, Target::Expr(e, Change::FlipBool)),
        _ => {}
    }
    let mut sub = |e: &mut Expr| walk_expr(prog, e, stmt, visit);
    match &mut e.kind {
        ExprKind::Tuple(es) | ExprKind::NamedTuple(es) | ExprKind::FunCall { args: es, .. } => {
            es.iter_mut().for_each(sub)
        }
        ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => sub(base),
        ExprKind::SeqAccess { base, index } | ExprKind::SetAccess { base, index } => {
            sub(base);
            sub(index);
        }
        ExprKind::MapAccess { base, key } => {
            sub(base);
            sub(key);
        }
        ExprKind::Keys(x)
        | ExprKind::Values(x)
        | ExprKind::Sizeof(x)
        | ExprKind::Cast(x)
        | ExprKind::Coerce(x)
        | ExprKind::Unary { operand: x, .. } => sub(x),
        ExprKind::Ctor { payload, .. } | ExprKind::Choose(payload) => payload.iter_mut().for_each(|x| sub(x)),
        ExprKind::Binary { lhs, rhs, .. } => {
            sub(lhs);
            sub(rhs);
        }
        ExprKind::Contains { item, collection } => {
            sub(item);
            sub(collection);
        }
        // format strings are messages, and other expressions have no parts
        _ => {}
    }
}

fn op_text(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::Then => "==>",
        BinOp::Iff => "<==>",
    }
}

/// The source of `span` on one line, shortened.
fn code(sm: &SourceMap, span: Span) -> String {
    shorten(&sm.text(span).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The change of `target`, where in the source it is, and how it reads.
fn describe(sm: &SourceMap, target: &Target<'_>) -> (Change, Span, String) {
    match target {
        Target::Stmt(s, c @ Change::Remove) if compound(s) => {
            (*c, s.span, format!("removed the whole `{}`", snippet(sm, s.span)))
        }
        Target::Stmt(s, c @ Change::Remove) => (*c, s.span, format!("removed `{}`", snippet(sm, s.span))),
        Target::Stmt(s, c @ Change::Force(v)) => {
            let StmtKind::If { cond, .. } = &s.kind else { unreachable!() };
            (*c, s.span, format!("`{}` replaced by `{}`", code(sm, cond.span), v))
        }
        Target::Expr(e, c @ Change::Operator(new)) => {
            let ExprKind::Binary { op, lhs, rhs } = &e.kind else { unreachable!() };
            let (old, new) = (op_text(*op), op_text(*new));
            let whole = code(sm, e.span);
            // the operator is between the operands
            let replaced = (lhs.span.file == e.span.file && rhs.span.file == e.span.file)
                .then(|| {
                    let (a, b) = (lhs.span.end.checked_sub(e.span.start)?, rhs.span.start.checked_sub(e.span.start)?);
                    let text = sm.text(e.span);
                    let between = text.get(a as usize..b as usize)?;
                    let at = between.find(old)?;
                    let mut s = text.to_string();
                    s.replace_range(a as usize + at..a as usize + at + old.len(), new);
                    Some(shorten(&s.split_whitespace().collect::<Vec<_>>().join(" ")))
                })
                .flatten();
            let description = match replaced {
                Some(r) => format!("`{}` → `{}`", whole, r),
                None => format!("`{}` → `{}` in `{}`", old, new, whole),
            };
            (*c, e.span, description)
        }
        Target::Expr(e, c @ (Change::DropConjunct { left } | Change::DropDisjunct { left })) => {
            let ExprKind::Binary { lhs, rhs, .. } = &e.kind else { unreachable!() };
            let kept = if *left { rhs } else { lhs };
            (*c, e.span, format!("`{}` → `{}`", code(sm, e.span), code(sm, kept.span)))
        }
        Target::Expr(e, c @ Change::DropNot) => {
            let ExprKind::Unary { operand, .. } = &e.kind else { unreachable!() };
            (*c, e.span, format!("`{}` → `{}`", code(sm, e.span), code(sm, operand.span)))
        }
        Target::Expr(e, c @ Change::FlipBool) => {
            let ExprKind::BoolLit(b) = e.kind else { unreachable!() };
            (*c, e.span, format!("`{}` → `{}`", b, !b))
        }
        _ => unreachable!("no such mutant"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "event eReq: int;\n\
        event eSeen: int;\n\
        machine Server {\n\
          var total: int;\n\
          start state S {\n\
            on eReq do (n: int) {\n\
              if (n < 3 && !(n == 0)) { total = total + n; }\n\
              announce eSeen, total;\n\
            }\n\
          }\n\
        }\n\
        spec Watch observes eSeen {\n\
          var last: int;\n\
          start state S { on eSeen do (t: int) { last = t; assert t >= 0, \"negative\"; } }\n\
        }\n\
        machine Main { start state S { entry { var s: Server; s = new Server(); send s, eReq, 1; } } }\n\
        test tc [main=Main]: assert Watch in { Server, Main };\n";

    fn program() -> Program {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", MODEL.to_string());
        crate::sema::compile(&sm, &crate::sema::Options::default())
            .unwrap_or_else(|_| panic!("does not compile"))
            .program
    }

    #[test]
    fn mutates_the_model_but_not_specs_or_test_drivers() {
        let prog = program();
        let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
        let ms = mutants(&prog, &funs, &probes);
        let described: Vec<_> = ms.iter().map(|m| (m.line, m.change.name(), m.description.as_str())).collect();
        assert_eq!(
            described,
            [
                (6, "empty-body", "removed the whole body (2 statements)"),
                (7, "remove-statement", "removed the whole `if (n < 3 && !(n == 0)) { total = total + n; }`"),
                (7, "condition-true", "`n < 3 && !(n == 0)` replaced by `true`"),
                (7, "condition-false", "`n < 3 && !(n == 0)` replaced by `false`"),
                (7, "swap-operator", "`n < 3 && !(n == 0)` → `n < 3 || !(n == 0)`"),
                (7, "drop-conjunct", "`n < 3 && !(n == 0)` → `!(n == 0)`"),
                (7, "drop-conjunct", "`n < 3 && !(n == 0)` → `n < 3`"),
                (7, "swap-operator", "`n < 3` → `n <= 3`"),
                (7, "remove-not", "`!(n == 0)` → `n == 0`"),
                (7, "swap-operator", "`n == 0` → `n != 0`"),
                (7, "remove-statement", "removed `total = total + n;`"),
                (7, "swap-operator", "`total + n` → `total - n`"),
                (8, "remove-statement", "removed `announce eSeen, total;`"),
            ]
        );
        assert!(ms.iter().all(|m| m.context == "Server.S, on eReq"));
        // the body contains every other mutant; the `if` everything on line 7
        let parents = parents(&ms);
        assert_eq!(parents[0], None);
        assert!(parents[1..]
            .iter()
            .zip(&ms[1..])
            .all(|(p, m)| *p == Some(if m.line == 7 && !m.coarse { 1 } else { 0 })));
    }

    #[test]
    fn ptst_global_helpers_are_not_mutated_but_their_design_callees_are() {
        for path in ["PTst/Network.p", "sample/ptst/Network.p", r"sample\PTst\Network.p"] {
            let mut sm = crate::syntax::SourceMap::new();
            sm.add(path, "fun Pick(): int { return DesignHelper() + 1; }");
            sm.add(
                "PSrc/Design.p",
                r#"
                fun DesignHelper(): int { return 2 + 1; }
                machine Server { start state S { entry { assert Pick() > 0; } } }
                machine Main { start state S { entry { new Server(); } } }
                test tc [main=Main]: { Main, Server };
            "#,
            );
            let prog = crate::sema::compile(&sm, &Default::default()).unwrap().program;
            let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
            let selected = |name| funs[prog.funs.iter().position(|f| f.name == name).unwrap()];
            assert!(!selected("Pick"), "{path}");
            assert!(selected("DesignHelper"), "a test helper must not hide production callees");
            let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
            let ms = mutants(&prog, &funs, &probes);
            assert!(ms.iter().all(|m| m.file != path));
            assert!(ms.iter().any(|m| m.context == "DesignHelper"));
        }
    }

    #[test]
    fn leaves_choices_alone() {
        let src = MODEL.replace("if (n < 3 && !(n == 0))", "if ($ && choose(3) == 0)");
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src);
        let prog = crate::sema::compile(&sm, &crate::sema::Options::default()).unwrap_or_else(|_| panic!()).program;
        let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
        let names: Vec<_> = mutants(&prog, &funs, &probes).iter().map(|m| m.description.clone()).collect();
        assert_eq!(
            names,
            [
                "removed the whole body (2 statements)",
                "removed the whole `if ($ && choose(3) == 0) { total = total + n; }`",
                "removed `total = total + n;`",
                "`total + n` → `total - n`",
                "removed `announce eSeen, total;`"
            ]
        );
    }

    #[test]
    fn removing_an_assignment_with_an_effect_always_changes_something() {
        let src = "event eTick: int;\n\
            fun Stamp(): int;\n\
            fun Twice(n: int): int { var m: int; m = n + n; return m; }\n\
            machine Server {\n\
              var handles: int;\n\
              var a: int; var b: int; var c: int; var d: int;\n\
              fun Mark(): int { handles = handles + 1; return 0; }\n\
              start state S {\n\
                on eTick do (n: int) {\n\
                  a = Mark();\n\
                  b = Stamp();\n\
                  c = Twice(n);\n\
                  d = n;\n\
                }\n\
              }\n\
            }\n\
            machine Main { start state S { entry { var s: Server; s = new Server(); send s, eTick, 0; } } }\n\
            test tc [main=Main]: { Server, Main };\n";
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src.to_string());
        let prog = crate::sema::compile(&sm, &crate::sema::Options::default()).unwrap_or_else(|_| panic!()).program;
        let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
        let ms = mutants(&prog, &funs, &probes);
        let watched = |what: &str| {
            let m = ms.iter().find(|m| m.description == format!("removed `{what}`")).unwrap();
            !m.changes.is_empty()
        };
        // the call writes a field, or is foreign: removing it changes more than `a`, `b`
        assert!(!watched("a = Mark();"));
        assert!(!watched("b = Stamp();"));
        // a function that only computes, and a plain value, change only the variable
        assert!(watched("c = Twice(n);"));
        assert!(watched("d = n;"));
    }

    #[test]
    fn change_names_are_the_suppressible_mutations() {
        let names = [
            Change::Remove,
            Change::Force(true),
            Change::Force(false),
            Change::Operator(BinOp::Lt),
            Change::DropNot,
            Change::FlipBool,
            Change::EmptyBody,
            Change::DropConjunct { left: true },
            Change::DropDisjunct { left: true },
            Change::RemoveDefer,
            Change::DeferToIgnore,
            Change::RemoveIgnore,
        ]
        .map(Change::name);
        assert_eq!(names.as_slice(), snappea_lint::MUTATIONS);
    }

    #[test]
    fn each_dropped_boolean_clause_is_detected_by_the_truth_table() {
        use crate::checker::engine::{Checker, CheckerOptions};
        use crate::checker::strategy::StrategyKind;
        for (condition, count) in [("p.a && p.b", 2), ("p.a || p.b", 2), ("p.a && (p.b || p.c)", 4)] {
            let source = format!(
                r#"
                machine Server {{ start state S {{ entry (p: (a: bool, b: bool, c: bool)) {{
                    var accepted: bool;
                    if ({condition}) {{ accepted = true; }}
                    assert accepted == ({condition}), "missing guard clause";
                }} }} }}
                machine Main {{ start state S {{ entry {{ new Server((a = $, b = $, c = $)); }} }} }}
                test tc [main=Main]: {{ Main, Server }};
            "#
            );
            let mut sm = SourceMap::new();
            sm.add("model.p", source);
            let prog = crate::sema::compile(&sm, &Default::default()).unwrap().program;
            let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
            let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
            let mutations: Vec<_> = mutants(&prog, &funs, &probes)
                .into_iter()
                .filter(|m| matches!(m.change, Change::DropConjunct { .. } | Change::DropDisjunct { .. }))
                .collect();
            assert_eq!(mutations.len(), count, "{condition}");
            let options = CheckerOptions { strategy: StrategyKind::Dfs, iterations: 0, ..Default::default() };
            let run = |p: &Program| Checker::new(p).check(&p.test_cases()[0], &options, &mut Vec::new()).unwrap();
            assert_eq!(run(&prog).report.num_bugs, 0);
            for m in mutations {
                let result = run(&apply(&prog, &m));
                assert!(result.report.num_bugs > 0, "{}", m.description);
                assert!(
                    result.report.bug_reports.iter().any(|b| b.contains("missing guard clause")),
                    "{:?}",
                    result.report.bug_reports
                );
            }
        }
    }

    #[test]
    fn applies_one_mutant() {
        let prog = program();
        let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
        let ms = mutants(&prog, &funs, &probes);
        let body = |p: &Program, m: &Mutant| format!("{:?}", p.fun(m.fun.unwrap()).body);
        for m in &ms {
            let mutated = apply(&prog, m);
            assert_ne!(body(&prog, m), body(&mutated, m), "{}", m.description);
            // nothing else changes
            for (i, f) in prog.funs.iter().enumerate().filter(|(i, _)| *i != m.fun.unwrap().idx()) {
                assert_eq!(format!("{:?}", f.body), format!("{:?}", mutated.funs[i].body));
            }
        }
        let removed = ms.iter().find(|m| m.change == Change::Remove && m.line == 8).unwrap();
        let mutated = apply(&prog, removed);
        let stmts = &mutated.fun(removed.fun.unwrap()).body.as_ref().unwrap().stmts;
        assert!(matches!(stmts.last().unwrap().kind, StmtKind::Nop));
    }
}
