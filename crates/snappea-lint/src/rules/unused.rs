use crate::{visit, Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::HashSet;

pub struct UnusedPayloadField;

impl Rule for UnusedPayloadField {
    fn id(&self) -> &'static str {
        "unused-payload-field"
    }
    fn description(&self) -> &'static str {
        "A spec handler omits a named event payload field that no companion spec reads in at least one selected test"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        self.check_selected(ctx, None, out);
    }
}

struct PayloadUse<'a> {
    spec: &'a str,
    event: &'a str,
    observed: bool,
    param: &'a Param,
    fields: Vec<String>,
    used: HashSet<String>,
}

impl UnusedPayloadField {
    pub(crate) fn check_selected(
        &self,
        ctx: &Context<'_>,
        selections: Option<&[crate::SpecSelection]>,
        out: &mut Vec<Finding>,
    ) {
        let mut payloads = Vec::new();
        for machine in ctx.machines().filter(|m| m.kind == MachineKind::Spec) {
            for item in machine.states.iter().flat_map(|s| &s.items) {
                let (events, handler) = match &item.kind {
                    StateItemKind::OnDo { events, handler } => (events, Some(handler)),
                    StateItemKind::OnGoto { events, with, .. } => (events, with.as_ref()),
                    _ => continue,
                };
                let parameters: Vec<_> = match handler {
                    Some(Handler::Anon(h)) => h.param.iter().map(|p| (p, &h.body)).collect(),
                    Some(Handler::Named(n)) => machine
                        .funs
                        .iter()
                        .filter(|f| f.name.name == n.name)
                        .flat_map(|f| f.body.iter().flat_map(|b| f.params.iter().map(move |p| (p, b))))
                        .collect(),
                    _ => Vec::new(),
                };
                for (param, body) in parameters {
                    let TypeKind::NamedTuple(fields) = &ctx.resolve_type(&param.ty).kind else { continue };
                    if only_fails(&body.stmts) {
                        continue;
                    }
                    let used = reads(param, body, fields);
                    for event in events {
                        payloads.push(PayloadUse {
                            spec: &machine.name.name,
                            event: &event.name,
                            param,
                            observed: machine.observes.iter().any(|name| name.name == event.name),
                            // a reply address (`client: machine`) routes the event; it is
                            // not data a spec can check
                            fields: fields
                                .iter()
                                .filter(|(_, ty)| !is_address(ctx, ty))
                                .map(|(name, _)| name.name.clone())
                                .collect(),
                            used: used.clone(),
                        });
                    }
                }
            }
        }
        for payload in &payloads {
            let groups: Vec<_> =
                selections.into_iter().flatten().filter(|s| s.specs.iter().any(|name| name == payload.spec)).collect();
            // A spec outside this test selection cannot fail its payload gate or
            // provide evidence to discharge another spec's obligations.
            if selections.is_some() && groups.is_empty() {
                continue;
            }
            for field in payload.fields.iter().filter(|field| !payload.used.contains(*field)) {
                let mut missing = Vec::new();
                let mut companions = std::collections::BTreeSet::new();
                for group in &groups {
                    let readers: Vec<_> = payloads
                        .iter()
                        .filter(|other| {
                            other.observed
                                && other.spec != payload.spec
                                && other.event == payload.event
                                && other.used.contains(field)
                                && group.specs.iter().any(|name| name == other.spec)
                        })
                        .collect();
                    if readers.is_empty() {
                        missing.push(group.name.as_str());
                    }
                    companions.extend(readers.iter().map(|p| p.spec));
                }
                if selections.is_some() && missing.is_empty() {
                    out.push(Finding::new("payload-field-checked-by-other-spec", payload.param.name.span,
                        format!("monitor '{}': payload field '{}.{}' of event '{}' is not read here; selected companion spec(s) {} read it in every selected test using this monitor",
                            payload.spec, payload.param.name.name, field, payload.event,
                            companions.into_iter().collect::<Vec<_>>().join(", "))));
                } else {
                    let scope = if missing.is_empty() {
                        String::new()
                    } else {
                        format!("; no selected companion spec reads this event field in test(s) {}", missing.join(", "))
                    };
                    out.push(Finding::new(
                        "unused-payload-field",
                        payload.param.name.span,
                        format!(
                            "spec '{}' never reads payload field '{}.{}' of event '{}' outside diagnostic output{}",
                            payload.spec, payload.param.name.name, field, payload.event, scope
                        ),
                    ));
                }
            }
        }
    }
}

/// Whether values of `ty` are machine references (`machine`, an interface or machine
/// name, or a collection of them): where to send, not data.
fn is_address(ctx: &Context<'_>, ty: &Type) -> bool {
    match &ctx.resolve_type(ty).kind {
        TypeKind::Machine => true,
        TypeKind::Seq(t) | TypeKind::Set(t) => is_address(ctx, t),
        TypeKind::Named(n) => ctx.declarations().any(|d| match d {
            TopDecl::Interface { name, .. } => name.name == n.name,
            TopDecl::Machine(m) => m.name.name == n.name,
            _ => false,
        }),
        _ => false,
    }
}

fn reads(p: &Param, body: &FunBody, fields: &[(Ident, Type)]) -> HashSet<String> {
    let mut used = HashSet::new();
    let mut direct_bases = HashSet::new();
    let mut writes = HashSet::new();
    let mut messages = HashSet::new();
    visit::stmts(
        &body.stmts,
        &mut |s| {
            if let StmtKind::Assign { lhs, .. } = &s.kind {
                written_place(lhs, &mut writes);
            }
            if let StmtKind::Assert { message: Some(message), .. } | StmtKind::Print(message) = &s.kind {
                visit::expr(message, &mut |e| {
                    messages.insert(e.span);
                });
            }
        },
        &mut |_| {},
    );
    // The walker also visits access bases. Remember those spans so p.field does
    // not count as passing/reading the whole tuple. Any whole-tuple use is treated
    // conservatively as using all fields (aliases, helper calls, returns, etc.).
    visit::stmts(&body.stmts, &mut |_| {}, &mut |e| {
        if messages.contains(&e.span) {
            return;
        }
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
            && !messages.contains(&e.span)
        {
            whole = true;
        }
    });
    if whole {
        fields.iter().map(|(name, _)| name.name.clone()).collect()
    } else {
        used
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

/// Whether a handler rejects the event unconditionally, with optional diagnostics.
/// Keep this narrow: a conditional rejection still needs to check accepted payloads.
fn only_fails(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match &s.kind {
            StmtKind::Print(e) => {
                // A helper can leave the handler with goto/raise; do not assume it
                // returns to the rejection. Literal/format diagnostics are safe.
                let mut calls = false;
                visit::expr(e, &mut |e| calls |= matches!(e.kind, ExprKind::FunCall { .. } | ExprKind::Ctor { .. }));
                if calls {
                    return false;
                }
            }
            StmtKind::NoStmt => {}
            StmtKind::Assert { cond, .. } => return matches!(cond.kind, ExprKind::BoolLit(false)),
            StmtKind::Compound(inner) if only_fails(inner) => return true,
            _ => return false,
        }
    }
    false
}
