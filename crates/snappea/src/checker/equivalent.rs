//! Mutants no test case can detect, found from the code alone (see
//! [`crate::checker::mutation`]):
//!
//! - removing an `if` without `else` whose condition is a nondeterministic choice (`$`,
//!   `choose()`): the mutant does what the original does when the choice is `false`,
//!   which the checker explores anyway (fault injection such as `if ($) { return; }`);
//! - swapping `>` and `>=` (or `<` and `<=`) in a guard that only assigns one operand to
//!   the other (`if (a > b) { b = a; }`): at equality the branch assigns the value
//!   already there;
//! - counting a freshness counter down instead of up (`version = version + 1`): when its
//!   values are only copied and compared with `==` and `!=` (or with 0), every value is
//!   still new.
//!
//! [`FaultModel`] finds a fourth kind, which still runs and is only judged when no test
//! case detects it: emptying the handler of an event every test case sends only
//! optionally (a crash injected with `if ($) { send server, eCrash; }`).

use rustc_hash::{FxHashMap, FxHashSet};

use crate::sema::{BinOp, Block, EventId, Expr, ExprKind, FunId, HandlerAction, PType, Program, Stmt, StmtKind};
use crate::sema::{TestCase, UnOp, VarId, VarRole};
use crate::syntax::Span;

/// Why removing the `if` statement `s` cannot be detected, if it cannot.
pub fn removed_choice_branch(s: &Stmt) -> Option<&'static str> {
    let StmtKind::If { cond, else_branch, .. } = &s.kind else { return None };
    (bare_choice(cond) && empty(else_branch))
        .then_some("the condition is a nondeterministic choice, so the original already runs without this branch")
}

/// `$` or `choose()`, possibly negated: a choice the checker takes both ways, with no
/// fairness requirement (unlike `$$`).
fn bare_choice(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Nondet | ExprKind::Choose(None) => true,
        ExprKind::Unary { op: UnOp::Not, operand } => bare_choice(operand),
        _ => false,
    }
}

fn empty(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Nop => true,
        StmtKind::Block(b) => b.stmts.iter().all(empty),
        _ => false,
    }
}

/// The statements `s` runs, through blocks: one statement for `{ x = 1; }`.
fn only(s: &Stmt) -> Vec<&Stmt> {
    match &s.kind {
        StmtKind::Block(b) => b.stmts.iter().flat_map(only).collect(),
        StmtKind::Nop => Vec::new(),
        _ => vec![s],
    }
}

/// Whether `a` and `b` are the same variable or field path, with nothing to evaluate
/// but variables and tuple fields.
fn same_place(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
        (ExprKind::TupleAccess { base: x, index: i }, ExprKind::TupleAccess { base: y, index: j })
        | (ExprKind::NamedTupleAccess { base: x, index: i }, ExprKind::NamedTupleAccess { base: y, index: j }) => {
            i == j && same_place(x, y)
        }
        _ => false,
    }
}

/// The conditions in `body` of guards that only assign one compared value to the other
/// (`if (a > b) { b = a; }`), with why swapping `<`/`<=` or `>`/`>=` there changes
/// nothing.
pub fn self_assigning_guards(body: &Block) -> FxHashMap<Span, &'static str> {
    let mut out = FxHashMap::default();
    crate::checker::checked::visit_stmts(body, &mut |s| {
        let StmtKind::If { cond, then_branch, else_branch } = &s.kind else { return };
        let ExprKind::Binary { op: BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, lhs, rhs } = &cond.kind else {
            return;
        };
        let [assign] = only(then_branch)[..] else { return };
        let StmtKind::Assign { lhs: to, rhs: from } = &assign.kind else { return };
        let swapped = (same_place(to, lhs) && same_place(from, rhs)) || (same_place(to, rhs) && same_place(from, lhs));
        if swapped && empty(else_branch) {
            out.insert(cond.span, "at equality the branch assigns the value already there");
        }
    });
    out
}

/// Finds freshness counters: fields only incremented in one place, reset to 0, copied
/// and compared with `==` or `!=`, for which counting down is as good as counting up.
pub struct Freshness<'p> {
    prog: &'p Program,
    counters: FxHashMap<VarId, bool>,
}

impl<'p> Freshness<'p> {
    pub fn new(prog: &'p Program) -> Self {
        Freshness { prog, counters: FxHashMap::default() }
    }

