//! Model lint policy, shared by the standalone lint command, the pre-check gate and
//! `snappea observe`.
use crate::driver::check::Gate;
use crate::host::{Color, Exit, Host, Stream};
use crate::project::Sources;
use crate::sema::diag::{render_coded, render_snippet, render_with_level};
use crate::sema::{Program, TestCase};
use crate::syntax::{ParseError, SourceMap};
use serde_json::{json, Value};
use snappea_lint::{Finding, SuppressionUse, TestIntent, UNUSED_SUPPRESSION};

/// Print all unsuppressed findings and return the blocking count. Does not execute models or write artifacts.
pub fn run(host: &mut dyn Host, sources: &Sources, prog: &Program) -> Result<usize, Exit> {
    let sm = sources.source_map();
    let findings = findings(host, &sm, prog)?;
    Ok(report(host, &sm, &findings))
}

/// The unsuppressed findings of the source rules and of the rules on the type-checked
/// program `prog` ([`snappea_lint::PROGRAM_RULES`]) for every test, in source order, with
/// an `unused-suppression` advisory for each lint suppression that matched nothing.
pub fn findings(host: &mut dyn Host, sm: &SourceMap, prog: &Program) -> Result<Vec<Finding>, Exit> {
    let intents = snappea_lint::test_intents(sm).unwrap_or_default();
    let (mut findings, used) =
        collect(sm, prog, &prog.test_cases(), &intents, false).map_err(|e| syntax_error(host, sm, e))?;
    findings.extend(unused_suppressions(sm, &findings, &used));
    sort(&mut findings);
    Ok(findings)
}

/// The findings for `tests`, and the suppressions that removed one. With `selected_only`,
/// source findings inside the declarations of other tests are left out.
fn collect(
    sm: &SourceMap,
    prog: &Program,
    tests: &[TestCase],
    intents: &[TestIntent],
    selected_only: bool,
) -> Result<(Vec<Finding>, Vec<SuppressionUse>), ParseError> {
    let (mut findings, mut used) = source_findings(sm, prog, tests)?;
    if selected_only {
        findings.retain(|f| {
            !prog.tests.iter().enumerate().any(|(i, t)| {
                t.span.contains(f.span)
                    && !tests
                        .iter()
                        .any(|tc| matches!(tc.kind, crate::sema::TestCaseKind::Test(id, _) if id.idx() == i))
            })
        });
    }
    let marked = snappea_lint::driver_machines(sm).unwrap_or_default();
    let drivers = crate::checker::driver_facts::driver_machines(prog, sm, &marked);
    let positive: Vec<_> =
        tests.iter().filter(|tc| intent(prog, tc, intents).is_none_or(|i| i.expected_bug.is_none())).cloned().collect();
    let mut program: Vec<Finding> =
        crate::checker::checked::lints_with_progress_tests(prog, tests, &positive, &drivers)
            .into_iter()
            .filter(|f| !f.span.is_dummy())
            .map(|f| {
                let finding = Finding::new(f.rule, f.span, f.message);
                match f.help {
                    Some(help) => finding.with_help(help),
                    None => finding,
                }
            })
            .collect();
    used.extend(snappea_lint::suppress_program_findings(sm, &mut program).unwrap_or_default());
    findings.extend(program);
    sort(&mut findings);
    Ok((findings, used))
}

fn sort(findings: &mut [Finding]) {
    findings.sort_by(|a, b| (a.span.file, a.span.start, a.rule).cmp(&(b.span.file, b.span.start, b.rule)));
}

fn source_findings(
    sm: &SourceMap,
    prog: &Program,
    tests: &[TestCase],
) -> Result<(Vec<Finding>, Vec<SuppressionUse>), ParseError> {
    if tests.is_empty() {
        return snappea_lint::lint_usage(sm);
    }
    let selections: Vec<_> = tests
        .iter()
        .map(|test| {
            let (module, _) = prog.test_case_module(test);
            snappea_lint::SpecSelection {
                name: test.name.clone(),
                specs: module
                    .monitor_map
                    .iter()
                    .filter_map(|(id, _)| {
                        let machine = prog.machine(*id);
                        (machine.is_spec() && !machine.is_scenario()).then(|| machine.name.clone())
                    })
                    .collect(),
            }
        })
        .collect();
    snappea_lint::lint_with_spec_selections_usage(sm, &selections)
}

