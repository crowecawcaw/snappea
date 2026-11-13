use crate::{visit, Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::HashSet;

pub struct AnnounceAndSend;

impl Rule for AnnounceAndSend {
    fn id(&self) -> &'static str {
        "announce-and-send"
    }
    fn description(&self) -> &'static str {
        "Adjacent announce and send expose the same event and payload to a spec twice"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        let observed: HashSet<&str> = ctx
            .machines()
            .filter(|m| m.kind == MachineKind::Spec)
            .flat_map(|m| m.observes.iter().map(|e| e.name.as_str()))
            .collect();
        let bodies = ctx.machines().flat_map(visit::bodies).chain(ctx.declarations().filter_map(|d| match d {
            TopDecl::Fun(f) => f.body.as_ref(),
            _ => None,
        }));
        for body in bodies {
            adjacent(&body.stmts, &observed, out);
            visit::stmts(
                &body.stmts,
                &mut |s| match &s.kind {
                    StmtKind::Compound(stmts) => adjacent(stmts, &observed, out),
                    StmtKind::Receive { cases } => {
                        for c in cases {
                            adjacent(&c.handler.body.stmts, &observed, out);
                        }
                    }
                    _ => {}
                },
                &mut |_| {},
            );
        }
    }
}

fn adjacent(stmts: &[Stmt], observed: &HashSet<&str>, out: &mut Vec<Finding>) {
    for pair in stmts.windows(2) {
        let (announced, sent, target, a, b) = match (&pair[0].kind, &pair[1].kind) {
            (StmtKind::Announce { event: a, args: aa }, StmtKind::Send { target, event: b, args: ba })
            | (StmtKind::Send { target, event: b, args: ba }, StmtKind::Announce { event: a, args: aa }) => {
                (a, b, target, aa, ba)
            }
            _ => continue,
        };
        let ExprKind::Name(event) = &announced.kind else { continue };
        // Do not guess across statements, branches, calls or nondeterministic values.
        // Even the send target can call a helper that changes the payload or leaves.
        if observed.contains(event.name.as_str())
            && same_stable(announced, sent)
            && same_stable(target, target)
            && same_args(a, b)
        {
            out.push(Finding::new("announce-and-send", pair[1].span, format!(
                "event '{}' is both announced and sent with the same payload in adjacent statements; specs observe sends too, so remove the redundant announce or suppress an intentional duplicate",
                event.name
            )));
        }
    }
}

fn same_args(a: &[Expr], b: &[Expr]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_stable(a, b))
}

/// Equal expression structure without source spans. Only repeatable reads are
/// accepted: helpers, constructors, choices and mutable runtime queries are excluded.
fn same_stable(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Name(a), ExprKind::Name(b)) => a.name == b.name,
        (ExprKind::IntLit(a), ExprKind::IntLit(b)) => a == b,
        (ExprKind::BoolLit(a), ExprKind::BoolLit(b)) => a == b,
        (ExprKind::NullLit, ExprKind::NullLit) | (ExprKind::This, ExprKind::This) => true,
        (ExprKind::Str { raw: a, args: aa, .. }, ExprKind::Str { raw: b, args: ba, .. }) => a == b && same_args(aa, ba),
        (ExprKind::UnnamedTuple(a), ExprKind::UnnamedTuple(b)) => same_args(a, b),
        (ExprKind::NamedTuple(a), ExprKind::NamedTuple(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|((an, a), (bn, b))| an.name == bn.name && same_stable(a, b))
        }
        (ExprKind::NamedTupleAccess { base: a, field: af }, ExprKind::NamedTupleAccess { base: b, field: bf }) => {
            af.name == bf.name && same_stable(a, b)
        }
        (ExprKind::TupleAccess { base: a, index: ai, .. }, ExprKind::TupleAccess { base: b, index: bi, .. }) => {
            ai == bi && same_stable(a, b)
        }
        (ExprKind::Index { base: a, index: ai }, ExprKind::Index { base: b, index: bi }) => {
            same_stable(a, b) && same_stable(ai, bi)
        }
        (ExprKind::Unary { op: ao, operand: a }, ExprKind::Unary { op: bo, operand: b }) => {
            ao == bo && same_stable(a, b)
        }
        (ExprKind::Binary { op: ao, lhs: a, rhs: ar, .. }, ExprKind::Binary { op: bo, lhs: b, rhs: br, .. }) => {
            ao == bo && same_stable(a, b) && same_stable(ar, br)
        }
        _ => false,
    }
}