    /// For an increment `f = f + k` (or `- k`) of an `int` field `f` by a literal: the
    /// span of `f + k`, and why changing its operator cannot be detected, if it cannot.
    pub fn counter(&mut self, s: &Stmt) -> Option<(Span, String)> {
        let StmtKind::Assign { lhs, rhs } = &s.kind else { return None };
        let field = increment(lhs, rhs)?;
        let decl = self.prog.var(field);
        if decl.role != VarRole::Field || !matches!(decl.ty.canonicalize(self.prog), PType::Int) {
            return None;
        }
        let prog = self.prog;
        let fresh = *self.counters.entry(field).or_insert_with(|| freshness_only(prog, field));
        fresh.then(|| {
            (
                rhs.span,
                format!(
                    "'{}' is only copied and compared with == and !=, so counting down gives values as new as counting up",
                    decl.name
                ),
            )
        })
    }
}

/// The field `lhs` when `lhs = rhs` is `f = f + k` or `f = f - k` with a literal `k`.
fn increment(lhs: &Expr, rhs: &Expr) -> Option<VarId> {
    let ExprKind::Var(f) = lhs.kind else { return None };
    let ExprKind::Binary { op: BinOp::Add | BinOp::Sub, lhs: a, rhs: k } = &rhs.kind else { return None };
    (matches!(a.kind, ExprKind::Var(g) if g == f) && matches!(k.kind, ExprKind::IntLit(_))).then_some(f)
}

/// What carries a counter's values: variables, named tuple fields by name, and the
/// payloads of events.
#[derive(Default)]
struct Carriers {
    vars: FxHashSet<VarId>,
    names: FxHashSet<String>,
    events: FxHashSet<EventId>,
    changed: bool,
}

/// The functions that receive the payloads of each event: handlers, `goto` functions and
/// entry functions, and `receive` cases.
fn payload_funs(prog: &Program) -> FxHashMap<EventId, Vec<FunId>> {
    let mut out: FxHashMap<EventId, Vec<FunId>> = FxHashMap::default();
    for st in &prog.states {
        for h in &st.handlers {
            let funs = match h.action {
                HandlerAction::Do(f) => vec![Some(f)],
                HandlerAction::Goto { target, with } => vec![with, prog.state(target).entry],
                _ => Vec::new(),
            };
            out.entry(h.event).or_default().extend(funs.into_iter().flatten());
        }
    }
    for f in &prog.funs {
        let Some(body) = &f.body else { continue };
        crate::checker::checked::visit_stmts(body, &mut |s| {
            if let StmtKind::Receive { cases } = &s.kind {
                for (e, f) in cases {
                    out.entry(*e).or_default().push(*f);
                }
            }
        });
    }
    out
}

/// Whether the values of `field` only ever go where negating them all would change
/// nothing: copied into variables, named tuple fields and parameters, compared with `==`
/// or `!=` against each other or 0, reset to 0, and changed only by its one increment.
/// What a carrier is copied into or compared with becomes a carrier too, until nothing
/// changes.
/// Then a mutant that counts down produces the negation of every value the original
/// does.
fn freshness_only(prog: &Program, field: VarId) -> bool {
    let mut c = Carriers::default();
    c.vars.insert(field);
    let receivers = payload_funs(prog);
    loop {
        c.changed = false;
        let mut increments = 0;
        for f in &prog.funs {
            let Some(body) = &f.body else { continue };
            let mut ok = true;
            crate::checker::checked::visit_stmts(body, &mut |s| ok &= c.stmt(prog, s, field, &mut increments));
            if !ok {
                return false;
            }
        }
        if increments != 1 {
            return false;
        }
        // a payload that carries reaches its receivers' parameter, and a receiver's
        // parameter that carries needs every payload of its events to
        for (e, funs) in &receivers {
            let params: Vec<VarId> = funs.iter().filter_map(|f| prog.fun(*f).params.first().copied()).collect();
            if c.events.contains(e) || params.iter().any(|p| c.vars.contains(p)) {
                c.changed |= c.events.insert(*e);
                for p in params {
                    c.changed |= c.vars.insert(p);
                }
            }
        }
        if !c.changed {
            return true;
        }
    }
}

impl Carriers {
    /// The named tuple field `index` of `base`'s type.
    fn name(prog: &Program, ty: &PType, index: u32) -> Option<String> {
        match ty.canonicalize(prog) {
            PType::NamedTuple(fs) => fs.get(index as usize).map(|(n, _)| n.clone()),
            _ => None,
        }
    }

