//! What the checks depend on: a backward dataflow over the whole program from everything
//! that can fail a test case, to find model code no check can ever notice.
//!
//! A value is *checked* when it can reach an assertion, a branch or loop condition, the
//! target or event of a send, an operation that can fail (an index, a map key, a
//! divisor, a cast), or the arguments of a foreign function: through assignments,
//! function calls and returns, `new` payloads and event payloads, into any machine,
//! spec monitors included. Everything is over-approximated: containers count as one
//! value, every handler of an event receives every payload sent with it, and every
//! condition counts. So a value that is *not* checked can never change whether a test
//! case passes, which is what [`lints`] reports and mutation testing relies on to skip
//! changes that only write such values.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::sema::{
    BinOp, Block, EventId, Expr, ExprKind, FunId, HandlerAction, InterfaceId, MachineId, MachineKind, Program, Stmt,
    StmtKind, Temperature, TestCase, VarId, VarRole,
};
use crate::syntax::Span;

/// A value the dataflow tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Node {
    /// A variable: a field, local, parameter or global parameter.
    Var(VarId),
    /// What a function returns.
    Ret(FunId),
    /// The payloads sent with an event.
    Payload(EventId),
}

/// The result of the dataflow.
#[derive(Debug, Default)]
pub struct Checked {
    checked: FxHashSet<Node>,
}

impl Checked {
    /// Whether `n` can reach a check (see the module documentation).
    pub fn checked(&self, n: Node) -> bool {
        self.checked.contains(&n)
    }

    /// Whether removing statement `s` can never change whether a test case passes: it
    /// only writes a variable no check depends on, and evaluates nothing else (no call,
    /// `new`, choice or operation that can fail).
    pub fn removable(&self, s: &Stmt) -> bool {
        let (lhs, rest): (&Expr, Vec<&Expr>) = match &s.kind {
            StmtKind::Assign { lhs, rhs } => (lhs, vec![rhs]),
            StmtKind::Insert { lhs, index, value } => (lhs, vec![index, value]),
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => (lhs, vec![value]),
            _ => return false,
        };
        match root(lhs) {
            Some(v) => !self.checked(Node::Var(v)) && pure_target(lhs) && rest.into_iter().all(pure),
            None => false,
        }
    }
}

/// The variable statement `s` writes, if it is an assignment, `+=` or `-=`.
pub fn written(s: &Stmt) -> Option<VarId> {
    match &s.kind {
        StmtKind::Assign { lhs, .. }
        | StmtKind::Insert { lhs, .. }
        | StmtKind::Add { lhs, .. }
        | StmtKind::Remove { lhs, .. } => root(lhs),
        _ => None,
    }
}

/// The variable an assignment target writes (`x` in `x.f[i] = v`).
pub fn root(lhs: &Expr) -> Option<VarId> {
    match &lhs.kind {
        ExprKind::Var(v) => Some(*v),
        ExprKind::TupleAccess { base, .. }
        | ExprKind::NamedTupleAccess { base, .. }
        | ExprKind::SeqAccess { base, .. }
        | ExprKind::SetAccess { base, .. }
        | ExprKind::MapAccess { base, .. } => root(base),
        _ => None,
    }
}

/// Whether `lhs += (index, ...)` appends to a sequence (`index` is `sizeof(lhs)`), which
/// cannot fail.
fn appends(prog: &Program, lhs: &Expr, index: &Expr) -> bool {
    matches!(lhs.ty.kind(prog), crate::sema::TypeKind::Sequence)
        && matches!(&index.kind, ExprKind::Sizeof(x) if same_place(x, lhs))
}

/// Whether `a` and `b` name the same variable, field or element by the same path.
fn same_place(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
        (ExprKind::TupleAccess { base: x, index: i }, ExprKind::TupleAccess { base: y, index: j }) => {
            i == j && same_place(x, y)
        }
        (ExprKind::NamedTupleAccess { base: x, index: i }, ExprKind::NamedTupleAccess { base: y, index: j }) => {
            i == j && same_place(x, y)
        }
        _ => false,
    }
}

/// Whether writing to `lhs` cannot fail: no sequence index on the way, and pure keys.
fn pure_target(lhs: &Expr) -> bool {
    match &lhs.kind {
        ExprKind::Var(_) => true,
        ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => pure_target(base),
        ExprKind::MapAccess { base, key } => pure(key) && pure_target(base),
        _ => false,
    }
}

/// Whether evaluating `e` has no effect but its value and cannot fail: no call, `new`,
/// choice, index, key, division or cast.
fn pure(e: &Expr) -> bool {
    let mut ok = true;
    crate::checker::mutation::visit_expr(e, &mut |e| {
        ok &= !matches!(
            e.kind,
            ExprKind::FunCall { .. }
                | ExprKind::Ctor { .. }
                | ExprKind::Nondet
                | ExprKind::FairNondet
                | ExprKind::Choose(_)
                | ExprKind::SeqAccess { .. }
                | ExprKind::MapAccess { .. }
                | ExprKind::SetAccess { .. }
                | ExprKind::Cast(_)
                | ExprKind::Binary { op: BinOp::Div | BinOp::Mod, .. }
        )
    });
    ok
}

