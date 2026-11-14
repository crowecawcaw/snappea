use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::{HashMap, HashSet};

pub struct UnselectedSpec;

impl Rule for UnselectedSpec {
    fn id(&self) -> &'static str {
        "unselected-spec"
    }
    fn description(&self) -> &'static str {
        "Spec is selected by no model test in the loaded project"
    }
    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        let modules: HashMap<_, _> = ctx
            .declarations()
            .filter_map(|d| match d {
                TopDecl::Module { name, expr, .. } => Some((name.name.as_str(), expr)),
                _ => None,
            })
            .collect();
        let mut selected = HashSet::new();
        for d in ctx.declarations() {
            match d {
                TopDecl::SafetyTest(t) => collect(&t.expr, &modules, &mut HashSet::new(), &mut selected),
                TopDecl::Implementation { expr, .. } => collect(expr, &modules, &mut HashSet::new(), &mut selected),
                TopDecl::RefinementTest { lhs, rhs, .. } => {
                    collect(lhs, &modules, &mut HashSet::new(), &mut selected);
                    collect(rhs, &modules, &mut HashSet::new(), &mut selected);
                }
                _ => {}
            }
        }
        for m in ctx.machines().filter(|m| m.kind == MachineKind::Spec) {
            if !selected.contains(m.name.name.as_str()) {
                out.push(Finding::new(self.id(), m.name.span, format!(
                    "spec '{}' is selected by no model test, so no model check runs it", m.name.name))
                    .with_help(format!("select it in a test whose model produces the events it observes, e.g. `test tcX [main=Main]: assert {} in {{ ... }};`", m.name.name)));
            }
        }
    }
}

fn collect<'a>(
    e: &'a ModExpr,
    modules: &HashMap<&str, &'a ModExpr>,
    visiting: &mut HashSet<String>,
    out: &mut HashSet<&'a str>,
) {
    match &e.kind {
        ModExprKind::Named(n) => {
            if visiting.insert(n.name.clone()) {
                if let Some(m) = modules.get(n.name.as_str()) {
                    collect(m, modules, visiting, out);
                }
            }
        }
        ModExprKind::Assert { monitors, module } => {
            out.extend(monitors.iter().map(|n| n.name.as_str()));
            collect(module, modules, visiting, out);
        }
        ModExprKind::Paren(m)
        | ModExprKind::HideEvents { module: m, .. }
        | ModExprKind::HideInterfaces { module: m, .. }
        | ModExprKind::Rename { module: m, .. }
        | ModExprKind::Main { module: m, .. } => collect(m, modules, visiting, out),
        ModExprKind::Compose(ms) | ModExprKind::Union(ms) => {
            for m in ms {
                collect(m, modules, visiting, out);
            }
        }
        ModExprKind::Bind(_) => {}
    }
}
