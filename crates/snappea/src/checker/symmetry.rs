//! Finds the `choose(n)` values an exhaustive search need not tell apart.
//!
//! A model picks a key with `choose(10)` and only stores it, sends it and compares it
//! for equality; or it injects a fault with `choose(100) < 5`. Most of the values then
//! behave alike: any key that is not yet in use is as good as another, and every value
//! under 5 crashes the node. Trying each of them repeats the same schedule under
//! another value, and a search that must try them all cannot finish.
//!
//! This analysis follows every value a `choose(n)` produces through the program and
//! collects what the program can tell about it:
//!
//! * the constants it may be compared with or share a variable with: literals, and the
//!   zero a field or an unassigned local starts as. They split `0..n` into the constants
//!   themselves and the gaps between them;
//! * whether it is compared with them by an ordering (`<`), so that the gaps differ, or
//!   only for equality, so that all the values that are no constant are alike;
//! * whether such values are compared with each other (`==`, `in`, a map key, a set
//!   element), so that two of them being equal matters, not only where each lies.
//!
//! The search then tries the constants, and of the others one value per gap; when the
//! values are compared with each other, the values already chosen and one unused value
//! (see [`ValueClass::offered`]). Three choices of a key from `choose(10)` then have 5
//! outcomes to try, not 1000, and `choose(100) < 5` has 2, not 100.
//!
//! It gives up on a value (taints it) when the program can tell more: arithmetic, an
//! ordering against another chosen value, a sequence index, a `choose` bound, a
//! conversion, a formatted string that is itself compared or kept in a set, a parameter
//! of the test, a foreign function, an event sent through a variable, a cast that changes its
//! shape. Collections iterate in insertion order, so holding a value does not order it.
//!
//! The analysis is flow-insensitive and by unification: values that may meet share a
//! class. It never claims a site it cannot prove, so the cost of a miss is only the
//! search done without it.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::sema::{
    BinOp, Block, EventId, Expr, ExprKind, FunId, HandlerAction, InterfaceId, MachineId, PType, Program, Stmt,
    StmtKind, UnOp, VarId, VarRole,
};
use crate::syntax::Span;

/// A `choose(n)` with a literal bound, and what the analysis found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub span: Span,
    pub bound: i64,
    /// Its class in [`ValueClasses::classes`], if the search can skip some of its values.
    pub class: Option<u32>,
    /// Else where the program tells its values apart, if it does: the use, or the
    /// value they meet. `None` too when every value is a constant the program names.
    pub blocked_by: Option<Span>,
}

/// What the program can tell about the values of some `choose(n)` sites that may meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueClass {
    /// The `n` of its sites.
    pub bound: i64,
    /// The constants in `0..n` the values may be compared with, sorted.
    pub constants: Vec<i64>,
    /// The values are compared with the constants by an ordering.
    pub ordered: bool,
    /// The values are compared with each other for equality.
    pub mutual: bool,
}