    /// Whether reading `e` reads a carrier.
    fn carries(&self, prog: &Program, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Var(v) => self.vars.contains(v),
            ExprKind::NamedTupleAccess { base, index } => {
                Self::name(prog, &base.ty, *index).is_some_and(|n| self.names.contains(&n))
            }
            _ => false,
        }
    }

    fn zero(e: &Expr) -> bool {
        matches!(e.kind, ExprKind::IntLit(0) | ExprKind::Default)
    }

    /// Makes what `target` writes a carrier; `false` when it is not a variable or named
    /// tuple field.
    fn add(&mut self, prog: &Program, target: &Expr) -> bool {
        let added = match &target.kind {
            ExprKind::Var(v) => self.vars.insert(*v),
            ExprKind::NamedTupleAccess { base, index } => match Self::name(prog, &base.ty, *index) {
                Some(n) => self.names.insert(n),
                None => return false,
            },
            _ => return false,
        };
        self.changed |= added;
        true
    }

    /// Checks one statement (not the ones nested in it), counting increments of `field`.
    fn stmt(&mut self, prog: &Program, s: &Stmt, field: VarId, increments: &mut usize) -> bool {
        match &s.kind {
            StmtKind::Assign { lhs, rhs } => {
                if increment(lhs, rhs) == Some(field) {
                    *increments += 1;
                    return true;
                }
                let target = self.carries(prog, lhs);
                if self.carries(prog, rhs) {
                    return self.add(prog, lhs) && self.parts(prog, lhs);
                }
                // anything else written into a carrier breaks the symmetry
                if target && !Self::zero(rhs) {
                    return false;
                }
                self.parts(prog, lhs) && self.expr(prog, rhs)
            }
            StmtKind::Insert { lhs, index, value } => {
                self.plain(prog, lhs) && self.expr(prog, index) && self.expr(prog, value)
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                self.plain(prog, lhs) && self.expr(prog, value)
            }
            StmtKind::FunCall { fun, args } => self.call(prog, *fun, args),
            StmtKind::Return(Some(e)) | StmtKind::Print(e) => self.expr(prog, e),
            StmtKind::While { cond, .. } | StmtKind::If { cond, .. } => self.expr(prog, cond),
            StmtKind::Foreach { collection, .. } => self.expr(prog, collection),
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                self.expr(prog, cond) && self.expr(prog, message)
            }
            StmtKind::Send { target, event, payload } => {
                self.expr(prog, target) && self.payload(prog, event, payload.as_ref())
            }
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.payload(prog, event, payload.as_ref())
            }
            StmtKind::Goto { payload, .. } | StmtKind::Ctor { payload, .. } => {
                payload.as_ref().is_none_or(|p| self.expr(prog, p))
            }
            _ => true,
        }
    }

    /// The payload of an event: a carrier makes the event's payloads carriers.
    fn payload(&mut self, prog: &Program, event: &Expr, payload: Option<&Expr>) -> bool {
        let Some(p) = payload else { return self.expr(prog, event) };
        let ExprKind::EventRef(e) = event.kind else { return self.expr(prog, event) && self.expr(prog, p) };
        // a payload converted to the event's type is the same value
        let p = match &p.kind {
            ExprKind::Coerce(x) | ExprKind::Cast(x) if self.carries(prog, x) => x,
            _ => p,
        };
        if self.carries(prog, p) {
            self.changed |= self.events.insert(e);
            return true;
        }
        if self.events.contains(&e) && !Self::zero(p) {
            return false;
        }
        self.expr(prog, p)
    }

    /// A target no carrier is written into: its parts are checked like any value.
    fn plain(&mut self, prog: &Program, target: &Expr) -> bool {
        !self.carries(prog, target) && self.expr(prog, target)
    }

    /// The parts of an assignment target that are evaluated (indexes and keys).
    fn parts(&mut self, prog: &Program, target: &Expr) -> bool {
        match &target.kind {
            ExprKind::Var(_) => true,
            ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => self.parts(prog, base),
            ExprKind::SeqAccess { base, index: key } | ExprKind::MapAccess { base, key } => {
                self.parts(prog, base) && self.expr(prog, key)
            }
            _ => self.expr(prog, target),
        }
    }

    fn call(&mut self, prog: &Program, fun: FunId, args: &[Expr]) -> bool {
        let params = &prog.fun(fun).params;
        for (i, a) in args.iter().enumerate() {
            let Some(&p) = params.get(i) else { return false };
            if self.carries(prog, a) {
                self.changed |= self.vars.insert(p);
            } else if (self.vars.contains(&p) && !Self::zero(a)) || !self.expr(prog, a) {
                return false;
            }
        }
        true
    }

    /// Whether `e` uses carriers only as allowed: compared with `==`/`!=` against
    /// another or 0, or copied into a named tuple field or a parameter.
    fn expr(&mut self, prog: &Program, e: &Expr) -> bool {
        if self.carries(prog, e) {
            return false;
        }
        match &e.kind {
            ExprKind::Binary { op: BinOp::Eq | BinOp::Neq, lhs, rhs } => {
                // what a carrier is compared with must carry the same kind of value
                let (l, r) = (self.carries(prog, lhs), self.carries(prog, rhs));
                match (l, r) {
                    (true, true) => true,
                    (true, false) => Self::zero(rhs) || self.add(prog, rhs),
                    (false, true) => Self::zero(lhs) || self.add(prog, lhs),
                    (false, false) => self.expr(prog, lhs) && self.expr(prog, rhs),
                }
            }
            ExprKind::NamedTuple(es) => {
                let PType::NamedTuple(fields) = e.ty.canonicalize(prog) else { return false };
                for ((name, _), x) in fields.iter().zip(es) {
                    if self.carries(prog, x) {
                        self.changed |= self.names.insert(name.clone());
                    } else if (self.names.contains(name) && !Self::zero(x)) || !self.expr(prog, x) {
                        return false;
                    }
                }
                true
            }
            ExprKind::FunCall { fun, args } => self.call(prog, *fun, args),
            _ => {
                let mut ok = true;
                crate::checker::mutation::visit_children(e, &mut |x| ok &= self.expr(prog, x));
                ok
            }
        }
    }
}

