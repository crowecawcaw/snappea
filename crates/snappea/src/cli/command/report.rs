//! What `snappea check` prints and writes about a run: summary, per-case lines and JSON.

use super::*;

/// Mutants of the selected code that got no verdict (see [`crate::driver::mutation::Report`]).
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::cli) struct Untested {
    /// The time limit came before their first turn.
    pub(in crate::cli) not_tested: usize,
    /// The time limit stopped them before their budget, undetected so far.
    pub(in crate::cli) under_searched: usize,
    /// The checker could not run them.
    pub(in crate::cli) failed: usize,
    /// In code no passing test case ran.
    pub(in crate::cli) unreached: usize,
    /// Whether `unreached` counts as untested: in `check mutation`, but not in `snappea
    /// check`, which reports that code as coverage gaps.
    pub(in crate::cli) count_unreached: bool,
}

impl Untested {
    pub(in crate::cli) fn total(&self) -> usize {
        self.not_tested + self.under_searched + self.failed + self.counted_unreached()
    }

    pub(in crate::cli) fn counted_unreached(&self) -> usize {
        if self.count_unreached {
            self.unreached
        } else {
            0
        }
    }

    pub(in crate::cli) fn any(&self) -> bool {
        self.total() > 0
    }

    /// E.g. `3 mutants not tested before the time limit, 1 in code no passing test case ran`.
    pub(in crate::cli) fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.not_tested > 0 {
            parts.push(format!(
                "{} mutant{} not tested before the time limit",
                self.not_tested,
                plural(self.not_tested)
            ));
        }
        if self.under_searched > 0 {
            parts.push(format!(
                "{} mutant{} not fully tested before the time limit",
                self.under_searched,
                plural(self.under_searched)
            ));
        }
        if self.counted_unreached() > 0 {
            parts.push(format!("{} mutant{} in code no passing test case ran", self.unreached, plural(self.unreached)));
        }
        if self.failed > 0 {
            parts.push(format!("{} mutant{} the checker could not run", self.failed, plural(self.failed)));
        }
        parts.join(", ")
    }
}

