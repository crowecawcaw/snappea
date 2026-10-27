//! Code coverage of P machines and monitors: which event handlers, statements and `if`
//! outcomes ran, and what never did. `snappea check` reports it over the explored
//! schedules, `snappea observe` over the observed events.
//!
//! A [`CodeMap`] lists the points of a scope (the machines of a test case, or one spec)
//! that can run, with their source locations, from code compiled with probes (see
//! [`Probe`]). A [`CodeCoverage`] is that list plus the points that ran. It needs no P
//! sources, so coverage from several runs or processes merges by the union of the
//! points that ran.

use std::collections::BTreeSet;

use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{json, Value};

use crate::checker::bytecode::{Probe, ProbeKind};
use crate::checker::coverage::{CoverageInfo, CoverageTables, ProbeSet};
use crate::sema::{EventId, FunId, HandlerAction, MachineId, Program, StateId, Stmt, StmtKind, TestCase};
use crate::syntax::{SourceMap, Span};

/// What a coverage point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointKind {
    /// A `do` or `goto` event handler of a state.
    Handler,
    /// The start of a function or receive case.
    Entry,
    Statement,
    /// The `if` condition was true.
    Then,
    /// The `if` condition was false.
    Else,
    ConditionTrue,
    ConditionFalse,
    /// Two consecutive state transitions, on one instance in one execution.
    TransitionPair,
    /// A state of a spec or scenario monitor was entered. Its handlers and entry/exit
    /// functions are its children, so an unreached state is reported once.
    State,
}

impl PointKind {
    fn name(self) -> &'static str {
        match self {
            PointKind::Handler => "handler",
            PointKind::Entry => "entry",
            PointKind::Statement => "statement",
            PointKind::Then => "then",
            PointKind::Else => "else",
            PointKind::ConditionTrue => "condition_true",
            PointKind::ConditionFalse => "condition_false",
            PointKind::TransitionPair => "transition_pair",
            PointKind::State => "state",
        }
    }

    fn from_name(s: &str) -> Option<PointKind> {
        [
            PointKind::Handler,
            PointKind::Entry,
            PointKind::Statement,
            PointKind::Then,
            PointKind::Else,
            PointKind::ConditionTrue,
            PointKind::ConditionFalse,
            PointKind::TransitionPair,
            PointKind::State,
        ]
        .into_iter()
        .find(|k| k.name() == s)
    }
}

/// A piece of code whose execution is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub kind: PointKind,
    pub file: String,
    pub line: u32,
    /// Where the code is, e.g. `Server.Serving, on eRequest`.
    pub context: String,
    /// What to report when it never runs, e.g. `never handled`.
    pub missed: String,
    /// The point that runs whenever this one does (an enclosing branch, function or
    /// handler); listed before this one.
    pub parent: Option<u32>,
}

/// Code that never ran: the outermost point that did not run, or a run of statements.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Uncovered {
    pub file: String,
    pub line: u32,
    /// Last line, for a run of statements.
    pub end_line: u32,
    pub context: String,
    pub what: String,
}

impl Uncovered {
    /// `file:line` or `file:line-end`.
    pub fn location(&self) -> String {
        if self.end_line > self.line {
            format!("{}:{}-{}", self.file, self.line, self.end_line)
        } else {
            format!("{}:{}", self.file, self.line)
        }
    }

    pub fn to_json(&self) -> Value {
        json!({"file": self.file, "line": self.line, "end_line": self.end_line, "context": self.context, "what": self.what})
    }
}

/// Covered and total counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Count {
    pub covered: usize,
    pub total: usize,
}

impl Count {
    fn add(&mut self, covered: bool) {
        self.total += 1;
        self.covered += usize::from(covered);
    }

    fn to_json(self) -> Value {
        json!({"covered": self.covered, "total": self.total})
    }
}

/// Coverage counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    /// Monitor states entered (spec and scenario monitors only).
    pub states: Count,
    pub statements: Count,
    pub branches: Count,
    pub handlers: Count,
    pub conditions: Count,
    pub transition_pairs: Count,
}