/// Emptying an event handler where the event is optional: the mutant's runs where the
/// event arrives look like the original's runs where it was never sent.
///
/// An event is optional in a test case when every send or raise of it in the test
/// case's machines (and in global functions) is in a branch of an `if` on `$` or
/// `choose()` that does nothing but send. The mutants judged this way empty a `do`
/// handler of optional events only, or remove its one statement, or empty a function
/// that is the one statement of such handlers; the function must run nowhere else (not
/// as an entry, exit or `goto ... with` function, a `receive` case, or another call).
pub struct FaultModel {
    /// For each event: the test cases (by index) it is optional in.
    optional: FxHashMap<EventId, Vec<bool>>,
    /// The events each handler function handles.
    handles: FxHashMap<FunId, Vec<EventId>>,
    /// For each function: the functions whose one statement calls it, and whether every
    /// call of it is such a statement.
    callers: FxHashMap<FunId, (Vec<FunId>, bool)>,
    /// Functions that also run other than as a `do` handler or the one statement of one.
    elsewhere: FxHashSet<FunId>,
}

impl FaultModel {
    pub fn new(prog: &Program, tests: &[TestCase]) -> FaultModel {
        // sends of each event, as (owner machine, optional)
        let mut sends: FxHashMap<EventId, Vec<(Option<crate::sema::MachineId>, bool)>> = FxHashMap::default();
        let mut callers: FxHashMap<FunId, (Vec<FunId>, bool)> = FxHashMap::default();
        for (i, f) in prog.funs.iter().enumerate() {
            let Some(body) = &f.body else { continue };
            let owner = f.owner;
            let mut optional_sends: FxHashSet<Span> = FxHashSet::default();
            crate::checker::checked::visit_stmts(body, &mut |s| {
                let StmtKind::If { cond, then_branch, else_branch } = &s.kind else { return };
                if !bare_choice(cond) {
                    return;
                }
                for branch in [then_branch, else_branch] {
                    let stmts = only(branch);
                    if !stmts.is_empty() && stmts.iter().all(|s| matches!(s.kind, StmtKind::Send { .. })) {
                        optional_sends.extend(stmts.iter().map(|s| s.span));
                    }
                }
            });
            crate::checker::checked::visit_stmts(body, &mut |s| match &s.kind {
                StmtKind::Send { event, .. } | StmtKind::Raise { event, .. } => {
                    let ExprKind::EventRef(e) = event.kind else { return };
                    sends.entry(e).or_default().push((owner, optional_sends.contains(&s.span)));
                }
                _ => {}
            });
            // calls: as the one statement of the body, or anywhere else
            let sole = match only_body(body)[..] {
                [s] => match &s.kind {
                    StmtKind::FunCall { fun, .. } => Some(*fun),
                    _ => None,
                },
                _ => None,
            };
            let mut calls = Vec::new();
            crate::checker::checked::visit_stmts(body, &mut |s| {
                if let StmtKind::FunCall { fun, .. } = &s.kind {
                    calls.push(*fun);
                }
                each_expr(s, &mut |e| {
                    crate::checker::mutation::visit_expr(e, &mut |x| {
                        if let ExprKind::FunCall { fun, .. } = x.kind {
                            calls.push(fun);
                        }
                    })
                });
            });
            for callee in calls {
                let entry = callers.entry(callee).or_insert((Vec::new(), true));
                if sole == Some(callee) {
                    entry.0.push(FunId(i as u32));
                } else {
                    entry.1 = false;
                }
            }
        }
        let optional = sends
            .into_iter()
            .map(|(e, list)| {
                let per_test = tests
                    .iter()
                    .map(|tc| {
                        let (module, _) = prog.test_case_module(tc);
                        let in_test = |m: Option<crate::sema::MachineId>| {
                            m.is_none_or(|m| module.interface_def.iter().any(|(_, d)| *d == m))
                        };
                        list.iter().all(|(m, optional)| *optional || !in_test(*m))
                    })
                    .collect();
                (e, per_test)
            })
            .collect();
        let mut handles: FxHashMap<FunId, Vec<EventId>> = FxHashMap::default();
        let mut elsewhere: FxHashSet<FunId> = FxHashSet::default();
        for s in &prog.states {
            elsewhere.extend(s.entry.into_iter().chain(s.exit));
            for h in &s.handlers {
                match h.action {
                    HandlerAction::Do(f) => handles.entry(f).or_default().push(h.event),
                    HandlerAction::Goto { with: Some(f), .. } => {
                        elsewhere.insert(f);
                    }
                    _ => {}
                }
            }
        }
        for funs in payload_funs(prog).into_values() {
            elsewhere.extend(funs.into_iter().filter(|f| !handles.contains_key(f)));
        }
        elsewhere.extend(callers.iter().filter(|(_, (_, only))| !only).map(|(f, _)| *f));
        FaultModel { optional, handles, callers, elsewhere }
    }

