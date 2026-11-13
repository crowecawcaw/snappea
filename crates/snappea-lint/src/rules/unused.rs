use crate::{visit, Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::HashSet;

pub struct UnusedPayloadField;

impl Rule for UnusedPayloadField {
    fn id(&self) -> &'static str {
        "unused-payload-field"
    }
    fn description(&self) -> &'static str {
        "A monitor handler never reads a named event payload field"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in ctx.machines().filter(|m| m.kind == MachineKind::Spec) {
            // Named handlers are functions with declared parameters. Check each body
            // once, and only when it is actually used as an event handler.
            let mut named = HashSet::new();
            for item in m.states.iter().flat_map(|s| &s.items) {
                let handler = match &item.kind {
                    StateItemKind::OnDo { handler, .. } => Some(handler),
                    StateItemKind::OnGoto { with, .. } => with.as_ref(),
                    _ => None,
                };
                match handler {
                    Some(Handler::Anon(h)) => {
                        if let Some(p) = &h.param {
                            check(ctx, p, &h.body, out);
                        }
                    }
                    Some(Handler::Named(n)) if named.insert(&n.name) => {
                        if let Some(f) = m.funs.iter().find(|f| f.name.name == n.name) {
                            if let Some(b) = &f.body {
                                for p in &f.params {
                                    check(ctx, p, b, out);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn check(ctx: &Context<'_>, p: &Param, body: &FunBody, out: &mut Vec<Finding>) {
    let TypeKind::NamedTuple(fields) = &ctx.resolve_type(&p.ty).kind else {
        return;
    };
    // A handler that only fails (`assert false`) reports that the event must not happen;
    // it has no reason to read every field.
    if only_fails(&body.stmts) {
        return;
    }
    let mut used = HashSet::new();
    let mut direct_bases = HashSet::new();
    let mut writes = HashSet::new();
    visit::stmts(
        &body.stmts,
        &mut |s| {
            if let StmtKind::Assign { lhs, .. } = &s.kind {
                written_place(lhs, &mut writes);
            }
        },
        &mut |_| {},
    );
    // The walker also visits access bases. Remember those spans so p.field does
    // not count as passing/reading the whole tuple. Any whole-tuple use is treated
    // conservatively as using all fields (aliases, helper calls, returns, etc.).
    visit::stmts(&body.stmts, &mut |_| {}, &mut |e| {
        if let ExprKind::NamedTupleAccess { base, field } = &e.kind {
            if matches!(&base.kind, ExprKind::Name(n) if n.name == p.name.name) {
                if !writes.contains(&e.span) {
                    used.insert(field.name.clone());
                }
                direct_bases.insert(base.span);
            }
        }
    });
    let mut whole = false;
    visit::stmts(&body.stmts, &mut |_| {}, &mut |e| {
        if matches!(&e.kind, ExprKind::Name(n) if n.name == p.name.name)
            && !direct_bases.contains(&e.span)
            && !writes.contains(&e.span)
        {
            whole = true;
        }
    });
    if whole {
        return;
    }
    for (field, _) in fields {
        if !used.contains(field.name.as_str()) {
            out.push(Finding::new(
                "unused-payload-field",
                p.name.span,
                format!(
                    "monitor payload field '{}.{}' is never read; check it or suppress this intentional abstraction",
                    p.name.name, field.name
                ),
            ));
        }
    }
}

/// Marks the place an assignment writes: the target and the bases it is reached through.
/// Map and sequence indexes (`m[p.id] = ...`) are reads, so they are not marked.
fn written_place(e: &Expr, writes: &mut HashSet<snappea_syntax::Span>) {
    writes.insert(e.span);
    match &e.kind {
        ExprKind::NamedTupleAccess { base, .. } | ExprKind::TupleAccess { base, .. } | ExprKind::Index { base, .. } => {
            written_place(base, writes)
        }
        _ => {}
    }
}

/// Whether a body is only `assert false` (with an optional message), possibly in braces.
fn only_fails(stmts: &[Stmt]) -> bool {
    match stmts {
        [s] => match &s.kind {
            StmtKind::Assert { cond, .. } => matches!(cond.kind, ExprKind::BoolLit(false)),
            StmtKind::Compound(inner) => only_fails(inner),
            _ => false,
        },
        _ => false,
    }
}
