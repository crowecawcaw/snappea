//! Extensible, source-based checks for weak P models. No I/O or schedule execution.
//!
//! Rules inspect a shared parsed [`Context`] and emit findings with stable IDs and
//! source spans. [`Registry`] applies source suppressions and orders diagnostics.

mod rules;
mod suppression;
mod visit;

use snappea_syntax::{ast::*, parse, FileId, ParseError, SourceMap, Span};

pub use rules::{
    NoModelMachine, PermissiveMonitor, ScenarioColdStart, ScenarioNeverAccepts, TautologicalAssert, UnobservedHandler,
    UnselectedSpec, UnusedPayloadField,
};
pub use suppression::MUTATIONS;

/// An actionable diagnostic. Every unsuppressed finding fails CLI linting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub span: Span,
    pub message: String,
}

impl Finding {
    pub fn new(rule: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self { rule, span, message: message.into() }
    }
}

/// Parsed files share the source map's file IDs. All project files must be included.
pub struct Context<'a> {
    pub sources: &'a SourceMap,
    pub programs: Vec<Program>,
}

impl<'a> Context<'a> {
    pub fn parse(sources: &'a SourceMap) -> Result<Self, ParseError> {
        let programs = sources
            .files()
            .iter()
            .enumerate()
            .map(|(i, f)| parse(FileId(i as u32), &f.text))
            .collect::<Result<_, _>>()?;
        Ok(Self { sources, programs })
    }

    pub fn declarations(&self) -> impl Iterator<Item = &TopDecl> {
        self.programs.iter().flat_map(|p| &p.decls)
    }

    pub fn machines(&self) -> impl Iterator<Item = &MachineDecl> {
        self.declarations().filter_map(|d| match d {
            TopDecl::Machine(m) => Some(m),
            _ => None,
        })
    }

    /// Resolve type aliases, with a bound for malformed/cyclic input.
    pub fn resolve_type<'b>(&'b self, ty: &'b Type) -> &'b Type {
        let mut current = ty;
        for _ in 0..64 {
            let TypeKind::Named(name) = &current.kind else { break };
            let Some(next) = self.declarations().find_map(|d| match d {
                TopDecl::TypeDef { name: n, ty: Some(t), .. } if n.name == name.name => Some(t),
                _ => None,
            }) else {
                break;
            };
            current = next;
        }
        current
    }
}

/// Implement and register a rule without changing the runner or suppression system.
pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, context: &Context<'_>, findings: &mut Vec<Finding>);
}

#[derive(Default)]
pub struct Registry {
    rules: Vec<Box<dyn Rule>>,
}

impl Registry {
    pub fn builtins() -> Self {
        let mut r = Self::default();
        r.register(PermissiveMonitor);
        r.register(NoModelMachine);
        r.register(UnselectedSpec);
        r.register(TautologicalAssert);
        r.register(UnusedPayloadField);
        r.register(UnobservedHandler);
        r.register(ScenarioNeverAccepts);
        r.register(ScenarioColdStart);
        r
    }

    /// Rule IDs must be unique and may not use the reserved suppression diagnostic ID.
    pub fn register(&mut self, rule: impl Rule + 'static) {
        assert!(
            !["invalid-suppression", "invalid-directive"].contains(&rule.id())
                && !self.rules.iter().any(|r| r.id() == rule.id()),
            "duplicate or reserved lint rule ID"
        );
        self.rules.push(Box::new(rule));
    }

    pub fn rules(&self) -> impl Iterator<Item = &dyn Rule> {
        self.rules.iter().map(|r| r.as_ref())
    }

    pub fn check(&self, context: &Context<'_>) -> Vec<Finding> {
        let mut findings = Vec::new();
        for rule in &self.rules {
            rule.check(context, &mut findings);
        }
        suppression::apply(context, &self.rules, &mut findings);
        findings.sort_by(|a, b| (a.span, a.rule, &a.message).cmp(&(b.span, b.rule, &b.message)));
        findings.dedup();
        findings
    }
}

pub fn lint(sources: &SourceMap) -> Result<Vec<Finding>, ParseError> {
    Ok(Registry::builtins().check(&Context::parse(sources)?))
}

/// The rules checking logs against specs enforces (`snappea observe`, test libraries):
/// definite mistakes in a spec, not the model-test rules such as `unselected-spec`.
pub const OBSERVATION_RULES: &[&str] = &["unobserved-handler"];

/// [`lint`] restricted to [`OBSERVATION_RULES`], plus malformed suppressions and
/// directives, so a misspelled suppression cannot hide a finding.
pub fn lint_observation(sources: &SourceMap) -> Result<Vec<Finding>, ParseError> {
    let mut findings = lint(sources)?;
    findings.retain(|f| {
        OBSERVATION_RULES.contains(&f.rule) || matches!(f.rule, "invalid-suppression" | "invalid-directive")
    });
    Ok(findings)
}

/// The code `// snappea-lint: allow mutation[:<kind>]` comments exclude from mutation
/// testing, with the ID each allows (`mutation` or `mutation:<kind>`).
pub fn mutation_suppressions(sources: &SourceMap) -> Result<Vec<(Span, String)>, ParseError> {
    Ok(suppression::mutation_suppressions(&Context::parse(sources)?))
}

/// The machines marked `// snappea: driver` (test code that drives the model), as the
/// spans of their declarations.
pub fn driver_machines(sources: &SourceMap) -> Result<Vec<Span>, ParseError> {
    Ok(suppression::driver_machines(&Context::parse(sources)?))
}

/// The source scopes intentionally omitted from `snappea check` coverage requirements.
pub fn coverage_suppressions(sources: &SourceMap) -> Result<Vec<Span>, ParseError> {
    let context = Context::parse(sources)?;
    let registry = Registry::builtins();
    Ok(suppression::coverage_suppressions(&context, &registry.rules))
}
