use crate::{visit, Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::{HashMap, HashSet};

pub struct TautologicalAssert;

impl Rule for TautologicalAssert {
    fn id(&self) -> &'static str {
        "tautological-assert"
    }
    fn description(&self) -> &'static str {
        "Assertion is guaranteed true by local scalar assignments or its own expression"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in ctx.machines() {
            for f in &m.funs {
                if let Some(b) = &f.body {
                    check_body(ctx, m, &f.params, b, out);
                }
            }
            for h in visit::handlers(m) {
                if let Handler::Anon(h) = h {
                    check_body(ctx, m, &h.param.iter().cloned().collect::<Vec<_>>(), &h.body, out);
                }
            }
        }
        for d in ctx.declarations() {
            if let TopDecl::Fun(f) = d {
                if let Some(b) = &f.body {
                    let mut paths = HashSet::new();
                    for p in &f.params {
                        scalar_paths(ctx, &p.name.name, &p.ty, &mut paths, 0);
                    }
                    for v in &b.vars {
                        for n in &v.names {
                            scalar_paths(ctx, &n.name, &v.ty, &mut paths, 0);
                        }
                    }
                    scan(&b.stmts, &paths, &mut Facts::default(), out);
                }
            }
        }
    }
}

fn check_body(ctx: &Context<'_>, m: &MachineDecl, params: &[Param], b: &FunBody, out: &mut Vec<Finding>) {
    let mut paths = HashSet::new();
    for v in &m.vars {
        for n in &v.names {
            scalar_paths(ctx, &n.name, &v.ty, &mut paths, 0);
        }
    }
    for p in params {
        paths.retain(|n| n != &p.name.name && !n.starts_with(&format!("{}.", p.name.name)));
        scalar_paths(ctx, &p.name.name, &p.ty, &mut paths, 0);
    }
    for v in &b.vars {
        for n in &v.names {
            paths.retain(|p| p != &n.name && !p.starts_with(&format!("{}.", n.name)));
            scalar_paths(ctx, &n.name, &v.ty, &mut paths, 0);
        }
    }
    scan(&b.stmts, &paths, &mut Facts::default(), out);
}

fn scalar_paths(ctx: &Context<'_>, prefix: &str, ty: &Type, paths: &mut HashSet<String>, depth: usize) {
    if depth > 32 {
        return;
    }
    match &ctx.resolve_type(ty).kind {
        TypeKind::Int | TypeKind::Bool | TypeKind::String => {
            paths.insert(prefix.to_string());
        }
        TypeKind::NamedTuple(fs) => {
            for (n, t) in fs {
                scalar_paths(ctx, &format!("{}.{}", prefix, n.name), t, paths, depth + 1);
            }
        }
        // In particular floats are excluded: NaN is not equal to itself.
        _ => {}
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Symbol(String, u64),
    Bool(bool),
    Int(String),
    Str(String),
}

#[derive(Default, Clone)]
struct Facts {
    values: HashMap<String, Value>,
    generation: u64,
    baseline: u64,
}
impl Facts {
    fn forget(&mut self) {
        self.values.clear();
        self.generation += 1;
        self.baseline = self.generation;
    }
    fn value(&self, e: &Expr, paths: &HashSet<String>) -> Option<Value> {
        if let Some(p) = visit::path(e) {
            return paths.contains(&p).then(|| self.values.get(&p).cloned().unwrap_or(Value::Symbol(p, self.baseline)));
        }
        match &e.kind {
            ExprKind::BoolLit(b) => Some(Value::Bool(*b)),
            ExprKind::IntLit(n) => Some(Value::Int(n.clone())),
            ExprKind::Str { raw, args, .. } if args.is_empty() => Some(Value::Str(raw.clone())),
            ExprKind::Unary { op: UnOp::Not, operand } => match self.value(operand, paths)? {
                Value::Bool(b) => Some(Value::Bool(!b)),
                _ => None,
            },
            ExprKind::Binary { op, lhs, rhs, .. } => {
                let a = self.value(lhs, paths)?;
                let b = self.value(rhs, paths)?;
                let answer = match op {
                    BinOp::Eq if a == b => true,
                    BinOp::Ne if a == b => false,
                    BinOp::And => match (a, b) {
                        (Value::Bool(a), Value::Bool(b)) => a && b,
                        _ => return None,
                    },
                    BinOp::Or => match (a, b) {
                        (Value::Bool(a), Value::Bool(b)) => a || b,
                        _ => return None,
                    },
                    _ => return None,
                };
                Some(Value::Bool(answer))
            }
            _ => None,
        }
    }
}

fn scan(stmts: &[Stmt], paths: &HashSet<String>, facts: &mut Facts, out: &mut Vec<Finding>) {
    for s in stmts {
        let mut effect = false;
        visit::stmts(std::slice::from_ref(s), &mut |_| {}, &mut |e| {
            if matches!(e.kind, ExprKind::FunCall { .. } | ExprKind::Ctor { .. }) {
                effect = true;
            }
        });
        if effect {
            facts.forget();
        }
        match &s.kind {
            StmtKind::Assert { cond, .. } => {
                if facts.value(cond, paths) == Some(Value::Bool(true)) {
                    out.push(Finding::new("tautological-assert", s.span,
                        "assertion is guaranteed true by its expression or preceding scalar assignments, so it can never fail"));
                }
            }
            StmtKind::Assign { lhs, rhs } => {
                let value = facts.value(rhs, paths);
                if let ExprKind::Name(n) = &lhs.kind {
                    if paths.contains(&n.name) {
                        // An unknown new value must not compare equal to an old symbol.
                        facts.generation += 1;
                        facts
                            .values
                            .insert(n.name.clone(), value.unwrap_or(Value::Symbol(n.name.clone(), facts.generation)));
                    } else {
                        facts.forget();
                    }
                } else {
                    facts.forget();
                }
            }
            StmtKind::Compound(ss) => scan(ss, paths, facts, out),
            StmtKind::If { then_branch, else_branch, .. } => {
                scan(std::slice::from_ref(then_branch), paths, &mut facts.clone(), out);
                if let Some(b) = else_branch {
                    scan(std::slice::from_ref(b), paths, &mut facts.clone(), out);
                }
                facts.forget();
            }
            StmtKind::While { body, .. } | StmtKind::Foreach { body, .. } => {
                facts.forget();
                scan(std::slice::from_ref(body), paths, &mut facts.clone(), out);
                facts.forget();
            }
            StmtKind::Receive { .. } => facts.forget(),
            StmtKind::Insert { lhs, .. } | StmtKind::Add { lhs, .. } | StmtKind::Remove { lhs, .. } => {
                if visit::path(lhs).is_none_or(|p| paths.iter().any(|s| s == &p || s.starts_with(&format!("{p}.")))) {
                    facts.forget();
                }
            }
            StmtKind::Return(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Goto { .. }
            | StmtKind::Raise { .. } => break,
            StmtKind::NoStmt
            | StmtKind::Print(_)
            | StmtKind::Announce { .. }
            | StmtKind::Send { .. }
            | StmtKind::Assume { .. } => {}
            _ => facts.forget(),
        }
        // Calls may modify machine state, including when nested in expressions.
        if effect {
            facts.forget();
        }
    }
}