impl ValueClass {
    /// The values to try for the next choice, sorted, when the schedule has chosen
    /// `used` in this class so far: the constants, and of each gap between them (of
    /// all the other values, unless `ordered`) the values in `used` and the smallest
    /// one that is not (just the smallest one, unless `mutual`).
    pub fn offered(&self, used: &[i64]) -> Vec<i64> {
        let mut out = self.constants.clone();
        let mut fresh = false;
        let mut lo = 0;
        let ends = self.constants.iter().copied().chain([self.bound]);
        for hi in ends {
            if self.mutual {
                out.extend((lo..hi).filter(|v| used.contains(v)));
            }
            let next = (lo..hi).find(|v| !self.mutual || !used.contains(v));
            if let Some(v) = next.filter(|_| self.ordered || !fresh) {
                out.push(v);
                fresh = true;
            }
            lo = hi + 1;
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The `choose(n)` sites of a program whose values a search need not all try.
#[derive(Debug, Clone, Default)]
pub struct ValueClasses {
    /// By the address of the bound expression, which the compiler has at hand.
    by_bound: FxHashMap<usize, u32>,
    pub classes: Vec<ValueClass>,
    pub sites: Vec<Site>,
}

impl ValueClasses {
    /// The class of the `choose` whose bound is `bound`, if it has one.
    pub fn class_of(&self, bound: &Expr) -> Option<u32> {
        self.by_bound.get(&(bound as *const Expr as usize)).copied()
    }
}

/// The smallest bound worth a class: `choose(2)` has two outcomes either way.
const MIN_BOUND: i64 = 3;

type Id = u32;

enum Kind {
    Scalar,
    Tuple(Vec<Id>),
    /// A sequence (`key` is its index), a set (`key` and `val` are its element) or a map.
    Coll {
        key: Id,
        val: Id,
    },
    /// Values of a shape this lost track of: everything in them is tainted.
    Top,
}

struct Node {
    parent: Id,
    kind: Kind,
    /// Where the program tells the values here apart, if it does.
    tainted: Option<Span>,
    /// The integer constants that may be here, or be compared for equality with what is.
    constants: Vec<i64>,
    /// Values here are compared with each other for equality.
    mutual: bool,
}

/// Runs the analysis over every function of `prog`.
pub fn analyze(prog: &Program) -> ValueClasses {
    let mut b = Builder {
        prog,
        nodes: Vec::new(),
        vars: FxHashMap::default(),
        rets: FxHashMap::default(),
        payloads: FxHashMap::default(),
        sites: Vec::new(),
        strings: Vec::new(),
        orderings: Vec::new(),
        ctor: ctor_targets(prog),
        defaulted: FxHashSet::default(),
        quiet: false,
        at: Span::dummy(),
    };
    for f in prog.funs.iter().filter(|f| f.parent.is_none()) {
        if let Some(body) = &f.body {
            let mut assigned: FxHashSet<VarId> = f.params.iter().copied().collect();
            b.assigned_block(body, &mut assigned);
        }
    }
    for (i, f) in prog.funs.iter().enumerate() {
        if let Some(body) = &f.body {
            b.block(FunId(i as u32), body);
        }
    }
    // every handler of an event receives its payloads
    for s in &prog.states {
        for h in &s.handlers {
            let p = b.payload_of(h.event);
            match h.action {
                HandlerAction::Do(f) => b.param(f, p),
                HandlerAction::Goto { target, with } => {
                    if let Some(f) = with {
                        b.param(f, p);
                    }
                    if let Some(f) = prog.state(target).entry {
                        b.param(f, p);
                    }
                }
                HandlerAction::Defer | HandlerAction::Ignore => {}
            }
        }
    }
    b.finish()
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

/// A literal integer, which a comparison names without holding a chosen value.
fn literal(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::IntLit(n) => Some(*n),
        ExprKind::Unary { op: UnOp::Neg, operand } => literal(operand).and_then(i64::checked_neg),
        _ => None,
    }
}

struct Builder<'a> {
    prog: &'a Program,
    nodes: Vec<Node>,
    vars: FxHashMap<VarId, Id>,
    rets: FxHashMap<FunId, Id>,
    payloads: FxHashMap<EventId, Id>,
    /// The bound expression, span, bound and value of each site.
    sites: Vec<(usize, Span, i64, Id)>,
    /// Each formatted string outside a message, the values in it, and where it is.
    strings: Vec<(Id, Vec<Id>, Span)>,
    /// The operands of each ordering, and where it is. Which side holds chosen values is
    /// only known once every flow is.
    orderings: Vec<(Id, Id, Span)>,
    ctor: FxHashMap<InterfaceId, Vec<MachineId>>,
    /// Locals that may be read before they are assigned, so may hold their default.
    defaulted: FxHashSet<VarId>,
    /// In a `print` or an assertion message, whose strings go nowhere.
    quiet: bool,
    /// The statement or expression being read, to blame for a taint.
    at: Span,
}

impl Builder<'_> {
    // ---- locals that may hold their default ------------------------------------------

    fn assigned_block(&mut self, b: &Block, a: &mut FxHashSet<VarId>) {
        for s in &b.stmts {
            self.assigned_stmt(s, a);
        }
    }

    /// Adds to `a` the locals `s` always assigns, and to `defaulted` those it may read
    /// while they are not in `a`. Jumps are ignored: what follows one does not run.
    fn assigned_stmt(&mut self, s: &Stmt, a: &mut FxHashSet<VarId>) {
        match &s.kind {
            StmtKind::Block(b) => self.assigned_block(b, a),
            StmtKind::Assign { lhs, rhs } => {
                self.reads(rhs, a);
                match lhs.kind {
                    ExprKind::Var(v) => {
                        a.insert(v);
                    }
                    _ => self.reads(lhs, a),
                }
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.reads(cond, a);
                let mut then = a.clone();
                self.assigned_stmt(then_branch, &mut then);
                self.assigned_stmt(else_branch, a);
                a.retain(|v| then.contains(v));
            }
            StmtKind::While { cond, body } => {
                self.reads(cond, a);
                self.assigned_stmt(body, &mut a.clone());
            }
            StmtKind::Foreach { item, collection, body } => {
                self.reads(collection, a);
                let mut inner = a.clone();
                inner.insert(*item);
                self.assigned_stmt(body, &mut inner);
            }
            // a case shares the frame of its function
            StmtKind::Receive { cases } => {
                for (_, case) in cases {
                    let f = self.prog.fun(*case);
                    if let Some(body) = &f.body {
                        let mut inner = a.clone();
                        inner.extend(f.params.iter().copied());
                        self.assigned_block(body, &mut inner);
                    }
                }
            }
            StmtKind::Assert { cond: x, message: y } | StmtKind::Assume { cond: x, message: y } => {
                self.reads(x, a);
                self.reads(y, a);
            }
            StmtKind::Insert { lhs, index, value } => {
                self.reads(lhs, a);
                self.reads(index, a);
                self.reads(value, a);
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                self.reads(lhs, a);
                self.reads(value, a);
            }
            StmtKind::Print(e) | StmtKind::Return(Some(e)) => self.reads(e, a),
            StmtKind::Ctor { payload, .. } | StmtKind::Goto { payload, .. } => {
                if let Some(p) = payload {
                    self.reads(p, a);
                }
            }
            StmtKind::FunCall { args, .. } => args.iter().for_each(|x| self.reads(x, a)),
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.reads(event, a);
                if let Some(p) = payload {
                    self.reads(p, a);
                }
            }
            StmtKind::Send { target, event, payload } => {
                self.reads(target, a);
                self.reads(event, a);
                if let Some(p) = payload {
                    self.reads(p, a);
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
        }
    }

    fn reads(&mut self, e: &Expr, a: &FxHashSet<VarId>) {
        visit_vars(e, &mut |v| {
            if self.prog.var(v).role == VarRole::Local && !a.contains(&v) {
                self.defaulted.insert(v);
            }
        });
    }

    // ---- shapes ----------------------------------------------------------------------

    fn fresh(&mut self, kind: Kind, tainted: Option<Span>) -> Id {
        let id = self.nodes.len() as Id;
        self.nodes.push(Node { parent: id, kind, tainted, constants: Vec::new(), mutual: false });
        id
    }

    /// A value that is no integer: a boolean, a machine, `null`.
    fn clean(&mut self) -> Id {
        self.fresh(Kind::Scalar, None)
    }

    /// A value the analysis does not know: a result of arithmetic, a size.
    fn unknown(&mut self) -> Id {
        self.fresh(Kind::Scalar, Some(self.at))
    }

    fn constant(&mut self, c: i64) -> Id {
        let id = self.clean();
        self.nodes[id as usize].constants.push(c);
        id
    }

    fn find(&mut self, mut id: Id) -> Id {
        while self.nodes[id as usize].parent != id {
            let up = self.nodes[self.nodes[id as usize].parent as usize].parent;
            self.nodes[id as usize].parent = up;
            id = up;
        }
        id
    }

    /// A value of type `ty`. With `default`, one that may be the type's default, whose
    /// integers are zero; its collections are empty either way.
    fn shape(&mut self, ty: &PType, default: bool) -> Id {
        let ty = ty.canonicalize(self.prog);
        self.shape_of(&ty, default)
    }

    fn shape_of(&mut self, ty: &PType, default: bool) -> Id {
        match ty {
            PType::Seq(t) => {
                let key = self.unknown();
                let val = self.shape_of(t, false);
                self.fresh(Kind::Coll { key, val }, None)
            }
            // a set compares its elements with each other, a map its keys
            PType::Set(t) => {
                let val = self.shape_of(t, false);
                self.mutual(val);
                self.fresh(Kind::Coll { key: val, val }, None)
            }
            PType::Map(k, v) => {
                let key = self.shape_of(k, false);
                self.mutual(key);
                let val = self.shape_of(v, false);
                self.fresh(Kind::Coll { key, val }, None)
            }
            PType::Tuple(ts) => {
                let fields = ts.iter().map(|t| self.shape_of(t, default)).collect();
                self.fresh(Kind::Tuple(fields), None)
            }
            PType::NamedTuple(fs) => {
                let fields = fs.iter().map(|(_, t)| self.shape_of(t, default)).collect();
                self.fresh(Kind::Tuple(fields), None)
            }
            PType::Int if default => self.constant(0),
            _ => self.clean(),
        }
    }

    /// Calls `visit` with `id` and every value inside it.
    fn each(&mut self, id: Id, visit: &mut dyn FnMut(&mut Node)) {
        let mut todo = vec![id];
        let mut seen: FxHashSet<Id> = FxHashSet::default();
        while let Some(id) = todo.pop() {
            let id = self.find(id);
            if !seen.insert(id) {
                continue;
            }
            let n = &mut self.nodes[id as usize];
            visit(n);
            match &n.kind {
                Kind::Tuple(fields) => todo.extend(fields.iter().copied()),
                Kind::Coll { key, val } => todo.extend([*key, *val]),
                Kind::Scalar | Kind::Top => {}
            }
        }
    }

    /// The program tells the values in `id` apart, at `self.at`.
    fn taint(&mut self, id: Id) {
        let why = self.at;
        self.taint_for(id, why);
    }

    fn taint_for(&mut self, id: Id, why: Span) {
        self.each(id, &mut |n| {
            n.tainted.get_or_insert(why);
        });
    }

    /// The values in `id` are compared with each other for equality.
    fn mutual(&mut self, id: Id) {
        self.each(id, &mut |n| n.mutual = true);
    }

    /// The values of `a` and `b` may meet: by an assignment, a comparison, a call.
    fn unify(&mut self, a: Id, b: Id) {
        let mut todo = vec![(a, b)];
        while let Some((a, b)) = todo.pop() {
            let (a, b) = (self.find(a), self.find(b));
            if a == b {
                continue;
            }
            let ka = std::mem::replace(&mut self.nodes[a as usize].kind, Kind::Scalar);
            let kb = std::mem::replace(&mut self.nodes[b as usize].kind, Kind::Scalar);
            let from = &mut self.nodes[b as usize];
            from.parent = a;
            let (tainted, mutual, constants) = (from.tainted, from.mutual, std::mem::take(&mut from.constants));
            let into = &mut self.nodes[a as usize];
            into.tainted = into.tainted.or(tainted);
            into.mutual |= mutual;
            for c in constants {
                if !into.constants.contains(&c) {
                    into.constants.push(c);
                }
            }
            let tainted = into.tainted;
            let kind = match (ka, kb) {
                (Kind::Scalar, Kind::Scalar) => Kind::Scalar,
                (Kind::Tuple(x), Kind::Tuple(y)) if x.len() == y.len() => {
                    todo.extend(x.iter().copied().zip(y));
                    Kind::Tuple(x)
                }
                (Kind::Coll { key, val }, Kind::Coll { key: k, val: v }) => {
                    todo.extend([(key, k), (val, v)]);
                    Kind::Coll { key, val }
                }
                // different shapes, as when a tuple is stored in an `any`
                (ka, kb) => {
                    let why = tainted.unwrap_or(self.at);
                    for k in [ka, kb] {
                        match k {
                            Kind::Tuple(fields) => fields.into_iter().for_each(|f| self.taint_for(f, why)),
                            Kind::Coll { key, val } => {
                                self.taint_for(key, why);
                                self.taint_for(val, why);
                            }
                            Kind::Scalar | Kind::Top => {}
                        }
                    }
                    self.nodes[a as usize].tainted = Some(why);
                    Kind::Top
                }
            };
            self.nodes[a as usize].kind = kind;
        }
    }

    /// Gives up on the shape of `id`.
    fn top(&mut self, id: Id) -> Id {
        let id = self.find(id);
        self.taint(id);
        self.nodes[id as usize].kind = Kind::Top;
        id
    }

    fn field(&mut self, id: Id, index: u32) -> Id {
        let id = self.find(id);
        match &self.nodes[id as usize].kind {
            Kind::Tuple(fields) if (index as usize) < fields.len() => fields[index as usize],
            _ => self.top(id),
        }
    }

    fn key(&mut self, id: Id) -> Id {
        let id = self.find(id);
        match self.nodes[id as usize].kind {
            Kind::Coll { key, .. } => key,
            _ => self.top(id),
        }
    }

    fn val(&mut self, id: Id) -> Id {
        let id = self.find(id);
        match self.nodes[id as usize].kind {
            Kind::Coll { val, .. } => val,
            _ => self.top(id),
        }
    }

    fn var(&mut self, v: VarId) -> Id {
        if let Some(id) = self.vars.get(&v) {
            return *id;
        }
        let decl = self.prog.var(v);
        let default = match decl.role {
            VarRole::Field => true,
            VarRole::Local => self.defaulted.contains(&v),
            VarRole::Param | VarRole::GlobalParam => false,
        };
        let id = self.shape(&decl.ty, default);
        // a parameter of the test differs from one test case to the next
        if decl.role == VarRole::GlobalParam {
            self.taint_for(id, decl.span);
        }
        self.vars.insert(v, id);
        id
    }

    fn ret(&mut self, f: FunId) -> Id {
        let decl = self.prog.fun(f);
        if decl.is_foreign() {
            let id = self.shape(&decl.ret, false);
            self.taint(id);
            return id;
        }
        if let Some(id) = self.rets.get(&f) {
            return *id;
        }
        let id = self.shape(&decl.ret, false);
        self.rets.insert(f, id);
        id
    }

    fn payload_of(&mut self, e: EventId) -> Id {
        if let Some(id) = self.payloads.get(&e) {
            return *id;
        }
        let id = self.shape(&self.prog.event(e).payload, false);
        self.payloads.insert(e, id);
        id
    }

    // ---- flows -----------------------------------------------------------------------

    /// `from` flows into the first parameter of `f`, if it has one.
    fn param(&mut self, f: FunId, from: Id) {
        if let Some(p) = self.prog.fun(f).params.first() {
            let p = self.var(*p);
            self.unify(from, p);
        }
    }

    /// `from` is the payload of `new I(...)`.
    fn ctor(&mut self, iface: InterfaceId, from: Option<Id>) {
        let Some(from) = from else { return };
        let entries: Vec<FunId> = self
            .ctor
            .get(&iface)
            .into_iter()
            .flatten()
            .filter_map(|m| self.prog.machine(*m).start_state.and_then(|s| self.prog.state(s).entry))
            .collect();
        if entries.is_empty() {
            self.taint(from);
        }
        for f in entries {
            self.param(f, from);
        }
    }

    /// The payload of an event expression: into the event's payload when it is known.
    fn payload(&mut self, event: &Expr, from: Option<Id>) {
        let Some(from) = from else { return };
        match event.kind {
            ExprKind::EventRef(e) => {
                let p = self.payload_of(e);
                self.unify(from, p);
            }
            _ => self.taint(from),
        }
    }

    fn call(&mut self, callee: FunId, args: &[Expr]) {
        let foreign = self.prog.fun(callee).is_foreign();
        let params = self.prog.fun(callee).params.clone();
        for (i, a) in args.iter().enumerate() {
            let v = self.expr(a);
            match params.get(i) {
                Some(p) if !foreign => {
                    let p = self.var(*p);
                    self.unify(v, p);
                }
                _ => self.taint(v),
            }
        }
    }

    fn block(&mut self, f: FunId, b: &Block) {
        for s in &b.stmts {
            self.stmt(f, s);
        }
    }

    fn message(&mut self, e: &Expr) {
        let outer = std::mem::replace(&mut self.quiet, true);
        self.expr(e);
        self.quiet = outer;
    }

    fn stmt(&mut self, f: FunId, s: &Stmt) {
        self.at = s.span;
        match &s.kind {
            StmtKind::Block(b) => self.block(f, b),
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                self.expr(cond);
                self.message(message);
            }
            StmtKind::Print(e) => self.message(e),
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    let v = self.expr(e);
                    let r = self.ret(f);
                    self.unify(v, r);
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
            // each case receives the payloads of its event
            StmtKind::Receive { cases } => {
                for (e, case) in cases {
                    let p = self.payload_of(*e);
                    self.param(*case, p);
                }
            }
            StmtKind::Assign { lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                self.unify(l, r);
            }
            StmtKind::Insert { lhs, index, value } => {
                let c = self.expr(lhs);
                let i = self.expr(index);
                let v = self.expr(value);
                // a sequence's key is its index, which is unknown
                let (key, val) = (self.key(c), self.val(c));
                self.unify(i, key);
                self.unify(v, val);
            }
            // `Remove` is by index from a sequence, by key or element otherwise
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                let c = self.expr(lhs);
                let v = self.expr(value);
                let key = self.key(c);
                self.unify(v, key);
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.stmt(f, body);
            }
            StmtKind::Foreach { item, collection, body } => {
                let c = self.expr(collection);
                let item = self.var(*item);
                let val = self.val(c);
                self.unify(item, val);
                self.stmt(f, body);
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.expr(cond);
                self.stmt(f, then_branch);
                self.stmt(f, else_branch);
            }
            StmtKind::Ctor { interface, payload } => {
                let p = payload.as_ref().map(|p| self.expr(p));
                self.ctor(*interface, p);
            }
            StmtKind::FunCall { fun, args } => self.call(*fun, args),
            StmtKind::Raise { event, payload } | StmtKind::Announce { event, payload } => {
                self.expr(event);
                let p = payload.as_ref().map(|p| self.expr(p));
                self.payload(event, p);
            }
            StmtKind::Send { target, event, payload } => {
                self.expr(target);
                self.expr(event);
                let p = payload.as_ref().map(|p| self.expr(p));
                self.payload(event, p);
            }
            StmtKind::Goto { state, payload } => {
                let p = payload.as_ref().map(|p| self.expr(p));
                if let (Some(p), Some(entry)) = (p, self.prog.state(*state).entry) {
                    self.param(entry, p);
                }
            }
        }
    }

