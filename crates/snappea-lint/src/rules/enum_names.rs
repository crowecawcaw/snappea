use crate::{Context, Finding, Rule};
use snappea_syntax::ast::*;
use std::collections::HashMap;

/// An enum value with the name of a machine, spec or scenario. P accepts it, but the
/// name then means two things: in an expression it is the enum value, in a type or
/// `new` the machine. A model that means one usually gets the other without an error.
pub struct EnumValueShadowsMachine;

impl Rule for EnumValueShadowsMachine {
    fn id(&self) -> &'static str {
        "enum-value-shadows-machine"
    }

    fn description(&self) -> &'static str {
        "An enum value has the name of a machine, so the name means the enum value in expressions and the machine in types"
    }

    fn check(&self, ctx: &Context<'_>, out: &mut Vec<Finding>) {
        let machines: HashMap<&str, &MachineDecl> = ctx.machines().map(|m| (m.name.name.as_str(), m)).collect();
        for d in ctx.declarations() {
            let TopDecl::Enum { name, elems, .. } = d else { continue };
            for e in elems {
                let Some(m) = machines.get(e.name.name.as_str()) else { continue };
                let kind = match m.kind {
                    MachineKind::Impl => "machine",
                    MachineKind::Spec => "spec",
                    MachineKind::Scenario => "scenario",
                };
                // the enum's name as a prefix keeps the value's meaning and is unlikely to
                // clash, e.g. `Role.Server` -> `RoleServer`
                let renamed = format!("{}{}", name.name, e.name.name);
                out.push(
                    Finding::new(
                        self.id(),
                        e.name.span,
                        format!(
                            "enum value '{}' of '{}' has the name of {} '{}'; in an expression the name means the enum value",
                            e.name.name, name.name, kind, e.name.name
                        ),
                    )
                    .with_help(format!(
                        "rename the enum value, e.g. to `{}`, and update its uses, so each name means one thing",
                        renamed
                    )),
                );
            }
        }
    }
}