/// Runs the dataflow over every function of `prog`.
pub fn analyze(prog: &Program) -> Checked {
    let mut b = Builder { prog, flows: FxHashMap::default(), sinks: Vec::new(), ctor: ctor_targets(prog) };
    for (i, f) in prog.funs.iter().enumerate() {
        if let Some(body) = &f.body {
            b.block(FunId(i as u32), body);
        }
    }
    // every handler of an event receives its payloads
    for s in &prog.states {
        for h in &s.handlers {
            let p = Node::Payload(h.event);
            match h.action {
                HandlerAction::Do(f) => b.param(f, vec![p]),
                HandlerAction::Goto { target, with } => {
                    if let Some(f) = with {
                        b.param(f, vec![p]);
                    }
                    if let Some(f) = prog.state(target).entry {
                        b.param(f, vec![p]);
                    }
                }
                HandlerAction::Defer | HandlerAction::Ignore => {}
            }
        }
    }
    let mut checked: FxHashSet<Node> = FxHashSet::default();
    let mut todo = std::mem::take(&mut b.sinks);
    while let Some(n) = todo.pop() {
        if checked.insert(n) {
            if let Some(from) = b.flows.get(&n) {
                todo.extend(from.iter().copied());
            }
        }
    }
    Checked { checked }
}

/// The machines `new I(...)` may create for each interface `I`.
fn ctor_targets(prog: &Program) -> FxHashMap<InterfaceId, Vec<MachineId>> {
    let mut out: FxHashMap<InterfaceId, Vec<MachineId>> = FxHashMap::default();
    let modules = prog.tests.iter().map(|t| &t.module).chain(prog.impls.iter().map(|i| &i.module));
    for module in modules {
        for (i, m) in &module.interface_def {
            out.entry(*i).or_default().push(*m);
        }
    }
    for (i, iface) in prog.interfaces.iter().enumerate() {
        if let Some(m) = iface.implicit_for {
            out.entry(InterfaceId(i as u32)).or_default().push(m);
        }
    }
    out
}

struct Builder<'a> {
    prog: &'a Program,
    /// For each value, the values that flow into it.
    flows: FxHashMap<Node, Vec<Node>>,
    /// Values a check reads directly.
    sinks: Vec<Node>,
    ctor: FxHashMap<InterfaceId, Vec<MachineId>>,
}