    /// The value of `e`, recording the flows and uses inside it.
    fn expr(&mut self, e: &Expr) -> Id {
        let outer = std::mem::replace(&mut self.at, e.span);
        let id = self.expr_at(e);
        self.at = outer;
        id
    }

    fn expr_at(&mut self, e: &Expr) -> Id {
        if let Some(c) = literal(e) {
            return self.constant(c);
        }
        match &e.kind {
            ExprKind::Var(v) => self.var(*v),
            ExprKind::EventRef(_)
            | ExprKind::BoolLit(_)
            | ExprKind::NullLit
            | ExprKind::Nondet
            | ExprKind::FairNondet
            | ExprKind::This
            | ExprKind::Choose(None) => self.clean(),
            // an `any` may compare these with an integer
            ExprKind::IntLit(_) | ExprKind::EnumElem(..) | ExprKind::FloatLit(_) | ExprKind::Error => self.unknown(),
            // What a string was made of shows only when it is compared: `finish` decides.
            ExprKind::Str { args, .. } => {
                let args: Vec<Id> = args.iter().map(|a| self.expr(a)).collect();
                let id = self.clean();
                if !self.quiet && !args.is_empty() {
                    self.strings.push((id, args, e.span));
                }
                id
            }
            ExprKind::Tuple(es) | ExprKind::NamedTuple(es) => {
                let fields = es.iter().map(|x| self.expr(x)).collect();
                self.fresh(Kind::Tuple(fields), None)
            }
            ExprKind::TupleAccess { base, index } | ExprKind::NamedTupleAccess { base, index } => {
                let b = self.expr(base);
                self.field(b, *index)
            }
            // by position
            ExprKind::SeqAccess { base, index } | ExprKind::SetAccess { base, index } => {
                let b = self.expr(base);
                let i = self.expr(index);
                self.taint(i);
                self.val(b)
            }
            ExprKind::MapAccess { base, key } => {
                let b = self.expr(base);
                let k = self.expr(key);
                let key = self.key(b);
                self.unify(k, key);
                self.val(b)
            }
            ExprKind::Keys(x) => {
                let b = self.expr(x);
                let val = self.key(b);
                let key = self.unknown();
                self.fresh(Kind::Coll { key, val }, None)
            }
            ExprKind::Values(x) => {
                let b = self.expr(x);
                let val = self.val(b);
                let key = self.unknown();
                self.fresh(Kind::Coll { key, val }, None)
            }
            // the size of a collection does not depend on which values it holds
            ExprKind::Sizeof(x) => {
                self.expr(x);
                self.unknown()
            }
            ExprKind::Default => self.shape(&e.ty, true),
            ExprKind::Ctor { interface, payload } => {
                let p = payload.as_ref().map(|p| self.expr(p));
                self.ctor(*interface, p);
                self.clean()
            }
            ExprKind::FunCall { fun, args } => {
                self.call(*fun, args);
                self.ret(*fun)
            }
            ExprKind::Unary { op, operand } => {
                let v = self.expr(operand);
                match op {
                    UnOp::Not => self.clean(),
                    UnOp::Neg => {
                        self.taint(v);
                        self.unknown()
                    }
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinOp::Eq | BinOp::Neq => {
                        self.unify(l, r);
                        // a literal names a constant; anything else may hold a chosen value
                        if literal(lhs).is_none() && literal(rhs).is_none() {
                            self.mutual(l);
                        }
                        self.clean()
                    }
                    BinOp::And | BinOp::Or | BinOp::Then | BinOp::Iff => self.clean(),
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        self.orderings.push((l, r, e.span));
                        self.clean()
                    }
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        self.taint(l);
                        self.taint(r);
                        self.unknown()
                    }
                }
            }
            ExprKind::Contains { item, collection } => {
                let i = self.expr(item);
                let c = self.expr(collection);
                let among = match collection.ty.canonicalize(self.prog) {
                    PType::Seq(_) => self.val(c),
                    _ => self.key(c),
                };
                self.unify(i, among);
                self.mutual(i);
                self.clean()
            }
            ExprKind::Cast(x) => {
                let v = self.expr(x);
                let to = self.shape(&e.ty, false);
                self.unify(v, to);
                to
            }
            ExprKind::Coerce(x) => {
                let v = self.expr(x);
                self.taint(v);
                let to = self.shape(&e.ty, false);
                self.taint(to);
                to
            }
            ExprKind::Choose(Some(x)) => match x.ty.canonicalize(self.prog) {
                PType::Seq(_) => {
                    let c = self.expr(x);
                    self.val(c)
                }
                PType::Set(_) | PType::Map(..) => {
                    let c = self.expr(x);
                    self.key(c)
                }
                _ => match x.kind {
                    ExprKind::IntLit(n) if n >= MIN_BOUND => {
                        let id = self.clean();
                        self.sites.push((&**x as *const Expr as usize, e.span, n, id));
                        id
                    }
                    _ => {
                        let bound = self.expr(x);
                        self.taint(bound);
                        self.unknown()
                    }
                },
            },
        }
    }

    fn finish(mut self) -> ValueClasses {
        let sites = std::mem::take(&mut self.sites);
        // the sites of a class must offer the same values
        let mut bounds: FxHashMap<Id, (i64, Span)> = FxHashMap::default();
        for (_, span, bound, id) in &sites {
            let root = self.find(*id);
            let first = *bounds.entry(root).or_insert((*bound, *span));
            if first.0 != *bound {
                self.taint_for(root, first.1);
            }
        }
        // A string that is compared, ordered, or goes where the analysis cannot follow
        // tells apart the values it was made of; one that is only passed around and
        // printed does not. A string may be made of strings, so repeat until none is new.
        let mut strings = std::mem::take(&mut self.strings);
        let compared: Vec<Id> = self.orderings.iter().flat_map(|(a, b, _)| [*a, *b]).collect();
        let compared: FxHashSet<Id> = compared.into_iter().map(|id| self.find(id)).collect();
        loop {
            let before = strings.len();
            let mut kept = Vec::new();
            for (id, args, span) in strings {
                let root = self.find(id);
                let node = &self.nodes[root as usize];
                if node.tainted.is_some() || node.mutual || compared.contains(&root) {
                    args.iter().for_each(|a| self.taint_for(*a, span));
                } else {
                    kept.push((id, args, span));
                }
            }
            strings = kept;
            if strings.len() == before {
                break;
            }
        }
        // An ordering against values that are not chosen compares with their constants,
        // if the analysis knows them all; one between chosen values tells them apart.
        // This taints only classes with sites, and reads the taint only of those
        // without, so the order of the orderings does not matter.
        let mut ordered: FxHashSet<Id> = FxHashSet::default();
        for (a, b, span) in std::mem::take(&mut self.orderings) {
            let (a, b) = (self.find(a), self.find(b));
            for (chosen, other) in [(a, b), (b, a)] {
                if !bounds.contains_key(&chosen) {
                    continue;
                }
                let against = &self.nodes[other as usize];
                if bounds.contains_key(&other) || against.tainted.is_some() {
                    let why = against.tainted.filter(|_| !bounds.contains_key(&other)).unwrap_or(span);
                    self.taint_for(chosen, why);
                } else {
                    let constants = against.constants.clone();
                    let into = &mut self.nodes[chosen as usize].constants;
                    into.extend(constants);
                    ordered.insert(chosen);
                }
            }
        }
        let mut out = ValueClasses::default();
        let mut ids: FxHashMap<Id, Option<u32>> = FxHashMap::default();
        for (expr, span, bound, id) in sites {
            let root = self.find(id);
            let node = &self.nodes[root as usize];
            let class = *ids.entry(root).or_insert_with(|| {
                if node.tainted.is_some() || !matches!(node.kind, Kind::Scalar) {
                    return None;
                }
                let mut constants: Vec<i64> =
                    node.constants.iter().copied().filter(|c| (0..bound).contains(c)).collect();
                constants.sort_unstable();
                constants.dedup();
                let class = ValueClass { bound, constants, ordered: ordered.contains(&root), mutual: node.mutual };
                // worth it only if the first choice already has fewer values to try
                ((class.offered(&[]).len() as i64) < bound).then(|| {
                    out.classes.push(class);
                    out.classes.len() as u32 - 1
                })
            });
            if let Some(c) = class {
                out.by_bound.insert(expr, c);
            }
            out.sites.push(Site { span, bound, class, blocked_by: node.tainted.filter(|_| class.is_none()) });
        }
        out
    }
}