/// The unsuppressed findings of [`snappea_lint::PROGRAM_RULES`] on `prog`.
pub fn program_findings(sm: &SourceMap, prog: &Program) -> Vec<Finding> {
    let intents = snappea_lint::test_intents(sm).unwrap_or_default();
    let tests = prog.test_cases();
    match collect(sm, prog, &tests, &intents, false) {
        Ok((findings, _)) => {
            findings.into_iter().filter(|f| snappea_lint::PROGRAM_RULES.iter().any(|(r, _)| *r == f.rule)).collect()
        }
        Err(_) => Vec::new(),
    }
}

/// An `unused-suppression` advisory for each lint suppression comment that removed no
/// finding, and for `/* snappea-lint: ... */` block comments, which are not read.
/// `findings` are the unsuppressed findings, to point at one the comment may be meant for.
fn unused_suppressions(sm: &SourceMap, findings: &[Finding], used: &[SuppressionUse]) -> Vec<Finding> {
    let Ok((suppressions, mut out)) = snappea_lint::lint_suppressions(sm) else {
        return Vec::new();
    };
    for s in suppressions {
        if used.iter().any(|u| u.comment == s.comment && u.rule == s.rule) {
            continue;
        }
        // the finding of this rule nearest the comment, in case it was meant for that one
        let near = findings
            .iter()
            .filter(|f| f.rule == s.rule)
            .min_by_key(|f| (f.span.file != s.comment.file, f.span.start.abs_diff(s.comment.end)));
        let help = match near {
            Some(f) => format!(
                "if it is meant for the `{}` finding at {}, move it to its own line directly above the code there ({}); otherwise remove it",
                s.rule,
                sm.location(f.span),
                reported_at(&s.rule)
            ),
            None => format!(
                "remove it; nothing reports `{}` now. If the finding reappears, suppress it at {}",
                s.rule,
                reported_at(&s.rule)
            ),
        };
        out.push(
            Finding::new(
                UNUSED_SUPPRESSION,
                s.comment,
                format!("suppression of '{}' matches no finding in the code it covers", s.rule),
            )
            .with_help(help),
        );
    }
    out
}

/// Where findings of `rule` are reported, which is where a suppression must cover them.
fn reported_at(rule: &str) -> &'static str {
    match rule {
        "unused-payload-field" | "payload-field-checked-by-other-spec" => {
            "the spec's handler for the event, not the test that selects the spec"
        }
        "unobserved-handler" => "the handler in the spec or scenario",
        "permissive-monitor" | "unselected-spec" => "the spec declaration",
        "scenario-never-accepts" => "the scenario declaration",
        "scenario-cold-start" => "the scenario's start state",
        "no-model-machine" => {
            "the test or implementation declaration, or `machine Main` for the implicit configuration"
        }
        "tautological-assert" => "the assert statement",
        "announce-and-send" => "the second of the two statements",
        "enum-value-shadows-machine" => "the enum declaration",
        "unchecked-state" => "the field declaration or its machine",
        "unchecked-event" => "the send or announce statement, or its machine",
        "no-progress-spec" => "the test declaration, or the machine that waits for a reply",
        "unplanned-driver-stage" => "the statement that starts the later stage, or the driver machine",
        "unreachable-statement" => "the statement that never runs",
        "unused-design-payload-field" => "the design machine's handler for the event",
        "shared-spec-helper" => "the helper function declaration",
        _ => "the declaration or statement the finding points at",
    }
}

/// Lints `sources` before a check. With [`Gate::Deny`] findings print as errors and stop
/// the check (exit status 2); advisory rules always warn. With [`Gate::Warn`] all print
/// as warnings and the check
/// goes on. Returns how many findings there were.
pub fn gate(host: &mut dyn Host, sources: &Sources, prog: &Program, gate: Gate) -> Result<usize, Exit> {
    gate_selected(host, sources, prog, gate, &prog.test_cases(), &[]).map_err(Exit::from)
}

/// Lint findings stopped a check: the exit status, and the blocking findings as JSON
/// (see [`findings_json`]) for a machine-readable result.
#[derive(Debug)]
pub struct LintFailed {
    pub exit: Exit,
    /// `None` when the sources did not parse.
    pub findings: Option<Value>,
    /// The line that ends the human-readable report.
    pub summary: String,
}