/// A concise top-level outcome. Mutation time limits change it only when `--deny mutation`
/// (or `check mutation`, `--strict`) requires every mutant to be tested; they never change whether the
/// test search or required-coverage gate passed.
#[allow(clippy::too_many_arguments)]
pub(in crate::cli) fn check_summary(
    failed: usize,
    test_cases: usize,
    incomplete: usize,
    coverage_failed: bool,
    coverage_gaps: usize,
    uncovered: &[crate::checker::code_coverage::Uncovered],
    mutation_time_limit_reached: bool,
    (mutants_undetected, mutation_failed): (usize, bool),
    untested: Untested,
    skipped: &[&str],
) -> Value {
    // What a pass does not cover, so a partial check is not read as a full one.
    let mut notes = Vec::new();
    if !skipped.is_empty() {
        notes.push(format!(
            "Not a full check: skipped {}. Check again without these options before finishing.",
            skipped.join(", ")
        ));
    }
    let timed_out = untested.not_tested + untested.under_searched;
    if failed == 0 && timed_out > 0 {
        let mut what = Vec::new();
        if untested.not_tested > 0 {
            what.push(format!("{} mutant{} not tested", untested.not_tested, plural(untested.not_tested)));
        }
        if untested.under_searched > 0 {
            what.push(format!(
                "{} not fully tested (no test case failed before the limit stopped {})",
                untested.under_searched,
                if untested.under_searched == 1 { "it" } else { "them" }
            ));
        }
        notes.push(format!(
            "Mutation testing reached its time limit with {}. Raise the limit before adding assertions \
             for them, e.g. `snappea check mutation --mutation-timeout 5m`, or name fewer test cases.",
            what.join(" and ")
        ));
    }
    if failed == 0 && untested.counted_unreached() > 0 {
        notes.push(format!(
            "{} mutant{} in code no passing test case ran, so no test case could detect {}. \
             Add a test case or scenario that runs that code; `snappea check` lists it.",
            untested.unreached,
            plural(untested.unreached),
            if untested.unreached == 1 { "it" } else { "them" }
        ));
    }
    if failed == 0 && untested.failed > 0 {
        notes.push(format!(
            "The checker could not run {} mutant{} (a snappea bug; please report it with the model).",
            untested.failed,
            plural(untested.failed)
        ));
    }
    let (status, message) = if failed > 0 {
        ("tests_failed", format!("{} of {} test case{} failed", failed, test_cases, plural(test_cases)))
    } else if coverage_failed {
        (
            "coverage_failed",
            format!(
                "Required coverage was not reached within the run budget ({} point{})",
                coverage_gaps,
                plural(coverage_gaps)
            ),
        )
    } else if mutation_failed && mutants_undetected > 0 {
        (
            "mutation_failed",
            "The test cases pass with changes to the model that a spec or assertion should detect".to_string(),
        )
    } else if mutation_failed {
        ("mutation_incomplete", format!("Mutation testing incomplete: {}", untested.describe()))
    } else if incomplete > 0 {
        (
            "search_incomplete",
            format!("No bugs found; search incomplete for {} test case{}", incomplete, plural(incomplete)),
        )
    } else {
        ("passed", "All test cases passed".to_string())
    };
    json!({
        "status": status,
        "message": message,
        "test_cases": test_cases,
        "failed": failed,
        "incomplete": incomplete,
        "coverage_failed": coverage_failed,
        "safety": {"status": if failed > 0 { "failed" } else if incomplete > 0 { "incomplete" } else { "passed" }},
        "coverage_gaps": coverage_gaps,
        "uncovered": uncovered.iter().take(5).map(|u| json!({
            "file": u.file,
            "line": u.line,
            "context": u.context,
            "what": u.what,
        })).collect::<Vec<_>>(),
        "uncovered_more": uncovered.len().saturating_sub(5),
        "mutation_time_limit_reached": mutation_time_limit_reached,
        "mutants_undetected": mutants_undetected,
        "mutants_not_tested": untested.not_tested,
        "mutants_under_searched": untested.under_searched,
        "mutants_unreached": untested.unreached,
        "mutants_failed_to_run": untested.failed,
        "skipped": skipped,
        "note": if notes.is_empty() { Value::Null } else { json!(notes.join(" ")) },
    })
}

/// Driver facts shown for a test case; `--json` has them all.
pub(in crate::cli) const SHOWN_DRIVER_FACTS: usize = 5;

/// The console lines for what a test case's driver did in every schedule.
pub(in crate::cli) fn driver_lines(d: &check::DriverReport, case: &check::CaseOutcome, hint: bool) -> Vec<String> {
    if d.facts.is_empty() {
        return Vec::new();
    }
    let schedules = match case.exhaustive_finished {
        Some(true) => "every schedule",
        _ => "every explored schedule",
    };
    let mut out = vec![format!("      Driver limits (held in {}):", schedules)];
    for f in d.facts.iter().take(SHOWN_DRIVER_FACTS) {
        out.push(format!("        {}", f.text));
    }
    if d.facts.len() > SHOWN_DRIVER_FACTS {
        let n = d.facts.len() - SHOWN_DRIVER_FACTS;
        out.push(format!("        ... and {} more (--json lists all)", n));
    }
    if hint {
        out.push("      hint: if the real environment can do more, extend the driver to try it".into());
    }
    out
}

/// How many distinct states per choice of its deepest schedule a passing test case's
/// complete search can reach and still be reported as having little to explore.
pub(in crate::cli) const LITTLE_TO_EXPLORE: usize = 3;

