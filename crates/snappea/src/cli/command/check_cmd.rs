//! `snappea check`, `check lint` and `check mutation`: options and the suite run.

use super::*;

pub(in crate::cli) const LINT_HELP: &str = "\
Usage: snappea check lint [SOURCE...] [--strict] [--list-rules]

Type-checks and lints .p or .pproj sources without exploring schedules or writing
checker output. Fast; use it while editing. The same rules run before `snappea check`.
Exit status: 0 with no blocking findings, 2 blocking findings or source errors.
Advisory rules are reported, and fail, only with --strict.

  --strict      Also report advisory rules (`--list-rules --json` marks them) and fail on them
  --list-rules  List available rule IDs and descriptions without loading sources

Each finding says what is wrong, where, and how to fix it. Fix it first; suppress
only an intended one, on its own line before the declaration or statement the
finding points at:
  // snappea-lint: allow unused-payload-field -- the spec checks counts, not amounts
With --strict, a suppression that matches no finding is reported as `unused-suppression`.
`snappea check TEST` lints only what the selected tests need; run this command to
see every finding.

Examples:
  snappea check lint
  snappea check lint Model.pproj
  snappea check lint --list-rules
";

pub(in crate::cli) const LINT_OPTS: &[Opt] = &[flag("list-rules", None), flag("strict", None)];

pub(in crate::cli) fn lint(args: &[String], host: &mut dyn Host) -> i32 {
    let a = match parse_args(args, LINT_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("check lint")),
    };
    if a.help {
        host.write(Stream::Out, &format!("{}\n{}", LINT_HELP, OUTPUT_HELP), None);
        return OK;
    }
    if a.has("list-rules") && !a.positional.is_empty() {
        return usage_error(host, "--list-rules does not take sources", Some("check lint"));
    }
    let mut cfg = CheckConfig::snappea();
    // nothing runs, so foreign code is not left out of a check
    cfg.ignore_foreign_files = true;
    match sources(&mut cfg.input, &a.positional) {
        Ok(rest) if rest.is_empty() => {}
        Ok(rest) => {
            let m = format!(
                "'{}' is not a .p or .pproj file; lint takes sources only, e.g. `snappea check lint src/*.p`",
                rest[0]
            );
            return usage_error(host, &m, Some("check lint"));
        }
        Err(m) => return usage_error(host, &m, Some("check lint")),
    }
    with_output(host, &a, "check lint", |host| {
        if a.has("list-rules") {
            let all: Vec<(&str, &str)> = snappea_lint::rules()
                .into_iter()
                .chain(snappea_lint::PROGRAM_RULES.iter().copied())
                .chain(snappea_lint::COMMENT_RULES.iter().copied())
                .collect();
            let rules: Vec<_> = all
                .iter()
                .map(|(id, description)| {
                    host.out_line(&format!("{}  {}", id, description));
                    json!({"id": id, "description": description, "severity": if snappea_lint::is_advisory(id) { "advisory" } else { "error" }})
                })
                .collect();
            return Ok((OK, json!({"rules": rules})));
        }
        let mut quiet = Buffered::new(host);
        let loaded = check::resolve_sources(&mut quiet, &mut cfg).and_then(|_| check::load(&mut quiet, &cfg));
        let Buffered { out, .. } = quiet;
        match loaded {
            Ok(loaded) => {
                let sm = &loaded.sources.map;
                let strict = a.has("strict");
                let findings = crate::driver::lint::shown(
                    crate::driver::lint::findings(sm, &loaded.linter, &loaded.analysis.program),
                    strict,
                );
                let errors = crate::driver::lint::report(host, sm, &findings, strict);
                let json = crate::driver::lint::findings_json(sm, &findings);
                if errors != 0 {
                    // the findings, not their text again: stderr has that
                    return Ok((ERROR, json!({"error": crate::driver::lint::SUMMARY, "findings": json})));
                }
                host.out_line("Lint passed.");
                Ok((OK, json!({"findings": json})))
            }
            Err(e) => {
                for (stream, text, color) in out {
                    host.write(stream, &text, color);
                }
                Err(e)
            }
        }
    })
}

pub(in crate::cli) const CHECK_OPTS: &[Opt] = &[
    flag("inspect-trace", None),
    value("timeout", None),
    value("workers", Some('j')),
    value("seed", None),
    value("expect-bug-seeds", None),
    value("max-bugs", None),
    value("latency", None),
    flag("timed", None),
    value("out", Some('o')),
    flag("verbose", Some('v')),
    flag("list-suppressions", None),
    value("mutation-timeout", None),
    flag("no-mutation", None),
    value("warn", None),
    value("deny", None),
    flag("no-driver-limits", None),
    flag("strict", None),
    flag("ignore-foreign-files", None),
    value("progress", None),
];

/// Default time limit for mutation testing, in seconds.
pub(in crate::cli) const MUTATION_TIMEOUT: u32 = 30;