    /// Why the undetected mutant `m` is likely equivalent, given the test cases (by index)
    /// that ran it, if it is.
    pub fn judge(&self, prog: &Program, m: &crate::checker::mutation::Mutant, tests: &[usize]) -> Option<String> {
        use crate::checker::mutation::Change;
        let body = prog.fun(m.fun).body.as_ref()?;
        let whole = match m.change {
            Change::EmptyBody => true,
            Change::Remove => matches!(only_body(body)[..], [s] if s.span == m.span),
            _ => false,
        };
        if !whole || tests.is_empty() {
            return None;
        }
        let mut events = Vec::new();
        if !self.fault_fun(m.fun, tests, &mut events, 0) {
            return None;
        }
        events.sort();
        events.dedup();
        let names: Vec<String> = events.iter().map(|e| format!("'{}'", prog.event(*e).name)).collect();
        Some(format!(
            "fault model: every send of {} is optional ($), so a handler that does nothing is the history where it was not sent",
            names.join(", ")
        ))
    }

    /// Whether `f` handles only events optional in `tests`, or is the one statement of
    /// such handlers only; collects the events.
    fn fault_fun(&self, f: FunId, tests: &[usize], events: &mut Vec<EventId>, depth: usize) -> bool {
        if depth > 4 || self.elsewhere.contains(&f) {
            return false;
        }
        if let Some(handled) = self.handles.get(&f) {
            let optional = |e: &EventId| self.optional.get(e).is_some_and(|o| tests.iter().all(|t| o[*t]));
            if handled.iter().all(optional) {
                events.extend(handled);
                return true;
            }
            return false;
        }
        match self.callers.get(&f) {
            Some((callers, true)) if !callers.is_empty() => {
                callers.iter().all(|c| self.fault_fun(*c, tests, events, depth + 1))
            }
            _ => false,
        }
    }
}