impl Summary {
    pub fn to_json(self) -> Value {
        json!({"states": self.states.to_json(), "statements": self.statements.to_json(), "branches": self.branches.to_json(), "handlers": self.handlers.to_json(),
            "conditions": self.conditions.to_json(), "transition_pairs": self.transition_pairs.to_json()})
    }
}

impl std::fmt::Display for Summary {
    /// `3/4 states, 41/50 statements, 6/8 branches, 9/12 event handlers` (states only
    /// when monitors are covered).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Summary { states: st, statements: s, branches: b, handlers: h, conditions: c, transition_pairs: t } = self;
        if st.total > 0 {
            write!(f, "{}/{} states, ", st.covered, st.total)?;
        }
        write!(
            f,
            "{}/{} statements, {}/{} branches, {}/{} event handlers",
            s.covered, s.total, b.covered, b.total, h.covered, h.total
        )?;
        if c.total > 0 {
            write!(f, ", {}/{} condition outcomes", c.covered, c.total)?;
        }
        if t.total > 0 {
            write!(f, ", {}/{} transition pairs", t.covered, t.total)?;
        }
        Ok(())
    }
}

/// The machines and functions coverage is measured over.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    machines: Vec<bool>,
    funs: Vec<bool>,
}

impl Scope {
    /// Loaded components outside the selected tests' modules, excluded from totals.
    pub fn excluded(&self, prog: &Program) -> Vec<Uncovered> {
        prog.machines
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                if self.machines.get(i).copied().unwrap_or(false) || m.span.is_dummy() {
                    return None;
                }
                let loc = prog.sources.location(m.span);
                Some(Uncovered {
                    file: loc.file.unwrap_or_default(),
                    line: loc.line.max(0) as u32,
                    end_line: loc.line.max(0) as u32,
                    context: m.name.clone(),
                    what: "not included in selected test cases".into(),
                })
            })
            .collect()
    }

    /// The machines and monitors of test case `tc`, and the functions they call.
    pub fn test_case(prog: &Program, tc: &TestCase) -> Scope {
        let (module, main) = prog.test_case_module(tc);
        let machines = module.interface_def.iter().map(|(_, m)| *m).chain(module.monitor_map.iter().map(|(m, _)| *m));
        Scope::of(prog, machines.chain(main))
    }

    /// Spec monitor `spec` and the functions it calls.
    pub fn machine(prog: &Program, spec: MachineId) -> Scope {
        Scope::of(prog, [spec])
    }

    fn of(prog: &Program, machines: impl IntoIterator<Item = MachineId>) -> Scope {
        let mut scope = Scope { machines: vec![false; prog.machines.len()], funs: vec![false; prog.funs.len()] };
        for m in machines {
            scope.machines[m.idx()] = true;
        }
        let mut work: Vec<FunId> = Vec::new();
        for (i, f) in prog.funs.iter().enumerate() {
            if f.owner.is_some_and(|m| scope.machines[m.idx()]) {
                scope.funs[i] = true;
                work.push(FunId(i as u32));
            }
        }
        while let Some(f) = work.pop() {
            for c in &prog.fun(f).callees {
                if !scope.funs[c.idx()] {
                    scope.funs[c.idx()] = true;
                    work.push(*c);
                }
            }
        }
        scope
    }

    /// Adds the machines and functions of `other`.
    pub fn add(&mut self, other: &Scope) {
        for (a, b) in [(&mut self.machines, &other.machines), (&mut self.funs, &other.funs)] {
            if a.len() < b.len() {
                a.resize(b.len(), false);
            }
            for (x, y) in a.iter_mut().zip(b) {
                *x |= *y;
            }
        }
    }
}

/// What ran in a run, for any scope: the probes and the `(state, event)` handlers.
#[derive(Debug, Clone, Default)]
pub struct Hits {
    probes: ProbeSet,
    handled: FxHashSet<(StateId, EventId)>,
    transition_pairs: FxHashSet<(StateId, StateId, StateId)>,
    /// Monitor states entered.
    states: FxHashSet<StateId>,
}