impl From<LintFailed> for Exit {
    fn from(e: LintFailed) -> Exit {
        e.exit
    }
}

/// Test-specific lints follow selection; expected failures do not need progress specs.
/// When `tests` leaves some out, findings only those tests have are skipped, with a note
/// saying how many, and suppressions are not reported as unused.
pub fn gate_selected(
    host: &mut dyn Host,
    sources: &Sources,
    prog: &Program,
    gate: Gate,
    tests: &[TestCase],
    intents: &[TestIntent],
) -> Result<usize, LintFailed> {
    let sm = sources.source_map();
    let parse_failed = |host: &mut dyn Host, e| LintFailed {
        exit: syntax_error(host, &sm, e),
        findings: None,
        summary: String::new(),
    };
    let all = prog.test_cases();
    let partial = tests.len() < all.len();
    let (mut findings, used) = match collect(&sm, prog, tests, intents, partial) {
        Ok(c) => c,
        Err(e) => return Err(parse_failed(host, e)),
    };
    let mut skipped = 0;
    if partial {
        // the blocking findings of the whole project that this selection does not have
        if let Ok((every, _)) = collect(&sm, prog, &all, intents, false) {
            skipped = every
                .iter()
                .filter(|f| !snappea_lint::is_advisory(f.rule))
                .filter(|f| !findings.iter().any(|g| g.rule == f.rule && g.span == f.span))
                .count();
        }
    } else {
        findings.extend(unused_suppressions(&sm, &findings, &used));
        sort(&mut findings);
    }
    let note = |host: &mut dyn Host| {
        if skipped > 0 {
            host.write(
                Stream::Err,
                &format!(
                    "note: {} lint finding(s) only in tests not selected were skipped; run `snappea check lint` to see them\n",
                    skipped
                ),
                None,
            );
        }
    };
    if gate == Gate::Deny {
        let count = report(host, &sm, &findings);
        note(host);
        if count > 0 {
            host.write(Stream::Err, &format!("{}\n", WARN_HINT), None);
            return Err(LintFailed {
                exit: Exit(2),
                findings: Some(findings_json(&sm, &findings)),
                summary: summary(count),
            });
        }
    } else {
        for f in &findings {
            let text = render_with_level(&sm, "warning", Some(f.rule), Some(f.span), &f.message, Some(&help(f)));
            host.write(Stream::Err, &text, Some(Color::Yellow));
            host.write(Stream::Err, "\n", None);
        }
        note(host);
    }
    if gate == Gate::Warn && !findings.is_empty() {
        host.write(Stream::Err, &format!("Lint: {} finding(s) reported as warnings (--warn lint); checking anyway. Fix or suppress them before relying on the result.\n", findings.len()), None);
    }
    Ok(findings.len())
}

/// Intent is attached to a declaration, so parameterized expansions inherit it.
pub fn intent<'a>(prog: &Program, test: &TestCase, intents: &'a [TestIntent]) -> Option<&'a TestIntent> {
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

fn syntax_error(host: &mut dyn Host, sm: &SourceMap, e: ParseError) -> Exit {
    let (headline, help) = crate::project::syntax_error(&e.message);
    let text = render_snippet(sm, Some(e.span), &headline, Some(&help));
    host.write(Stream::Err, &text, None);
    Exit(2)
}

