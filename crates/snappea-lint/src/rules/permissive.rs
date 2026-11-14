use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::HashSet;

pub struct PermissiveMonitor;

impl Rule for PermissiveMonitor {
    fn id(&self) -> &'static str {
        "permissive-monitor"
    }
    fn description(&self) -> &'static str {
        "Monitor accepts all sequences of its observed events without checking a property"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in ctx.machines().filter(|m| m.kind == MachineKind::Spec) {
            if permissive(m) {
                out.push(Finding::new(
                    self.id(),
                    m.name.span,
                    format!(
                        "spec '{}' accepts every sequence of its observed events, so it can never fail",
                        m.name.name
                    ),
                ));
            }
        }
    }
}

// Deliberately narrow proof: total event handling in every reachable state, no hot
// states, and only empty handlers or unconditional state changes. Missing handlers
// can reject traces, so absence of assertions alone is never enough.
fn permissive(m: &MachineDecl) -> bool {
    if m.observes.is_empty() {
        return false;
    }
    let Some(start) = m.states.iter().find(|s| s.is_start) else {
        return false;
    };
    let mut todo = vec![start.name.name.as_str()];
    let mut seen = HashSet::new();
    while let Some(name) = todo.pop() {
        if !seen.insert(name) {
            continue;
        }
        let Some(state) = m.states.iter().find(|s| s.name.name == name) else {
            return false;
        };
        if state.temperature == Temperature::Hot {
            return false;
        }
        let mut handled = HashSet::new();
        for item in &state.items {
            match &item.kind {
                StateItemKind::Entry(h) | StateItemKind::Exit(h) => {
                    if !passive(h, m, &mut todo) {
                        return false;
                    }
                }
                StateItemKind::OnDo { events, handler } => {
                    if !passive(handler, m, &mut todo) {
                        return false;
                    }
                    handled.extend(events.iter().map(|e| e.name.as_str()));
                }
                StateItemKind::OnGoto { events, target, with } => {
                    if with.as_ref().is_some_and(|h| !passive(h, m, &mut todo)) {
                        return false;
                    }
                    todo.push(&target.name);
                    handled.extend(events.iter().map(|e| e.name.as_str()));
                }
                StateItemKind::Ignore(events) => handled.extend(events.iter().map(|e| e.name.as_str())),
                StateItemKind::Defer(_) => return false,
            }
        }
        if !m.observes.iter().all(|e| handled.contains(e.name.as_str())) {
            return false;
        }
    }
    true
}

fn passive<'a>(h: &'a Handler, m: &'a MachineDecl, targets: &mut Vec<&'a str>) -> bool {
    match h {
        Handler::Anon(h) => passive_stmts(&h.body.stmts, targets),
        Handler::Named(n) => m
            .funs
            .iter()
            .find(|f| f.name.name == n.name)
            .and_then(|f| f.body.as_ref())
            .is_some_and(|b| passive_stmts(&b.stmts, targets)),
    }
}

fn passive_stmts<'a>(stmts: &'a [Stmt], targets: &mut Vec<&'a str>) -> bool {
    stmts.iter().all(|s| match &s.kind {
        StmtKind::NoStmt => true,
        StmtKind::Compound(ss) => passive_stmts(ss, targets),
        // Reject arguments: their evaluation could fail.
        StmtKind::Goto { state, args } if args.is_empty() => {
            targets.push(&state.name);
            true
        }
        _ => false,
    })
}
