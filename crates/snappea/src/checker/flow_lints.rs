//! Lints on how machines wait for each other and leave states, over the type-checked
//! program (see [`crate::checker::checked::lints`], which runs them):
//!
//! * `unplanned-driver-stage`: a test's driver starts more model work only after the
//!   model notifies it, and announces no plan a spec observes. If the model never sends
//!   the notification, the later stage never starts and every spec passes.
//! * `unreachable-statement`: a statement follows a `goto`, a `raise`, or a call to a
//!   function that always does one, so it never runs.
//!
//! [`waiting_machines`] finds the request/reply waits `no-progress-spec` uses for design
//! machines.

use rustc_hash::FxHashSet;

use crate::checker::checked::{each_expr, machine_funs, visit_stmts, Finding};
use crate::sema::{
    Block, EventId, ExprKind, FunId, HandlerAction, MachineId, MachineKind, Program, StateId, Stmt, StmtKind,
    Temperature, TestDecl,
};
use crate::syntax::Span;

/// The functions `f` runs: itself, what it calls, and its receive cases.
fn reach(prog: &Program, f: FunId) -> Vec<FunId> {
    let mut seen: FxHashSet<FunId> = FxHashSet::default();
    let mut todo = vec![f];
    let mut out = Vec::new();
    while let Some(f) = todo.pop() {
        if !seen.insert(f) {
            continue;
        }
        out.push(f);
        todo.extend(prog.fun(f).callees.iter().copied());
        if let Some(b) = &prog.fun(f).body {
            visit_stmts(b, &mut |s| {
                if let StmtKind::Receive { cases } = &s.kind {
                    todo.extend(cases.iter().map(|(_, c)| *c));
                }
            });
        }
    }
    out
}

/// Calls `visit` with each statement the functions in `funs` run directly.
fn each_stmt(prog: &Program, funs: &[FunId], visit: &mut dyn FnMut(&Stmt)) {
    for f in funs {
        if let Some(b) = &prog.fun(*f).body {
            visit_stmts(b, visit);
        }
    }
}

/// The events `funs` send with `send` (not `announce`), by name.
fn sends(prog: &Program, funs: &[FunId]) -> FxHashSet<EventId> {
    let mut out = FxHashSet::default();
    each_stmt(prog, funs, &mut |s| {
        if let StmtKind::Send { event, .. } = &s.kind {
            if let ExprKind::EventRef(e) = event.kind {
                out.insert(e);
            }
        }
    });
    out
}

/// The function a handler runs, if any.
fn handler_fun(action: &HandlerAction) -> Option<FunId> {
    match action {
        HandlerAction::Do(f) => Some(*f),
        HandlerAction::Goto { with, .. } => *with,
        _ => None,
    }
}

/// The machines of test `t`'s module (with its main machine) that are not monitors.
fn machines_of(prog: &Program, t: &TestDecl) -> Vec<MachineId> {
    let mut out: Vec<MachineId> = t.module.interface_def.iter().map(|(_, m)| *m).collect();
    out.push(t.main);
    out.sort();
    out.dedup();
    out.retain(|m| !prog.machine(*m).is_spec());
    out
}

/// The events other machines of `machines` send while handling an event `x` sends: the
/// replies to `x`'s requests.
fn replies(prog: &Program, machines: &[MachineId], x: MachineId) -> FxHashSet<EventId> {
    let requests = sends(prog, &machine_funs(prog, x));
    let mut out = FxHashSet::default();
    for y in machines.iter().filter(|y| **y != x) {
        let decl = prog.machine(*y);
        let mut handlers: Vec<FunId> = Vec::new();
        for s in &decl.states {
            for h in &prog.state(*s).handlers {
                if requests.contains(&h.event) {
                    handlers.extend(handler_fun(&h.action));
                    if let HandlerAction::Goto { target, .. } = h.action {
                        handlers.extend(prog.state(target).entry);
                    }
                }
            }
        }
        each_stmt(prog, &machine_funs(prog, *y), &mut |s| {
            if let StmtKind::Receive { cases } = &s.kind {
                handlers.extend(cases.iter().filter(|(e, _)| requests.contains(e)).map(|(_, f)| *f));
            }
        });
        for f in handlers {
            out.extend(sends(prog, &reach(prog, f)));
        }
    }
    out
}