impl Hits {
    /// The hits recorded in activity coverage `info`.
    pub fn from_info(prog: &Program, tables: &CoverageTables, info: &CoverageInfo) -> Hits {
        let mut handled = FxHashSet::default();
        for (i, st) in prog.states.iter().enumerate() {
            let s = StateId(i as u32);
            for h in &st.handlers {
                if info.received.contains(&(tables.plain(s), h.event.0)) {
                    handled.insert((s, h.event));
                }
            }
        }
        Hits {
            probes: info.probes.clone(),
            handled,
            transition_pairs: info.transition_pairs.clone(),
            states: info.monitor_states.clone(),
        }
    }

    /// Whether code coverage probe `id` ran.
    pub fn ran(&self, id: u32) -> bool {
        self.probes.contains(id)
    }

    pub fn add(&mut self, other: &Hits) {
        self.probes.union(&other.probes);
        self.handled.extend(other.handled.iter().copied());
        self.transition_pairs.extend(other.transition_pairs.iter().copied());
        self.states.extend(other.states.iter().copied());
    }
}

/// The points of a scope, and which probe or handler each stands for.
#[derive(Debug, Clone, Default)]
pub struct CodeMap {
    fingerprint: Option<String>,
    points: Vec<Point>,
    /// Source spans used for scoped coverage suppressions, aligned with `points`.
    point_spans: Vec<Span>,
    /// Point of each probe, by probe id.
    probe_point: Vec<Option<u32>>,
    handler_point: FxHashMap<(StateId, EventId), u32>,
    transition_point: FxHashMap<(StateId, StateId, StateId), u32>,
    state_point: FxHashMap<StateId, u32>,
}

