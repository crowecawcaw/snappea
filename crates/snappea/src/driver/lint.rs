//! Model lint policy, shared by the standalone lint command, the pre-check gate and
//! `snappea observe`.
use crate::host::{Color, Exit, Host, Stream};
use crate::project::Sources;
use crate::sema::diag::{render_coded, render_snippet};
use crate::syntax::SourceMap;
use snappea_lint::Finding;

/// Print all unsuppressed lint findings. Does not execute models or write artifacts.
pub fn run(host: &mut dyn Host, sources: &Sources) -> Result<usize, Exit> {
    let sm = sources.source_map();
    let findings = snappea_lint::lint(&sm).map_err(|e| syntax_error(host, &sm, e))?;
    Ok(report(host, &sm, &findings))
}

/// Like [`run`], with only the rules that apply when checking logs against specs
/// ([`snappea_lint::OBSERVATION_RULES`]).
pub fn run_observation(host: &mut dyn Host, sm: &SourceMap) -> Result<usize, Exit> {
    let findings = snappea_lint::lint_observation(sm).map_err(|e| syntax_error(host, sm, e))?;
    Ok(report(host, sm, &findings))
}

fn syntax_error(host: &mut dyn Host, sm: &SourceMap, e: crate::syntax::ParseError) -> Exit {
    let help = crate::project::parse_help(&e.message);
    let text = render_snippet(sm, Some(e.span), &format!("syntax error: {}", e.message), Some(&help));
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
        "no-model-machine" => return "put modeled behavior in a separate `machine` and include it with the entry driver in the test module; for an intentional trace-only configuration, suppress this rule immediately before its test or implementation declaration, or before `machine Main` for the implicit default configuration".to_string(),
        "unobserved-handler" => "add the event to the monitor's `observes` list, or remove the handler",
        "scenario-never-accepts" => "mark the state the scenario should reach `cold`",
        "scenario-cold-start" => "mark the start state `hot` (or leave it unmarked) and mark the accepting state `cold`",
        _ => return suppress,
    };
    format!("{}; {}", fix, suppress)
}

/// A finding with its source and fix, ending in a newline.
pub fn render(sm: &SourceMap, f: &Finding) -> String {
    render_coded(sm, Some(f.rule), Some(f.span), &f.message, Some(&help(f.rule)))
}

/// The line that ends a list of `n` findings.
pub fn summary(n: usize) -> String {
    format!("Lint failed: {} finding(s). See docs/linting.md for fixes and scoped suppression comments.", n)
}

/// Prints `findings` with their source and fix, then a count; returns how many there were.
pub fn report(host: &mut dyn Host, sm: &SourceMap, findings: &[Finding]) -> usize {
    for f in findings {
        host.write(Stream::Err, &render(sm, f), Some(Color::Red));
        host.write(Stream::Err, "\n", None);
    }
    if !findings.is_empty() {
        host.write(Stream::Err, &format!("{}\n", summary(findings.len())), None);
    }
    findings.len()
}
