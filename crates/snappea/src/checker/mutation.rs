//! Mutants of a P model: small changes to its code, each of which a good test suite
//! should notice. `snappea check` runs the test cases against each mutant; a mutant that
//! every test case still passes marks code whose effect no spec or assertion checks.
//!
//! A mutant changes one place in the code of the machines under test (never a spec
//! monitor or a test case's main machine, which are the tests themselves):
//!
//! - removes a statement with an effect: an assignment, collection update, `send`,
//!   `raise`, `announce`, `goto`, `new` or function call;
//! - forces the condition of an `if` to `true` or to `false`;
//! - swaps an operator: `<` and `<=`, `>` and `>=`, `==` and `!=`, `+` and `-` (numbers),
//!   `&&` and `||`;
//! - removes a `!`;
//! - flips a `true` or `false` literal.
//!
//! Conditions on a nondeterministic choice (`$`, `$$`, `choose`) are not forced, swapped
//! or negated: that only changes which choices take which branch, and the checker
//! explores every choice anyway.
//!
//! Mutants are found by walking function bodies in a fixed order; [`apply`] walks the
//! same way to find the place again, so a [`Mutant`] needs no pointer into the IR.
//! Assertions, `print` and format strings are never changed.

use rustc_hash::FxHashMap;

use crate::checker::bytecode::{Probe, ProbeKind};
use crate::checker::code_coverage::{fun_context, shorten, snippet};
use crate::sema::{
    BinOp, Block, Expr, ExprKind, FunId, MachineId, MachineKind, PType, Program, Stmt, StmtKind, TestCase, UnOp,
};
use crate::syntax::{SourceMap, Span};

/// What a mutant changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The statement is removed.
    Remove,
    /// The `if` condition is replaced by a constant.
    Force(bool),
    /// The binary operator is replaced.
    Operator(BinOp),
    /// `!e` becomes `e`.
    DropNot,
    /// A boolean literal is negated.
    FlipBool,
}

impl Change {
    /// The mutation's ID, as JSON results and `mutation:<id>` suppressions name it (see
    /// [`snappea_lint::MUTATIONS`]).
    pub fn name(self) -> &'static str {
        match self {
            Change::Remove => "remove-statement",
            Change::Force(true) => "condition-true",
            Change::Force(false) => "condition-false",
            Change::Operator(_) => "swap-operator",
            Change::DropNot => "remove-not",
            Change::FlipBool => "flip-boolean",
        }
    }
}

/// One mutant.
#[derive(Debug, Clone)]
pub struct Mutant {
    /// The function whose body is changed.
    pub fun: FunId,
    /// The place in the body, in [`walk`] order.
    site: u32,
    pub change: Change,
    /// The code coverage probe (see [`Probe`]) that runs whenever the changed code does:
    /// only a test case that ran it can notice the change.
    pub probe: u32,
    /// The changed code.
    pub span: Span,
    pub file: String,
    pub line: u32,
    /// Where the code is, e.g. `Server.Serving, on eRequest`.
    pub context: String,
    /// The change, e.g. ``removed `send client, eResp;` ``.
    pub description: String,
}

/// The functions a mutant may change: those of the machines the test cases check (not
/// spec monitors, a test case's main machine, nor the other machines in `drivers`, see
/// [`crate::checker::driver_facts::driver_machines`]), and the functions they call.
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
    funs
}

/// The mutants of the functions in `funs` (see [`mutable_funs`]), with `probes`, the code
/// coverage probes of the program's code
/// ([`compile_program_with_probes`](crate::checker::compile::compile_program_with_probes)).
pub fn mutants(prog: &Program, funs: &[bool], probes: &[Probe]) -> Vec<Mutant> {
    let probe_of: FxHashMap<(ProbeKind, FunId, Span), u32> =
        probes.iter().enumerate().rev().map(|(id, p)| ((p.kind, p.fun, p.span), id as u32)).collect();
    let sm: &SourceMap = &prog.sources;
    let mut out = Vec::new();
    for (i, f) in prog.funs.iter().enumerate() {
        let fun = FunId(i as u32);
        let Some(body) = f.body.clone().filter(|_| funs.get(i).copied().unwrap_or(false)) else { continue };
        let mut body = body;
        let context = fun_context(prog, fun);
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
            let (change, at, description) = describe(sm, &target);
            if at.is_dummy() {
                return;
            }
            let loc = sm.location(at);
            out.push(Mutant {
                fun,
                site: this,
                change,
                probe,
                span: at,
                file: loc.file.unwrap_or_default(),
                line: loc.line.max(0) as u32,
                context: context.clone(),
                description,
            });
        });
    }
    out
}