impl CodeMap {
    /// The points of `scope`, from `probes` (the probes of the program's code). Code that
    /// must never run (`assert false`, and what starts with it) has no point.
    pub fn new(prog: &Program, probes: &[Probe], scope: &Scope) -> CodeMap {
        let sm: &SourceMap = &prog.sources;
        let mut map = CodeMap {
            fingerprint: prog.source_fingerprint.clone(),
            probe_point: vec![None; probes.len()],
            ..CodeMap::default()
        };
        let included = |f: FunId| scope.funs.get(f.idx()).copied().unwrap_or(false);
        // root entry probe of each function
        let entries: FxHashMap<FunId, usize> = probes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == ProbeKind::Entry && p.parent.is_none())
            .map(|(id, p)| (p.fun, id))
            .collect();
        // handlers, and the functions only one handler runs
        let mut runs: FxHashMap<FunId, Vec<u32>> = FxHashMap::default();
        for (i, m) in prog.machines.iter().enumerate() {
            if !scope.machines.get(i).copied().unwrap_or(false) {
                continue;
            }
            for s in &m.states {
                let st = prog.state(*s);
                // monitor states: whether a check ever entered them
                let state_point = if m.is_spec() {
                    let context = format!("{}.{}", m.name, st.name);
                    let point = map.push(sm, PointKind::State, st.span, context, "state never reached".into(), None);
                    if let Some(p) = point {
                        map.state_point.insert(*s, p);
                        for f in st.entry.iter().chain(&st.exit) {
                            runs.entry(*f).or_default().push(p);
                        }
                    }
                    point
                } else {
                    None
                };
                for h in &st.handlers {
                    let (fun, missed) = match h.action {
                        HandlerAction::Do(f) => (Some(f), "never handled".to_string()),
                        HandlerAction::Goto { target, with } => {
                            (with, format!("goto {} never taken", prog.state(target).name))
                        }
                        _ => continue,
                    };
                    if fun.and_then(|f| entries.get(&f)).is_some_and(|id| must_not_run(probes, *id)) {
                        continue;
                    }
                    let context = format!("{}.{}, on {}", m.name, st.name, prog.event(h.event).name);
                    let Some(point) = map.push(sm, PointKind::Handler, h.span, context, missed, state_point) else {
                        continue;
                    };
                    map.handler_point.insert((*s, h.event), point);
                    if let Some(f) = fun {
                        runs.entry(f).or_default().push(point);
                    }
                }
            }
        }
        let mut excluded = vec![false; probes.len()];
        for (id, p) in probes.iter().enumerate() {
            excluded[id] = must_not_run(probes, id) || p.parent.is_some_and(|q| excluded[q as usize]);
            if excluded[id] || !included(p.fun) {
                continue;
            }
            let parent = match p.parent {
                Some(q) => map.probe_point[q as usize],
                // run by exactly one handler
                None => runs.get(&p.fun).filter(|h| h.len() == 1).map(|h| h[0]),
            };
            let (kind, missed) = match p.kind {
                ProbeKind::Entry if prog.fun(p.fun).parent.is_some() => {
                    (PointKind::Entry, "receive case never ran".to_string())
                }
                ProbeKind::Entry => (PointKind::Entry, "never ran".to_string()),
                ProbeKind::Stmt => (PointKind::Statement, format!("not reached: {}", snippet(sm, p.span))),
                ProbeKind::Then => (PointKind::Then, format!("{} never true", condition(sm, p.span))),
                ProbeKind::Else => (PointKind::Else, format!("{} never false", condition(sm, p.span))),
                ProbeKind::ConditionTrue => {
                    (PointKind::ConditionTrue, format!("condition {} never true", snippet(sm, p.span)))
                }
                ProbeKind::ConditionFalse => {
                    (PointKind::ConditionFalse, format!("condition {} never false", snippet(sm, p.span)))
                }
            };
            map.probe_point[id] = map.push(sm, kind, p.span, fun_context(prog, p.fun), missed, parent);
        }
        let edges = transition_edges(prog, scope);
        for (a, b) in &edges {
            for (_, c) in edges.range((*b, StateId(0))..=(*b, StateId(u32::MAX))) {
                let st = prog.state(*b);
                let context = prog.machine(st.machine).name.clone();
                let missed = format!(
                    "{} -> {} -> {} never taken consecutively",
                    prog.state(*a).name,
                    st.name,
                    prog.state(*c).name
                );
                if let Some(p) = map.push(sm, PointKind::TransitionPair, st.span, context, missed, None) {
                    map.transition_point.insert((*a, *b, *c), p);
                }
            }
        }
        map
    }

    fn push(
        &mut self,
        sm: &SourceMap,
        kind: PointKind,
        span: Span,
        context: String,
        missed: String,
        parent: Option<u32>,
    ) -> Option<u32> {
        if span.is_dummy() {
            return None;
        }
        let loc = sm.location(span);
        let line = loc.line.max(0) as u32;
        self.points.push(Point { kind, file: loc.file.unwrap_or_default(), line, context, missed, parent });
        self.point_spans.push(span);
        Some(self.points.len() as u32 - 1)
    }

    /// The coverage of the scope: none of its points ran.
    pub fn empty(&self) -> CodeCoverage {
        CodeCoverage { fingerprint: self.fingerprint.clone(), points: self.points.clone(), hit: BTreeSet::new() }
    }

    /// The coverage of the scope from `hits`.
    pub fn coverage(&self, hits: &Hits) -> CodeCoverage {
        let mut c = self.empty();
        self.record(&mut c, hits);
        c
    }

    /// Adds `hits` to coverage `c` of the scope (made by [`CodeMap::empty`]).
    pub fn record(&self, c: &mut CodeCoverage, hits: &Hits) {
        for (id, point) in self.probe_point.iter().enumerate() {
            if let Some(p) = point.filter(|_| hits.probes.contains(id as u32)) {
                c.hit.insert(p);
            }
        }
        for (h, p) in &self.handler_point {
            if hits.handled.contains(h) {
                c.hit.insert(*p);
            }
        }
        for (t, p) in &self.transition_point {
            if hits.transition_pairs.contains(t) {
                c.hit.insert(*p);
            }
        }
        for (st, p) in &self.state_point {
            if hits.states.contains(st) {
                c.hit.insert(*p);
            }
        }
    }

    /// Required points not reached by any selected test and not covered by a scoped
    /// `coverage` suppression. Conditions and transition pairs are advisory.
    pub fn required_uncovered(&self, coverage: &CodeCoverage, suppressions: &[Span]) -> Vec<Uncovered> {
        let mut out = Vec::new();
        for (i, p) in self.points.iter().enumerate() {
            let point = self.point_spans[i];
            if coverage.hit.contains(&(i as u32))
                || !matches!(p.kind, PointKind::Statement | PointKind::Handler | PointKind::Then | PointKind::Else)
                || suppressions
                    .iter()
                    .any(|scope| scope.file == point.file && scope.start <= point.start && scope.end >= point.end)
            {
                continue;
            }
            out.push(Uncovered {
                file: p.file.clone(),
                line: p.line,
                end_line: p.line,
                context: p.context.clone(),
                what: p.missed.clone(),
            });
        }
        out.sort();
        out
    }

    /// Required coverage counts after scoped suppressions.
    pub fn required_summary(&self, coverage: &CodeCoverage, suppressions: &[Span]) -> Summary {
        let mut s = Summary::default();
        for (i, p) in self.points.iter().enumerate() {
            let point = self.point_spans[i];
            if suppressions
                .iter()
                .any(|scope| scope.file == point.file && scope.start <= point.start && scope.end >= point.end)
            {
                continue;
            }
            let hit = coverage.hit.contains(&(i as u32));
            match p.kind {
                PointKind::Statement => s.statements.add(hit),
                PointKind::Then | PointKind::Else => s.branches.add(hit),
                PointKind::Handler => s.handlers.add(hit),
                _ => {}
            }
        }
        s
    }
}