/// The events other machines of `machines` send, directly or through a chain of
/// handlers, while handling an event `x` sends: the replies to `x`'s requests, also
/// when they pass through a middle machine (`x` -> coordinator -> participant ->
/// coordinator -> `x`). Over-approximates: any handler of a downstream event counts.
fn downstream_replies(prog: &Program, machines: &[MachineId], x: MachineId) -> FxHashSet<EventId> {
    let others: Vec<MachineId> = machines.iter().copied().filter(|y| *y != x).collect();
    let mut seen: FxHashSet<EventId> = FxHashSet::default();
    let mut todo: Vec<EventId> = sends(prog, &machine_funs(prog, x)).into_iter().collect();
    let mut out = FxHashSet::default();
    while let Some(e) = todo.pop() {
        if !seen.insert(e) {
            continue;
        }
        for y in &others {
            for f in handlers_of(prog, *y, e) {
                for reply in sends(prog, &reach(prog, f)) {
                    out.insert(reply);
                    todo.push(reply);
                }
            }
        }
    }
    out
}

/// The functions machine `m` runs on receiving `event`: its handlers, the entry of the
/// state a `goto` handler enters, and `receive` cases.
fn handlers_of(prog: &Program, m: MachineId, event: EventId) -> Vec<FunId> {
    let mut out = Vec::new();
    for s in &prog.machine(m).states {
        for h in prog.state(*s).handlers.iter().filter(|h| h.event == event) {
            out.extend(handler_fun(&h.action));
            if let HandlerAction::Goto { target, .. } = h.action {
                out.extend(prog.state(target).entry);
            }
        }
    }
    each_stmt(prog, &machine_funs(prog, m), &mut |s| {
        if let StmtKind::Receive { cases } = &s.kind {
            out.extend(cases.iter().filter(|(e, _)| *e == event).map(|(_, f)| *f));
        }
    });
    out
}

/// The machines of test `t` that send a request and then wait for its reply: in a
/// `receive`, or in a state whose only handlers are for replies. Each wait comes with the
/// reply it waits for and where (the `receive` or the state).
pub fn waiting_machines(
    prog: &Program,
    t: &TestDecl,
    skip: &dyn Fn(MachineId) -> bool,
) -> Vec<(MachineId, EventId, Span)> {
    let machines = machines_of(prog, t);
    let mut out = Vec::new();
    for x in machines.iter().copied().filter(|x| !skip(*x)) {
        let replies = replies(prog, &machines, x);
        if replies.is_empty() {
            continue;
        }
        let mut waits: Vec<(EventId, Span)> = Vec::new();
        each_stmt(prog, &machine_funs(prog, x), &mut |s| {
            if let StmtKind::Receive { cases } = &s.kind {
                waits.extend(cases.iter().map(|(e, _)| (*e, s.span)).find(|(e, _)| replies.contains(e)));
            }
        });
        for s in &prog.machine(x).states {
            let handled: Vec<EventId> = prog
                .state(*s)
                .handlers
                .iter()
                .filter(|h| !matches!(h.action, HandlerAction::Ignore | HandlerAction::Defer))
                .map(|h| h.event)
                .collect();
            if !handled.is_empty() && handled.iter().all(|e| replies.contains(e)) {
                waits.push((handled[0], prog.state(*s).span));
            }
        }
        out.extend(waits.into_iter().filter(|(_, span)| !span.is_dummy()).map(|(e, span)| (x, e, span)));
    }
    out
}

/// See the module documentation.
pub fn lints(prog: &Program, drivers: &[bool], out: &mut Vec<Finding>) {
    lints_for_tests(prog, &prog.test_cases(), drivers, out);
}

pub fn lints_for_tests(prog: &Program, tests: &[crate::sema::TestCase], drivers: &[bool], out: &mut Vec<Finding>) {
    unplanned_driver_stage(prog, tests, drivers, out);
    unreachable_statements(prog, out);
}

/// A driver, the replies to its own requests, and the states it enters and the
/// functions it runs before any notification from the model.
type DriverStart = (MachineId, FxHashSet<EventId>, Vec<StateId>, Vec<FunId>);