/// `prog` with mutant `m` applied.
pub fn apply(prog: &Program, m: &Mutant) -> Program {
    let mut out = prog.clone();
    let mut body = out.funs[m.fun.idx()].body.take().expect("a mutant changes a function with a body");
    let mut site = 0u32;
    walk(prog, &mut body, &mut |_, target| {
        if site == m.site {
            change(target);
        }
        site += 1;
    });
    out.funs[m.fun.idx()].body = Some(body);
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
    );
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

/// The operator a mutant puts in place of `op` in `e`, if any.
fn swapped(prog: &Program, op: BinOp, e: &Expr) -> Option<BinOp> {
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
fn visit_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Tuple(es) | ExprKind::NamedTuple(es) | ExprKind::FunCall { args: es, .. } => {
            es.iter().for_each(|e| visit_expr(e, f))
        }
        ExprKind::Str { args, .. } => args.iter().for_each(|e| visit_expr(e, f)),
        ExprKind::TupleAccess { base: x, .. }
        | ExprKind::NamedTupleAccess { base: x, .. }
        | ExprKind::Keys(x)
        | ExprKind::Values(x)
        | ExprKind::Sizeof(x)
        | ExprKind::Cast(x)
        | ExprKind::Coerce(x)
        | ExprKind::Unary { operand: x, .. } => visit_expr(x, f),
        ExprKind::SeqAccess { base: a, index: b }
        | ExprKind::SetAccess { base: a, index: b }
        | ExprKind::MapAccess { base: a, key: b }
        | ExprKind::Binary { lhs: a, rhs: b, .. }
        | ExprKind::Contains { item: a, collection: b } => {
            visit_expr(a, f);
            visit_expr(b, f);
        }
        ExprKind::Ctor { payload, .. } | ExprKind::Choose(payload) => payload.iter().for_each(|x| visit_expr(x, f)),
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
            if let Some(new) = swapped(prog, *op, e) {
                visit(stmt, Target::Expr(e, Change::Operator(new)));
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
                (7, "condition-true", "`n < 3 && !(n == 0)` replaced by `true`"),
                (7, "condition-false", "`n < 3 && !(n == 0)` replaced by `false`"),
                (7, "swap-operator", "`n < 3 && !(n == 0)` → `n < 3 || !(n == 0)`"),
                (7, "swap-operator", "`n < 3` → `n <= 3`"),
                (7, "remove-not", "`!(n == 0)` → `n == 0`"),
                (7, "swap-operator", "`n == 0` → `n != 0`"),
                (7, "remove-statement", "removed `total = total + n;`"),
                (7, "swap-operator", "`total + n` → `total - n`"),
                (8, "remove-statement", "removed `announce eSeen, total;`"),
            ]
        );
        assert!(ms.iter().all(|m| m.context == "Server.S, on eReq"));
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
            ["removed `total = total + n;`", "`total + n` → `total - n`", "removed `announce eSeen, total;`"]
        );
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
        ]
        .map(Change::name);
        assert_eq!(names.as_slice(), snappea_lint::MUTATIONS);
    }

    #[test]
    fn applies_one_mutant() {
        let prog = program();
        let funs = mutable_funs(&prog, &prog.test_cases(), &[]);
        let probes = crate::checker::compile::compile_program_with_probes(&prog).probes;
        let ms = mutants(&prog, &funs, &probes);
        let body = |p: &Program, m: &Mutant| format!("{:?}", p.fun(m.fun).body);
        for m in &ms {
            let mutated = apply(&prog, m);
            assert_ne!(body(&prog, m), body(&mutated, m), "{}", m.description);
            // nothing else changes
            for (i, f) in prog.funs.iter().enumerate().filter(|(i, _)| *i != m.fun.idx()) {
                assert_eq!(format!("{:?}", f.body), format!("{:?}", mutated.funs[i].body));
            }
        }
        let removed = ms.iter().find(|m| m.change == Change::Remove && m.line == 8).unwrap();
        let mutated = apply(&prog, removed);
        let stmts = &mutated.fun(removed.fun).body.as_ref().unwrap().stmts;
        assert!(matches!(stmts.last().unwrap().kind, StmtKind::Nop));
    }
}