/// The points of a scope, and those that ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeCoverage {
    /// Identity of the P sources (see [`Program::source_fingerprint`]).
    pub fingerprint: Option<String>,
    pub points: Vec<Point>,
    /// Indexes of the points that ran.
    pub hit: BTreeSet<u32>,
}

impl CodeCoverage {
    fn ran(&self, p: Option<u32>) -> bool {
        p.is_none_or(|i| self.hit.contains(&i))
    }

    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for (i, p) in self.points.iter().enumerate() {
            let hit = self.hit.contains(&(i as u32));
            match p.kind {
                PointKind::Statement => s.statements.add(hit),
                PointKind::Then | PointKind::Else => s.branches.add(hit),
                PointKind::Handler => s.handlers.add(hit),
                PointKind::ConditionTrue | PointKind::ConditionFalse => s.conditions.add(hit),
                PointKind::TransitionPair => s.transition_pairs.add(hit),
                PointKind::State => s.states.add(hit),
                PointKind::Entry => {}
            }
        }
        s
    }

    /// What never ran, by file and line: for each piece of code that did not run, the
    /// outermost one (a handler never triggered, not each of its statements), with
    /// consecutive statements that were never reached as one entry.
    pub fn uncovered(&self) -> Vec<Uncovered> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.points.len() {
            let p = &self.points[i];
            let this = i as u32;
            i += 1;
            if self.hit.contains(&this) || !self.ran(p.parent) {
                continue;
            }
            let mut end_line = p.line;
            if p.kind == PointKind::Statement {
                // the run of unreached statements that starts here, and what they contain
                while let Some(q) = self.points.get(i) {
                    let sibling = q.kind == PointKind::Statement && q.parent == p.parent;
                    let nested = q.parent.is_some_and(|par| par >= this && !self.hit.contains(&par));
                    if !(sibling || nested) || self.hit.contains(&(i as u32)) {
                        break;
                    }
                    if sibling {
                        end_line = end_line.max(q.line);
                    }
                    i += 1;
                }
            }
            out.push(Uncovered {
                file: p.file.clone(),
                line: p.line,
                end_line,
                context: p.context.clone(),
                what: p.missed.clone(),
            });
        }
        out.sort();
        out.dedup();
        out
    }

    /// Adds the points `other` ran; both must be coverage of the same code.
    pub fn merge(&mut self, other: &CodeCoverage) -> Result<(), String> {
        if self.fingerprint != other.fingerprint {
            return Err("coverage of different P sources".into());
        }
        // file names depend on where each run found the sources, which the fingerprint
        // already identifies
        let same = |a: &Point, b: &Point| {
            (a.kind, a.line, &a.context, &a.missed, a.parent) == (b.kind, b.line, &b.context, &b.missed, b.parent)
        };
        if self.points.len() != other.points.len() || !self.points.iter().zip(&other.points).all(|(a, b)| same(a, b)) {
            return Err("coverage of different code (or of another snappea version)".into());
        }
        self.hit.extend(other.hit.iter().copied());
        Ok(())
    }

    /// The counts and what never ran; with `points`, also the points and those that ran,
    /// which [`CodeCoverage::from_json`] reads back.
    pub fn to_json(&self, points: bool) -> Value {
        let mut v = self.summary().to_json();
        v["uncovered"] = self.uncovered().iter().map(Uncovered::to_json).collect();
        if points {
            v["source_fingerprint"] = json!(self.fingerprint);
            v["points"] = self
                .points
                .iter()
                .map(|p| {
                    json!({"kind": p.kind.name(), "file": p.file, "line": p.line, "context": p.context,
                        "missed": p.missed, "parent": p.parent})
                })
                .collect();
            v["hit"] = json!(self.hit);
        }
        v
    }

    pub fn from_json(v: &Value) -> Result<CodeCoverage, String> {
        let points = v["points"].as_array().ok_or("coverage has no points")?;
        let text = |p: &Value, k: &str| p[k].as_str().map(str::to_string).ok_or(format!("point {k} must be a string"));
        let mut out =
            CodeCoverage { fingerprint: v["source_fingerprint"].as_str().map(str::to_string), ..Default::default() };
        for (i, p) in points.iter().enumerate() {
            let kind = p["kind"].as_str().and_then(PointKind::from_name).ok_or("unknown point kind")?;
            let parent = match &p["parent"] {
                Value::Null => None,
                n => {
                    Some(n.as_u64().filter(|n| (*n as usize) < i).ok_or("point parent must be an earlier point")? as u32)
                }
            };
            let line = p["line"].as_u64().ok_or("point line must be a number")? as u32;
            out.points.push(Point {
                kind,
                file: text(p, "file")?,
                line,
                context: text(p, "context")?,
                missed: text(p, "missed")?,
                parent,
            });
        }
        for h in v["hit"].as_array().ok_or("coverage has no hit list")? {
            let h = h.as_u64().filter(|h| (*h as usize) < points.len()).ok_or("hit must be a point index")?;
            out.hit.insert(h as u32);
        }
        Ok(out)
    }
}