fn unplanned_driver_stage(prog: &Program, tests: &[crate::sema::TestCase], drivers: &[bool], out: &mut Vec<Finding>) {
    let mut reported: FxHashSet<Span> = FxHashSet::default();
    for (i, t) in prog.tests.iter().enumerate() {
        if !tests.iter().any(|tc| matches!(tc.kind, crate::sema::TestCaseKind::Test(id, _) if id.idx() == i)) {
            continue;
        }
        let machines = machines_of(prog, t);
        let is_driver = |m: MachineId| m == t.main || drivers.get(m.idx()).copied().unwrap_or(false);
        let model: Vec<MachineId> = machines.iter().copied().filter(|m| !is_driver(*m)).collect();
        if model.is_empty() {
            continue;
        }
        let model_funs: Vec<FunId> = model.iter().flat_map(|m| machine_funs(prog, *m)).collect();
        // what the model sends, and the requests it handles
        let from_model = sends(prog, &model_funs);
        let mut requests: FxHashSet<EventId> = FxHashSet::default();
        for m in &model {
            for s in &prog.machine(*m).states {
                for h in &prog.state(*s).handlers {
                    if !matches!(h.action, HandlerAction::Ignore | HandlerAction::Defer) {
                        requests.insert(h.event);
                    }
                }
            }
        }
        each_stmt(prog, &model_funs, &mut |s| {
            if let StmtKind::Receive { cases } = &s.kind {
                requests.extend(cases.iter().map(|(e, _)| *e));
            }
        });
        let model_ifaces: FxHashSet<_> =
            t.module.interface_def.iter().filter(|(_, m)| model.contains(m)).map(|(i, _)| *i).collect();
        // events a spec with a progress obligation observes: a plan announced with one
        // keeps that spec `hot` until the planned work is done
        let observed: FxHashSet<EventId> = t
            .module
            .monitor_map
            .iter()
            .filter(|(m, _)| {
                let spec = prog.machine(*m);
                spec.kind == MachineKind::Spec
                    && spec.states.iter().any(|s| prog.state(*s).temperature == Temperature::Hot)
            })
            .flat_map(|(m, _)| prog.machine(*m).observes.iter().copied())
            .collect();
        let mut stages: Vec<DriverStart> = Vec::new();
        for d in machines.iter().copied().filter(|m| is_driver(*m)) {
            // a reply to the driver's own request, through any chain of model handlers: a
            // hot spec on pending requests sees it missing (and `no-progress-spec`
            // reports a test without one)
            let replies = downstream_replies(prog, &machines, d);
            let notification = |e: EventId| from_model.contains(&e) && !replies.contains(&e);
            let mut early: Vec<StateId> = prog.machine(d).start_state.into_iter().collect();
            let mut early_funs: Vec<FunId> = Vec::new();
            let mut i = 0;
            while i < early.len() {
                let st = prog.state(early[i]);
                i += 1;
                let mut next: Vec<StateId> = Vec::new();
                let mut funs: Vec<FunId> = st.entry.into_iter().collect();
                for h in st.handlers.iter().filter(|h| !notification(h.event)) {
                    funs.extend(handler_fun(&h.action));
                    if let HandlerAction::Goto { target, .. } = h.action {
                        next.push(target);
                    }
                }
                for f in funs {
                    let reached = reach(prog, f);
                    each_stmt(prog, &reached, &mut |s| {
                        if let StmtKind::Goto { state, .. } = s.kind {
                            next.push(state);
                        }
                    });
                    early_funs.extend(reached);
                }
                for s in next {
                    if !early.contains(&s) {
                        early.push(s);
                    }
                }
            }
            stages.push((d, replies, early, early_funs));
        }
        // a driver that announces, before waiting on the model, an event a spec with a
        // hot state observes has told the specs its plan
        let mut planned = false;
        for (_, _, _, funs) in &stages {
            each_stmt(prog, funs, &mut |s| {
                if let StmtKind::Announce { event, .. } = &s.kind {
                    planned |= matches!(event.kind, ExprKind::EventRef(e) if observed.contains(&e));
                }
            });
        }
        if planned {
            continue;
        }
        for (d, replies, early, _) in stages {
            let notifications = |e: EventId| from_model.contains(&e) && !replies.contains(&e);
            let decl = prog.machine(d);
            // what runs only after a notification: its handlers, and the states it leads to
            let mut later: Vec<(FunId, EventId)> = Vec::new();
            for s in &decl.states {
                let st = prog.state(*s);
                for h in st.handlers.iter().filter(|h| notifications(h.event)) {
                    later.extend(handler_fun(&h.action).map(|f| (f, h.event)));
                    if let HandlerAction::Goto { target, .. } = h.action {
                        if !early.contains(&target) {
                            later.extend(prog.state(target).entry.map(|f| (f, h.event)));
                        }
                    }
                    if let Some(f) = handler_fun(&h.action) {
                        each_stmt(prog, &reach(prog, f), &mut |s| {
                            if let StmtKind::Goto { state, .. } = s.kind {
                                if !early.contains(&state) {
                                    later.extend(prog.state(state).entry.map(|f| (f, h.event)));
                                }
                            }
                        });
                    }
                }
            }
            for (f, after) in later {
                let mut work: Option<(Span, String)> = None;
                each_stmt(prog, &reach(prog, f), &mut |s| {
                    if work.is_some() {
                        return;
                    }
                    let mut created = None;
                    if let StmtKind::Ctor { interface, .. } = s.kind {
                        created = Some(interface);
                    }
                    each_expr(s, &mut |e| {
                        if let ExprKind::Ctor { interface, .. } = e.kind {
                            created = created.or(Some(interface));
                        }
                    });
                    if let Some(i) = created.filter(|i| model_ifaces.contains(i)) {
                        work = Some((s.span, format!("creates {}", prog.interfaces[i.idx()].name)));
                    } else if let StmtKind::Send { event, .. } = &s.kind {
                        // forwarding the event it got (a proxy) starts nothing new
                        if let ExprKind::EventRef(e) = event.kind {
                            if requests.contains(&e) && e != after {
                                work = Some((s.span, format!("sends '{}'", prog.event(e).name)));
                            }
                        }
                    }
                });
                let Some((span, what)) = work else { continue };
                if span.is_dummy() || !reported.insert(span) {
                    continue;
                }
                out.push(Finding {
                    help: None,
                    rule: "unplanned-driver-stage",
                    span,
                    message: format!(
                        "driver {} {} only after the model sends '{}', and announces no plan a spec observes, so if the model never sends it this stage never starts and every spec still passes",
                        decl.name,
                        what,
                        prog.event(after).name
                    ),
                });
            }
        }
    }
}