impl Builder<'_> {
    fn flow(&mut self, from: Vec<Node>, to: Node) {
        self.flows.entry(to).or_default().extend(from);
    }

    /// `from` flows into the first parameter of `f`, if it has one.
    fn param(&mut self, f: FunId, from: Vec<Node>) {
        if let Some(p) = self.prog.fun(f).params.first() {
            self.flow(from, Node::Var(*p));
        }
    }

    /// `from` is the payload of `new I(...)`.
    fn ctor(&mut self, iface: InterfaceId, from: Vec<Node>) {
        let entries: Vec<FunId> = self
            .ctor
            .get(&iface)
            .into_iter()
            .flatten()
            .filter_map(|m| self.prog.machine(*m).start_state.and_then(|s| self.prog.state(s).entry))
            .collect();
        if entries.is_empty() {
            self.sinks.extend(from);
            return;
        }
        for f in entries {
            self.param(f, from.clone());
        }
    }

    /// The payload of an event expression: into the event's payload when it is known,
    /// else checked.
    fn payload(&mut self, event: &Expr, from: Vec<Node>) {
        match event.kind {
            ExprKind::EventRef(e) => self.flow(from, Node::Payload(e)),
            _ => self.sinks.extend(from),
        }
    }

    fn checked(&mut self, f: FunId, e: &Expr) {
        let r = self.reads(f, e);
        self.sinks.extend(r);
    }

    fn block(&mut self, f: FunId, b: &Block) {
        for s in &b.stmts {
            self.stmt(f, s);
        }
    }

    fn stmt(&mut self, f: FunId, s: &Stmt) {
        match &s.kind {
            StmtKind::Block(b) => self.block(f, b),
            StmtKind::Assert { cond, .. } | StmtKind::Assume { cond, .. } => self.checked(f, cond),
            StmtKind::Print(e) => {
                // calls in it still run
                self.reads(f, e);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    let r = self.reads(f, e);
                    self.flow(r, Node::Ret(f));
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
            // each case receives the payloads of its event
            StmtKind::Receive { cases } => {
                for (e, case) in cases {
                    self.param(*case, vec![Node::Payload(*e)]);
                }
            }
            StmtKind::Assign { lhs, rhs } => {
                let mut r = self.reads(f, rhs);
                self.write(f, lhs, &mut r);
            }
            StmtKind::Insert { lhs, index, value } => {
                let mut r = self.reads(f, value);
                let i = self.reads(f, index);
                r.extend(i.iter().copied());
                if appends(self.prog, lhs, index) {
                    self.write(f, lhs, &mut r);
                } else {
                    // inserting into a sequence past its end, or a key a map has, fails
                    self.sinks.extend(i);
                    self.write_container(f, lhs, &mut r);
                }
            }
            StmtKind::Add { lhs, value } => {
                let mut r = self.reads(f, value);
                self.write(f, lhs, &mut r);
            }
            StmtKind::Remove { lhs, value } => {
                // removing at a sequence index can fail
                let r = self.reads(f, value);
                self.sinks.extend(r.iter().copied());
                let mut r = r;
                self.write_container(f, lhs, &mut r);
            }
            StmtKind::While { cond, body } => {
                self.checked(f, cond);
                self.stmt(f, body);
            }
            StmtKind::Foreach { item, collection, body } => {
                let r = self.reads(f, collection);
                self.sinks.extend(r.iter().copied());
                self.flow(r, Node::Var(*item));
                self.stmt(f, body);
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.checked(f, cond);
                self.stmt(f, then_branch);
                self.stmt(f, else_branch);
            }
            StmtKind::Ctor { interface, payload } => {
                let r = payload.as_ref().map(|p| self.reads(f, p)).unwrap_or_default();
                self.ctor(*interface, r);
            }
            StmtKind::FunCall { fun, args } => self.call(f, *fun, args),
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.checked(f, event);
                let r = payload.as_ref().map(|p| self.reads(f, p)).unwrap_or_default();
                self.payload(event, r);
            }
            StmtKind::Send { target, event, payload } => {
                self.checked(f, target);
                self.checked(f, event);
                let r = payload.as_ref().map(|p| self.reads(f, p)).unwrap_or_default();
                self.payload(event, r);
            }
            StmtKind::Goto { state, payload } => {
                let r = payload.as_ref().map(|p| self.reads(f, p)).unwrap_or_default();
                if let Some(entry) = self.prog.state(*state).entry {
                    self.param(entry, r);
                }
            }
        }
    }

    /// An assignment of `values` to `lhs`: they, and the keys on the way, flow into the
    /// variable written.
    fn write(&mut self, f: FunId, lhs: &Expr, values: &mut Vec<Node>) {
        self.lvalue(f, lhs, values);
        match root(lhs) {
            Some(v) => self.flow(std::mem::take(values), Node::Var(v)),
            None => self.sinks.append(values),
        }
    }

    /// Like [`Builder::write`], for an operation on the container at `lhs` whose size
    /// decides whether it fails: the container is checked too.
    fn write_container(&mut self, f: FunId, lhs: &Expr, values: &mut Vec<Node>) {
        if let Some(v) = root(lhs) {
            self.sinks.push(Node::Var(v));
        }
        self.write(f, lhs, values);
    }

    /// The reads along an assignment target: sequence indexes can fail, map keys add.
    fn lvalue(&mut self, f: FunId, lhs: &Expr, out: &mut Vec<Node>) {
        match &lhs.kind {
            ExprKind::TupleAccess { base, .. } | ExprKind::NamedTupleAccess { base, .. } => self.lvalue(f, base, out),
            ExprKind::SeqAccess { base, index } => {
                let r = self.reads(f, index);
                self.sinks.extend(r.iter().copied());
                if let Some(v) = root(base) {
                    self.sinks.push(Node::Var(v));
                }
                out.extend(r);
                self.lvalue(f, base, out);
            }
            ExprKind::MapAccess { base, key } | ExprKind::SetAccess { base, index: key } => {
                let r = self.reads(f, key);
                out.extend(r);
                self.lvalue(f, base, out);
            }
            _ => {}
        }
    }

    fn call(&mut self, f: FunId, callee: FunId, args: &[Expr]) {
        let foreign = self.prog.fun(callee).is_foreign();
        let params = self.prog.fun(callee).params.clone();
        for (i, a) in args.iter().enumerate() {
            let r = self.reads(f, a);
            match params.get(i) {
                Some(p) if !foreign => self.flow(r, Node::Var(*p)),
                _ => self.sinks.extend(r),
            }
        }
    }

    /// The values `e` reads, recording the flows and checks inside it.
    fn reads(&mut self, f: FunId, e: &Expr) -> Vec<Node> {
        let mut out = Vec::new();
        self.reads_into(f, e, &mut out);
        out
    }

    fn reads_into(&mut self, f: FunId, e: &Expr, out: &mut Vec<Node>) {
        match &e.kind {
            ExprKind::Var(v) => out.push(Node::Var(*v)),
            ExprKind::FunCall { fun, args } => {
                self.call(f, *fun, args);
                out.push(Node::Ret(*fun));
            }
            ExprKind::Ctor { interface, payload } => {
                let r = payload.as_ref().map(|p| self.reads(f, p)).unwrap_or_default();
                self.ctor(*interface, r);
            }
            // operations that can fail: what they read decides whether they do
            ExprKind::SeqAccess { base, index }
            | ExprKind::SetAccess { base, index }
            | ExprKind::MapAccess { base, key: index } => {
                let r = self.reads(f, base).into_iter().chain(self.reads(f, index)).collect::<Vec<_>>();
                self.sinks.extend(r.iter().copied());
                out.extend(r);
            }
            ExprKind::Binary { op: BinOp::Div | BinOp::Mod, lhs, rhs } => {
                self.reads_into(f, lhs, out);
                let r = self.reads(f, rhs);
                self.sinks.extend(r.iter().copied());
                out.extend(r);
            }
            ExprKind::Cast(x) | ExprKind::Choose(Some(x)) => {
                let r = self.reads(f, x);
                self.sinks.extend(r.iter().copied());
                out.extend(r);
            }
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Contains { item: lhs, collection: rhs } => {
                self.reads_into(f, lhs, out);
                self.reads_into(f, rhs, out);
            }
            ExprKind::Tuple(es) | ExprKind::NamedTuple(es) | ExprKind::Str { args: es, .. } => {
                for x in es {
                    self.reads_into(f, x, out);
                }
            }
            ExprKind::TupleAccess { base: x, .. }
            | ExprKind::NamedTupleAccess { base: x, .. }
            | ExprKind::Keys(x)
            | ExprKind::Values(x)
            | ExprKind::Sizeof(x)
            | ExprKind::Coerce(x)
            | ExprKind::Unary { operand: x, .. } => self.reads_into(f, x, out),
            ExprKind::EnumElem(..)
            | ExprKind::EventRef(_)
            | ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::NullLit
            | ExprKind::Nondet
            | ExprKind::FairNondet
            | ExprKind::This
            | ExprKind::Default
            | ExprKind::Choose(None)
            | ExprKind::Error => {}
        }
    }
}