/// Whether a passing test case explored every schedule and they reached few more states
/// than one run does ([`LITTLE_TO_EXPLORE`] per step): the model has nearly one behavior
/// under the test, so the test checks little concurrency. A note, not a failure.
pub(in crate::cli) fn little_to_explore(case: &check::CaseOutcome) -> bool {
    case.bugs == 0
        && case.exhaustive_finished == Some(true)
        && case.exhaustive_states.is_some_and(|(states, depth)| states <= LITTLE_TO_EXPLORE * depth.max(1))
}

/// How far a passing test case's search got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cli) enum Verdict {
    /// Every schedule was checked.
    Proved,
    /// Not every schedule, but the second half of the search reached no new coverage.
    Settled,
    /// New coverage was still turning up when the search stopped.
    Finding,
    /// Stopped with nothing to judge by: memory ran out, or coverage was not tracked.
    Unjudged,
}

impl Verdict {
    /// The name in JSON.
    fn name(self) -> &'static str {
        match self {
            Verdict::Proved => "proved",
            Verdict::Settled => "settled",
            Verdict::Finding => "finding_new_behavior",
            Verdict::Unjudged => "unjudged",
        }
    }
}

pub(in crate::cli) fn verdict(case: &check::CaseOutcome) -> Verdict {
    match case.settled {
        _ if case.exhaustive_finished == Some(true) => Verdict::Proved,
        _ if case.memory_stop.is_some() => Verdict::Unjudged,
        Some(true) => Verdict::Settled,
        Some(false) => Verdict::Finding,
        None => Verdict::Unjudged,
    }
}

/// `3 test cases not proved: 2 settled, 1 not settled`, for `incomplete`
/// searches of which `unsettled` did not settle.
pub(in crate::cli) fn unproved(incomplete: usize, unsettled: usize) -> String {
    let mut parts = Vec::new();
    if incomplete > unsettled {
        parts.push(format!("{} settled", incomplete - unsettled));
    }
    if unsettled > 0 {
        parts.push(format!("{} not settled", unsettled));
    }
    format!("{} test case{} not proved: {}", incomplete, plural(incomplete), parts.join(", "))
}

pub(in crate::cli) fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Why a passing test case's search did not cover every schedule, for the JSON report:
/// `memory_limit` (memory use neared the limit, so the search stopped),
/// `exhaustive_search_unfinished` (the search ran out of time or states), `timed_model_sampled` (time
/// tracking keeps clocks growing, so a model that loops cannot be exhausted) or `sampled`
/// (too many schedules: the search sampled them).
pub(in crate::cli) fn incomplete_reason(case: &check::CaseOutcome) -> Option<&'static str> {
    if case.bugs > 0 || case.exhaustive_finished == Some(true) {
        None
    } else if case.memory_stop.is_some() {
        Some("memory_limit")
    } else if case.exhaustive_unfinished.is_some() {
        Some("exhaustive_search_unfinished")
    } else if case.time_tracked {
        Some("timed_model_sampled")
    } else {
        Some("sampled")
    }
}

/// The next step for a search that memory use stopped.
pub(in crate::cli) fn memory_hint(m: check::MemoryStop) -> String {
    format!(
        "search stopped at {} MB of {} MB memory; shrink bounds (fewer machines, retries or faults per test case) or split the model into smaller test cases",
        m.used_mb, m.limit_mb
    )
}

/// A `--timeout` worth trying for a search still finding new behavior after `elapsed`
/// seconds: five times as long, up to a minute. A search that has not settled in longer
/// than that is too large to: suggesting ever more time only costs it.
pub(in crate::cli) fn longer_timeout(elapsed: f64) -> Option<&'static str> {
    [(30.0, "30s"), (60.0, "1m")].iter().find(|(secs, _)| *secs >= 5.0 * elapsed).map(|(_, s)| *s)
}

/// Whether a search's schedules mostly reach states it already explored: at least
/// 10 schedules per distinct state. Independent steps run in many orders that
/// end in the same states, which a forwarder or a needless concurrent machine multiplies.
pub(in crate::cli) fn mostly_revisits(case: &check::CaseOutcome) -> Option<usize> {
    const REVISITS: usize = 10;
    case.exhaustive_states.map(|(states, _)| states).filter(|&states| states > 0 && case.schedules >= REVISITS * states)
}

