//! Small shared AST walkers. Rules decide what expressions mean.
use snappea_syntax::ast::*;

pub(crate) fn expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::UnnamedTuple(es)
        | ExprKind::Ctor { args: es, .. }
        | ExprKind::FunCall { args: es, .. }
        | ExprKind::Str { args: es, .. } => {
            for e in es {
                expr(e, f);
            }
        }
        ExprKind::NamedTuple(es) => {
            for (_, e) in es {
                expr(e, f);
            }
        }
        ExprKind::NamedTupleAccess { base, .. }
        | ExprKind::TupleAccess { base, .. }
        | ExprKind::Test { instance: base, .. }
        | ExprKind::Flying(base)
        | ExprKind::Sent(base)
        | ExprKind::Keys(base)
        | ExprKind::Values(base)
        | ExprKind::Sizeof(base)
        | ExprKind::Unary { operand: base, .. }
        | ExprKind::Cast { expr: base, .. }
        | ExprKind::Quant { body: base, .. }
        | ExprKind::Choose(Some(base)) => expr(base, f),
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Index { base: lhs, index: rhs }
        | ExprKind::Targets { instance: lhs, target: rhs } => {
            expr(lhs, f);
            expr(rhs, f);
        }
        _ => {}
    }
}

pub(crate) fn stmts(stmts: &[Stmt], statement: &mut impl FnMut(&Stmt), expression: &mut impl FnMut(&Expr)) {
    for s in stmts {
        statement(s);
        let mut ex = |e: &Expr| expr(e, expression);
        match &s.kind {
            StmtKind::Compound(ss) => self::stmts(ss, statement, expression),
            StmtKind::Assert { cond, message } | StmtKind::Assume { cond, message } => {
                ex(cond);
                if let Some(m) = message {
                    ex(m);
                }
            }
            StmtKind::Print(e) | StmtKind::Return(Some(e)) => ex(e),
            StmtKind::Assign { lhs, rhs } => {
                ex(lhs);
                ex(rhs);
            }
            StmtKind::Insert { lhs, index, value } => {
                ex(lhs);
                ex(index);
                ex(value);
            }
            StmtKind::Add { lhs, value } | StmtKind::Remove { lhs, value } => {
                ex(lhs);
                ex(value);
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                ex(cond);
                self::stmts(std::slice::from_ref(then_branch), statement, expression);
                if let Some(b) = else_branch {
                    self::stmts(std::slice::from_ref(b), statement, expression);
                }
            }
            StmtKind::While { cond, body } => {
                ex(cond);
                self::stmts(std::slice::from_ref(body), statement, expression);
            }
            StmtKind::Foreach { collection, invariants, body, .. } => {
                ex(collection);
                for e in invariants {
                    ex(e);
                }
                self::stmts(std::slice::from_ref(body), statement, expression);
            }
            StmtKind::Ctor { args, .. } | StmtKind::FunCall { args, .. } | StmtKind::Goto { args, .. } => {
                for e in args {
                    ex(e);
                }
            }
            StmtKind::Raise { event, args } | StmtKind::Announce { event, args } => {
                ex(event);
                for e in args {
                    ex(e);
                }
            }
            StmtKind::Send { target, event, args } => {
                ex(target);
                ex(event);
                for e in args {
                    ex(e);
                }
            }
            StmtKind::Receive { cases } => {
                for c in cases {
                    self::stmts(&c.handler.body.stmts, statement, expression);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn handlers(m: &MachineDecl) -> impl Iterator<Item = &Handler> {
    m.states.iter().flat_map(|s| &s.items).filter_map(|i| match &i.kind {
        StateItemKind::Entry(h) | StateItemKind::Exit(h) | StateItemKind::OnDo { handler: h, .. } => Some(h),
        StateItemKind::OnGoto { with, .. } => with.as_ref(),
        _ => None,
    })
}

pub(crate) fn bodies(m: &MachineDecl) -> impl Iterator<Item = &FunBody> {
    m.funs.iter().filter_map(|f| f.body.as_ref()).chain(handlers(m).filter_map(|h| match h {
        Handler::Anon(h) => Some(&h.body),
        _ => None,
    }))
}

pub(crate) fn path(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Name(n) => Some(n.name.clone()),
        ExprKind::NamedTupleAccess { base, field } => Some(format!("{}.{}", path(base)?, field.name)),
        _ => None,
    }
}