/// What a machine is to the test cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Created by a test case, and neither its main machine nor a driver.
    Model,
    /// A test case's main machine, or marked as a driver (see
    /// [`crate::checker::driver_facts::driver_machines`]).
    Driver,
    /// A spec monitor a test case asserts.
    Spec,
    /// In no test case.
    Unused,
}

/// The role of each machine in `tests`, given the driver machines `drivers`.
pub fn roles(prog: &Program, tests: &[TestCase], drivers: &[bool]) -> Vec<Role> {
    let mut roles = vec![Role::Unused; prog.machines.len()];
    for tc in tests {
        let (module, main) = prog.test_case_module(tc);
        for (_, m) in &module.interface_def {
            if roles[m.idx()] == Role::Unused {
                roles[m.idx()] = Role::Model;
            }
        }
        for (m, _) in &module.monitor_map {
            if prog.machine(*m).kind == MachineKind::Spec {
                roles[m.idx()] = Role::Spec;
            }
        }
        if let Some(m) = main {
            roles[m.idx()] = Role::Driver;
        }
    }
    for (i, d) in drivers.iter().enumerate() {
        if *d && roles[i] != Role::Unused {
            roles[i] = Role::Driver;
        }
    }
    roles
}

/// A finding of [`lints`]: its rule, where, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub span: Span,
    pub message: String,
}

/// The functions each machine runs: its methods and handlers, the functions they call,
/// and their receive cases.
pub(crate) fn machine_funs(prog: &Program, m: MachineId) -> Vec<FunId> {
    let mut seen: FxHashSet<FunId> = FxHashSet::default();
    let mut todo: Vec<FunId> = prog.machine(m).methods.clone();
    for s in &prog.machine(m).states {
        let s = prog.state(*s);
        todo.extend(s.entry.into_iter().chain(s.exit));
        for h in &s.handlers {
            match h.action {
                HandlerAction::Do(f) => todo.push(f),
                HandlerAction::Goto { with: Some(f), .. } => todo.push(f),
                _ => {}
            }
        }
    }
    while let Some(f) = todo.pop() {
        if seen.insert(f) {
            todo.extend(prog.fun(f).callees.iter().copied());
            if let Some(b) = &prog.fun(f).body {
                visit_stmts(b, &mut |s| {
                    if let StmtKind::Receive { cases } = &s.kind {
                        todo.extend(cases.iter().map(|(_, c)| *c));
                    }
                });
            }
        }
    }
    let mut v: Vec<FunId> = seen.into_iter().collect();
    v.sort();
    v
}

/// Calls `visit` with every statement in `b`, nested ones included.
pub(crate) fn visit_stmts(b: &Block, visit: &mut dyn FnMut(&Stmt)) {
    fn go(s: &Stmt, visit: &mut dyn FnMut(&Stmt)) {
        visit(s);
        match &s.kind {
            StmtKind::Block(b) => b.stmts.iter().for_each(|s| go(s, visit)),
            StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => go(body, visit),
            StmtKind::If { then_branch, else_branch, .. } => {
                go(then_branch, visit);
                go(else_branch, visit);
            }
            _ => {}
        }
    }
    b.stmts.iter().for_each(|s| go(s, visit));
}

