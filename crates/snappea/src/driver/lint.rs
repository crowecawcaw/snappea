//! Model lint policy, shared by the standalone lint command, the pre-check gate and
//! `snappea observe`.
use crate::driver::check::Gate;
use crate::host::{Color, Exit, Host, Stream};
use crate::project::Sources;
use crate::sema::diag::{render_coded, render_snippet, render_with_level};
use crate::sema::Program;
use crate::syntax::SourceMap;
use snappea_lint::Finding;

/// Print all unsuppressed findings and return the blocking count. Does not execute models or write artifacts.
pub fn run(host: &mut dyn Host, sources: &Sources, prog: &Program) -> Result<usize, Exit> {
    let sm = sources.source_map();
    let findings = findings(host, &sm, prog)?;
    Ok(report(host, &sm, &findings))
}

/// The unsuppressed findings of the source rules and of the rules on the type-checked
/// program `prog` ([`snappea_lint::PROGRAM_RULES`]), in source order.
pub fn findings(host: &mut dyn Host, sm: &SourceMap, prog: &Program) -> Result<Vec<Finding>, Exit> {
    let mut findings = snappea_lint::lint(sm).map_err(|e| syntax_error(host, sm, e))?;
    findings.extend(program_findings(sm, prog));
    findings.sort_by(|a, b| (a.span.file, a.span.start, a.rule).cmp(&(b.span.file, b.span.start, b.rule)));
    Ok(findings)
}

/// The unsuppressed findings of [`snappea_lint::PROGRAM_RULES`] on `prog`.
pub fn program_findings(sm: &SourceMap, prog: &Program) -> Vec<Finding> {
    let marked = snappea_lint::driver_machines(sm).unwrap_or_default();
    let drivers = crate::checker::driver_facts::driver_machines(prog, sm, &marked);
    let tests = prog.test_cases();
    let intents = snappea_lint::test_intents(sm).unwrap_or_default();
    let positive: Vec<_> = tests
        .iter()
        .filter(|tc| intent(prog, tc, &intents).is_none_or(|i| i.expected_bug.is_none()))
        .cloned()
        .collect();
    let mut findings: Vec<Finding> =
        crate::checker::checked::lints_with_progress_tests(prog, &tests, &positive, &drivers)
            .into_iter()
            .filter(|f| !f.span.is_dummy())
            .map(|f| Finding::new(f.rule, f.span, f.message))
            .collect();
    let _ = snappea_lint::suppress_program_findings(sm, &mut findings);
    findings
}

/// Lints `sources` before a check. With [`Gate::Deny`] findings print as errors and stop
/// the check (exit status 2); advisory rules always warn. With [`Gate::Warn`] all print
/// as warnings and the check
/// goes on. Returns how many findings there were.
pub fn gate(host: &mut dyn Host, sources: &Sources, prog: &Program, gate: Gate) -> Result<usize, Exit> {
    gate_selected(host, sources, prog, gate, &prog.test_cases(), &[])
}

/// Test-specific lints follow selection; expected failures do not need progress specs.
pub fn gate_selected(
    host: &mut dyn Host,
    sources: &Sources,
    prog: &Program,
    gate: Gate,
    tests: &[crate::sema::TestCase],
    intents: &[snappea_lint::TestIntent],
) -> Result<usize, Exit> {
    let sm = sources.source_map();
    let mut findings = snappea_lint::lint(&sm).map_err(|e| syntax_error(host, &sm, e))?;
    findings.retain(|f| {
        !prog.tests.iter().enumerate().any(|(i, t)| {
            t.span.file == f.span.file
                && t.span.start <= f.span.start
                && t.span.end >= f.span.end
                && !tests.iter().any(|tc| matches!(tc.kind, crate::sema::TestCaseKind::Test(id, _) if id.idx() == i))
        })
    });
    let marked = snappea_lint::driver_machines(&sm).unwrap_or_default();
    let drivers = crate::checker::driver_facts::driver_machines(prog, &sm, &marked);
    let positive: Vec<_> =
        tests.iter().filter(|tc| intent(prog, tc, intents).is_none_or(|i| i.expected_bug.is_none())).cloned().collect();
    let mut program: Vec<Finding> =
        crate::checker::checked::lints_with_progress_tests(prog, tests, &positive, &drivers)
            .into_iter()
            .filter(|f| !f.span.is_dummy())
            .map(|f| Finding::new(f.rule, f.span, f.message))
            .collect();
    let _ = snappea_lint::suppress_program_findings(&sm, &mut program);
    findings.extend(program);
    findings.sort_by(|a, b| (a.span.file, a.span.start, a.rule).cmp(&(b.span.file, b.span.start, b.rule)));
    if gate == Gate::Deny {
        let count = report(host, &sm, &findings);
        if count > 0 {
            host.write(Stream::Err, &format!("{}\n", WARN_HINT), None);
            return Err(Exit(2));
        }
    } else {
        for f in &findings {
            let text = render_with_level(&sm, "warning", Some(f.rule), Some(f.span), &f.message, Some(&help(f.rule)));
            host.write(Stream::Err, &text, Some(Color::Yellow));
            host.write(Stream::Err, "\n", None);
        }
    }
    if gate == Gate::Warn && !findings.is_empty() {
        host.write(Stream::Err, &format!("Lint: {} finding(s) reported as warnings (--warn lint); checking anyway. Fix or suppress them before relying on the result.\n", findings.len()), None);
    }
    Ok(findings.len())
}