/// Syntactic state edges, including gotos in entry/exit actions and called helpers.
/// Composing these edges gives structural targets, not a reachability proof: guards
/// and payload constraints can make a pair infeasible.
fn transition_edges(prog: &Program, scope: &Scope) -> BTreeSet<(StateId, StateId)> {
    fn scan(s: &Stmt, targets: &mut BTreeSet<StateId>, calls: &mut Vec<FunId>) {
        match &s.kind {
            StmtKind::Goto { state, .. } => {
                targets.insert(*state);
            }
            StmtKind::Block(b) => {
                for s in &b.stmts {
                    if matches!(&s.kind, StmtKind::Assert { cond, .. } if matches!(cond.kind, crate::sema::ExprKind::BoolLit(false)))
                    {
                        break;
                    }
                    scan(s, targets, calls);
                }
            }
            StmtKind::If { then_branch, else_branch, .. } => {
                scan(then_branch, targets, calls);
                scan(else_branch, targets, calls);
            }
            StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => scan(body, targets, calls),
            StmtKind::Receive { cases } => calls.extend(cases.iter().map(|(_, f)| *f)),
            _ => {}
        }
    }
    let mut edges = BTreeSet::new();
    for (i, st) in prog.states.iter().enumerate() {
        if !scope.machines.get(st.machine.idx()).copied().unwrap_or(false) {
            continue;
        }
        let mut targets = BTreeSet::new();
        let mut work: Vec<FunId> = st.entry.into_iter().chain(st.exit).collect();
        for h in &st.handlers {
            match h.action {
                HandlerAction::Goto { target, with } => {
                    targets.insert(target);
                    work.extend(with);
                }
                HandlerAction::Do(f) => work.push(f),
                _ => {}
            }
        }
        let mut visited = FxHashSet::default();
        while let Some(f) = work.pop() {
            if !visited.insert(f) {
                continue;
            }
            let f = prog.fun(f);
            work.extend(f.callees.iter().copied());
            if let Some(body) = &f.body {
                for s in &body.stmts {
                    if matches!(&s.kind, StmtKind::Assert { cond, .. } if matches!(cond.kind, crate::sema::ExprKind::BoolLit(false)))
                    {
                        break;
                    }
                    scan(s, &mut targets, &mut work);
                }
            }
        }
        edges.extend(
            targets.into_iter().filter(|t| prog.state(*t).machine == st.machine).map(|t| (StateId(i as u32), t)),
        );
    }
    edges
}