/// The lints the dataflow supports, over the machines of `tests`:
///
/// * `unchecked-state`: a field of a model machine or spec is written, but no check
///   depends on it, so a wrong value there can never fail a test case.
/// * `unchecked-event`: a model machine sends an event no spec observes, and handling
///   it can change nothing a check depends on.
/// * `no-progress-spec`: a test case's driver waits on the model's events, but none of
///   its specs has a hot state, so a model that stops answering passes.
pub fn lints(prog: &Program, tests: &[TestCase], drivers: &[bool]) -> Vec<Finding> {
    let checked = analyze(prog);
    let roles = roles(prog, tests, drivers);
    let mut out = Vec::new();
    let funs: Vec<Vec<FunId>> = (0..prog.machines.len()).map(|m| machine_funs(prog, MachineId(m as u32))).collect();
    // unchecked-state
    for (m, decl) in prog.machines.iter().enumerate() {
        if !matches!(roles[m], Role::Model | Role::Spec) {
            continue;
        }
        let mut writes: FxHashMap<VarId, usize> = FxHashMap::default();
        for f in &funs[m] {
            if let Some(b) = &prog.fun(*f).body {
                visit_stmts(b, &mut |s| {
                    let lhs = match &s.kind {
                        StmtKind::Assign { lhs, .. }
                        | StmtKind::Insert { lhs, .. }
                        | StmtKind::Add { lhs, .. }
                        | StmtKind::Remove { lhs, .. } => lhs,
                        _ => return,
                    };
                    if let Some(v) = root(lhs) {
                        *writes.entry(v).or_default() += 1;
                    }
                });
            }
        }
        for v in &decl.fields {
            let var = prog.var(*v);
            let Some(n) = writes.get(v) else { continue };
            if var.role != VarRole::Field || checked.checked(Node::Var(*v)) {
                continue;
            }
            let what = if roles[m] == Role::Spec { "spec" } else { "machine" };
            out.push(Finding {
                rule: "unchecked-state",
                span: var.span,
                message: format!(
                    "{} {} writes field '{}' {} time{}, but no assertion, condition, spec or sent event depends on it, so a wrong value there passes every test case",
                    what,
                    decl.name,
                    var.name,
                    n,
                    if *n == 1 { "" } else { "s" }
                ),
            });
        }
    }
    // unchecked-event: an event matters when a spec observes it, a `receive` waits for
    // it, or a handler of it can change something a check depends on
    let observed: FxHashSet<EventId> = prog
        .machines
        .iter()
        .enumerate()
        .filter(|(m, _)| roles[*m] == Role::Spec)
        .flat_map(|(_, d)| d.observes.iter().copied())
        .collect();
    let mut matters: FxHashSet<EventId> = observed.clone();
    // `halt` stops the machine
    matters.extend((0..prog.events.len()).map(|e| EventId(e as u32)).filter(|e| prog.event(*e).is_halt()));
    // an event a machine handles in some states but not in others can fail a test case
    // as unhandled
    for (m, decl) in prog.machines.iter().enumerate() {
        if !matches!(roles[m], Role::Model | Role::Driver) {
            continue;
        }
        let handled = |s: &crate::sema::StateId, e: EventId| prog.state(*s).handlers.iter().any(|h| h.event == e);
        for s in &decl.states {
            for h in &prog.state(*s).handlers {
                if decl.states.iter().any(|t| !handled(t, h.event)) {
                    matters.insert(h.event);
                }
            }
        }
    }
    let mut sent: Vec<(EventId, Span, MachineId, bool)> = Vec::new();
    for (m, fs) in funs.iter().enumerate() {
        for f in fs {
            if let Some(b) = &prog.fun(*f).body {
                visit_stmts(b, &mut |s| match &s.kind {
                    StmtKind::Receive { cases } => matters.extend(cases.iter().map(|(e, _)| *e)),
                    StmtKind::Send { event, .. } | StmtKind::Announce { event, .. } if roles[m] == Role::Model => {
                        if let ExprKind::EventRef(e) = event.kind {
                            let announce = matches!(s.kind, StmtKind::Announce { .. });
                            sent.push((e, s.span, MachineId(m as u32), announce));
                        }
                    }
                    _ => {}
                });
            }
        }
    }
    loop {
        let before = matters.len();
        for s in &prog.states {
            if roles[s.machine.idx()] == Role::Unused {
                continue;
            }
            for h in &s.handlers {
                if matters.contains(&h.event) {
                    continue;
                }
                let effect = match h.action {
                    HandlerAction::Defer | HandlerAction::Ignore => false,
                    HandlerAction::Goto { .. } => true,
                    HandlerAction::Do(f) => {
                        checked.checked(Node::Payload(h.event)) || has_effect(prog, &checked, &matters, f)
                    }
                };
                if effect {
                    matters.insert(h.event);
                }
            }
        }
        if matters.len() == before {
            break;
        }
    }
    let mut reported: FxHashSet<EventId> = FxHashSet::default();
    for (e, span, m, announce) in sent {
        if !matters.contains(&e) && reported.insert(e) {
            // scenarios observe events but check nothing: say so, so the author does not
            // take the scenario for the check
            let scenarios: Vec<&str> = prog
                .machines
                .iter()
                .filter(|d| d.is_scenario() && d.observes.contains(&e))
                .map(|d| d.name.as_str())
                .collect();
            let message = if !scenarios.is_empty() {
                format!(
                    "machine {} {} '{}', but only scenario{} {} observe{} it, and scenarios check nothing, so the model passes whatever it {}; observe it in a spec{}",
                    prog.machine(m).name,
                    if announce { "announces" } else { "sends" },
                    prog.event(e).name,
                    if scenarios.len() == 1 { "" } else { "s" },
                    scenarios.join(", "),
                    if scenarios.len() == 1 { "s" } else { "" },
                    if announce { "announces" } else { "sends" },
                    if announce { ", or let the scenario watch a sent event instead" } else { "" }
                )
            } else if announce {
                format!(
                    "machine {} announces '{}', but no spec observes it, so the model passes whatever it announces",
                    prog.machine(m).name,
                    prog.event(e).name
                )
            } else {
                format!(
                    "machine {} sends '{}', but no spec observes it and no machine that handles it changes anything a check depends on, so the model passes whatever it sends",
                    prog.machine(m).name,
                    prog.event(e).name
                )
            };
            out.push(Finding { rule: "unchecked-event", span, message });
        }
    }
    // no-progress-spec
    let mut waiting_reported: FxHashSet<Span> = FxHashSet::default();
    for t in &prog.tests {
        let module = &t.module;
        let hot = module.monitor_map.iter().any(|(m, _)| {
            prog.machine(*m).kind == MachineKind::Spec
                && prog.machine(*m).states.iter().any(|s| prog.state(*s).temperature == Temperature::Hot)
        });
        if hot {
            continue;
        }
        let in_module = |m: MachineId| m == t.main || module.interface_def.iter().any(|(_, x)| *x == m);
        let is_driver = |m: MachineId| m == t.main || drivers.get(m.idx()).copied().unwrap_or(false);
        // events the model machines of this test send
        let mut from_model: FxHashSet<EventId> = FxHashSet::default();
        for (m, fs) in funs.iter().enumerate() {
            let id = MachineId(m as u32);
            if !in_module(id) || is_driver(id) || prog.machine(id).is_spec() {
                continue;
            }
            for f in fs {
                if let Some(b) = &prog.fun(*f).body {
                    visit_stmts(b, &mut |s| {
                        if let StmtKind::Send { event, .. } = &s.kind {
                            if let ExprKind::EventRef(e) = event.kind {
                                from_model.insert(e);
                            }
                        }
                    });
                }
            }
        }
        // ... that a driver of this test waits for
        let waits = funs.iter().enumerate().any(|(m, fs)| {
            let id = MachineId(m as u32);
            in_module(id)
                && is_driver(id)
                && (prog.machine(id).states.iter().any(|s| {
                    prog.state(*s).handlers.iter().any(|h| {
                        from_model.contains(&h.event)
                            && !matches!(h.action, HandlerAction::Ignore | HandlerAction::Defer)
                    })
                }) || fs.iter().any(|f| {
                    let mut found = false;
                    if let Some(b) = &prog.fun(*f).body {
                        visit_stmts(b, &mut |s| {
                            if let StmtKind::Receive { cases } = &s.kind {
                                found |= cases.iter().any(|(e, _)| from_model.contains(e));
                            }
                        });
                    }
                    found
                }))
        });
        if waits {
            out.push(Finding {
                rule: "no-progress-spec",
                span: t.span,
                message: format!(
                    "test '{}' waits for the model's responses, but none of its specs has a hot state, so a model that stops responding passes",
                    t.name
                ),
            });
            continue;
        }
        // a design machine that sends a request and waits for the reply, reported where
        // it waits, once for all the tests
        for (m, e, span) in crate::checker::flow_lints::waiting_machines(prog, t, &is_driver) {
            if !waiting_reported.insert(span) {
                continue;
            }
            out.push(Finding {
                rule: "no-progress-spec",
                span,
                message: format!(
                    "machine {} waits for '{}' after sending a request, but no spec of test '{}' has a hot state, so a model that never replies passes",
                    prog.machine(m).name,
                    prog.event(e).name,
                    t.name
                ),
            });
        }
    }
    crate::checker::flow_lints::lints(prog, drivers, &mut out);
    out.sort_by(|a, b| (a.span, a.rule).cmp(&(b.span, b.rule)));
    out
}

