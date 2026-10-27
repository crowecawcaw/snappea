use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;

pub struct UnobservedHandler;

impl Rule for UnobservedHandler {
    fn id(&self) -> &'static str {
        "unobserved-handler"
    }

    fn description(&self) -> &'static str {
        "Spec or scenario handles an event its observes list leaves out, so the handler never runs"
    }

    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in ctx.machines().filter(|m| m.kind != MachineKind::Impl) {
            let observed = |e: &Ident| m.observes.iter().any(|o| o.name == e.name);
            for state in &m.states {
                for item in &state.items {
                    let events = match &item.kind {
                        StateItemKind::OnDo { events, .. }
                        | StateItemKind::OnGoto { events, .. }
                        | StateItemKind::Ignore(events) => events,
                        _ => continue,
                    };
                    for e in events.iter().filter(|e| !observed(e)) {
                        out.push(Finding::new(
                            self.id(),
                            e.span,
                            format!(
                                "{} '{}' handles event '{}' in state '{}', but '{}' is not in its observes list, so this handler never runs",
                                kind(m),
                                m.name.name,
                                e.name,
                                state.name.name,
                                e.name
                            ),
                        ));
                    }
                }
            }
        }
    }
}

fn kind(m: &MachineDecl) -> &'static str {
    if m.kind == MachineKind::Scenario {
        "scenario"
    } else {
        "spec"
    }
}
