//! Extensible, source-based checks for weak P models. No I/O or schedule execution.
//!
//! Rules inspect a shared parsed [`Context`] and emit findings with stable IDs and
//! source spans. [`Registry`] applies source suppressions and orders diagnostics.

mod rules;
mod suppression;
mod visit;

use snappea_syntax::{ast::*, parse, FileId, ParseError, SourceMap, Span};

pub use rules::{
    AnnounceAndSend, EnumValueShadowsMachine, NoModelMachine, PermissiveMonitor, ScenarioColdStart,
    ScenarioNeverAccepts, TautologicalAssert, UnobservedHandler, UnselectedSpec, UnusedPayloadField,
};
pub use suppression::{MutationSuppression, TestIntent, MUTATIONS};

/// Intent declarations on test cases; malformed declarations are lint findings.
pub fn test_intents(sources: &SourceMap) -> Result<Vec<TestIntent>, ParseError> {
    Ok(suppression::test_intents(&Context::parse(sources)?))
}

/// Rules that need the type-checked program, which `snappea` runs on it (see
/// `snappea::checker::checked`): their IDs and descriptions, so suppression comments and
/// `--list-rules` know them.
pub const PROGRAM_RULES: &[(&str, &str)] = &[
    (
        "payload-field-checked-by-other-spec",
        "Advisory: this spec omits a payload field that companion specs read in every selected test using it.",
    ),
    (
        "unused-design-payload-field",
        "Advisory: a design event handler never reads a named payload field outside diagnostic output.",
    ),
    (
        "shared-spec-helper",
        "Advisory: a selected spec and design machine call the same global helper, so a shared mistake may escape the oracle.",
    ),
    (
        "unchecked-state",
        "A model or spec field is written, but no assertion, condition, spec or sent event depends on it.",
    ),
    (
        "unchecked-event",
        "The model sends an event that no spec observes and whose handlers change nothing a check depends on.",
    ),
    (
        "no-progress-spec",
        "A test waits for the model's responses, but none of its specs has a hot state, so a model that stops passes.",
    ),
    (
        "unplanned-driver-stage",
        "A driver starts more work only after a model notification, and announces no plan a spec observes.",
    ),
    (
        "unreachable-statement",
        "A statement follows a goto, raise, or call that always leaves the state, so it never runs.",
    ),
];

/// The findings about `snappea-lint:` and `snappea:` comments themselves, with their
/// descriptions. They cannot be suppressed: fix or remove the comment.
pub const COMMENT_RULES: &[(&str, &str)] = &[
    (
        "invalid-suppression",
        "A `// snappea-lint: allow` comment is malformed, names an unknown rule, or covers no code.",
    ),
    ("invalid-directive", "A `// snappea:` directive is unknown, malformed, or not before the declaration it marks."),
    (
        UNUSED_SUPPRESSION,
        "Advisory: a lint suppression matches no finding in the code it covers, or is a block comment, which is not read.",
    ),
];

/// Removes from `findings` (of [`PROGRAM_RULES`]) those a `// snappea-lint: allow`
/// comment covers. Returns the suppressions that covered at least one finding.
pub fn suppress_program_findings(
    sources: &SourceMap,
    findings: &mut Vec<Finding>,
) -> Result<Vec<SuppressionUse>, ParseError> {
    let allowed = suppression::program_suppressions(&Context::parse(sources)?);
    let mut used = Vec::new();
    findings.retain(|f| {
        let by: Vec<_> = allowed.iter().filter(|s| s.rule == f.rule && s.scope.contains(f.span)).collect();
        used.extend(by.iter().map(|s| SuppressionUse { comment: s.comment, rule: s.rule.clone() }));
        by.is_empty()
    });
    Ok(used)
}

/// The rule of the advisory about a lint suppression that has no effect: it matches no
/// finding, or it is written in a form linting does not read.
pub const UNUSED_SUPPRESSION: &str = "unused-suppression";

/// Whether a rule reports an advisory rather than blocking a check. Advisory findings
/// remain warnings even with `--deny lint`; source suppressions work as usual.
pub fn is_advisory(rule: &str) -> bool {
    matches!(
        rule,
        "shared-spec-helper"
            | "unused-design-payload-field"
            | "payload-field-checked-by-other-spec"
            | UNUSED_SUPPRESSION
    )
}

/// An actionable diagnostic; advisory rules do not fail CLI linting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub span: Span,
    pub message: String,
    /// The likely fix for this finding, when it is more specific than the rule's general
    /// one (for example a suggested name).
    pub help: Option<String>,
}

impl Finding {
    pub fn new(rule: &'static str, span: Span, message: impl Into<String>) -> Self {
        Self { rule, span, message: message.into(), help: None }
    }

    /// This finding with a specific likely fix (see [`Finding::help`]).
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// A `// snappea-lint: allow` suppression of one lint rule: the comment, the code it
/// covers, and the rule. Coverage and mutation suppressions are not lint suppressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintSuppression {
    pub comment: Span,
    pub scope: Span,
    pub rule: String,
}

/// A suppression comment that removed a finding of `rule`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuppressionUse {
    pub comment: Span,
    pub rule: String,
}