/// Whether running `f` (and what it calls) can change something a check depends on,
/// given the events that matter: an assertion, a write to a checked variable, a send,
/// announce or raise of an event that matters, a `new`, a `goto`, or a `receive`.
fn has_effect(prog: &Program, checked: &Checked, matters: &FxHashSet<EventId>, f: FunId) -> bool {
    let mut seen: FxHashSet<FunId> = FxHashSet::default();
    let mut todo = vec![f];
    while let Some(f) = todo.pop() {
        if !seen.insert(f) {
            continue;
        }
        let decl = prog.fun(f);
        let Some(b) = &decl.body else { return true };
        todo.extend(decl.callees.iter().copied());
        let mut effect = false;
        visit_stmts(b, &mut |s| {
            effect |= match &s.kind {
                StmtKind::Assert { .. }
                | StmtKind::Assume { .. }
                | StmtKind::Goto { .. }
                | StmtKind::Ctor { .. }
                | StmtKind::Receive { .. } => true,
                StmtKind::Assign { lhs, .. }
                | StmtKind::Insert { lhs, .. }
                | StmtKind::Add { lhs, .. }
                | StmtKind::Remove { lhs, .. } => {
                    root(lhs).is_none_or(|v| checked.checked(Node::Var(v)) || prog.var(v).role == VarRole::GlobalParam)
                }
                StmtKind::Send { event, .. } | StmtKind::Announce { event, .. } | StmtKind::Raise { event, .. } => {
                    match event.kind {
                        ExprKind::EventRef(e) => matters.contains(&e),
                        _ => true,
                    }
                }
                _ => false,
            };
            // `new` inside an expression
            let mut ctor = false;
            each_expr(s, &mut |e| ctor |= matches!(e.kind, ExprKind::Ctor { .. }));
            effect |= ctor;
        });
        if effect {
            return true;
        }
    }
    false
}