/// Uncovered entries shown on the console.
pub const SHOWN_UNCOVERED: usize = 20;

/// `heading` and the console lines of `uncovered`: the first [`SHOWN_UNCOVERED`], then how
/// many more (`hint` says where to see them).
pub fn uncovered_lines(heading: &str, uncovered: &[Uncovered], hint: &str) -> Vec<String> {
    if uncovered.is_empty() {
        return Vec::new();
    }
    let shown = &uncovered[..uncovered.len().min(SHOWN_UNCOVERED)];
    let width = shown.iter().map(|u| u.location().len()).max().unwrap_or(0);
    let mut out = vec![heading.to_string()];
    out.extend(shown.iter().map(|u| format!("  {:width$}  {}: {}", u.location(), u.context, u.what, width = width)));
    if uncovered.len() > shown.len() {
        out.push(format!("  ... {} more{}", uncovered.len() - shown.len(), hint));
    }
    out
}

/// Whether probe `id` marks code that must never run: an `assert false` statement, or a
/// branch or function that starts with one.
fn must_not_run(probes: &[Probe], id: usize) -> bool {
    let first_child = probes.get(id + 1).filter(|c| c.kind == ProbeKind::Stmt && c.parent == Some(id as u32));
    probes[id].asserts_false || first_child.is_some_and(|c| c.asserts_false)
}

/// Where function `f` runs, e.g. `Server.Serving, on eRequest`, `Server.Init entry`,
/// `Server.helper` or `helper`.
pub(crate) fn fun_context(prog: &Program, f: FunId) -> String {
    let decl = prog.fun(f);
    if let Some(parent) = decl.parent {
        return fun_context(prog, parent);
    }
    let Some(m) = decl.owner else { return decl.name.clone() };
    let machine = prog.machine(m);
    for s in &machine.states {
        let st = prog.state(*s);
        if st.entry == Some(f) {
            return format!("{}.{} entry", machine.name, st.name);
        }
        if st.exit == Some(f) {
            return format!("{}.{} exit", machine.name, st.name);
        }
        for h in &st.handlers {
            if matches!(h.action, HandlerAction::Do(g) | HandlerAction::Goto { with: Some(g), .. } if g == f) {
                return format!("{}.{}, on {}", machine.name, st.name, prog.event(h.event).name);
            }
        }
    }
    if decl.name.is_empty() {
        machine.name.clone()
    } else {
        format!("{}.{}", machine.name, decl.name)
    }
}