/// How a statement always leaves the state, if it does: `goto S`, `raise E`, or a call
/// to a function that always does one.
fn leaves(prog: &Program, s: &Stmt, seen: &mut Vec<FunId>) -> Option<String> {
    match &s.kind {
        StmtKind::Goto { state, .. } => Some(format!("goto {}", prog.state(*state).name)),
        StmtKind::Raise { event, .. } => Some(match event.kind {
            ExprKind::EventRef(e) => format!("raise {}", prog.event(e).name),
            _ => "raise".into(),
        }),
        StmtKind::Block(b) => block_leaves(prog, b, seen),
        StmtKind::If { then_branch, else_branch, .. } => {
            let a = leaves(prog, then_branch, seen)?;
            leaves(prog, else_branch, seen).map(|_| a)
        }
        StmtKind::FunCall { fun, .. } => {
            if seen.contains(fun) {
                return None;
            }
            seen.push(*fun);
            let how = prog.fun(*fun).body.as_ref().and_then(|b| block_leaves(prog, b, seen));
            seen.pop();
            how
        }
        _ => None,
    }
}

/// How a block always leaves the state, if it does (see [`leaves`]). A `return`, `break`
/// or `continue` that may run first means it may not.
fn block_leaves(prog: &Program, b: &Block, seen: &mut Vec<FunId>) -> Option<String> {
    for s in &b.stmts {
        if let Some(how) = leaves(prog, s, seen) {
            return Some(how);
        }
        if exits(s) {
            return None;
        }
    }
    None
}

/// Whether `s` contains a `return`, `break` or `continue`.
fn exits(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => true,
        StmtKind::Block(b) => b.stmts.iter().any(exits),
        StmtKind::If { then_branch, else_branch, .. } => exits(then_branch) || exits(else_branch),
        StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => exits(body),
        _ => false,
    }
}

fn unreachable_statements(prog: &Program, out: &mut Vec<Finding>) {
    fn scan(prog: &Program, b: &Block, out: &mut Vec<Finding>) {
        for (i, s) in b.stmts.iter().enumerate() {
            match &s.kind {
                StmtKind::Block(inner) => scan(prog, inner, out),
                StmtKind::If { then_branch, else_branch, .. } => {
                    for branch in [then_branch, else_branch] {
                        scan(prog, &as_block(branch), out);
                    }
                }
                StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => scan(prog, &as_block(body), out),
                _ => {}
            }
            let Some(next) = b.stmts.get(i + 1) else { continue };
            if next.span.is_dummy() || s.span.is_dummy() {
                continue;
            }
            if let Some(how) = leaves(prog, s, &mut Vec::new()) {
                let (before, help) = match &s.kind {
                    StmtKind::FunCall { fun, .. } => {
                        let name = &prog.fun(*fun).name;
                        (
                            format!("the call to {} before it", name),
                            format!("remove it, or move it before the call to {} if it should run", name),
                        )
                    }
                    StmtKind::If { .. } => (
                        "every branch of the `if` before it".to_string(),
                        "remove it, or move it into the branches before they leave the state if it should run"
                            .to_string(),
                    ),
                    _ => (
                        "the statement before it".to_string(),
                        format!("remove it, or move it before the `{}` if it should run", how),
                    ),
                };
                out.push(Finding {
                    help: Some(help),
                    rule: "unreachable-statement",
                    span: next.span,
                    message: format!("this statement never runs: {} always leaves the state ({})", before, how),
                });
            }
        }
    }
    fn as_block(s: &Stmt) -> Block {
        match &s.kind {
            StmtKind::Block(b) => b.clone(),
            _ => Block { stmts: vec![s.clone()], span: s.span },
        }
    }
    for f in &prog.funs {
        if f.owner.is_none() && f.parent.is_none() && f.span.is_dummy() {
            continue;
        }
        if let Some(b) = &f.body {
            scan(prog, b, out);
        }
    }
}
