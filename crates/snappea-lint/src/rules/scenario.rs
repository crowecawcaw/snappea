use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;

/// The scenario machines of the sources.
fn scenarios<'a>(ctx: &'a Context<'_>) -> impl Iterator<Item = &'a MachineDecl> {
    ctx.machines().filter(|m| m.kind == MachineKind::Scenario)
}

pub struct ScenarioNeverAccepts;

impl Rule for ScenarioNeverAccepts {
    fn id(&self) -> &'static str {
        "scenario-never-accepts"
    }

    fn description(&self) -> &'static str {
        "Scenario has no accepting (cold) state, so it can never be covered"
    }

    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in scenarios(ctx) {
            if !m.states.iter().any(|s| s.temperature == Temperature::Cold) {
                out.push(Finding::new(
                    self.id(),
                    m.name.span,
                    format!(
                        "scenario '{}' has no accepting (cold) state, so it can never be marked satisfied and always reports 0 coverage",
                        m.name.name
                    ),
                ));
            }
        }
    }
}

pub struct ScenarioColdStart;

impl Rule for ScenarioColdStart {
    fn id(&self) -> &'static str {
        "scenario-cold-start"
    }

    fn description(&self) -> &'static str {
        "Scenario starts in an accepting (cold) state, so it accepts before anything happens"
    }

    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        for m in scenarios(ctx) {
            if let Some(s) = m.states.iter().find(|s| s.is_start && s.temperature == Temperature::Cold) {
                out.push(Finding::new(
                    self.id(),
                    s.name.span,
                    format!(
                        "scenario '{}' starts in accepting (cold) state '{}', so it accepts before any event; it counts as covered only after events re-enter an accepting state",
                        m.name.name, s.name.name
                    ),
                ));
            }
        }
    }
}