/// Intent is attached to a declaration, so parameterized expansions inherit it.
pub fn intent<'a>(
    prog: &Program,
    test: &crate::sema::TestCase,
    intents: &'a [snappea_lint::TestIntent],
) -> Option<&'a snappea_lint::TestIntent> {
    match test.kind {
        crate::sema::TestCaseKind::Test(id, _) => intents.iter().find(|i| i.span == prog.tests[id.idx()].span),
        _ => None,
    }
}

/// How to check a model before its lint findings are addressed.
const WARN_HINT: &str =
    "hint: to look for bugs before addressing these, pass --warn lint; the findings are then reported as warnings";

/// Like [`run`], with only the rules that apply when checking logs against specs
/// ([`snappea_lint::OBSERVATION_RULES`]).
pub fn run_observation(host: &mut dyn Host, sm: &SourceMap) -> Result<usize, Exit> {
    let findings = snappea_lint::lint_observation(sm).map_err(|e| syntax_error(host, sm, e))?;
    Ok(report(host, sm, &findings))
}

fn syntax_error(host: &mut dyn Host, sm: &SourceMap, e: crate::syntax::ParseError) -> Exit {
    let (headline, help) = crate::project::syntax_error(&e.message);
    let text = render_snippet(sm, Some(e.span), &headline, Some(&help));
    host.write(Stream::Err, &text, None);
    Exit(2)
}

/// The likely fix for a finding of `rule`, then how to suppress an intended one.
pub fn help(rule: &str) -> String {
    let suppress = format!(
        "if this is intended, suppress it with `// snappea-lint: allow {} -- reason` just before the declaration or statement",
        rule
    );
    let fix = match rule {
        "unused-design-payload-field" => "use the field in this receiver when its value affects the contract, remove it from the event when unnecessary, or suppress the intentional omission",
        "shared-spec-helper" => "compute expected results independently in the spec; sharing a proven representation or formatting utility may be intentional",
        "no-model-machine" => return "put modeled behavior in a separate `machine` and include it with the entry driver in the test module; for an intentional trace-only configuration, suppress this rule immediately before its test or implementation declaration, or before `machine Main` for the implicit default configuration".to_string(),
        "unobserved-handler" => "add the event to the monitor's `observes` list, or remove the handler",
        "scenario-never-accepts" => "mark the state the scenario should reach `cold`",
        "unchecked-state" => "assert on the field in a spec or the test driver, send it in an event a spec observes, or remove it if the model does not need it",
        "unchecked-event" => "make a spec observe the event and assert on it, or have the machine that receives it act on it; remove the send if nothing should",
        "no-progress-spec" => "add a spec with a `hot` state the model leaves only by answering (for example, hot while a request is pending), and select it with `assert` in the test",
        "unplanned-driver-stage" => "announce the plan first (e.g. `announce eOpsPlanned, n;` before starting any work) and make a spec observe it and stay `hot` until every planned operation finished",
        "unreachable-statement" => "remove the statement, or move it before the call that leaves the state",
        "enum-value-shadows-machine" => "rename the enum value, e.g. with a prefix such as `ModeSlowReply`, so each name means one thing",
        "scenario-cold-start" => "mark the start state `hot` (or leave it unmarked) and mark the accepting state `cold`",
        _ => return suppress,
    };
    format!("{}; {}", fix, suppress)
}

/// A finding with its source and fix, ending in a newline.
pub fn render(sm: &SourceMap, f: &Finding) -> String {
    if snappea_lint::is_advisory(f.rule) {
        render_with_level(sm, "warning", Some(f.rule), Some(f.span), &f.message, Some(&help(f.rule)))
    } else {
        render_coded(sm, Some(f.rule), Some(f.span), &f.message, Some(&help(f.rule)))
    }
}

/// The line that ends a list of `n` findings.
pub fn summary(n: usize) -> String {
    format!("Lint failed: {} finding(s). See docs/linting.md for fixes and scoped suppression comments.", n)
}

/// Prints findings with their source and fix; returns only the blocking finding count.
pub fn report(host: &mut dyn Host, sm: &SourceMap, findings: &[Finding]) -> usize {
    let errors = findings.iter().filter(|f| !snappea_lint::is_advisory(f.rule)).count();
    for f in findings {
        let color = if snappea_lint::is_advisory(f.rule) { Color::Yellow } else { Color::Red };
        host.write(Stream::Err, &render(sm, f), Some(color));
        host.write(Stream::Err, "\n", None);
    }
    if errors > 0 {
        host.write(Stream::Err, &format!("{}\n", summary(errors)), None);
    }
    errors
}