/// Every well-formed lint suppression of the sources (see [`LintSuppression`]), and the
/// [`UNUSED_SUPPRESSION`] advisories for `/* snappea-lint: ... */` block comments, which
/// linting does not read.
pub fn lint_suppressions(sources: &SourceMap) -> Result<(Vec<LintSuppression>, Vec<Finding>), ParseError> {
    let ctx = Context::parse(sources)?;
    let registry = Registry::builtins();
    Ok((suppression::lint_suppressions(&ctx, &registry.rules), suppression::block_comment_suppressions(&ctx)))
}

/// Every lint rule ID a suppression may name: the built-in source rules and
/// [`PROGRAM_RULES`].
pub fn rule_ids() -> Vec<&'static str> {
    let registry = Registry::builtins();
    registry.rules().map(|r| r.id()).chain(PROGRAM_RULES.iter().map(|(id, _)| *id)).collect()
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
        r.register(EnumValueShadowsMachine);
        r.register(AnnounceAndSend);
        r
    }

    /// Rule IDs must be unique and may not use the reserved suppression diagnostic ID.
    pub fn register(&mut self, rule: impl Rule + 'static) {
        assert!(
            !["invalid-suppression", "invalid-directive", UNUSED_SUPPRESSION].contains(&rule.id())
                && !self.rules.iter().any(|r| r.id() == rule.id()),
            "duplicate or reserved lint rule ID"
        );
        self.rules.push(Box::new(rule));
    }

    pub fn rules(&self) -> impl Iterator<Item = &dyn Rule> {
        self.rules.iter().map(|r| r.as_ref())
    }

    pub fn check(&self, context: &Context<'_>) -> Vec<Finding> {
        self.check_selected(context, None)
    }

    fn check_selected(&self, context: &Context<'_>, selections: Option<&[SpecSelection]>) -> Vec<Finding> {
        self.check_selected_usage(context, selections).0
    }

    /// [`Registry::check_selected`], with the suppressions that removed a finding.
    fn check_selected_usage(
        &self,
        context: &Context<'_>,
        selections: Option<&[SpecSelection]>,
    ) -> (Vec<Finding>, Vec<SuppressionUse>) {
        let mut findings = Vec::new();
        for rule in &self.rules {
            if rule.id() == "unused-payload-field" && selections.is_some() {
                UnusedPayloadField.check_selected(context, selections, &mut findings);
            } else {
                rule.check(context, &mut findings);
            }
        }
        let used = suppression::apply(context, &self.rules, &mut findings);
        findings.sort_by(|a, b| (a.span, a.rule, &a.message).cmp(&(b.span, b.rule, &b.message)));
        findings.dedup();
        (findings, used)
    }
}

pub fn lint(sources: &SourceMap) -> Result<Vec<Finding>, ParseError> {
    Ok(Registry::builtins().check(&Context::parse(sources)?))
}

/// Resolved spec monitors selected together by one concrete test configuration.
/// Scenarios must not appear here: they describe evidence, not correctness checks.
pub struct SpecSelection {
    pub name: String,
    pub specs: Vec<String>,
}

/// Lint with resolved test selection, allowing complementary specs to share payload
/// validation only within each test. Source-only callers can continue using [`lint`].
pub fn lint_with_spec_selections(
    sources: &SourceMap,
    selections: &[SpecSelection],
) -> Result<Vec<Finding>, ParseError> {
    Ok(Registry::builtins().check_selected(&Context::parse(sources)?, Some(selections)))
}

/// [`lint_with_spec_selections`], with the suppressions that removed a finding, for
/// reporting suppressions that match nothing.
pub fn lint_with_spec_selections_usage(
    sources: &SourceMap,
    selections: &[SpecSelection],
) -> Result<(Vec<Finding>, Vec<SuppressionUse>), ParseError> {
    Ok(Registry::builtins().check_selected_usage(&Context::parse(sources)?, Some(selections)))
}

/// [`lint`], with the suppressions that removed a finding.
pub fn lint_usage(sources: &SourceMap) -> Result<(Vec<Finding>, Vec<SuppressionUse>), ParseError> {
    Ok(Registry::builtins().check_selected_usage(&Context::parse(sources)?, None))
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
pub fn mutation_suppression_details(sources: &SourceMap) -> Result<Vec<MutationSuppression>, ParseError> {
    Ok(suppression::mutation_suppression_details(&Context::parse(sources)?))
}

pub fn mutation_suppressions(sources: &SourceMap) -> Result<Vec<(Span, String)>, ParseError> {
    Ok(suppression::mutation_suppressions(&Context::parse(sources)?))
}

/// The machines marked `// snappea: driver` (test code that drives the model), as the
/// spans of their declarations.
pub fn driver_machines(sources: &SourceMap) -> Result<Vec<Span>, ParseError> {
    Ok(suppression::driver_machines(&Context::parse(sources)?))
}

/// The source scopes intentionally omitted from `snappea check` coverage requirements,
/// with the reason each `// snappea-lint: allow coverage -- reason` gives.
pub fn coverage_suppressions(sources: &SourceMap) -> Result<Vec<(Span, String)>, ParseError> {
    let context = Context::parse(sources)?;
    let registry = Registry::builtins();
    Ok(suppression::coverage_suppressions(&context, &registry.rules))
}