/// The machines ready at the most scheduling points with a choice, e.g. `Replica, Writer`.
pub(in crate::cli) fn busiest_machines(machines: &[crate::checker::branching::MachineBranching]) -> String {
    let mut ms: Vec<_> = machines.iter().filter(|m| m.ready > 0.0).collect();
    ms.sort_by(|a, b| b.ready.total_cmp(&a.ready));
    ms.iter().take(3).map(|m| m.machine.as_str()).collect::<Vec<_>>().join(", ")
}

/// Where an average schedule branches: `31 scheduling points with a choice; ready at
/// them: Network 25, Sender 20; choices made: Network 3`.
pub(in crate::cli) fn branching_text(points: f64, machines: &[crate::checker::branching::MachineBranching]) -> String {
    let n = |x: f64| if x >= 9.95 { format!("{:.0}", x) } else { format!("{:.1}", x) };
    let list = |count: fn(&crate::checker::branching::MachineBranching) -> f64| {
        let mut ms: Vec<_> = machines.iter().filter(|m| count(m) > 0.0).collect();
        ms.sort_by(|a, b| count(b).total_cmp(&count(a)));
        ms.iter().take(3).map(|m| format!("{} {}", m.machine, n(count(m)))).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("{} scheduling points with a choice", n(points));
    let ready = list(|m| m.ready);
    if !ready.is_empty() {
        out.push_str(&format!("; ready at them: {}", ready));
    }
    let choices = list(|m| m.choices);
    if !choices.is_empty() {
        out.push_str(&format!("; `$`/`choose` choices: {}", choices));
    }
    out
}

pub(in crate::cli) struct SeedFailure {
    pub(in crate::cli) seed: u32,
    pub(in crate::cli) matched: bool,
    pub(in crate::cli) identity: Option<crate::checker::runtime::BugIdentity>,
}

pub(in crate::cli) fn bug_identity_json(identity: &crate::checker::runtime::BugIdentity) -> Value {
    json!({"kind": identity.kind, "machine": identity.machine, "state": identity.state,
        "file": identity.file, "line": identity.line, "column": identity.column})
}

pub(in crate::cli) fn expected_seed_checks(runs: &std::collections::BTreeMap<String, Vec<SeedFailure>>) -> Vec<Value> {
    runs.iter()
        .map(|(name, runs)| {
            let identities: std::collections::BTreeSet<_> = runs.iter().filter_map(|r| r.identity.as_ref()).collect();
            let all_matched = runs.iter().all(|r| r.matched);
            let status = if identities.len() > 1 {
                "different_failures"
            } else if !all_matched || runs.iter().any(|r| r.identity.is_none()) {
                "incomplete"
            } else {
                "consistent"
            };
            json!({"test_case": name, "status": status, "all_matched": all_matched,
            "runs": runs.iter().map(|r| json!({"seed": r.seed, "matched": r.matched,
                "identity": r.identity.as_ref().map(bug_identity_json)})).collect::<Vec<_>>()})
        })
        .collect()
}

/// Match the diagnostic message, never source excerpts or suggested fixes. Native
/// diagnostics place those after a location/help line; assertion and liveness reports
/// without a rendered snippet retain their whole message, including newlines.
pub(in crate::cli) fn failure_message(report: &str, sources: &crate::syntax::SourceMap) -> String {
    if let Some(assertion) = report.strip_prefix("Assertion Failed: ") {
        for source in sources.files() {
            let prefix = format!("{}:", source.name.replace('\\', "\\\\"));
            let Some(rest) = assertion.strip_prefix(&prefix) else { continue };
            let Some((line, rest)) = rest.split_once(':') else { continue };
            if line.parse::<u32>().is_err() {
                continue;
            }
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let message = rest[end..].strip_prefix(' ').unwrap_or(&rest[end..]);
            return format!("Assertion Failed: {message}");
        }
        return report.to_string();
    }
    report
        .lines()
        .take_while(|line| {
            let line = line.trim_start();
            !line.starts_with("-->") && !line.starts_with("= help:")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One test case's result as JSON.
pub(in crate::cli) fn case_json(case: &check::CaseOutcome, cfg: &CheckConfig, boundary: bool) -> Value {
    let mut v = json!({
        "name": case.test_case,
        "result": if case.bugs > 0 { "bug" } else { "ok" },
        "schedules": case.schedules,
        "workers": case.workers,
        "seconds": round_ms(case.elapsed),
        "time_limit_reached": case.timed_out,
        "coverage_stopped_growing": case.plateaued,
    });
    if case.bugs == 0 {
        v["verdict"] = json!(verdict(case).name());
    }
    if let Some(finished) = case.exhaustive_finished {
        v["search_complete"] = json!(finished);
    }
    if let Some(reason) = incomplete_reason(case) {
        v["incomplete_reason"] = json!(reason);
    }
    if let Some(m) = case.memory_stop {
        v["memory_limit_reached"] = json!({"used_mb": m.used_mb, "limit_mb": m.limit_mb});
        v["hint"] = json!(memory_hint(m));
    }
    if !case.auto_handoffs.is_empty() {
        v["auto_handoffs"] = json!(case
            .auto_handoffs
            .iter()
            .map(|h| json!({
                "worker": h.worker,
                "after_schedules": h.after_schedules,
                "choices": h.choices,
                "reason": h.reason,
            }))
            .collect::<Vec<_>>());
    }
    if let Some(frontier) = case.exhaustive_unfinished {
        v["untried_alternatives"] = json!(frontier);
    }

    if let Some((states, _)) = case.exhaustive_states.filter(|_| case.bugs == 0) {
        v["distinct_states"] = json!(states);
    }
    if let Some((points, machines)) = &case.branching {
        let r = |x: f64| (x * 10.0).round() / 10.0;
        v["branching"] = json!({
            "scheduling_points_with_choice": r(*points),
            "machines": machines.iter().map(|m| json!({
                "machine": m.machine,
                "ready_at_scheduling_points": r(m.ready),
                "choices": r(m.choices),
            })).collect::<Vec<_>>(),
        });
    }
    if !case.collection_sizes.is_empty() {
        v["collection_sizes"] = json!(case
            .collection_sizes
            .iter()
            .map(|c| json!({
                "machine": c.machine, "field": c.field, "file": c.file, "line": c.line,
                "min": c.min, "max": c.max, "observations": c.observations,
            }))
            .collect::<Vec<_>>());
    }
    if !boundary && little_to_explore(case) {
        v["little_to_explore"] = json!(true);
    }
    if case.bugs > 0 {
        v["bug"] = json!({
            "report": case.bug_report,
            "identity": case.bug_identity.as_ref().map(bug_identity_json),
            "schedule": case.schedule_file,
            "replay": case.schedule_file.as_ref().map(|s| replay_command(s, cfg)),
            "also_hot": hot_specs_json(&case.hot_specs),
        });
        if case.collected_bugs {
            v["more_bugs"] = json!(case
                .more_bugs
                .iter()
                .map(|m| json!({
                    "report": m.bug.report,
                    "identity": m.bug.identity.as_ref().map(bug_identity_json),
                    "also_hot": hot_specs_json(&m.bug.hot_specs),
                    "schedule": m.schedule_file,
                    "trace": m.trace_file,
                    "replay": replay_command(&m.schedule_file, cfg),
                }))
                .collect::<Vec<_>>());
        }
    }
    v
}

pub(in crate::cli) fn hot_specs_json(specs: &[crate::checker::runtime::HotSpec]) -> Value {
    json!(specs.iter().map(|h| json!({"spec": h.spec, "state": h.state, "kind": h.kind})).collect::<Vec<_>>())
}

/// The lines naming the specs other than the failing one that were hot when a bug was
/// found: those the same execution violates, then those that are only context. A
/// deadlock's report already names every hot spec.
pub(in crate::cli) fn hot_spec_lines(
    identity: Option<&crate::checker::runtime::BugIdentity>,
    specs: &[crate::checker::runtime::HotSpec],
) -> Vec<String> {
    if identity.is_some_and(|i| i.kind == "deadlock") {
        return Vec::new();
    }
    let list = |kind: &str| {
        specs
            .iter()
            .filter(|h| h.kind == kind)
            .map(|h| format!("{} in '{}'", h.spec, h.state))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut lines = Vec::new();
    let failing = list("liveness");
    if !failing.is_empty() {
        lines.push(format!("also hot at this point: {failing}"));
    }
    let context = list("hot");
    if !context.is_empty() {
        lines.push(format!("also hot at this point (context; these specs have not failed): {context}"));
    }
    lines
}

/// One test case's result line, and for a bug its report and how to replay it.
pub(in crate::cli) fn print_case(host: &mut dyn Host, case: &check::CaseOutcome, cfg: &CheckConfig, boundary: bool) {
    for handoff in case.auto_handoffs.iter().filter(|_| cfg.verbose) {
        host.out_line(&format!(
            "      auto {}: worker {} switched from exhaustive search to sampling after {} schedules ({} exhaustive choices): {}",
            case.test_case, handoff.worker, handoff.after_schedules, handoff.choices, handoff.reason
        ));
    }
    let s = case.schedules;
    // a bug ends an exhaustive search early, so its extent only matters without one
    let schedules = match case.exhaustive_finished.filter(|_| case.bugs == 0) {
        Some(true) => format!("all {} schedule{}", s, plural(s)),
        Some(false) => format!("{} schedule{}, search incomplete", s, plural(s)),
        None => format!("{} schedule{}", s, plural(s)),
    };
    let workers = if case.workers > 1 { format!(", {} workers", case.workers) } else { String::new() };
    let limit = format!(
        "{}{}",
        workers,
        if incomplete_reason(case) == Some("timed_model_sampled") {
            if case.timed_out {
                ", timed model: sampled, time limit reached"
            } else {
                ", timed model: sampled"
            }
        } else if case.memory_stop.is_some() {
            ", memory limit reached"
        } else if case.timed_out {
            ", time limit reached"
        } else if case.plateaued {
            ", stopped: coverage stopped growing"
        } else {
            ""
        }
    );
    let time = duration(case.elapsed);
    if case.bugs == 0 {
        let verdict = verdict(case);
        let what = match verdict {
            Verdict::Proved => "proved",
            Verdict::Settled => "no bug found, search settled",
            Verdict::Finding => "no bug found, still finding new behavior",
            Verdict::Unjudged if case.memory_stop.is_some() => "no bug found, memory limit reached",
            Verdict::Unjudged => "no bug found",
        };
        // how much was searched is detail: the verdict says what it amounts to
        let detail = if cfg.verbose { format!(", {} schedule{}{}", s, plural(s), workers) } else { String::new() };
        host.out_line(&format!("ok    {} ({}{}, {})", case.test_case, what, detail, time));
        if let Some(m) = case.memory_stop {
            host.out_line(&format!("      hint: {}", memory_hint(m)));
        }
        if !boundary && little_to_explore(case) {
            host.out_line(
                "      note: every schedule reaches nearly the same states, so this test checks little more than one run would",
            );
            host.out_line(
                "      hint: give the model something to get wrong: a second client, concurrent requests, a failure, or a choice",
            );
        }
        if cfg.verbose && !case.collection_sizes.is_empty() {
            let fields = case
                .collection_sizes
                .iter()
                .take(5)
                .map(|c| format!("{}.{} {}..{}", c.machine, c.field, c.min, c.max))
                .collect::<Vec<_>>()
                .join(", ");
            host.out_line(&format!("      Sampled collection sizes (elements): {fields}"));
            host.out_line("      note: ranges combine sampled states and instances; they do not prove growth or identify the cause of search size (--json lists fields and locations)");
        }
        if verdict != Verdict::Proved && incomplete_reason(case) == Some("timed_model_sampled") {
            host.out_line("      note: timed model: clocks only grow, so a loop never returns to an explored state; the search sampled schedules");
        }
        match verdict {
            Verdict::Settled => {
                let busiest = case.branching.as_ref().map(|(_, machines)| busiest_machines(machines)).unwrap_or_default();
                host.out_line(&format!(
                    "      note: not proved: too many schedules to check them all in this time{}",
                    if busiest.is_empty() { String::new() } else { format!("; most choices come from {}", busiest) }
                ));
                host.out_line("      hint: for a proof, shrink the test: fewer machines, messages, faults or choices (see `snappea guide modeling`)");
            }
            Verdict::Finding => match longer_timeout(case.elapsed) {
                Some(timeout) => host.out_line(&format!(
                    "      hint: new behavior was still turning up when the search stopped; run longer, e.g. --timeout {}",
                    timeout
                )),
                None => {
                    host.out_line(&format!(
                        "      note: new behavior was still turning up after {}: the test is too large to settle",
                        time
                    ));
                    host.out_line("      hint: shrink or split the test so each part settles: fewer machines, messages, faults or choices (see `snappea guide modeling`)");
                }
            },
            Verdict::Proved | Verdict::Unjudged => {}
        }
        if cfg.verbose {
            if let Some(states) = mostly_revisits(case).filter(|_| verdict != Verdict::Proved) {
                host.out_line(&format!(
                    "      note: {} schedules reached only {} distinct states: most run independent steps in other orders to the same states",
                    case.schedules, states
                ));
            }
            let branching = case.branching.as_ref().filter(|(points, _)| *points >= 1.0);
            if let Some((points, machines)) = branching {
                host.out_line(&format!("      Branching per schedule: {}", branching_text(*points, machines)));
            }
        }
        return;
    }
    host.write(Stream::Out, "FAIL", Some(Color::Red));
    // the search went on after the first bug: say when that bug was found
    let found = match case.first_bug_schedule.filter(|&n| case.collected_bugs && n > 0) {
        Some(n) => format!("{} schedule{}; {} searched{}", n, plural(n), s, workers),
        None => format!("{}{}", schedules, limit),
    };
    host.out_line(&format!("  {} (bug found after {}, {})", case.test_case, found, time));
    if let Some(report) = &case.bug_report {
        for l in report.lines() {
            host.out_line(&format!("      {}", l));
        }
    }
    for l in hot_spec_lines(case.bug_identity.as_ref(), &case.hot_specs) {
        host.out_line(&format!("      {}", l));
    }
    if let Some(schedule) = &case.schedule_file {
        host.out_line(&format!("      Replay: {}", replay_command(schedule, cfg)));
    }
    if case.more_bugs.is_empty() {
        return;
    }
    let n = case.more_bugs.len();
    host.out_line(&format!("      {} more distinct failure{} in this test case:", n, plural(n)));
    for m in &case.more_bugs {
        host.out_line(&format!("      - {}", m.bug.report.lines().next().unwrap_or_default()));
        host.out_line(&format!("        Replay: {}", replay_command(&m.schedule_file, cfg)));
    }
}

/// The `snappea replay` command for `schedule`, with the sources the check was given.
pub(in crate::cli) fn replay_command(schedule: &str, cfg: &CheckConfig) -> String {
    let mut words = vec!["snappea".to_string(), "replay".into(), schedule.to_string()];
    words.extend(cfg.input.pproj.iter().cloned());
    words.extend(cfg.input.pfiles.iter().cloned());
    if cfg.ignore_foreign_files {
        words.push("--ignore-foreign-files".into());
    }
    words.iter().map(|w| shell_quote(w)).collect::<Vec<_>>().join(" ")
}

/// Quotes `w` for a POSIX shell when it needs it.
pub(in crate::cli) fn shell_quote(w: &str) -> String {
    if !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@+,".contains(c)) {
        w.to_string()
    } else {
        format!("'{}'", w.replace('\'', "'\\''"))
    }
}

pub(in crate::cli) fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Seconds, to the millisecond (for JSON).
pub(in crate::cli) fn round_ms(secs: f64) -> f64 {
    (secs * 1000.0).round() / 1000.0
}

/// Seconds for people: `0.18s`, `12.3s`, `2m05s`, `1h02m`.
pub(in crate::cli) fn duration(secs: f64) -> String {
    let secs = secs.max(0.0);
    if secs < 9.995 {
        format!("{:.2}s", secs)
    } else if secs < 59.95 {
        format!("{:.1}s", secs)
    } else if secs < 3599.5 {
        let s = secs.round() as u64;
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        let m = (secs / 60.0).round() as u64;
        format!("{}h{:02}m", m / 60, m % 60)
    }
}

/// `, peak memory 120 MB`, or nothing when the host does not know.
pub(in crate::cli) fn memory(host: &dyn Host) -> String {
    let mb = host.max_memory_gb() * 1024.0;
    if mb > 0.0 {
        format!(", peak memory {:.0} MB", mb.max(1.0))
    } else {
        String::new()
    }
}

/// One line naming what fails a check whose test cases passed, printed last: agents
/// filter long reports with `tail` and `grep`, and the summary above names only the
/// first failure. A failed test case already ends the report with its bug.
pub(in crate::cli) fn failure_recap(code: i32, result: &Value, mutation_failed: bool) -> Option<String> {
    if code == OK || result["summary"]["failed"].as_u64() != Some(0) {
        return None;
    }
    // `what` at the first few distinct `sites`, of `total`
    let part = |what: &str, mut sites: Vec<String>, total: usize| {
        sites.dedup();
        let shown: Vec<_> = sites.iter().take(3).cloned().collect();
        let more = sites.len().max(total).saturating_sub(shown.len());
        let more = if more > 0 { format!(" and {more} more") } else { String::new() };
        format!("{what} at {}{more}", shown.join(", "))
    };
    let at = |v: &Value| format!("{}:{}", v["file"].as_str().unwrap_or_default(), v["line"]);
    let mut parts = Vec::new();
    let mut rules: Vec<(&str, Vec<String>)> = Vec::new();
    for f in result["lint"]["findings"].as_array().into_iter().flatten().filter(|f| f["severity"] == "error") {
        let rule = f["rule"].as_str().unwrap_or_default();
        match rules.iter_mut().find(|(r, _)| *r == rule) {
            Some((_, sites)) => sites.push(at(f)),
            None => rules.push((rule, vec![at(f)])),
        }
    }
    parts.extend(rules.into_iter().map(|(rule, sites)| part(&format!("lint {rule}"), sites, 0)));
    let summary = &result["summary"];
    if summary["coverage_failed"] == true {
        let sites = summary["uncovered"].as_array().into_iter().flatten().map(at).collect();
        parts.push(part("required coverage", sites, summary["coverage_gaps"].as_u64().unwrap_or(0) as usize));
    }
    if mutation_failed {
        let sites: Vec<String> = result["mutation"]["undetected"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|m| std::iter::once(m).chain(m["undetected_inside"].as_array().into_iter().flatten()))
            .map(at)
            .collect();
        parts.push(if sites.is_empty() {
            "mutants not tested in time".to_string()
        } else {
            part("undetected mutants", sites, 0)
        });
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "error: check failed (exit {code}): {}. Fix or suppress each finding above, then check again.",
        parts.join("; ")
    ))
}