/// Calls `f` with each expression directly in statement `s` (not nested statements).
/// An assertion's message is not among them.
pub(crate) fn each_expr(s: &Stmt, f: &mut dyn FnMut(&Expr)) {
    let mut go = |e: &Expr| crate::checker::mutation::visit_expr(e, f);
    match &s.kind {
        StmtKind::Assert { cond, .. } | StmtKind::Assume { cond, .. } => go(cond),
        StmtKind::Print(e) => go(e),
        StmtKind::Return(Some(e)) => go(e),
        StmtKind::Assign { lhs, rhs } => {
            go(lhs);
            go(rhs);
        }
        StmtKind::Insert { lhs, index, value } => {
            go(lhs);
            go(index);
            go(value);
        }
        StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
            go(lhs);
            go(value);
        }
        StmtKind::While { cond, .. } | StmtKind::If { cond, .. } => go(cond),
        StmtKind::Foreach { collection, .. } => go(collection),
        StmtKind::FunCall { args, .. } => args.iter().for_each(go),
        StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
            go(event);
            payload.iter().for_each(go);
        }
        StmtKind::Send { target, event, payload } => {
            go(target);
            go(event);
            payload.iter().for_each(go);
        }
        StmtKind::Ctor { payload, .. } | StmtKind::Goto { payload, .. } => payload.iter().for_each(go),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str) -> Program {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src.to_string());
        crate::sema::compile(&sm, &crate::sema::Options::default())
            .unwrap_or_else(|_| panic!("does not compile"))
            .program
    }

    fn field(prog: &Program, machine: &str, name: &str) -> Node {
        let m = prog.machines.iter().find(|m| m.name == machine).unwrap();
        Node::Var(*m.fields.iter().find(|v| prog.var(**v).name == name).unwrap())
    }

    /// Values reach checks through calls, returns, payloads, `new` and specs; values that
    /// only flow into each other, or into a log, do not.
    const MODEL: &str = "event eReq: int;\n\
        event eSeen: int;\n\
        machine Server {\n\
          var total: int; var viaCall: int; var viaNew: int; var copy: int; var log: seq[int]; var keyed: map[int, int];\n\
          start state S {\n\
            on eReq do (n: int) {\n\
              total = total + n;\n\
              viaCall = Twice(n);\n\
              if (Positive(viaCall)) { copy = total; }\n\
              viaNew = n;\n\
              new Helper(viaNew);\n\
              log += (sizeof(log), copy);\n\
              keyed[n] = copy;\n\
              announce eSeen, total;\n\
            }\n\
          }\n\
          fun Twice(x: int): int { return x + x; }\n\
          fun Positive(x: int): bool { return x > 0; }\n\
        }\n\
        machine Helper { start state S { entry (k: int) { assert k < 100; } } }\n\
        spec Watch observes eSeen { start state S { on eSeen do (t: int) { assert t >= 0; } } }\n\
        machine Main { start state S { entry { var s: Server; s = new Server(); send s, eReq, 1; } } }\n\
        test tc [main=Main]: assert Watch in { Server, Helper, Main };\n";

    #[test]
    fn finds_what_checks_depend_on() {
        let prog = program(MODEL);
        let c = analyze(&prog);
        for f in ["total", "viaCall", "viaNew"] {
            assert!(c.checked(field(&prog, "Server", f)), "{} should be checked", f);
        }
        for f in ["copy", "log", "keyed"] {
            assert!(!c.checked(field(&prog, "Server", f)), "{} should not be checked", f);
        }
    }

    #[test]
    fn reports_fields_no_check_depends_on() {
        let prog = program(MODEL);
        let tests = prog.test_cases();
        let found: Vec<_> = lints(&prog, &tests, &vec![false; prog.machines.len()])
            .into_iter()
            .filter(|f| f.rule == "unchecked-state")
            .map(|f| f.message.split('\'').nth(1).unwrap().to_string())
            .collect();
        assert_eq!(found, ["copy", "log", "keyed"]);
    }

    fn findings(src: &str, rule: &str) -> Vec<String> {
        let prog = program(src);
        let tests = prog.test_cases();
        lints(&prog, &tests, &vec![false; prog.machines.len()])
            .into_iter()
            .filter(|f| f.rule == rule)
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn an_event_only_a_scenario_observes_is_named_as_such() {
        let src = "event eConflict;\n\
            machine Store { start state S { entry { announce eConflict; } } }\n\
            scenario Conflicted observes eConflict { start state W { on eConflict goto D; } cold state D { } }\n\
            machine Main { start state S { entry { new Store(); } } }\n\
            test tc [main=Main]: { Store, Main };\n";
        assert_eq!(
            findings(src, "unchecked-event"),
            ["machine Store announces 'eConflict', but only scenario Conflicted observes it, and scenarios check nothing, so the model passes whatever it announces; observe it in a spec, or let the scenario watch a sent event instead"]
        );
    }

    /// A driver that starts a second doctor only once the first says it is done.
    const STAGES: &str = "event eDone;\n\
        event eTreat: int;\n\
        event ePlanned: int;\n\
        spec Treated observes eTreat, ePlanned { start state S { on eTreat do (n: int) { assert n > 0; } ignore ePlanned; } }\n\
        machine Doctor { start state S { entry (owner: Main) { announce eTreat, 1; send owner, eDone; } } }\n\
        machine Main {\n\
          start state First { entry { PLAN new Doctor(this); } on eDone goto Second; }\n\
          state Second { entry { new Doctor(this); } ignore eDone; }\n\
        }\n\
        test tc [main=Main]: assert Treated in { Doctor, Main };\n";

    #[test]
    fn a_driver_stage_started_by_a_model_notification_needs_an_announced_plan() {
        assert_eq!(
            findings(&STAGES.replace("PLAN", ""), "unplanned-driver-stage"),
            ["driver Main creates Doctor only after the model sends 'eDone', and announces no plan a spec observes, so if the model never sends it this stage never starts and every spec still passes"]
        );
        assert!(findings(&STAGES.replace("PLAN", "announce ePlanned, 2;"), "unplanned-driver-stage").is_empty());
    }

    #[test]
    fn a_design_machine_waiting_for_a_reply_needs_a_hot_spec() {
        let src = "event ePing: Client;\n\
            event ePong;\n\
            spec Pongs observes ePong { start state S { on ePong do { assert true; } } }\n\
            machine Counter { start state S { on ePing do (c: Client) { send c, ePong; } } }\n\
            machine Client {\n\
              start state Init { entry (c: Counter) { send c, ePing, this; goto Waiting; } }\n\
              state Waiting { on ePong goto Done; }\n\
              state Done { }\n\
            }\n\
            machine Main { start state S { entry { new Client(new Counter()); } } }\n\
            test tc [main=Main]: assert Pongs in { Counter, Client, Main };\n";
        assert_eq!(
            findings(src, "no-progress-spec"),
            ["machine Client waits for 'ePong' after sending a request, but no spec of test 'tc' has a hot state, so a model that never replies passes"]
        );
        // a server waiting for requests is not waiting for a reply
        assert!(findings(
            &src.replace("state Waiting { on ePong goto Done; }", "state Waiting { }"),
            "no-progress-spec"
        )
        .is_empty());
        assert!(findings(&src.replace("start state S { on ePong", "start hot state S { on ePong"), "no-progress-spec")
            .is_empty());
    }

    #[test]
    fn statements_after_a_call_that_always_leaves_the_state_are_unreachable() {
        let src = "event eT;\n\
            machine Main {\n\
              start state S { entry { send this, eT; } on eT do { if ($) { Resolve(); return; } Maybe(); return; } }\n\
              state Done { }\n\
              fun Resolve() { print \"done\"; goto Done; }\n\
              fun Maybe() { if ($) { goto Done; } }\n\
            }\n\
            test tc [main=Main]: { Main };\n";
        assert_eq!(
            findings(src, "unreachable-statement"),
            ["this statement never runs: the call to Resolve before it always leaves the state (goto Done)"]
        );
    }

    #[test]
    fn only_pure_writes_of_unchecked_values_are_removable() {
        let prog = program(MODEL);
        let c = analyze(&prog);
        let handler = prog.funs.iter().find(|f| f.body.as_ref().is_some_and(|b| b.stmts.len() == 8)).unwrap();
        let removable: Vec<bool> = handler.body.as_ref().unwrap().stmts.iter().map(|s| c.removable(s)).collect();
        // `copy = total` is inside the `if`; the log append and the map write are removable
        assert_eq!(removable, [false, false, false, false, false, true, true, false]);
    }
}