pub(in crate::cli) fn check(args: &[String], host: &mut dyn Host) -> i32 {
    // a test case named `lint` or `mutation` is checked with a glob: `lint*`
    match args.first().map(String::as_str) {
        Some("lint") => return lint(&args[1..], host),
        Some("mutation") => return mutation(&args[1..], host),
        _ => {}
    }
    let parsed = match parse_args(args, CHECK_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    if parsed.help {
        host.write(Stream::Out, &format!("{}\n{}", CHECK_HELP, OUTPUT_HELP), None);
        return OK;
    }
    let (mut cfg, mut suite) = (CheckConfig::snappea(), SuiteConfig::default());
    let patterns = match check_config(&parsed, &mut cfg, &mut suite) {
        Ok(p) => p,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    let mutation = match mutation_timeout(&parsed, MUTATION_TIMEOUT) {
        Ok(t) => t,
        Err(m) => return usage_error(host, &m, Some("check")),
    };
    with_output(host, &parsed, "check", |host| {
        run_check(host, cfg, suite, &patterns, parsed.get("latency"), Mutation::After(mutation))
    })
}

/// Default time limit of `check mutation`, in seconds.
pub(in crate::cli) const MUTATION_ONLY_TIMEOUT: u32 = 300;

pub(in crate::cli) fn mutation(args: &[String], host: &mut dyn Host) -> i32 {
    const CMD: Option<&str> = Some("check mutation");
    let parsed = match parse_args(args, CHECK_OPTS) {
        Ok(a) => a,
        Err(m) => return usage_error(host, &m, CMD),
    };
    if parsed.help {
        host.write(Stream::Out, &format!("{}\n{}", MUTATION_HELP, OUTPUT_HELP), None);
        return OK;
    }
    if parsed.has("no-mutation") {
        return usage_error(host, "'--no-mutation' does not apply to `check mutation`", CMD);
    }
    let (mut cfg, mut suite) = (CheckConfig::snappea(), SuiteConfig::default());
    suite.gates.set("mutation", Gate::Deny).expect("a known check");
    let patterns = match check_config(&parsed, &mut cfg, &mut suite) {
        Ok(p) => p,
        Err(m) => return usage_error(host, &m, CMD),
    };
    let limit = match mutation_timeout(&parsed, MUTATION_ONLY_TIMEOUT) {
        Ok(t) => t.expect("--no-mutation is rejected above"),
        Err(m) => return usage_error(host, &m, CMD),
    };
    with_output(host, &parsed, "check mutation", |host| {
        run_check(host, cfg, suite, &patterns, parsed.get("latency"), Mutation::Only(limit))
    })
}

/// Whether and how a check runs mutation testing (time limits in seconds).
#[derive(Debug, Clone, Copy)]
pub(in crate::cli) enum Mutation {
    /// After the check's report, unless `None` (`--no-mutation`).
    After(Option<u32>),
    /// `check mutation`: instead of the coverage and scenario reports.
    Only(u32),
}

/// The time limit for mutation testing in seconds (`default` without
/// `--mutation-timeout`), `None` to skip it.
pub(in crate::cli) fn mutation_timeout(a: &Args, default: u32) -> Result<Option<u32>, String> {
    match (a.has("no-mutation"), a.get("mutation-timeout")) {
        (true, Some(_)) => Err("use only one of '--no-mutation' and '--mutation-timeout'".into()),
        (true, None) => Ok(None),
        (false, None) => Ok(Some(default)),
        (false, Some(t)) => match parse_duration(t) {
            Some(t) => Ok(Some(t)),
            None => Err(format!("bad duration '{}' (examples: 90, 90s, 5m, 1h)", t)),
        },
    }
}

/// Fills `cfg` and `suite` from the `check` options; returns the test patterns.
pub(in crate::cli) fn check_config(
    a: &Args,
    cfg: &mut CheckConfig,
    suite: &mut SuiteConfig,
) -> Result<Vec<String>, String> {
    let patterns = sources(&mut cfg.input, &a.positional)?;
    if let Some(t) = a.get("timeout") {
        cfg.timeout =
            Some(parse_duration(t).ok_or_else(|| format!("bad duration '{}' (examples: 90, 90s, 5m, 1h)", t))?);
        suite.budget = None;
    }
    cfg.workers = match uint(a, "workers")? {
        Some(0) => return Err("'--workers' needs at least 1 worker".into()),
        n => n,
    };
    if let Some(s) = a.get("seed") {
        cfg.seed =
            Some(s.parse().map_err(|_| format!("'--seed' needs a whole number from 0 to 4294967295, not '{}'", s))?);
    }
    cfg.max_bugs = match uint(a, "max-bugs")? {
        Some(0) => return Err("'--max-bugs' needs at least 1 (1 stops each test case at its first bug)".into()),
        Some(n) => n,
        None => cfg.max_bugs,
    };
    if let Some(seeds) = a.get("expect-bug-seeds") {
        suite.expected_bug_seeds = seeds
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "'--expect-bug-seeds' needs comma-separated seed numbers, e.g. 1,2,3".to_string())?;
        let distinct: std::collections::BTreeSet<_> = suite.expected_bug_seeds.iter().collect();
        if !(2..=32).contains(&suite.expected_bug_seeds.len()) || distinct.len() != suite.expected_bug_seeds.len() {
            return Err("'--expect-bug-seeds' needs 2 to 32 distinct seeds".into());
        }
    }
    if let Some(l) = a.get("latency") {
        cfg.latency_profile = split_latency(l).1.map(str::to_string);
    }
    cfg.timed = a.has("timed");
    cfg.inspection_trace = a.has("inspect-trace");
    if let Some(o) = a.get("out") {
        cfg.output_path = o.to_string();
    }
    cfg.verbose = a.has("verbose");
    suite.list_suppressions = a.has("list-suppressions");
    cfg.driver_limits = !a.has("no-driver-limits");
    suite.gates.strict = a.has("strict");
    cfg.ignore_foreign_files = a.has("ignore-foreign-files");
    cfg.progress = (progress_mode(a)? != ProgressMode::Off).then(Position::default);
    gates(a, &mut suite.gates)?;
    Ok(patterns)
}

/// Applies `--warn` and `--deny` (each repeatable, or a comma list) to `gates` in the
/// order given; the last one naming a check (or `all`) wins, so `--warn all --deny
/// mutation` warns on all but mutation.
pub(in crate::cli) fn gates(a: &Args, gates: &mut check::Gates) -> Result<(), String> {
    // the last flag that set the mutation gate: `--deny mutation` there conflicts with
    // `--no-mutation`, but `--deny all` does not
    let mut last_mutation = None;
    for (option, value) in a.options.iter().filter(|(l, _)| matches!(*l, "warn" | "deny")) {
        let gate = if *option == "warn" { Gate::Warn } else { Gate::Deny };
        for name in value.split(',').map(str::trim) {
            if name != "all" && !check::Gates::CHECKS.contains(&name) {
                let known = check::Gates::CHECKS.join(", ");
                return Err(format!("unknown check '{}' for '--{}': use {} or all", name, option, known));
            }
            gates.set(name, gate)?;
            if name == "mutation" || name == "all" {
                last_mutation = Some((name, gate));
            }
        }
    }
    if a.has("no-mutation") && last_mutation == Some(("mutation", Gate::Deny)) {
        return Err("use only one of '--no-mutation' and '--deny mutation'".into());
    }
    Ok(())
}

/// `replay`'s `--max-steps`, `--fair-max-steps` and `--fail-on-max-steps`, for schedules
/// that `snappea-compat check` found with those bounds: a schedule replays only with the
/// step bounds it was found with.
pub(in crate::cli) fn step_options(a: &Args, cfg: &mut CheckConfig) -> Result<(), String> {
    if let Some(n) = uint(a, "max-steps")? {
        cfg.steps = StepBounds::from_unfair(n);
    }
    if let Some(n) = uint(a, "fair-max-steps")? {
        cfg.steps = cfg.steps.with_fair(n);
    }
    if cfg.steps.fair < cfg.steps.unfair {
        return Err(format!(
            "'--fair-max-steps' ({}) must be at least '--max-steps' ({}); raise it, or leave it out to use 10 x '--max-steps'",
            cfg.steps.fair, cfg.steps.unfair
        ));
    }
    cfg.fail_on_max_steps = a.has("fail-on-max-steps");
    Ok(())
}

/// Least time a test case gets from the shared budget.
pub(in crate::cli) const MIN_SHARE: f64 = 0.25;

pub(in crate::cli) fn run_check(
    host: &mut dyn Host,
    mut cfg: CheckConfig,
    suite: SuiteConfig,
    patterns: &[String],
    latency: Option<&str>,
    mutation: Mutation,
) -> Outcome {
    let only = matches!(mutation, Mutation::Only(_));
    let start = host.now();
    // the upstream-style progress shows only with --verbose
    let mut con = Quiet::new(host, !cfg.verbose);
    if let Some(l) = latency {
        cfg.latency = Some(check::load_latency(&mut con, split_latency(l).0)?);
    }
    check::resolve_sources(&mut con, &mut cfg)?;
    let loaded = check::load(&mut con, &cfg)?;
    let gates = suite.gates;

    let names: Vec<String> = loaded.analysis.program.test_cases().into_iter().map(|t| t.name).collect();
    let selected = select_tests(&names, patterns).map_err(|m| {
        error(con.inner, &m);
        Exit(ERROR)
    })?;
    if selected.is_empty() {
        error(con.inner, "the sources declare no test case; add one, e.g. `test tcMain [main=Main]: { Main };`");
        return Err(Exit(ERROR));
    }
    let all_tests = loaded.analysis.program.test_cases();
    let selected_tests: Vec<_> = all_tests.iter().filter(|t| selected.contains(&t.name)).cloned().collect();
    let sm = &loaded.sources.map;
    let intents = loaded.linter.test_intents(sm);
    // Blocking lint findings fail the check, but the test cases still run so their bugs
    // are reported, and mutation testing runs when none failed, so one run lists every
    // gap; coverage is skipped. The findings print after the test cases.
    let (lint_findings, lint_failed) = match crate::driver::lint::gate_selected(
        con.inner,
        &loaded,
        gates.lint,
        gates.strict,
        &selected_tests,
        &intents,
    ) {
        Ok(n) => (n, None),
        Err(e) => (0, Some(e)),
    };
    let prog = &loaded.analysis.program;
    let planned = plan(prog, selected, &selected_tests, &intents, &suite.expected_bug_seeds).map_err(|m| {
        error(con.inner, &m);
        Exit(ERROR)
    })?;
    let n = planned.len();
    con.inner.out_line(&format!("Checking {}: {} test case{}", cfg.project_name, n, plural(n)));
    let probes = crate::checker::compile::compile_program_with_probes(prog).probes;
    let drivers = crate::checker::driver_facts::driver_machines(prog, sm, &loaded.linter);
    let (mut positive_scope, mut positive_design) = (Scope::default(), Scope::default());
    for tc in &selected_tests {
        if crate::driver::lint::intent(prog, tc, &intents).is_none_or(|i| i.expected_bug.is_none()) {
            positive_scope.add(&Scope::test_case(prog, tc));
            positive_design.add(&Scope::partition_drivers(prog, tc, &drivers).1);
        }
    }
    let ctx = CaseContext {
        loaded: &loaded,
        all_tests: &all_tests,
        intents: &intents,
        probes: &probes,
        drivers: &drivers,
        positive_design: &positive_design,
        details: !only && lint_failed.is_none(),
        budget: suite.budget,
    };
    let SuiteState {
        failed,
        incomplete,
        unsettled,
        mut scope,
        mut hits,
        selected_scope,
        mut negative_driver_scope,
        negative_hits,
        passed,
        scenarios,
        cases,
        seed_runs,
    } = run_cases(&mut con, &mut cfg, &ctx, planned)?;
    let seed_checks = expected_seed_checks(&seed_runs);
    for check in &seed_checks {
        let name = check["test_case"].as_str().unwrap();
        match check["status"].as_str().unwrap() {
            "different_failures" => con.inner.out_line(&format!("      warning: {name} failed at different sites across seeds; inspect each seed's failure identity and replay, then split or tighten the negative control")),
            "incomplete" => con.inner.out_line(&format!("      note: {name}: failure identity comparison is incomplete; a seed did not match or its failure has no captured identity")),
            _ => con.inner.out_line(&format!("      {name}: expected failure identity consistent across {} seeds (evidence from these runs, not a guarantee)", check["runs"].as_array().unwrap().len())),
        }
    }
    negative_driver_scope.exclude(&positive_design);
    hits.add(&CodeMap::new(prog, &probes, &negative_driver_scope).restrict_hits(&negative_hits));
    scope.add(&negative_driver_scope);
    let map = CodeMap::new(prog, &probes, &scope);
    let all_cases = map.coverage(&hits);
    let coverage_suppressions = loaded.linter.coverage_suppressions(sm);
    // A declared failure can stop a negative-only driver before its remaining
    // work runs. Preserve that evidence, but require coverage only for components
    // selected by a positive test (including shared drivers and their helpers).
    let required_map = CodeMap::new(prog, &probes, &positive_scope);
    let required_suite = required_map.coverage(&hits);
    let required = required_map.required_summary(&required_suite, &coverage_suppressions);
    let mut required_uncovered = required_map.required_uncovered(&required_suite, &coverage_suppressions);
    let mut negative_only_scope = negative_driver_scope.clone();
    negative_only_scope.exclude(&positive_scope);
    let negative_only = CodeMap::new(prog, &probes, &negative_only_scope).coverage(&negative_hits);
    let negative_only_gaps = negative_only.uncovered();
    let (mut uncovered, mut uncovered_suppressed) = map.split_uncovered(&all_cases, &coverage_suppressions);
    // a scenario no test case satisfied is a gap like code no test case ran
    let (scenario_gaps, scenario_accepted) = scenario::gaps(prog, &scenarios, &coverage_suppressions);
    for (list, add) in [
        (&mut required_uncovered, &scenario_gaps),
        (&mut uncovered, &scenario_gaps),
        (&mut uncovered_suppressed, &scenario_accepted),
    ] {
        list.extend(add.iter().cloned());
        list.sort();
    }
    let behavioral = scenario::contract(prog, &scenarios, incomplete == 0 && failed == 0, &coverage_suppressions);
    let broad_exemptions: Vec<_> = coverage_suppressions
        .iter()
        .flat_map(|(span, reason)| {
            prog.machines
                .iter()
                .filter(move |m| {
                    !m.span.is_dummy()
                        && span.file == m.span.file
                        && span.start <= m.span.start
                        && span.end >= m.span.end
                })
                .map(|m| {
                    let loc = prog.sources.location(m.span);
                    let counts = map.hidden_points(&all_cases, m.span);
                    json!({"machine": m.name, "file": loc.file, "line": loc.line, "reason": reason,
                    "hidden_points": counts.values().sum::<usize>(), "hidden_kinds": counts})
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let required_missing = required.statements.covered < required.statements.total
        || required.branches.covered < required.branches.total
        || required.handlers.covered < required.handlers.total
        || !scenario_gaps.is_empty();
    let required_gap_count = required.statements.total - required.statements.covered + required.branches.total
        - required.branches.covered
        + required.handlers.total
        - required.handlers.covered
        + scenario_gaps.len();
    let coverage_gated = !only && failed == 0 && lint_failed.is_none() && required_missing;
    let coverage_warned = coverage_gated && gates.coverage == Gate::Warn;
    let coverage_failed = coverage_gated && gates.coverage == Gate::Deny;
    let coverage_failure_message = format!(
        "Required coverage was not reached within the run budget ({} point{}).",
        required_gap_count,
        plural(required_gap_count)
    );
    let coverage_failure_help = format!(
        "Add scenarios that reach the listed sites, or increase the timeout if exploration ran out of time.{} For deliberate omissions, add a scoped `coverage` suppression with a reason; use --warn coverage only for exploratory checking.",
        if scenario_gaps.is_empty() {
            ""
        } else {
            " For a scenario never satisfied, add or extend a test case that drives the model through it."
        }
    );
    let excluded = selected_scope.excluded(prog);
    if !only && lint_failed.is_none() {
        if n > 1 {
            con.inner.out_line(&format!("Coverage of all test cases: {}", all_cases.summary()));
        }
        let heading = if n > 1 { "Not covered by any test case:" } else { "Not covered by the test case:" };
        let (conditions, pairs) = required_map.advisory_uncovered(&required_suite, &coverage_suppressions);
        let (mut positive_uncovered, _) = required_map.split_uncovered(&required_suite, &coverage_suppressions);
        positive_uncovered.extend(scenario_gaps.iter().cloned());
        positive_uncovered.sort();
        let actionable: Vec<_> = positive_uncovered
            .iter()
            .filter(|gap| conditions.binary_search(gap).is_err() && pairs.binary_search(gap).is_err())
            .cloned()
            .collect();
        for l in uncovered_lines(heading, &actionable, " (--json lists all)") {
            con.inner.out_line(&l);
        }
        if !actionable.is_empty() {
            if incomplete == 0 && failed == 0 {
                con.inner.out_line("No schedule of the selected tests reaches these gaps: their searches completed. Run other tests or add a scenario that reaches the missing behavior.");
            }
            con.inner.out_line("For an impossible spec event, use a handler with `assert false` to state that contract. For an unreachable defensive branch, remove it and document why it is unnecessary, or use `allow coverage` with a reason.");
        }
        for l in uncovered_lines(
            "Informational condition coverage (does not fail the check):",
            &conditions,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        if !pairs.is_empty() {
            con.inner.out_line(&format!(
                "{} transition pairs not taken (informational; -v to list, --json lists all).",
                pairs.len()
            ));
            if cfg.verbose {
                for l in uncovered_lines("Informational transition pairs:", &pairs, " (--json lists all)") {
                    con.inner.out_line(&l);
                }
            }
        }
        for l in uncovered_lines(
            "Expected-bug-only driver gaps (report-only; the expected failure can stop the driver):",
            &negative_only_gaps,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        for l in uncovered_lines("Not covered, suppressed:", &uncovered_suppressed, " (--json lists all)") {
            con.inner.out_line(&l);
        }
        for l in uncovered_lines(
            "Outside the selected tests (excluded from coverage totals):",
            &excluded,
            " (--json lists all)",
        ) {
            con.inner.out_line(&l);
        }
        if failed > 0 {
            con.inner.out_line("Coverage includes partial results from test cases stopped by their bugs.");
        }
        if coverage_failed {
            con.inner.out_line(&format!("{coverage_failure_message} {coverage_failure_help}"));
        } else if coverage_warned {
            con.inner.out_line(&format!(
                "Warning: required coverage was not reached within the run budget ({} point{}; --warn coverage).",
                required_gap_count,
                plural(required_gap_count)
            ));
        }
        let not_proved = format!("no unexpected failure found; {}", unproved(incomplete, unsettled));
        con.inner.out_line(&format!(
            "Safety: {}.",
            if failed > 0 {
                "test expectations failed"
            } else if incomplete > 0 {
                not_proved.as_str()
            } else {
                "test expectations met; bounded searches complete"
            }
        ));
        con.inner.out_line(&format!("Behavioral coverage contract: {}.", behavioral["status"].as_str().unwrap()));
        for o in behavioral["obligations"].as_array().unwrap() {
            con.inner.out_line(&format!(
                "  [{}] {} ({}), {}:{}; observer progress {}/{} states",
                o["status"].as_str().unwrap(),
                o["name"].as_str().unwrap(),
                if o["required"] == true { "required" } else { "optional" },
                o["file"].as_str().unwrap_or(""),
                o["line"],
                o["best_states_visited"],
                o["monitor_states"]
            ));
        }
        con.inner.out_line(&format!("Code coverage (raw): {}", all_cases.summary()));
        con.inner.out_line(&format!("Code coverage (required after exemptions): {required}"));
        for e in &broad_exemptions {
            con.inner.out_line(&format!("Broad coverage exemption: {}:{} exempts entire machine {}; {} uncovered points hidden ({:?}). Reason: {}. Prefer a narrower scope; --json lists details.",
                e["file"].as_str().unwrap_or(""), e["line"], e["machine"].as_str().unwrap(),
                e["hidden_points"], e["hidden_kinds"], e["reason"].as_str().unwrap()));
        }
        if !uncovered_suppressed.is_empty() {
            con.inner.out_line("A clean required coverage gate with exemptions does not establish complete code or behavioral coverage.");
        }
        if !scenarios.is_empty() {
            if incomplete == 0 && failed == 0 {
                con.inner.write(Stream::Out, &scenario::merge(&scenarios), None);
            }
            if cfg.verbose {
                for line in scenario::evidence_lines(&scenarios) {
                    con.inner.out_line(&line);
                }
            }
        }
    }
    let mut skipped = Vec::new();
    if coverage_warned {
        skipped.push("required coverage (--warn coverage reported gaps as warnings)");
    }
    if matches!(mutation, Mutation::After(None)) && failed == 0 && lint_failed.is_none() {
        skipped.push("mutation testing (--no-mutation)");
    }
    let (mutation, undetected, untested) = mutation_phase(
        &mut con,
        mutation,
        &cfg,
        &suite,
        &ctx,
        failed,
        &passed,
        lint_failed.is_some(),
        selected_tests.len() < all_tests.len(),
        &coverage_suppressions,
        &mut skipped,
    );
    let seconds = con.inner.now() - start;
    let (elapsed, memory) = (duration(seconds), memory(con.inner));
    let mutation_time_limit_reached = mutation.get("time_limit_reached").and_then(Value::as_bool).unwrap_or(false);
    // `--deny mutation`, `check mutation` and `--strict` do not pass with mutants they
    // did not test
    let mutation_failed = (undetected > 0 || (untested.any() && (gates.mutation_complete || gates.strict)))
        && gates.mutation == Gate::Deny;
    let mut summary = check_summary(
        failed,
        n,
        incomplete,
        coverage_failed,
        // coverage was not checked when lint failed
        if lint_failed.is_some() { 0 } else { required_gap_count },
        if lint_failed.is_some() { &[] } else { &required_uncovered },
        mutation_time_limit_reached,
        (undetected, mutation_failed),
        untested,
        &skipped,
    );
    summary["settled"] = json!(incomplete - unsettled);
    if summary["status"] == "search_incomplete" {
        summary["message"] = json!(format!("No bugs found; {}", unproved(incomplete, unsettled)));
    }
    summary["scenario_gaps"] = json!(scenario_gaps.len());
    summary["lint_failed"] = json!(lint_failed.is_some());
    if let Some(lint) = &lint_failed {
        if failed == 0 {
            summary["status"] = json!("lint_failed");
            summary["message"] = json!(lint.summary);
        } else {
            summary["message"] = json!(format!("{}; lint failed", summary["message"].as_str().unwrap_or_default()));
        }
    }
    let out = match cfg.output_path.trim_end_matches('/') {
        "" if cfg.output_path.is_empty() => ".",
        o => o,
    };
    // what was reported but did not fail the check
    let mut warnings = serde_json::Map::new();
    let differing = seed_checks.iter().filter(|c| c["status"] == "different_failures").count();
    if differing > 0 {
        warnings.insert("expected_bug_failure_identity".into(), json!(differing));
    }
    if lint_findings > 0 {
        warnings.insert("lint".into(), json!(lint_findings));
    }
    if coverage_warned {
        warnings.insert("coverage".into(), json!(required_gap_count));
    }
    if undetected > 0 && !mutation_failed {
        warnings.insert("mutation".into(), json!(undetected));
    }
    if untested.any() && !mutation_failed {
        warnings.insert("mutation_untested".into(), json!(untested.total()));
    }
    if let Some(advisories) = mutation.get("suppression_advisories").and_then(Value::as_array).filter(|a| !a.is_empty())
    {
        warnings.insert("mutation_suppressions".into(), json!(advisories.len()));
    }
    // The console names what was reported as a warning, without counts: a count invites
    // working it down, and one finding matters as much as ten.
    let with_warnings = if failed > 0 || warnings.is_empty() {
        String::new()
    } else {
        let parts: Vec<&str> = warnings
            .keys()
            .map(|k| match k.as_str() {
                "lint" => "lint findings",
                "coverage" => "coverage gaps",
                "expected_bug_failure_identity" => "expected-bug tests with differing failures",
                "mutation_suppressions" => "mutation suppression advisories",
                "mutation_untested" => "mutants not tested",
                "mutation" => "undetected mutants",
                other => other,
            })
            .collect();
        format!(", with warnings: {}", parts.join(", "))
    };
    let success = if selected_tests
        .iter()
        .any(|tc| crate::driver::lint::intent(prog, tc, &intents).is_some_and(|i| i.expected_bug.is_some()))
    {
        "Test expectations met"
    } else {
        "No bugs found"
    };
    let lint_line = lint_failed.as_ref().map(|lint| {
        lint.print(con.inner, sm);
        let mutation_ran = mutation.get("skipped").is_none();
        let skipped = match (only, mutation_ran) {
            (true, false) => "; mutation testing skipped",
            (false, false) => "; coverage and mutation testing skipped",
            (true, true) => "",
            (false, true) => "; coverage skipped",
        };
        let lint_part = if undetected > 0 {
            "lint failed, and the test cases pass with each change listed as error[undetected-mutant]".to_string()
        } else {
            "lint failed".to_string()
        };
        if failed > 0 {
            format!(
                "{} of {} test case{} failed and {} in {}{}{}. Fix the bugs first (replay each with the command above), then fix or suppress the lint findings. Output: {}/",
                failed, n, plural(n), lint_part, elapsed, memory, skipped, out
            )
        } else {
            let incomplete = if incomplete > 0 {
                format!("; {}", unproved(incomplete, unsettled))
            } else {
                String::new()
            };
            format!(
                "{success} in {}{}{}, but {}{}. Fix or suppress each finding above, then check again. Output: {}/",
                elapsed, memory, incomplete, lint_part, skipped, out
            )
        }
    });
    // nothing was skipped or left as a warning: say that the model needs no more work
    let done =
        if skipped.is_empty() && warnings.is_empty() && !untested.any() { format!("{DONE} ") } else { String::new() };
    con.inner.out_line(&if let Some(line) = lint_line {
        line
    } else if only && failed == 0 && passed.is_empty() {
        format!(
            "Test expectations met in {}{}; mutation testing skipped: no passing positive test case.",
            elapsed, memory
        )
    } else if only && failed == 0 {
        match undetected {
            0 if !untested.any() && mutation["mutants"].as_u64() == Some(0) => format!(
                "No mutant ran in {}{}{}: there was none, or every one was suppressed or equivalent.",
                elapsed, memory, with_warnings
            ),
            0 if !untested.any() => {
                format!("🫛 Mutation testing passed in {}{}{}. {}", elapsed, memory, with_warnings, DONE)
            }
            0 if mutation_failed => {
                format!("Mutation testing incomplete in {}{}: {}.", elapsed, memory, untested.describe())
            }
            0 => format!(
                "Mutation testing incomplete in {}{}{}: {}.",
                elapsed,
                memory,
                with_warnings,
                untested.describe()
            ),
            _ if mutation_failed => format!("Mutation testing failed in {}{}: {}", elapsed, memory, UNDETECTED),
            _ => format!("Mutation testing found undetected mutants in {}{}{}.", elapsed, memory, with_warnings),
        }
    } else if failed == 0 && mutation_failed && !coverage_failed && undetected > 0 {
        format!("{success} in {}{}, but mutation testing failed: {} Output: {}/", elapsed, memory, UNDETECTED, out)
    } else if failed == 0 && mutation_failed && !coverage_failed {
        format!("Mutation testing incomplete in {}{}: {}. Output: {}/", elapsed, memory, untested.describe(), out)
    } else if failed == 0 && coverage_failed {
        format!(
            "Coverage requirements were not met for {} point{}. Output: {}/",
            required_gap_count,
            plural(required_gap_count),
            out
        )
    } else if failed == 0 && incomplete > 0 {
        format!(
            "🫛 {success} in {}{}{}. {}. Output: {}/",
            elapsed,
            memory,
            with_warnings,
            capitalized(&unproved(incomplete, unsettled)),
            out
        )
    } else if failed == 0 && n == 1 {
        format!("🫛 The test case passed in {}{}{}. {}Output: {}/", elapsed, memory, with_warnings, done, out)
    } else if failed == 0 {
        format!("🫛 All {} test cases passed in {}{}{}. {}Output: {}/", n, elapsed, memory, with_warnings, done, out)
    } else {
        format!("{} of {} test case{} failed in {}{}. Output: {}/", failed, n, plural(n), elapsed, memory, out)
    });
    if let Some(note) = summary["note"].as_str() {
        con.inner.out_line(&format!("warning: {}{}", note[..1].to_lowercase(), &note[1..]));
    }
    let mut coverage = all_cases.to_json(false);
    coverage["uncovered"] = uncovered.iter().map(|u| u.to_json()).collect();
    coverage["uncovered_suppressed"] = uncovered_suppressed.iter().map(|u| u.to_json()).collect();
    coverage["excluded_components"] = excluded.iter().map(|u| u.to_json()).collect();
    coverage["partial"] = json!(failed > 0);
    coverage["required"] = required.to_json();
    coverage["expected_bug_driver_only"] = negative_only.to_json(false);
    coverage["expected_bug_driver_only"]["required"] = json!(false);
    coverage["uncovered_required"] = required_uncovered.iter().map(|u| u.to_json()).collect();
    coverage["gate_bypassed"] = json!(!only && gates.coverage == Gate::Warn);
    coverage["failure"] = if coverage_failed {
        json!({"message": coverage_failure_message, "help": coverage_failure_help})
    } else {
        Value::Null
    };
    if lint_failed.is_some() {
        coverage = json!({"skipped": LINT_SKIP});
    }
    let mut result = json!({
        "summary": summary,
        "project": cfg.project_name,
        "test_cases": cases,
        "seconds": round_ms(seconds),
        "output": format!("{}/", out),
        "mutation": mutation,
        "warnings": warnings,
        "expected_bug_seed_checks": seed_checks,
    });
    if !only {
        result["coverage"] = coverage;
        result["behavioral_coverage"] = behavioral;
        result["coverage_exemptions"] = json!(broad_exemptions);
        result["scenarios"] = scenarios.iter().map(|a| serde_json::from_str::<Value>(&a.to_json()).unwrap()).collect();
    }
    let code = if failed > 0 || coverage_failed || mutation_failed { BUG } else { OK };
    let (code, result) = with_lint_failure(code, result, failed, lint_failed.as_deref());
    // Last, so `tail` and `grep ^error` keep it.
    if let Some(recap) = failure_recap(code, &result, mutation_failed) {
        con.inner.out_line(&recap);
    }
    Ok((code, result))
}

/// One check of a selected test case: an expected-bug test runs once per
/// `--expect-bug-seeds` seed.
pub(in crate::cli) struct PlannedCase {
    pub(in crate::cli) name: String,
    pub(in crate::cli) seed: Option<u32>,
    /// The test expects a bug (`snappea: expect-bug`).
    pub(in crate::cli) expects_bug: bool,
}

/// The checks to run for the `selected` test cases, in order.
pub(in crate::cli) fn plan(
    prog: &crate::sema::Program,
    selected: Vec<String>,
    tests: &[crate::sema::TestCase],
    intents: &[snappea_lint::TestIntent],
    seeds: &[u32],
) -> Result<Vec<PlannedCase>, String> {
    let expects_bug = |name: &str| {
        tests
            .iter()
            .find(|t| t.name == name)
            .is_some_and(|tc| crate::driver::lint::intent(prog, tc, intents).is_some_and(|i| i.expected_bug.is_some()))
    };
    if !seeds.is_empty() && !selected.iter().any(|n| expects_bug(n)) {
        return Err("--expect-bug-seeds requires a selected test with a snappea: expect-bug marker".into());
    }
    let mut planned = Vec::new();
    for name in selected {
        let expects = expects_bug(&name);
        if expects && !seeds.is_empty() {
            planned.extend(seeds.iter().map(|&seed| PlannedCase {
                name: name.clone(),
                seed: Some(seed),
                expects_bug: true,
            }));
        } else {
            planned.push(PlannedCase { name, seed: None, expects_bug: expects });
        }
    }
    Ok(planned)
}

/// What [`run_cases`] reads besides the config.
pub(in crate::cli) struct CaseContext<'a> {
    pub(in crate::cli) loaded: &'a check::Loaded,
    pub(in crate::cli) all_tests: &'a [crate::sema::TestCase],
    pub(in crate::cli) intents: &'a [snappea_lint::TestIntent],
    pub(in crate::cli) probes: &'a [crate::checker::bytecode::Probe],
    pub(in crate::cli) drivers: &'a [bool],
    /// Design code of the positive test cases, left out of expected-bug driver coverage.
    pub(in crate::cli) positive_design: &'a Scope,
    /// Print per-case coverage and driver limits: not for `check mutation`, or once lint failed.
    pub(in crate::cli) details: bool,
    /// Seconds the test cases share ([`SuiteConfig::budget`]).
    pub(in crate::cli) budget: Option<f64>,
}

/// What the test cases of a run found, together.
#[derive(Default)]
pub(in crate::cli) struct SuiteState {
    pub(in crate::cli) failed: usize,
    pub(in crate::cli) incomplete: usize,
    /// Incomplete searches that did not settle either.
    pub(in crate::cli) unsettled: usize,
    /// Code of the positive test cases, and what it ran.
    pub(in crate::cli) scope: Scope,
    pub(in crate::cli) hits: Hits,
    /// Code of every selected test case.
    pub(in crate::cli) selected_scope: Scope,
    /// Driver code of the expected-bug test cases that found their bug, and what it ran.
    pub(in crate::cli) negative_driver_scope: Scope,
    pub(in crate::cli) negative_hits: Hits,
    /// Positive test cases that passed, for mutation testing.
    pub(in crate::cli) passed: Vec<(crate::sema::TestCase, Hits)>,
    pub(in crate::cli) scenarios: Vec<scenario::Artifact>,
    /// Each case's JSON result.
    pub(in crate::cli) cases: Vec<Value>,
    pub(in crate::cli) seed_runs: std::collections::BTreeMap<String, Vec<SeedFailure>>,
}

/// Checks each planned case with its share of the budget, printing a line or a report per
/// case. Leaves `cfg`'s seed and bug limit as it found them.
pub(in crate::cli) fn run_cases(
    con: &mut Quiet,
    cfg: &mut CheckConfig,
    ctx: &CaseContext,
    planned: Vec<PlannedCase>,
) -> Result<SuiteState, Exit> {
    let prog = &ctx.loaded.analysis.program;
    let n = planned.len();
    let mut st = SuiteState::default();
    // The driver-limits hint is the same for every test case; say it once per run.
    let mut driver_hint_shown = false;
    let check_start = con.inner.now();
    let (base_seed, base_max_bugs) = (cfg.seed, cfg.max_bugs);
    for (i, PlannedCase { name, seed, expects_bug }) in planned.into_iter().enumerate() {
        cfg.test_case_name = name;
        cfg.seed = seed.or(base_seed);
        cfg.seed_artifact = seed;
        // the shared budget left, split between the test cases left
        cfg.time_share = ctx.budget.map(|b| {
            let left = b - (con.inner.now() - check_start);
            (left / (n - i) as f64).max(MIN_SHARE)
        });
        cfg.progress = cfg.progress.map(|_| Position { test: i + 1, tests: n });
        // an expected-bug test passes on its first matching bug: searching on would only
        // spend the budget the other test cases share
        cfg.max_bugs = if expects_bug { 1 } else { base_max_bugs };
        check::set_output_directory(con, cfg)?;
        let case = check::testing_process(con, ctx.loaded, cfg)?;
        let tc = ctx.all_tests.iter().find(|t| t.name == case.test_case).unwrap();
        let intent = crate::driver::lint::intent(prog, tc, ctx.intents);
        let expected = intent.and_then(|i| i.expected_bug.as_deref());
        let boundary = intent.is_some_and(|i| i.boundary.is_some());
        let matched = expected.is_some_and(|text| {
            case.bugs > 0
                && case
                    .bug_report
                    .as_deref()
                    .is_some_and(|r| failure_message(r, &ctx.loaded.sources.map).contains(text))
        });
        if let Some(seed) = seed {
            st.seed_runs.entry(case.test_case.clone()).or_default().push(SeedFailure {
                seed,
                matched,
                identity: case.bug_identity.clone(),
            });
        }
        if expected.is_some() && !matched || expected.is_none() && case.bugs > 0 {
            st.failed += 1;
        } else if expected.is_none() && case.exhaustive_finished != Some(true) {
            st.incomplete += 1;
            st.unsettled += usize::from(verdict(&case) != Verdict::Settled);
        }
        if let Some(expected) = expected {
            con.inner.out_line(&format!(
                "{}  {}{} (expected bug: {:?})",
                if matched { "ok  " } else { "FAIL" },
                case.test_case,
                seed.map(|s| format!(" [seed {s}]")).unwrap_or_default(),
                expected
            ));
            if !matched {
                let reason = if case.bugs > 0 {
                    "the observed failure did not match"
                } else if case.exhaustive_finished == Some(true) {
                    "search complete, no bug: this test cannot reach the expected flaw"
                } else {
                    "search incomplete, no matching bug found within the run budget"
                };
                con.inner.out_line(&format!("      {reason}; check the test and expected failure text"));
            }
            if let Some(report) = &case.bug_report {
                for l in report.lines() {
                    con.inner.out_line(&format!("      {l}"));
                }
            }
            for l in hot_spec_lines(case.bug_identity.as_ref(), &case.hot_specs) {
                con.inner.out_line(&format!("      {l}"));
            }
            if let Some(schedule) = &case.schedule_file {
                con.inner.out_line(&format!("      Replay: {}", replay_command(schedule, cfg)));
            }
            if matched && case.schedules == 1 {
                con.inner.out_line("      note: the expected bug appeared in the first schedule; check that the control exercises the intended flaw");
            }
        } else {
            print_case(con.inner, &case, cfg, boundary);
        }
        let mut result = case_json(&case, cfg, boundary);
        if let Some(seed) = seed {
            result["seed"] = json!(seed);
        }
        if let Some(text) = expected {
            result["result"] = json!(if matched { "expected_bug" } else { "expectation_failed" });
            let outcome = if matched {
                "matched"
            } else if case.bugs > 0 {
                "wrong_failure"
            } else if case.exhaustive_finished == Some(true) {
                "unreachable"
            } else {
                "not_found"
            };
            result["expected_bug"] = json!({"text": text, "matched": matched, "outcome": outcome});
        }
        if let Some(reason) = intent.and_then(|i| i.boundary.as_deref()) {
            result["boundary"] = json!(reason);
        }
        if let Some((s, h)) = &case.code_coverage {
            st.selected_scope.add(s);
            let summary = CodeMap::new(prog, ctx.probes, s).coverage(h).summary();
            if ctx.details {
                con.inner.out_line(&format!(
                    "      Coverage: {}{}",
                    summary,
                    match (case.bugs > 0, case.collected_bugs) {
                        (false, _) => "",
                        (true, false) => " (partial: stopped at first bug)",
                        (true, true) => " (partial: stopped after the bugs)",
                    }
                ));
            }
            result["coverage"] = summary.to_json();
            result["coverage"]["partial"] = json!(case.bugs > 0);
            if expected.is_some() {
                let (mut driver_scope, design_scope) = Scope::partition_drivers(prog, tc, ctx.drivers);
                driver_scope.exclude(ctx.positive_design);
                let driver_coverage = CodeMap::new(prog, ctx.probes, &driver_scope).coverage(h);
                result["expected_bug_driver_coverage"] = driver_coverage.to_json(false);
                result["expected_bug_driver_coverage"]["included"] = json!(matched);
                let design_map = CodeMap::new(prog, ctx.probes, &design_scope);
                let design_coverage = design_map.coverage(h);
                result["expected_bug_design_coverage"] = design_coverage.summary().to_json();
                result["expected_bug_design_coverage"]["partial"] =
                    json!(case.bugs > 0 || case.exhaustive_finished != Some(true));
                if ctx.details {
                    con.inner.out_line(&format!(
                        "      Expected-bug design coverage (separate): {}",
                        design_coverage.summary()
                    ));
                }
                if matched {
                    st.negative_driver_scope.add(&driver_scope);
                    st.negative_hits.add(h);
                }
            } else {
                st.scope.add(s);
                st.hits.add(h);
            }
            if expected.is_none() && case.bugs == 0 {
                if let Some(tc) = ctx.all_tests.iter().find(|t| t.name == case.test_case) {
                    st.passed.push((tc.clone(), h.clone()));
                }
            }
        }
        if let Some(d) = case.driver.as_ref().filter(|_| case.bugs == 0) {
            // on request: the limits are long, and JSON always has them
            if ctx.details && cfg.verbose {
                for l in driver_lines(d, &case, !driver_hint_shown) {
                    con.inner.out_line(&l);
                }
                driver_hint_shown |= !d.facts.is_empty();
            }
            result["driver"] = json!({
                "machines": d.machines,
                "repeats": d.repeats,
                "facts": d.facts.iter().map(|f| f.to_json()).collect::<Vec<_>>(),
            });
        }
        if !case.scenario_coverage.scenarios.is_empty() {
            result["scenarios"] = serde_json::from_str(&case.scenario_coverage.to_json()).unwrap();
            if expected.is_none() {
                st.scenarios.push(case.scenario_coverage.clone());
            }
        }
        st.cases.push(result);
    }
    cfg.seed = base_seed;
    cfg.max_bugs = base_max_bugs;
    cfg.seed_artifact = None;
    Ok(st)
}

/// What lint failing skips, as reported in the JSON result.
const LINT_SKIP: &str = "lint failed";

/// Runs mutation testing after the test cases, or says why it is skipped (adding that to
/// `skipped`): its JSON result, the undetected mutants and those left untested.
#[allow(clippy::too_many_arguments)]
pub(in crate::cli) fn mutation_phase(
    con: &mut Quiet,
    mutation: Mutation,
    cfg: &CheckConfig,
    suite: &SuiteConfig,
    ctx: &CaseContext,
    failed: usize,
    passed: &[(crate::sema::TestCase, Hits)],
    lint_failed: bool,
    partial_selection: bool,
    coverage_suppressions: &[(crate::syntax::Span, String)],
    skipped: &mut Vec<&'static str>,
) -> (Value, usize, Untested) {
    let only = matches!(mutation, Mutation::Only(_));
    let mut undetected = 0;
    let mut untested = Untested::default();
    let json = match mutation {
        Mutation::After(None) => json!({"skipped": "--no-mutation"}),
        // a test case that failed leaves lint as the next thing to fix; mutation testing
        // still runs after a lint failure, since its findings are independent of lint's
        _ if lint_failed && failed > 0 => json!({"skipped": LINT_SKIP}),
        _ if passed.is_empty() && failed == 0 => {
            skipped.push("mutation testing (no passing positive test case)");
            con.inner.out_line("Mutation testing skipped: no passing positive test case was selected.");
            json!({"skipped": "no passing positive test case"})
        }
        _ if failed > 0 => {
            con.inner.out_line("Mutation testing skipped: a test case failed.");
            json!({"skipped": "a test case failed"})
        }
        // a test case that was not selected may be the one that detects a mutant
        Mutation::After(Some(_)) if partial_selection => {
            skipped.push("mutation testing (only some test cases were selected)");
            json!({"skipped": "only some test cases were selected"})
        }
        Mutation::After(Some(limit)) | Mutation::Only(limit) => {
            let suppressed = ctx.loaded.linter.mutation_suppressions(&ctx.loaded.sources.map);
            let report = crate::driver::mutation::run(
                con,
                &ctx.loaded.analysis.program,
                cfg,
                passed,
                &suppressed,
                coverage_suppressions,
                ctx.drivers,
                f64::from(limit),
                only,
                suite.gates.strict,
            );
            for l in report.lines(SHOWN_UNCOVERED) {
                con.inner.out_line(&l);
            }
            if report.blanket_suppressions > 0 {
                con.inner.out_line("hint: blanket `allow mutation` suppressions hide every mutation kind in their scope; prefer mutation:<kind>. Use --list-suppressions to inspect their scope.");
            }
            if suite.list_suppressions || cfg.verbose {
                for line in report.suppression_lines() {
                    con.inner.out_line(&line);
                }
            }
            // those inside an undetected coarse mutant too
            undetected = report.failing().len();
            untested = Untested {
                not_tested: report.not_tested(),
                under_searched: report.under_searched(),
                failed: report.failed_to_run(),
                unreached: report.unreached.len(),
                // `snappea check` reports code no test case ran as coverage gaps
                count_unreached: only,
            };
            report.to_json()
        }
    };
    (json, undetected, untested)
}

/// Ends the summary of a check with nothing left to fix, so work on the model can stop.
pub(in crate::cli) const DONE: &str = "Nothing left to fix: more tests are optional.";
/// Ends the summary of a check that undetected mutants failed.
pub(in crate::cli) const UNDETECTED: &str =
    "the test cases pass with each change listed as error[undetected-mutant] above. For each, add what its help names, or suppress it with a reason.";

/// A check's exit status and result when its blocking lint findings `lint` failed it:
/// 1 when a test case failed (the bug is the first thing to fix), else 2, as when lint
/// alone runs. The result keeps the lint-failure fields `error` and `lint.findings`.
pub(in crate::cli) fn with_lint_failure(
    code: i32,
    mut result: Value,
    failed: usize,
    lint: Option<&crate::driver::lint::LintFailed>,
) -> (i32, Value) {
    let Some(lint) = lint else {
        return (code, result);
    };
    result["error"] = json!(lint.summary);
    result["lint"] = json!({"failed": true, "errors": lint.count, "findings": lint.findings});
    (if failed > 0 { BUG } else { ERROR }, result)
}