/// Calls `visit` with every variable `e` mentions.
fn visit_vars(e: &Expr, visit: &mut dyn FnMut(VarId)) {
    match &e.kind {
        ExprKind::Var(v) => visit(*v),
        ExprKind::Tuple(es) | ExprKind::NamedTuple(es) | ExprKind::Str { args: es, .. } => {
            es.iter().for_each(|x| visit_vars(x, visit));
        }
        ExprKind::FunCall { args, .. } => args.iter().for_each(|x| visit_vars(x, visit)),
        ExprKind::TupleAccess { base: x, .. }
        | ExprKind::NamedTupleAccess { base: x, .. }
        | ExprKind::Keys(x)
        | ExprKind::Values(x)
        | ExprKind::Sizeof(x)
        | ExprKind::Cast(x)
        | ExprKind::Coerce(x)
        | ExprKind::Choose(Some(x))
        | ExprKind::Unary { operand: x, .. } => visit_vars(x, visit),
        ExprKind::SeqAccess { base: x, index: y }
        | ExprKind::SetAccess { base: x, index: y }
        | ExprKind::MapAccess { base: x, key: y }
        | ExprKind::Binary { lhs: x, rhs: y, .. }
        | ExprKind::Contains { item: x, collection: y } => {
            visit_vars(x, visit);
            visit_vars(y, visit);
        }
        ExprKind::Ctor { payload, .. } => {
            if let Some(p) = payload {
                visit_vars(p, visit);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn analyzed(body: &str) -> ValueClasses {
        let src = format!(
            "event ePut: (key: int, val: int);\n\
             event eGet: int;\n\
             machine Store {{\n\
               var kv: map[int, int];\n\
               var last: int;\n\
               var seen: set[int];\n\
               start state S {{\n\
                 on ePut do (p: (key: int, val: int)) {{ kv[p.key] = p.val; }}\n\
                 on eGet do (k: int) {{ {body} }}\n\
               }}\n\
             }}\n\
             machine Main {{\n\
               start state S {{\n\
                 entry {{\n\
                   var s: machine; var k: int;\n\
                   s = new Store();\n\
                   send s, ePut, (key = choose(10), val = choose(1024));\n\
                   k = choose(10);\n\
                   send s, eGet, k;\n\
                 }}\n\
               }}\n\
             }}\n"
        );
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("model.p", src);
        let prog = crate::sema::compile(&sm, &crate::sema::Options::default())
            .unwrap_or_else(|e| panic!("does not compile: {e:?}"))
            .program;
        analyze(&prog)
    }

    /// What the search tries first for the key of the put, its value and the key of the
    /// get, when the handler of the get runs `body`: `None` is every value.
    fn first(body: &str) -> [Option<Vec<i64>>; 3] {
        let names = analyzed(body);
        assert_eq!(names.sites.iter().map(|s| s.bound).collect::<Vec<_>>(), [10, 1024, 10]);
        let of = |i: usize| names.sites[i].class.map(|c| names.classes[c as usize].offered(&[]));
        [of(0), of(1), of(2)]
    }

    fn class(bound: i64, constants: &[i64], ordered: bool, mutual: bool) -> ValueClass {
        ValueClass { bound, constants: constants.to_vec(), ordered, mutual }
    }

    #[test]
    fn offers_one_value_of_those_the_program_cannot_tell_apart() {
        // nothing is known of the values: any one will do
        assert_eq!(class(10, &[], false, false).offered(&[3]), [0]);
        // compared with each other: those chosen, and one that is not
        assert_eq!(class(10, &[], false, true).offered(&[]), [0]);
        assert_eq!(class(10, &[], false, true).offered(&[0, 1]), [0, 1, 2]);
        assert_eq!(class(3, &[], false, true).offered(&[0, 1, 2]), [0, 1, 2]);
        // compared with constants for equality: the constants, and one other value
        assert_eq!(class(10, &[0, 7], false, false).offered(&[]), [0, 1, 7]);
        assert_eq!(class(10, &[0, 7], false, true).offered(&[1, 8]), [0, 1, 2, 7, 8]);
        // by an ordering: one value of each gap between them
        assert_eq!(class(100, &[5], true, false).offered(&[]), [0, 5, 6]);
        assert_eq!(class(100, &[0, 99], true, false).offered(&[]), [0, 1, 99]);
        assert_eq!(class(100, &[5], true, true).offered(&[0, 6]), [0, 1, 5, 6, 7]);
    }

    #[test]
    fn sites_whose_values_never_meet_have_classes_of_their_own() {
        let classes = |body| analyzed(body).sites.iter().map(|s| s.class).collect::<Vec<_>>();
        assert_eq!(classes(""), [Some(0), Some(1), Some(2)]);
        assert_eq!(classes("if (k in kv) { seen += (kv[k]); }"), [Some(0), Some(1), Some(0)]);
    }

    #[test]
    fn values_only_stored_and_compared_need_one_unused_value() {
        let one = Some(vec![0]);
        assert_eq!(first(""), [one.clone(), one.clone(), one.clone()]);
        for body in [
            "if (k in kv) { seen += (kv[k]); }",
            "var v: int; if (k in kv) { v = kv[k]; assert v == kv[k], format(\"{0}\", v); }",
            "var t: (int, seq[int]); t = (k, keys(kv)); assert t.0 in t.1;",
            "var x: int; foreach (x in keys(kv)) { seen += (x); } print format(\"{0}\", seen); assert k in kv;",
            "var x: int; x = k; if (k == x) { seen += (1); } assert k in kv;",
            // a string that is only passed around does not show what it was made of
            "var t: string; t = format(\"{0}\", k); print t; assert k in kv;",
        ] {
            let names = analyzed(body);
            assert_eq!(names.sites[0].class, names.sites[2].class, "{body}");
            assert!(names.classes[names.sites[0].class.unwrap() as usize].mutual, "{body}");
            assert_eq!(first(body)[0], one, "{body}");
        }
    }

    #[test]
    fn constants_the_values_meet_are_tried_too() {
        let one = Some(vec![0]);
        // a literal, of the keys only and of the values only
        assert_eq!(first("if (k == 7) { seen += (1); }"), [one.clone(), one.clone(), Some(vec![0, 7])]);
        assert_eq!(first("if (k in kv) { assert kv[k] != 7; }")[1], Some(vec![0, 7]));
        // one outside the domain is no value of it
        assert_eq!(first("if (k == -1) { seen += (1); }")[2], one);
        // the zero a field starts as, and a local read before it is assigned
        assert_eq!(first("if (k == last) { seen += (1); }")[2], Some(vec![0, 1]));
        assert_eq!(first("last = k;")[2], Some(vec![0, 1]));
        assert_eq!(first("var x: int; if ($) { x = k; } seen += (x);")[2], Some(vec![0, 1]));
        // an ordering against constants: a value of each gap
        assert_eq!(first("if (k < 3) { seen += (1); }")[2], Some(vec![0, 3, 4]));
        assert_eq!(first("last = 3; if (last >= k) { seen += (1); }")[2], Some(vec![0, 1, 3, 4]));
    }

    #[test]
    fn values_the_program_tells_apart_are_all_tried() {
        // arithmetic, a sequence index, a choose bound, a conversion, a string that is kept
        for body in [
            "seen += (k + 1);",
            "var q: seq[int]; q += (k, 1);",
            "seen += (choose(k));",
            "var f: float; f = k to float;",
            "var t: string; t = format(\"{0}\", k); assert t != \"7\";",
            "var t: set[string]; t += (format(\"{0}\", format(\"{0}\", k)));",
            // an ordering against a value that is not known, or against another choice
            "if (k < sizeof(kv)) { seen += (1); }",
            // a tuple in an `any` loses its shape
            "var a: any; a = (k, 1);",
        ] {
            let names = analyzed(body);
            assert_eq!(names.sites[2].class, None, "{body}");
            assert!(names.sites[2].blocked_by.is_some(), "{body}");
            assert!(names.sites[1].class.is_some(), "{body}");
        }
        // an ordering between two choices
        let names = analyzed("if (k < choose(10)) { seen += (1); }");
        assert_eq!(names.sites.iter().filter(|s| s.class.is_none()).count(), 2);
        // keys and values that meet have different bounds
        assert_eq!(first("if (k in kv) { assert kv[k] != k; }"), [None, None, None]);
        assert_eq!(first("if (k in kv) { kv[k] = kv[k] * 2; }")[1], None);
    }
}
