use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::{HashMap, HashSet};

pub struct NoModelMachine;

impl Rule for NoModelMachine {
    fn id(&self) -> &'static str {
        "no-model-machine"
    }

    fn description(&self) -> &'static str {
        "Runnable test configuration contains no implementation machine besides its entry machine"
    }

    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        let machines: HashMap<_, _> = ctx.machines().map(|m| (m.name.name.as_str(), m)).collect();
        let modules: HashMap<_, _> = ctx
            .declarations()
            .filter_map(|d| match d {
                TopDecl::Module { name, expr, .. } => Some((name.name.as_str(), expr)),
                _ => None,
            })
            .collect();

        let has_declared_implementation = ctx.declarations().any(|d| matches!(d, TopDecl::Implementation { .. }));
        let has_test = ctx.declarations().any(|d| matches!(d, TopDecl::SafetyTest(_)));
        let has_runnable = has_declared_implementation || has_test;

        for decl in ctx.declarations() {
            match decl {
                TopDecl::SafetyTest(test) => {
                    check_configuration(
                        self.id(),
                        &test.expr,
                        Some((&test.main.name, test.main.span)),
                        &modules,
                        &machines,
                        out,
                    );
                }
                TopDecl::Implementation { main: Some(main), expr, .. } => {
                    check_configuration(self.id(), expr, Some((&main.name, main.span)), &modules, &machines, out);
                }
                _ => {}
            }
        }

        // The checker synthesizes DefaultImpl only when the project has no explicit
        // implementation or safety test and a machine named Main exists.
        if !has_runnable {
            if let Some(main) = machines.get("Main") {
                let selected: HashSet<_> =
                    machines.iter().filter_map(|(name, m)| (m.kind != MachineKind::Spec).then_some(*name)).collect();
                report_if_empty(self.id(), "Main", main.name.span, &selected, &machines, out);
            }
        }
    }
}

fn check_configuration<'a>(
    rule: &'static str,
    expr: &'a ModExpr,
    main: Option<(&str, snappea_syntax::Span)>,
    modules: &HashMap<&str, &'a ModExpr>,
    machines: &HashMap<&str, &MachineDecl>,
    out: &mut Vec<Finding>,
) {
    let Some((main_name, main_span)) = main else {
        return;
    };
    let mut selected = HashSet::new();
    collect_machines(expr, modules, &mut HashSet::new(), &mut selected);
    report_if_empty(rule, main_name, main_span, &selected, machines, out);
}

fn report_if_empty(
    rule: &'static str,
    main_name: &str,
    span: snappea_syntax::Span,
    selected: &HashSet<&str>,
    machines: &HashMap<&str, &MachineDecl>,
    out: &mut Vec<Finding>,
) {
    let has_model = selected
        .iter()
        .any(|name| *name != main_name && machines.get(name).is_some_and(|machine| machine.kind == MachineKind::Impl));
    if !has_model {
        out.push(Finding::new(
            rule,
            span,
            format!(
                "test configuration has no implementation machine besides entry machine '{}', so nothing separate from the driver is modeled",
                main_name
            ),
        ));
    }
}

fn collect_machines<'a>(
    expr: &'a ModExpr,
    modules: &HashMap<&str, &'a ModExpr>,
    visiting: &mut HashSet<String>,
    out: &mut HashSet<&'a str>,
) {
    match &expr.kind {
        ModExprKind::Named(name) => {
            if let Some(module) = modules.get(name.name.as_str()) {
                if visiting.insert(name.name.clone()) {
                    collect_machines(module, modules, visiting, out);
                }
            }
        }
        ModExprKind::Bind(bindings) => {
            out.extend(bindings.iter().map(|binding| binding.machine.name.as_str()));
        }
        ModExprKind::Assert { module, .. }
        | ModExprKind::Paren(module)
        | ModExprKind::HideEvents { module, .. }
        | ModExprKind::HideInterfaces { module, .. }
        | ModExprKind::Rename { module, .. }
        | ModExprKind::Main { module, .. } => {
            collect_machines(module, modules, visiting, out);
        }
        ModExprKind::Compose(parts) | ModExprKind::Union(parts) => {
            for part in parts {
                collect_machines(part, modules, visiting, out);
            }
        }
    }
}