/// The statements of a body with an effect: all but `return` and `nop`.
fn only_body(b: &Block) -> Vec<&Stmt> {
    b.stmts.iter().flat_map(only).filter(|s| !matches!(s.kind, StmtKind::Return(None))).collect()
}

/// Calls `f` with the expressions statement `s` itself evaluates.
fn each_expr(s: &Stmt, f: &mut dyn FnMut(&Expr)) {
    match &s.kind {
        StmtKind::Assign { lhs, rhs } => {
            f(lhs);
            f(rhs);
        }
        StmtKind::Insert { lhs, index, value } => {
            f(lhs);
            f(index);
            f(value);
        }
        StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
            f(lhs);
            f(value);
        }
        StmtKind::Return(Some(e)) | StmtKind::Print(e) => f(e),
        StmtKind::While { cond, .. } | StmtKind::If { cond, .. } => f(cond),
        StmtKind::Foreach { collection, .. } => f(collection),
        StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
            f(cond);
            f(message);
        }
        StmtKind::Send { target, event, payload } => {
            f(target);
            f(event);
            if let Some(p) = payload {
                f(p);
            }
        }
        StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
            f(event);
            if let Some(p) = payload {
                f(p);
            }
        }
        StmtKind::Goto { payload: Some(p), .. } | StmtKind::Ctor { payload: Some(p), .. } => f(p),
        StmtKind::FunCall { args, .. } => args.iter().for_each(f),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::checker::mutation::{mutable_funs, mutants, Mutant};
    use crate::sema::Program;

    fn program(src: &str) -> Program {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src.to_string());
        crate::sema::compile(&sm, &crate::sema::Options::default())
            .unwrap_or_else(|_| panic!("does not compile"))
            .program
    }

    fn all(prog: &Program) -> Vec<Mutant> {
        let funs = mutable_funs(prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(prog).probes;
        mutants(prog, &funs, &probes)
    }

    /// Whether the mutant described `what` is skipped as equivalent.
    fn skipped(ms: &[Mutant], what: &str) -> bool {
        let m = ms
            .iter()
            .find(|m| m.description == what)
            .unwrap_or_else(|| panic!("no mutant {what}: {:?}", ms.iter().map(|m| &m.description).collect::<Vec<_>>()));
        m.equivalent.is_some()
    }

    const STORE: &str = "type tRec = (value: int, version: int);
        event eLoad: machine; event eLoaded: tRec;
        event eCas: (client: machine, expected: int, value: int); event eCasResp: bool;
        event eSeen: int;
        machine Store {
          var value: int; var version: int; var high: int;
          start state S {
            on eLoad do (c: machine) { send c, eLoaded, (value = value, version = version); }
            on eCas do (r: (client: machine, expected: int, value: int)) {
              if (r.expected != version) { send r.client, eCasResp, false; return; }
              value = r.value;
              version = version + 1;
              if (value > high) { high = value; }
              announce eSeen, high;
              send r.client, eCasResp, true;
            }
          }
        }
        spec Rising observes eSeen {
          var last: int;
          start state S { on eSeen do (v: int) { assert v >= last, \"falling\"; last = v; } }
        }
        machine Main {
          start state S {
            entry {
              var s: Store; var rec: tRec;
              s = new Store();
              send s, eLoad, this;
              receive { case eLoaded: (r: tRec) { rec = r; } }
              if ($) { send s, eCas, (client = this, expected = rec.version, value = rec.value + 1); }
            }
            ignore eCasResp;
          }
        }
        test tc [main=Main]: assert Rising in { Store, Main };";

    #[test]
    fn freshness_counters_and_self_assigning_guards_are_equivalent() {
        let ms = all(&program(STORE));
        assert!(skipped(&ms, "`version + 1` → `version - 1`"));
        assert!(skipped(&ms, "`value > high` → `value >= high`"));
        // removing the increment, or the guard, is not
        assert!(!skipped(&ms, "removed `version = version + 1;`"));
        assert!(!skipped(&ms, "removed the whole `if (value > high) { high = value; }`"));
        // a counter whose values are ordered is not only fresh
        let ordered = STORE.replace("if (r.expected != version)", "if (r.expected < version)");
        assert!(!skipped(&all(&program(&ordered)), "`version + 1` → `version - 1`"));
        // nor one that a spec orders, or that is set to another value
        let announced = STORE.replace("announce eSeen, high;", "announce eSeen, version;");
        assert!(!skipped(&all(&program(&announced)), "`version + 1` → `version - 1`"));
        let set = STORE.replace("value = r.value;", "value = r.value; version = 7;");
        assert!(!skipped(&all(&program(&set)), "`version + 1` → `version - 1`"));
        let twice = STORE.replace("value = r.value;", "value = r.value; version = version + 1;");
        assert!(!skipped(&all(&program(&twice)), "`version + 1` → `version - 1`"));
        // a guard that assigns something else
        let other = STORE.replace("{ high = value; }", "{ high = value + 1; }");
        assert!(!skipped(&all(&program(&other)), "`value > high` → `value >= high`"));
    }

    #[test]
    fn removing_a_branch_on_a_plain_choice_is_equivalent() {
        let src = "event eReq: machine; event eResp;
            machine Server { start state S { on eReq do (c: machine) {
              if ($) { return; }
              if ($$) { send c, eResp; }
              if ($) { send c, eResp; } else { send c, eResp; }
              send c, eResp;
            } } }
            machine Main { start state S { entry { send new Server(), eReq, this; } ignore eResp; } }
            test tc [main=Main]: { Server, Main };";
        let ms = all(&program(src));
        assert!(skipped(&ms, "removed the whole `if ($) { return; }`"));
        // a fair choice must take both ways, and an `else` runs either way
        assert!(!skipped(&ms, "removed the whole `if ($$) { send c, eResp; }`"));
        assert!(!skipped(&ms, "removed the whole `if ($) { send c, eResp; } else { send c, eResp; }`"));
        let m = ms.iter().find(|m| m.description == "removed `send c, eResp;`" && m.line == 5).unwrap();
        assert!(m.under_choice);
        assert!(!ms.iter().find(|m| m.line == 6).unwrap().under_choice);
    }

    #[test]
    fn emptying_the_handler_of_an_optional_fault_is_likely_equivalent() {
        let src = "event eCrash; event eReq: machine; event eResp;
            machine Server {
              var epoch: int;
              start state S {
                on eCrash do { Restart(); }
                on eReq do (c: machine) { send c, eResp; epoch = epoch; }
              }
              fun Restart() { epoch = epoch + 1; goto S; }
            }
            event eWipe;
            machine Cache {
              var n: int;
              start state S { entry Wipe; on eWipe do Wipe; on eReq do (c: machine) { n = n + 1; send c, eResp; } }
              fun Wipe() { n = 0; }
            }
            machine Crasher { start state S { entry (t: Server) {
              var c: Cache; c = new Cache();
              if ($) { send t, eCrash; }
              if ($) { send c, eWipe; }
              send c, eReq, this;
            } } }
            machine Main { start state S {
              entry { var s: Server; s = new Server(); new Crasher(s); send s, eReq, this; }
              ignore eResp;
            } }
            machine Always { start state S { entry (t: Server) { send t, eCrash; } } }
            machine Main2 { start state S {
              entry { var s: Server; s = new Server(); new Always(s); }
            } }
            test tc [main=Main]: { Server, Crasher, Cache, Main };
            test tc2 [main=Main2]: { Server, Always, Main2 };";
        let prog = program(src);
        let ms = all(&prog);
        let tests = prog.test_cases();
        let faults = super::FaultModel::new(&prog, &tests);
        let tc = tests.iter().position(|t| t.name == "tc").unwrap();
        let tc2 = tests.iter().position(|t| t.name == "tc2").unwrap();
        let judge = |what: &str, on: &[usize]| {
            let m = ms.iter().find(|m| m.description == what).unwrap();
            faults.judge(&prog, m, on)
        };
        let why = judge("removed `Restart();`", &[tc]).expect("an optional crash");
        assert!(why.contains("'eCrash'"), "{why}");
        assert!(judge("removed the whole body (2 statements)", &[tc]).is_some(), "Restart is only the crash handler");
        // part of the restart is not the history without a crash
        assert!(judge("removed `epoch = epoch + 1;`", &[tc]).is_none());
        // a test case that always crashes it
        assert!(judge("removed `Restart();`", &[tc, tc2]).is_none());
        // a request handler: its event is always sent
        assert!(judge("removed `send c, eResp;`", &[tc]).is_none());
        // a handler that is also an entry function runs without the fault too
        assert!(judge("removed `n = 0;`", &[tc]).is_none());
    }
}