/// `if (cond)` of the `if` statement at `span`, shortened.
fn condition(sm: &SourceMap, span: Span) -> String {
    let text = sm.text(span);
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 1 => return shorten(&text[..=i].split_whitespace().collect::<Vec<_>>().join(" ")),
            ')' => depth -= 1,
            _ => {}
        }
    }
    snippet(sm, span)
}

/// The first line of the source of `span`, shortened.
pub(crate) fn snippet(sm: &SourceMap, span: Span) -> String {
    shorten(sm.text(span).lines().next().unwrap_or("").trim().trim_end_matches('{').trim_end())
}

pub(crate) fn shorten(line: &str) -> String {
    const MAX: usize = 60;
    if line.chars().count() > MAX {
        format!("{}...", line.chars().take(MAX - 3).collect::<String>())
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(kind: PointKind, line: u32, parent: Option<u32>) -> Point {
        Point { kind, file: "a.p".into(), line, context: "M".into(), missed: format!("missed {line}"), parent }
    }

    #[test]
    fn reports_the_outermost_code_that_never_ran_and_merges_by_union() {
        let points = vec![
            point(PointKind::Handler, 1, None),
            point(PointKind::Entry, 1, Some(0)),
            point(PointKind::Statement, 2, Some(1)),
            point(PointKind::Then, 2, Some(2)),
            point(PointKind::Statement, 3, Some(3)),
            point(PointKind::Statement, 5, Some(1)),
            point(PointKind::Statement, 6, Some(1)),
        ];
        let mut a = CodeCoverage { fingerprint: Some("f".into()), points, hit: BTreeSet::new() };
        let lines = |c: &CodeCoverage| c.uncovered().iter().map(|u| u.location()).collect::<Vec<_>>();
        // nothing ran: only the handler
        assert_eq!(lines(&a), ["a.p:1"]);
        // the handler ran its first statement only: the branch, and the last two statements
        let mut b = a.clone();
        b.hit.extend([0, 1, 2]);
        assert_eq!(lines(&b), ["a.p:2", "a.p:5-6"]);
        a.merge(&b).unwrap();
        assert_eq!(a.summary().statements, Count { covered: 1, total: 4 });
        assert_eq!(CodeCoverage::from_json(&a.to_json(true)).unwrap(), a);
        let mut other = a.clone();
        other.points.pop();
        assert!(a.merge(&other).is_err());
    }

    #[test]
    fn required_gate_keeps_nested_gaps_and_ignores_advisory_points() {
        let span = |start, end| Span::new(crate::syntax::FileId(0), start, end);
        let points = vec![
            point(PointKind::Handler, 1, None),
            point(PointKind::Entry, 1, Some(0)),
            point(PointKind::Statement, 2, Some(1)),
            point(PointKind::Statement, 3, Some(1)),
            point(PointKind::Then, 4, Some(2)),
            point(PointKind::ConditionTrue, 4, Some(2)),
            point(PointKind::TransitionPair, 5, None),
        ];
        let point_spans = [span(1, 2), span(2, 3), span(4, 5), span(6, 7), span(8, 9), span(10, 11), span(12, 13)];
        let map = CodeMap { points, point_spans: point_spans.to_vec(), ..Default::default() };
        let coverage = map.empty();
        let required = map.required_summary(&coverage, &[span(4, 5)]);
        assert_eq!(required.statements, Count { covered: 0, total: 1 });
        assert_eq!(required.branches, Count { covered: 0, total: 1 });
        assert_eq!(required.handlers, Count { covered: 0, total: 1 });
        assert_eq!((required.conditions.total, required.transition_pairs.total), (0, 0));
        let gaps = map.required_uncovered(&coverage, &[span(4, 5)]);
        assert_eq!(gaps.len(), 3);
        assert!(gaps.iter().any(|u| u.line == 3), "an uncalled Entry does not hide required statements");
    }
}