/// The likely fix for finding `f`, then, for a rule that can be suppressed, how to
/// suppress an intended one.
pub fn help(f: &Finding) -> String {
    let rule = f.rule;
    let general = match rule {
        // these are about the suppression comments themselves: fix them
        "invalid-suppression" => {
            let fix = "write `// snappea-lint: allow rule-id[, rule-id] -- reason` on its own line directly above the code it covers; `snappea check lint --list-rules` lists the rule IDs";
            return join(f.help.as_deref(), fix);
        }
        "invalid-directive" => {
            let fix = "directives are `// snappea: driver` before a machine, and `// snappea: expect-bug \"failure text\"` or `// snappea: boundary -- reason` before a test";
            return join(f.help.as_deref(), fix);
        }
        UNUSED_SUPPRESSION => return f.help.clone().unwrap_or_else(|| "remove the suppression".into()),
        "no-model-machine" => return "put modeled behavior in a separate `machine` and include it with the entry driver in the test module; for an intentional trace-only configuration, suppress this rule immediately before its test or implementation declaration, or before `machine Main` for the implicit default configuration".to_string(),
        "permissive-monitor" => "make the spec able to fail: assert an invariant over what it observes, leave out a handler for an event that must not arrive in that state (an unhandled event fails the check), or mark a state `hot` while work is pending",
        "unselected-spec" => "select it with `assert SpecName in ...` in a test whose model produces the events it observes",
        "tautological-assert" => "compare against a value the model produced independently (an event payload, or state another machine reported), or remove the assertion; an assertion that cannot fail checks nothing",
        "announce-and-send" => "remove the `announce`, since specs observe the `send`; if the announcement means something else, give it its own event",
        "unused-payload-field" => "if the contract depends on the field, check it against what the spec knows (for example, that a reply carries the value the request asked for); do not add an assertion that cannot fail just to read it",
        "payload-field-checked-by-other-spec" => "keep each spec focused on its own obligation; review whether this division of payload checks matches the contract",
        "unused-design-payload-field" => "use the field in this receiver when its value affects the contract, or remove it from the event when nothing needs it",
        "shared-spec-helper" => "compute expected results independently in the spec; sharing a proven representation or formatting utility may be intentional",
        "unobserved-handler" => "add the event to the monitor's `observes` list, or remove the handler",
        "scenario-never-accepts" => "mark the state the scenario should reach `cold`",
        "unchecked-state" => "if a wrong value would be a bug, check it: assert on it in a spec or the test driver, or send it in an event a spec observes; otherwise remove the field",
        "unchecked-event" => "make a spec observe the event and check it, or have the machine that receives it act on it; remove the send if nothing should",
        "no-progress-spec" => "add a spec with a `hot` state the model leaves only by answering (for example, hot while a request is pending), and select it with `assert` in the test",
        "unplanned-driver-stage" => "announce the plan before starting any work (e.g. `announce eOpsPlanned, n;` in the driver's first entry), and make a selected spec observe it and stay `hot` until every planned operation finished",
        "unreachable-statement" => "remove the statement, or move it before what leaves the state if it should run",
        "enum-value-shadows-machine" => "rename the enum value, e.g. by prefixing the enum's name, so each name means one thing",
        "scenario-cold-start" => "mark the start state `hot` (or leave it unmarked) and mark the accepting state `cold`",
        _ => "fix the reported problem",
    };
    let fix = f.help.as_deref().unwrap_or(general);
    format!(
        "{}; if this is intended, suppress it with `// snappea-lint: allow {} -- reason` on its own line directly above {}",
        fix,
        rule,
        reported_at(rule)
    )
}

/// The specific fix of a malformed comment: a suggestion, then `general` otherwise.
fn join(specific: Option<&str>, general: &str) -> String {
    match specific {
        Some(s) if s.ends_with('?') => format!("{} Otherwise {}", s, general),
        Some(s) => s.to_string(),
        None => general.to_string(),
    }
}

/// A finding with its source and fix, ending in a newline.
pub fn render(sm: &SourceMap, f: &Finding) -> String {
    if snappea_lint::is_advisory(f.rule) {
        render_with_level(sm, "warning", Some(f.rule), Some(f.span), &f.message, Some(&help(f)))
    } else {
        render_coded(sm, Some(f.rule), Some(f.span), &f.message, Some(&help(f)))
    }
}

/// The line that ends a list of `n` findings.
pub fn summary(n: usize) -> String {
    format!(
        "Lint failed: {} finding(s). Each finding's help says how to fix it, or how to suppress an intended one; `snappea check lint --list-rules` describes the rules.",
        n
    )
}

/// Findings as JSON: rule, severity, message, help, and source location of each.
pub fn findings_json(sm: &SourceMap, findings: &[Finding]) -> Value {
    findings
        .iter()
        .map(|f| {
            let loc = sm.location(f.span);
            let severity = if snappea_lint::is_advisory(f.rule) { "advisory" } else { "error" };
            json!({"rule": f.rule, "severity": severity, "message": f.message, "help": help(f), "file": loc.file, "line": loc.line, "column": loc.col})
        })
        .collect()
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
