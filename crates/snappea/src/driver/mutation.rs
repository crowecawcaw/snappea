//! Mutation testing for `snappea check`: runs the passing test cases against mutants of
//! the model (see [`crate::checker::mutation`]) and reports the mutants no test case
//! detects.
//!
//! A mutant runs only the test cases that ran the code it changes, smallest first: the
//! test cases whose unchanged search was complete in the fewest schedules, then the rest.
//! The schedules are explored in rounds: each round gives every mutant still undetected
//! one turn of [`TURN`] schedules on the next of its test cases, so when the time limit
//! stops the run, every mutant has had about the same share. The first phase gives each
//! mutant one turn per test case; the second gives the undetected ones more, up to a
//! [`budget`] per test case learned from how many schedules the first phase's detections
//! needed. A test case whose unchanged search was complete in at most [`EXACT`] schedules
//! gets one turn of up to [`SCHEDULES`]: enough for the mutant's search to complete too,
//! which makes its verdict there exact. With several workers there is no wait at
//! the end of a round: a free worker takes the next turn of the mutant with the fewest
//! turns, so one slow turn holds up only its own mutant. A mutant's turns depend only on its own
//! results and a fixed seed, so its verdict for a given number of turns does not depend
//! on the machine; the time limit only decides how many turns it gets.
//!
//! A mutant inside a coarse one (see [`mutation::parents`]) gets only its first, short
//! turns until a test case detects the coarse mutant, and the rest of its budget only
//! after every other mutant had its own. When no test case detects the coarse mutant, no
//! test case notices the code it removes at all: that is the gap to close first, and
//! the undetected mutants inside it are reported with it ([`Verdict::Subsumed`]). So when
//! the time limit stops a run, what it cut is the confirmation of mutants inside a gap
//! already found, which is most of what mutation testing costs; given the time, every
//! mutant gets the same budget as without coarse mutants.
//!
//! A mutant that keeps the model from finishing, so that nearly every schedule runs to
//! the step bound while the unchanged model's schedules mostly end, is stopped after a
//! few schedules (see [`runaway`]). It counts as detected, by the step bound: a change
//! that breaks the model this badly is not the gap mutation testing looks for, and
//! running it to the end is what makes mutation testing slow.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::checker::code_coverage::Hits;
use crate::checker::engine::{Checker, CheckerOptions, Output};
use crate::checker::mutation::{self, Mutant};
use crate::checker::strategy::StrategyKind;
use crate::driver::check::{engine_options, CheckConfig};
use crate::driver::progress::{Phase, Tracker};
use crate::host::{Host, Job};
use crate::sema::{Program, TestCase};
use crate::syntax::Span;

/// Most schedules a mutant gets on each test case that runs its code; the learned budget
/// (see [`budget`]) is often less.
pub const SCHEDULES: usize = 1000;
/// Schedules of the largest unchanged search, complete, after which a mutant's turn on the
/// test case runs to completion (up to [`SCHEDULES`]) instead of in short turns.
pub const EXACT: usize = 1000;
/// Most schedules of one turn: one test case, for one mutant, in one round.
pub const TURN: usize = 100;
/// Schedules of a mutant's first turn on each test case. Most mutants a test case detects,
/// it detects within a few schedules, so short first turns find the test case that
/// detects them sooner than long turns on the others.
pub const FIRST_TURN: usize = 10;

/// Schedules of a mutant's turn on a test case it ran `visit` turns before:
/// [`FIRST_TURN`], three times as many each visit, at most [`TURN`].
pub fn turn_length(visit: u32) -> usize {
    (FIRST_TURN * 3usize.pow(visit.min(4))).min(TURN)
}
/// Schedules of a turn before a mutant can be stopped as a runaway (see [`runaway`]).
pub const RUNAWAY_SAMPLE: usize = 10;

/// Whether a mutant whose `hits` of `schedules` schedules stopped at the step bound, the
/// first `leading` of them in a row, keeps the model from finishing, while at most 40% of
/// the unchanged model's schedules (`baseline`) did: at least 90% of at least
/// [`RUNAWAY_SAMPLE`] schedules at the bound, or sooner, enough leading ones in a row that
/// the unchanged model would do it less than 1% of the time (at least 2). A model whose
/// schedules mostly run to the bound anyway (a server that never stops) has no runaways.
pub fn runaway(hits: usize, schedules: usize, leading: usize, baseline: f64) -> bool {
    if baseline > 0.4 {
        return false;
    }
    let row = leading >= 2 && baseline.powi(leading as i32) < 0.01;
    row || (schedules >= RUNAWAY_SAMPLE && hits * 10 >= schedules * 9)
}

/// How many times the longest schedule of the unchanged model that ended a mutant's
/// schedules may run (see [`step_cap`]).
pub const STEP_FACTOR: usize = 10;

/// The step bound for the mutants of a test case whose unchanged schedules that ended took
/// at most `longest` steps: [`STEP_FACTOR`] times that, but no more than `bound`, and none
/// (`bound`) when no schedule ended. A mutant that runs far longer than the model ever
/// does is stopped there instead of at the step bound, which can be thousands of times
/// longer.
pub fn step_cap(longest: Option<usize>, bound: usize) -> usize {
    longest.map_or(bound, |n| (n.max(1) * STEP_FACTOR).min(bound))
}

/// The work bound per step (see [`CheckerOptions::max_work_per_step`]; 0 is none) for the
/// mutants of a test case whose unchanged schedules did at most `peak` work in one step:
/// [`STEP_FACTOR`] times that (at least 1000), but no more than `bound`. A mutant that
/// loops inside one step, such as a `while` whose counter no longer moves, fails there
/// instead of running to the default bound, which can take seconds a schedule.
pub fn work_cap(peak: u64, bound: u64) -> u64 {
    let cap = peak.max(1000) * STEP_FACTOR as u64;
    if bound == 0 {
        cap
    } else {
        cap.min(bound)
    }
}

/// Detected mutants needed before the budget is learned from them (see [`budget`]).
pub const BUDGET_SAMPLE: usize = 20;
/// How many times the schedules most detected mutants needed an undetected mutant gets
/// (see [`budget`]).
pub const BUDGET_FACTOR: usize = 3;

/// Schedules an undetected mutant gets on each test case that runs its code, learned from
/// `needed`: the schedules each mutant detected in its first turns ran before a test case
/// failed. It is
/// [`BUDGET_FACTOR`] times what 95% of them needed, at least [`TURN`] and at most
/// [`SCHEDULES`]; with fewer than [`BUDGET_SAMPLE`] it is [`SCHEDULES`]. Most mutants a test case detects
/// at all, it detects early; the rest of the schedules mostly confirm survivors.
pub fn budget(needed: &mut [usize]) -> usize {
    if needed.len() < BUDGET_SAMPLE {
        return SCHEDULES;
    }
    needed.sort_unstable();
    let p95 = needed[(needed.len() * 95).div_ceil(100) - 1];
    ((p95 * BUDGET_FACTOR).div_ceil(FIRST_TURN) * FIRST_TURN).clamp(TURN, SCHEDULES)
}

/// What the test cases found for one mutant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A test case failed with the change, or (`step_bound`) the change made nearly every
    /// schedule run to the step bound (see [`runaway`]).
    Detected { test: String, step_bound: bool },
    /// Every test case that ran the changed code passed.
    Undetected {
        tests: Vec<String>,
        schedules: usize,
        /// Every test case explored all of its schedules.
        search_complete: bool,
        /// The time limit stopped it before a test case whose search was incomplete got
        /// its budget.
        short: bool,
        /// The change would never have changed a value in the test cases' runs of the
        /// unchanged model (see [`Mutant::changes`]), so it was not run: no test case
        /// reaches a state where it matters.
        unchanged: bool,
    },
    /// The time limit came before its first turn.
    NotTested,
    /// A `// snappea-lint: allow mutation` comment excludes it.
    Suppressed,
    /// No test case can detect it, as the code shows (see [`Mutant::equivalent`]): not run.
    Equivalent { why: String },
    /// No test case detected it, and it empties the handler of an event every test case
    /// that ran it sends only optionally: the history where the event was not sent (see
    /// [`crate::checker::equivalent::FaultModel`]). Not counted as undetected.
    LikelyEquivalent { why: String, tests: Vec<String>, schedules: usize },
    /// No test case detected it, nor the coarse mutant `by` around it (an index into
    /// [`Report::mutants`]), which is reported in its place: until a test case notices
    /// that code removed, the coarse mutant is the gap to close first.
    Subsumed { by: usize, schedules: usize, search_complete: bool, short: bool, unchanged: bool },
}

/// The mutants of a suite and their verdicts.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub mutants: Vec<Mutant>,
    pub verdicts: Vec<Verdict>,
    pub seconds: f64,
    pub time_limit_reached: bool,
    /// Schedules each undetected mutant could get on each test case (see [`budget`]).
    pub budget: usize,
    /// Undetected mutants got up to [`SCHEDULES`] on each test case (`check mutation`), not
    /// only the learned budget.
    pub deepened: bool,
}

/// An undetected mutant: the test cases it ran, its schedules, whether their searches
/// were complete, and whether it never changed a value (see [`Verdict::Undetected`]).
pub type Undetected<'a> = (&'a Mutant, &'a [String], usize, bool, bool);

impl Report {
    fn count(&self, f: impl Fn(&Verdict) -> bool) -> usize {
        self.verdicts.iter().filter(|v| f(v)).count()
    }

    pub fn detected(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Detected { .. }))
    }

    /// Mutants detected only because their schedules ran to the step bound.
    pub fn step_bound(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Detected { step_bound: true, .. }))
    }

    pub fn not_tested(&self) -> usize {
        self.count(|v| *v == Verdict::NotTested)
    }

    pub fn suppressed(&self) -> usize {
        self.count(|v| *v == Verdict::Suppressed)
    }

    /// Mutants not run because no test case can detect them (see [`Verdict::Equivalent`]).
    pub fn equivalent(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Equivalent { .. }))
    }

    /// Undetected mutants that are likely equivalent (see [`Verdict::LikelyEquivalent`]).
    pub fn likely_equivalent(&self) -> usize {
        self.count(|v| matches!(v, Verdict::LikelyEquivalent { .. }))
    }

    /// Undetected mutants reported with the undetected coarse mutant around them.
    pub fn subsumed(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Subsumed { .. }))
    }

    /// How many mutants are inside undetected mutant `i` (see [`Verdict::Subsumed`]).
    fn inside(&self, i: usize) -> usize {
        self.count(|v| matches!(v, Verdict::Subsumed { by, .. } if *by == i))
    }

    /// Mutants that ran.
    pub fn tested(&self) -> usize {
        self.mutants.len() - self.not_tested() - self.suppressed() - self.equivalent()
    }

    /// Undetected mutants the time limit left with fewer schedules than the budget and an
    /// incomplete search.
    pub fn short(&self) -> usize {
        self.count(|v| match v {
            Verdict::Undetected { unchanged: true, .. } | Verdict::Subsumed { unchanged: true, .. } => false,
            Verdict::Undetected { short, .. } | Verdict::Subsumed { short, .. } => *short,
            _ => false,
        })
    }

    /// Every undetected mutant ran, changed a value, and had an incomplete search, so a
    /// deeper search than the quick check's may still detect it (see [`Report::deepened`]).
    pub fn deeper_search_may_detect(&self) -> bool {
        let mut any = false;
        let all = self.mutants.iter().zip(&self.verdicts).all(|(m, v)| match v {
            Verdict::Undetected { search_complete, unchanged, .. }
            | Verdict::Subsumed { search_complete, unchanged, .. } => {
                any = true;
                !search_complete && !unchanged && m.unchecked.is_none()
            }
            _ => true,
        });
        any && all && !self.deepened
    }

    /// The mutants no test case detected, by file and line.
    pub fn undetected(&self) -> Vec<Undetected<'_>> {
        self.undetected_indexed().into_iter().map(|(_, u)| u).collect()
    }

    /// [`Report::undetected`], with each mutant's index.
    fn undetected_indexed(&self) -> Vec<(usize, Undetected<'_>)> {
        let mut out: Vec<_> = self
            .mutants
            .iter()
            .zip(&self.verdicts)
            .enumerate()
            .filter_map(|(i, (m, v))| match v {
                Verdict::Undetected { tests, schedules, search_complete, unchanged, .. } => {
                    Some((i, (m, tests.as_slice(), *schedules, *search_complete, *unchanged)))
                }
                _ => None,
            })
            .collect();
        out.sort_by(|(_, a), (_, b)| (&a.0.file, a.0.line).cmp(&(&b.0.file, b.0.line)));
        out
    }

    /// The console lines: the counts, then the first `shown` undetected mutants; with
    /// `inner`, each followed by the undetected mutants inside it.
    pub fn lines(&self, shown: usize, inner: bool) -> Vec<String> {
        let tested = self.tested();
        let mut skipped = String::new();
        if self.suppressed() > 0 {
            skipped.push_str(&format!(", {} suppressed", self.suppressed()));
        }
        if self.equivalent() > 0 {
            skipped.push_str(&format!(", {} equivalent (not run)", self.equivalent()));
        }
        let mut out = vec![format!(
            "Mutation testing: {} of {} mutant{} detected{} ({:.1}s)",
            self.detected(),
            tested,
            plural(tested),
            skipped,
            self.seconds
        )];
        let bound = self.step_bound();
        if bound > 0 {
            out.push(format!(
                "{} of them because nearly every schedule ran to the step bound: the change made the model loop without finishing.",
                bound
            ));
        }
        let undetected = self.undetected_indexed();
        if !undetected.is_empty() {
            out.push("Not detected by any test case (every test case still passes with the change):".into());
            let location = |m: &Mutant| format!("{}:{}", m.file, m.line);
            let list = &undetected[..undetected.len().min(shown)];
            let width = list.iter().map(|(_, (m, ..))| location(m).len()).max().unwrap_or(0);
            for (i, (m, _, schedules, complete, unchanged)) in list {
                // the time limit left it a lighter check
                let short = if let Some(v) = &m.unchecked {
                    format!(" (only writes '{}', which no check depends on: see the unchecked-state lint)", v)
                } else if *unchanged {
                    format!(" (never changes a value in the test cases: {})", m.unchanged)
                } else if matches!(self.verdicts[*i], Verdict::Undetected { short: true, .. }) && !complete {
                    format!(" (only {} schedules)", schedules)
                } else {
                    String::new()
                };
                let inside = match self.inside(*i) {
                    0 => String::new(),
                    n if inner => format!(" (and {} more undetected inside it:)", n),
                    n => format!(" (and {} more undetected inside it; -v lists them)", n),
                };
                out.push(format!(
                    "  {:width$}  {}: {}{}{}",
                    location(m),
                    m.context,
                    m.description,
                    inside,
                    short,
                    width = width
                ));
                if inner {
                    for (c, _) in self
                        .mutants
                        .iter()
                        .zip(&self.verdicts)
                        .filter(|(_, v)| matches!(v, Verdict::Subsumed { by, .. } if by == i))
                    {
                        out.push(format!("      {}  {}", location(c), c.description));
                    }
                }
            }
            if undetected.len() > list.len() {
                out.push(format!("  ... {} more (--json lists all)", undetected.len() - list.len()));
            }
            let fault_like = undetected.iter().filter(|(_, (m, ..))| m.under_choice).count();
            if fault_like > 0 {
                out.push(format!(
                    "hint: {} of these change{} code under a `$` or `choose()` condition, which looks like fault injection. If the machine stands in for the environment (a network, a remote service, a timer), mark it `// snappea: driver` so it is not mutated.",
                    fault_like,
                    if fault_like == 1 { "s" } else { "" }
                ));
            }
            if self.deeper_search_may_detect() {
                out.push(format!(
                    "hint: none of these searches was complete. `snappea check mutation` gives each mutant up to {} schedules per test case: run it before adding assertions or suppressions.",
                    SCHEDULES
                ));
            }
        }
        let likely: Vec<(&Mutant, &String)> = self
            .mutants
            .iter()
            .zip(&self.verdicts)
            .filter_map(|(m, v)| match v {
                Verdict::LikelyEquivalent { why, .. } => Some((m, why)),
                _ => None,
            })
            .collect();
        if !likely.is_empty() {
            out.push("Likely equivalent, not counted as undetected:".into());
            for (m, why) in likely.iter().take(shown) {
                out.push(format!("  {}:{}  {}: {} ({})", m.file, m.line, m.context, m.description, why));
            }
            if likely.len() > shown {
                out.push(format!("  ... {} more (--json lists all)", likely.len() - shown));
            }
        }
        if self.time_limit_reached {
            let (n, short) = (self.not_tested(), self.short());
            let mut what = Vec::new();
            if short > 0 {
                what.push(format!(
                    "{} undetected mutant{} got fewer than {} schedules",
                    short,
                    plural(short),
                    self.budget
                ));
            }
            if n > 0 {
                what.push(format!("{} mutant{} not tested", n, plural(n)));
            }
            if !what.is_empty() {
                out.push(format!(
                    "Mutation testing reached its time limit: {} (--mutation-timeout raises it).",
                    what.join(", ")
                ));
            }
        }
        out
    }

    pub fn to_json(&self) -> Value {
        json!({
            "mutants": self.tested(),
            "detected": self.detected(),
            "undetected": self.undetected_indexed().iter().map(|(i, (m, tests, schedules, complete, unchanged))| json!({
                "file": m.file,
                "line": m.line,
                "context": m.context,
                "mutation": m.change.name(),
                "change": m.description,
                "test_cases": tests,
                "schedules": schedules,
                "search_complete": complete,
                "changes_a_value": !unchanged || m.unchecked.is_some(),
                "unchecked_variable": m.unchecked,
                "undetected_inside": self.mutants.iter().zip(&self.verdicts)
                    .filter_map(|(m, v)| match v {
                        Verdict::Subsumed { by, schedules, search_complete, unchanged, .. } if by == i => Some(json!({
                            "file": m.file,
                            "line": m.line,
                            "mutation": m.change.name(),
                            "change": m.description,
                            "schedules": schedules,
                            "search_complete": search_complete,
                            "changes_a_value": !unchanged || m.unchecked.is_some(),
                            "unchecked_variable": m.unchecked,
                        })),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "likely_equivalent": self.mutants.iter().zip(&self.verdicts).filter_map(|(m, v)| match v {
                Verdict::LikelyEquivalent { why, tests, schedules } => Some(json!({
                    "file": m.file,
                    "line": m.line,
                    "context": m.context,
                    "mutation": m.change.name(),
                    "change": m.description,
                    "reason": why,
                    "test_cases": tests,
                    "schedules": schedules,
                })),
                _ => None,
            }).collect::<Vec<_>>(),
            "equivalent": self.mutants.iter().zip(&self.verdicts).filter_map(|(m, v)| match v {
                Verdict::Equivalent { why } => Some(json!({
                    "file": m.file,
                    "line": m.line,
                    "context": m.context,
                    "mutation": m.change.name(),
                    "change": m.description,
                    "reason": why,
                })),
                _ => None,
            }).collect::<Vec<_>>(),
            "detected_by_step_bound": self.step_bound(),
            "suppressed": self.suppressed(),
            "undetected_inside_undetected": self.subsumed(),
            "not_tested": self.not_tested(),
            "time_limit_reached": self.time_limit_reached,
            "schedules_per_mutant": self.budget,
            "deeper_search_may_detect": self.deeper_search_may_detect(),
            "seconds": (self.seconds * 1000.0).round() / 1000.0,
        })
    }
}

/// Runs the passing test cases `passed` (each with the code it ran) against the mutants
/// of the code they ran, for at most `time_limit` seconds. `suppressed` lists the code
/// suppression comments exclude, with the ID each allows (see
/// [`snappea_lint::mutation_suppressions`]). With `deepen`, once every mutant had its
/// [`budget`], the undetected ones get more turns, up to [`SCHEDULES`], until the time
/// limit: a deeper search for a CI gate, where a quick check stops at the budget.
#[allow(clippy::too_many_arguments)]
pub fn run(
    host: &mut dyn Host,
    prog: &Program,
    cfg: &CheckConfig,
    passed: &[(TestCase, Hits)],
    suppressed: &[(Span, String)],
    drivers: &[bool],
    time_limit: f64,
    deepen: bool,
) -> Report {
    let start = host.now();
    let deadline = start + time_limit;
    let tests: Vec<TestCase> = passed.iter().map(|(tc, _)| tc.clone()).collect();
    let probes = crate::checker::compile::compile_program_with_probes(prog).probes;
    let funs = mutation::mutable_funs(prog, &tests, drivers);
    // only mutants of code a passing test case ran: coverage reports the rest
    let mutants: Vec<Mutant> = mutation::mutants(prog, &funs, &probes)
        .into_iter()
        .filter(|m| passed.iter().any(|(_, h)| h.ran(m.probe)))
        .collect();
    let faults = crate::checker::equivalent::FaultModel::new(prog, &tests);
    let mut states: Vec<State> = mutants
        .iter()
        .map(|m| {
            let allowed = |id: &str| id == "mutation" || id.strip_prefix("mutation:") == Some(m.change.name());
            let inside = |s: &Span| s.file == m.span.file && s.start <= m.span.start && m.span.end <= s.end;
            // only the test cases where the change would change a value can notice it
            let covering: Vec<usize> = (0..passed.len()).filter(|t| m.changed(&passed[*t].1)).collect();
            let reached = match covering.is_empty() {
                true => (0..passed.len()).filter(|t| passed[*t].1.ran(m.probe)).collect(),
                false => Vec::new(),
            };
            let exact = vec![false; covering.len()];
            let suppressed = suppressed.iter().any(|(s, id)| inside(s) && allowed(id));
            State {
                reached,
                suppressed,
                equivalent: m.equivalent.clone().filter(|_| !suppressed),
                fault: faults.judge(prog, m, &covering),
                ..State::new(covering, exact)
            }
        })
        .collect();
    // each mutant waits for the nearest coarse mutant around it that is not suppressed
    let parents = mutation::parents(&mutants);
    let mut children = vec![Vec::new(); mutants.len()];
    let mut parent = vec![None; mutants.len()];
    for i in 0..mutants.len() {
        let mut p = parents[i];
        // the mutants inside one that is not judged by its own detection run on their own
        let independent = |s: &State| s.suppressed || s.equivalent.is_some() || s.fault.is_some();
        while let Some(q) = p.filter(|q| independent(&states[*q])) {
            p = parents[q];
        }
        if let Some(p) = p.filter(|_| !states[i].suppressed && states[i].equivalent.is_none()) {
            states[i].blocked = true;
            children[p].push(i);
            parent[i] = Some(p);
        }
    }
    // the order of each round: shuffled, the same in every run
    let mut order: Vec<usize> = (0..mutants.len()).collect();
    order.sort_by_cached_key(|&i| {
        let m = &mutants[i];
        let mut h = rustc_hash::FxHasher::default();
        (&m.file, m.line, &m.description).hash(&mut h);
        (h.finish(), i)
    });
    let opts = CheckerOptions {
        strategy: StrategyKind::Auto,
        iterations: TURN,
        json_trace: false,
        xml_trace: false,
        activity_coverage: false,
        verbose: false,
        full_exploration: false,
        ..engine_options(cfg, cfg.seed.unwrap_or(0))
    };
    // for the test cases a mutant runs: how often their schedules run to the step bound
    // without a change, and the step bound for their mutants (see [`step_cap`])
    let mut baseline = vec![1.0; passed.len()];
    let mut test_opts = vec![opts.clone(); passed.len()];
    // schedules of each test case's complete search without a change, if it completed
    let mut size: Vec<Option<usize>> = vec![None; passed.len()];
    let base = Checker::new(prog);
    for t in 0..passed.len() {
        if states.iter().any(|s| !s.suppressed && s.equivalent.is_none() && s.covering.contains(&t)) {
            let mut out = Watch::new(Box::new(Deadline { host: &*host, deadline }), 1.0);
            let Ok(res) = base.check(&passed[t].0, &opts, &mut out) else { continue };
            if res.cancelled || out.schedules == 0 {
                continue;
            }
            baseline[t] = out.hits as f64 / out.schedules as f64;
            let fair = step_cap(out.longest, opts.max_fair_steps);
            // a liveness spec may stay hot for the same share of the shorter bound
            let hot = opts.liveness_temperature_threshold.map(|n| n * fair / opts.max_fair_steps.max(1));
            let capped = CheckerOptions {
                max_work_per_step: work_cap(res.report.max_step_work, opts.max_work_per_step),
                max_machines: res.report.max_machines.max(1) * STEP_FACTOR,
                max_inbox: res.report.max_inbox.max(1) * STEP_FACTOR,
                max_fair_steps: fair,
                max_unfair_steps: step_cap(out.longest, opts.max_unfair_steps),
                liveness_temperature_threshold: hot,
                ..opts.clone()
            };
            // only if the unchanged model still passes with it, so a liveness spec is not
            // reported only because it may stay hot for less long
            let mut out = Deadline { host: &*host, deadline };
            let complete = |r: &crate::checker::engine::CheckResult| {
                (!r.cancelled && r.report.num_bugs == 0 && r.exhaustive.as_ref().is_some_and(|x| x.complete()))
                    .then(|| r.report.explored_fair_schedules + r.report.explored_unfair_schedules)
            };
            let Ok(r) = base.check(&passed[t].0, &capped, &mut out) else { continue };
            if r.cancelled || r.report.num_bugs > 0 {
                continue;
            }
            size[t] = complete(&r);
            test_opts[t] = capped;
            // whether a search longer than a turn completes, in a small share of the time
            if size[t].is_none() {
                let mut out = Deadline { host: &*host, deadline: deadline.min(host.now() + time_limit / 20.0) };
                let exact = CheckerOptions { iterations: EXACT, ..test_opts[t].clone() };
                size[t] = base.check(&passed[t].0, &exact, &mut out).ok().and_then(|r| complete(&r));
            }
        }
    }
    // smallest searches first: they are cheap, and often enough to detect a mutant
    for s in &mut states {
        s.covering.sort_by_key(|&t| (size[t].is_none(), size[t], t));
        s.exact = s.covering.iter().map(|&t| size[t].is_some_and(|n| n <= EXACT)).collect();
    }
    let threads = host.threads();
    let mut time_limit_reached = false;
    let testable = states.iter().filter(|s| !s.suppressed && s.equivalent.is_none()).count();
    let mut progress =
        cfg.progress.map(|_| Tracker::new(Phase::Mutation { detected: 0, mutants: testable }, start, Some(time_limit)));
    let mut detected = 0;
    if let Some(t) = progress.as_mut() {
        t.tick(host, |_| {});
    }
    let covering: Vec<Vec<usize>> = states.iter().map(|s| s.covering.clone()).collect();
    let job = Mutants {
        prog,
        mutants: &mutants,
        covering: &covering,
        children: &children,
        passed,
        opts: &test_opts,
        baseline: &baseline,
    };
    let pending = states.iter().filter(|s| s.next_turn(0).is_some()).count();
    let workers = threads.map_or(1, |t| cfg.workers.unwrap_or(t.available)).clamp(1, pending.max(1));
    // first one turn on each covering test case; then more turns up to a budget learned
    // from how many schedules the detected mutants needed, first for the mutants not
    // waiting for a coarse one, then for all; then (`deepen`) up to the most
    let mut standard = 0;
    for phase in 0..3 + usize::from(deepen) {
        if phase == 2 {
            for s in &mut states {
                s.blocked = false;
            }
        }
        if phase == 1 {
            // coarse mutants are detected sooner than the others: not a guide to their budget
            let mut needed: Vec<usize> =
                states.iter().zip(&mutants).filter(|(_, m)| !m.coarse).filter_map(|(s, _)| s.needed).collect();
            standard = budget(&mut needed);
        }
        let max_schedules = if phase == 3 { SCHEDULES } else { standard };
        let stopped = match threads {
            Some(threads) if workers > 1 => job.run_threads(
                host,
                threads,
                workers,
                deadline,
                max_schedules,
                &order,
                &mut states,
                &mut progress,
                &mut detected,
            ),
            _ => job.run_rounds(host, deadline, max_schedules, &order, &mut states, &mut progress, &mut detected),
        };
        if stopped {
            // deepening is optional: the limit matters only to a mutant still short of the budget
            time_limit_reached = phase < 3 || states.iter().any(|s| s.next_turn(standard).is_some());
            break;
        }
    }
    let verdicts = verdicts(&states, &parent, passed, standard);
    let report = Report {
        mutants,
        verdicts,
        seconds: host.now() - start,
        time_limit_reached,
        budget: standard,
        deepened: deepen,
    };
    if let Some(t) = progress.as_mut() {
        let (detected, tested) = (report.detected(), report.tested());
        t.finish(host, |phase| *phase = Phase::Mutation { detected, mutants: tested });
    }
    report
}

/// The verdict of each mutant, given the coarse mutant around each (`parent`): an
/// undetected mutant whose coarse mutant is undetected too is [`Verdict::Subsumed`] by
/// the outermost of the undetected coarse mutants around it.
fn verdicts(states: &[State], parent: &[Option<usize>], passed: &[(TestCase, Hits)], budget: usize) -> Vec<Verdict> {
    let own: Vec<Verdict> = states.iter().map(|s| s.verdict(passed, budget)).collect();
    let mut out = own.clone();
    for (i, v) in own.iter().enumerate() {
        let Verdict::Undetected { schedules, search_complete, short, unchanged, .. } = v else { continue };
        let mut by = None;
        let mut a = parent[i];
        while let Some(p) = a.filter(|p| matches!(own[*p], Verdict::Undetected { .. })) {
            by = Some(p);
            a = parent[p];
        }
        if let Some(by) = by {
            out[i] = Verdict::Subsumed {
                by,
                schedules: *schedules,
                search_complete: *search_complete,
                short: *short,
                unchanged: *unchanged,
            };
        }
    }
    out
}

/// Sends a progress snapshot, if one is due, with `detected` mutants detected so far.
fn report_detected(host: &mut dyn Host, progress: &mut Option<Tracker>, detected: usize) {
    if let Some(t) = progress.as_mut() {
        t.tick(host, |phase| {
            if let Phase::Mutation { detected: d, .. } = phase {
                *d = detected;
            }
        });
    }
}

/// Where a mutant is in its rounds.
#[derive(Debug, Default)]
struct State {
    suppressed: bool,
    /// Why no test case can detect it (see [`Mutant::equivalent`]): it does not run.
    equivalent: Option<String>,
    /// Why it is likely equivalent if no test case detects it (see
    /// [`Verdict::LikelyEquivalent`]).
    fault: Option<String>,
    /// It gets only its first turns until the coarse mutant around it is detected (see
    /// [`mutation::parents`]).
    blocked: bool,
    /// The test cases that ran its code, as indexes into the passing test cases.
    covering: Vec<usize>,
    /// Whether each of them explored all its schedules.
    complete: Vec<bool>,
    /// Whether each of them explored all its schedules without the change in at most
    /// [`EXACT`] schedules: its turn there runs until its search completes.
    exact: Vec<bool>,
    /// Turns taken.
    turns: usize,
    /// Schedules its turns on each of them could run.
    planned: Vec<usize>,
    /// Turns taken on each of them.
    visits: Vec<u32>,
    /// Whether its next turns on each of them run fair schedules only (see
    /// [`TurnResult::live_cycles`]).
    fair: Vec<bool>,
    /// Schedules run on each of them.
    ran_on: Vec<usize>,
    schedules: usize,
    /// The test case that detected it.
    detected: Option<usize>,
    /// It was detected by running its schedules to the step bound.
    step_bound: bool,
    /// The schedules the test case that failed ran until it did, if it ran some and its
    /// turns were not run to completion (see [`State::exact`]).
    needed: Option<usize>,
    /// Test cases that ran it.
    ran: Vec<usize>,
    /// When the change would change no value in any test case (so it does not run), the
    /// test cases that ran the code.
    reached: Vec<usize>,
}

/// One turn: which of the covering test cases (an index into [`State::covering`]), how
/// many times it ran before, and how many schedules it runs.
#[derive(Debug, Clone, Copy)]
struct Turn {
    test: usize,
    visit: u32,
    schedules: usize,
    /// Run fair schedules only (see [`TurnResult::live_cycles`]).
    fair: bool,
}

/// What a turn found.
#[derive(Debug, Clone, Copy)]
struct TurnResult {
    bug: bool,
    /// Its schedules ran to the step bound (see [`runaway`]); counts as a bug.
    step_bound: bool,
    schedules: usize,
    complete: bool,
    /// Its exhaustive search pruned a cycle while a liveness monitor was hot, without
    /// telling whether the model can repeat it forever: the mutant may loop without
    /// progress, which only fair schedules check, so its next turns there run them.
    live_cycles: bool,
}

impl State {
    /// A mutant to run on the test cases `covering`, smallest first; `exact` says which of
    /// them it runs to completion (see [`State::exact`]).
    fn new(covering: Vec<usize>, exact: Vec<bool>) -> State {
        let k = covering.len();
        State {
            covering,
            exact,
            complete: vec![false; k],
            planned: vec![0; k],
            visits: vec![0; k],
            fair: vec![false; k],
            ran_on: vec![0; k],
            ..State::default()
        }
    }

    /// The next turn, or none when the mutant is done: detected, suppressed, waiting after
    /// its first turns, out of its `budget` of schedules on every test case, or every
    /// search complete. A mutant gets at least one turn per test case, in order.
    fn next_turn(&self, budget: usize) -> Option<Turn> {
        let k = self.covering.len();
        let first = self.turns < k;
        if self.suppressed || self.equivalent.is_some() || (self.blocked && !first) || self.detected.is_some() {
            return None;
        }
        // first one turn on each test case; then round robin over those whose search is
        // not complete and that have budget left
        let test = if first {
            self.turns
        } else {
            // a test case run to completion (see [`State::exact`]) had all it gets
            let open = |p: usize| !self.complete[p] && !self.exact[p] && self.planned[p] < budget;
            (0..k).map(|d| (self.turns + d) % k).find(|&p| open(p))?
        };
        let visit = self.visits[test];
        let schedules = if self.exact[test] {
            SCHEDULES
        } else if first {
            turn_length(visit)
        } else {
            turn_length(visit).min(budget - self.planned[test])
        };
        Some(Turn { test, visit, schedules, fair: self.fair[test] })
    }

    fn record(&mut self, turn: Turn, r: TurnResult) {
        self.turns += 1;
        self.planned[turn.test] += turn.schedules;
        self.visits[turn.test] += 1;
        self.fair[turn.test] |= r.live_cycles;
        self.ran_on[turn.test] += r.schedules;
        self.schedules += r.schedules;
        let t = self.covering[turn.test];
        if !self.ran.contains(&t) {
            self.ran.push(t);
        }
        if r.bug || r.step_bound {
            self.detected = Some(t);
            self.step_bound = r.step_bound && !r.bug;
            if r.bug && r.schedules > 0 && !self.exact[turn.test] {
                self.needed = Some(self.ran_on[turn.test]);
            }
        }
        if r.complete {
            self.complete[turn.test] = true;
        }
    }

    /// Whether a test case whose search did not complete has budget left (see
    /// [`Verdict::Undetected`]).
    fn short(&self, budget: usize) -> bool {
        (0..self.covering.len()).any(|i| !self.complete[i] && self.planned[i] < budget)
    }

    fn verdict(&self, passed: &[(TestCase, Hits)], budget: usize) -> Verdict {
        if self.suppressed {
            return Verdict::Suppressed;
        }
        if let Some(why) = &self.equivalent {
            return Verdict::Equivalent { why: why.clone() };
        }
        if let Some(t) = self.detected {
            return Verdict::Detected { test: passed[t].0.name.clone(), step_bound: self.step_bound };
        }
        let names = |tests: &[usize]| {
            let mut tests = tests.to_vec();
            tests.sort();
            tests.iter().map(|t| passed[*t].0.name.clone()).collect()
        };
        if !self.reached.is_empty() {
            return Verdict::Undetected {
                tests: names(&self.reached),
                schedules: 0,
                search_complete: false,
                short: false,
                unchanged: true,
            };
        }
        if self.turns == 0 {
            return Verdict::NotTested;
        }
        if let Some(why) = &self.fault {
            return Verdict::LikelyEquivalent { why: why.clone(), tests: names(&self.ran), schedules: self.schedules };
        }
        Verdict::Undetected {
            tests: names(&self.ran),
            schedules: self.schedules,
            search_complete: self.complete.iter().all(|c| *c),
            short: self.short(budget),
            unchanged: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Mutants<'a> {
    prog: &'a Program,
    mutants: &'a [Mutant],
    /// The test cases that ran each mutant's code (see [`State::covering`]).
    covering: &'a [Vec<usize>],
    /// The mutants waiting for each one to be detected.
    children: &'a [Vec<usize>],
    passed: &'a [(TestCase, Hits)],
    /// The options of each test case's mutant runs.
    opts: &'a [CheckerOptions],
    /// Share of each test case's schedules that run to the step bound without a change.
    baseline: &'a [f64],
}

impl Mutants<'_> {
    /// Takes the turns of every mutant in rounds on this thread, in `order` each round,
    /// until each is done (see [`State::next_turn`]) or `deadline`, recording them in
    /// `states`; returns whether the deadline stopped it. Counts the mutants detected in
    /// `detected`.
    #[allow(clippy::too_many_arguments)]
    fn run_rounds(
        &self,
        host: &mut dyn Host,
        deadline: f64,
        budget: usize,
        order: &[usize],
        states: &mut [State],
        progress: &mut Option<Tracker>,
        detected: &mut usize,
    ) -> bool {
        loop {
            let round: Vec<(usize, Turn)> =
                order.iter().filter_map(|&i| states[i].next_turn(budget).map(|t| (i, t))).collect();
            if round.is_empty() {
                return false;
            }
            for (i, turn) in round {
                let mut out = Deadline { host: &*host, deadline };
                let Some(r) = self.turn(i, &turn, &mut out) else { return true };
                *detected += usize::from(r.bug || r.step_bound);
                states[i].record(turn, r);
                // they take their turns from the next round
                self.release(i, states);
                report_detected(host, progress, *detected);
            }
            if host.now() >= deadline {
                return true;
            }
        }
    }

    /// Once mutant `i` is detected, lets the mutants waiting for it run, and returns them.
    fn release(&self, i: usize, states: &mut [State]) -> Vec<usize> {
        if states[i].detected.is_none() {
            return Vec::new();
        }
        let released: Vec<usize> = self.children[i].iter().copied().filter(|&c| states[c].blocked).collect();
        for &c in &released {
            states[c].blocked = false;
        }
        released
    }

    /// Takes `turn` for mutant `i`; `None` when `out` was cancelled first.
    fn turn(&self, i: usize, turn: &Turn, out: &mut dyn Output) -> Option<TurnResult> {
        if out.cancelled() {
            return None;
        }
        let prog = mutation::apply(self.prog, &self.mutants[i]);
        let checker = Checker::new(&prog);
        let t = self.covering[i][turn.test];
        let tc = &self.passed[t].0;
        // each visit to a test case explores other schedules
        let seed = self.opts[t].seed.wrapping_add(turn.visit.wrapping_mul(7919));
        let mut opts = CheckerOptions { seed, iterations: turn.schedules, ..self.opts[t].clone() };
        if turn.fair {
            opts.strategy = StrategyKind::Feedback;
        }
        let mut watch = Watch::new(Box::new(Forward(out)), self.baseline[t]);
        let res = match checker.check(tc, &opts, &mut watch) {
            Ok(res) => res,
            // a change the checker cannot run is one a test case notices
            Err(_) => {
                return Some(TurnResult {
                    bug: true,
                    step_bound: false,
                    schedules: 0,
                    complete: false,
                    live_cycles: false,
                })
            }
        };
        let step_bound = watch.runaway;
        if res.report.num_bugs == 0 && res.cancelled && !step_bound {
            return None;
        }
        Some(TurnResult {
            bug: res.report.num_bugs > 0,
            step_bound,
            schedules: res.report.explored_fair_schedules + res.report.explored_unfair_schedules,
            complete: res.exhaustive.as_ref().is_some_and(|x| x.complete()),
            live_cycles: res.exhaustive.as_ref().is_some_and(|x| x.liveness_unverified()),
        })
    }

    /// Takes the turns of every mutant on `workers` threads, until each is done (see
    /// [`State::next_turn`]) or `deadline`, recording them in `states`; returns whether
    /// the deadline stopped it.
    /// A worker takes the waiting turn of the mutant with the fewest turns (then the first
    /// in `order`), so a slow turn holds up only its own mutant, not the other workers.
    /// Counts the mutants detected in `detected`.
    #[allow(clippy::too_many_arguments)]
    fn run_threads(
        &self,
        host: &mut dyn Host,
        threads: crate::host::Threads,
        workers: usize,
        deadline: f64,
        budget: usize,
        order: &[usize],
        states: &mut [State],
        progress: &mut Option<Tracker>,
        detected: &mut usize,
    ) -> bool {
        let mut rank = vec![0; states.len()];
        for (r, &i) in order.iter().enumerate() {
            rank[i] = r;
        }
        // waiting turns as (turns taken, rank, mutant, test, visit), smallest first
        type Waiting = std::cmp::Reverse<(usize, usize, usize, usize, u32, usize, bool)>;
        struct Queue {
            waiting: std::collections::BinaryHeap<Waiting>,
            running: usize,
            closed: bool,
        }
        let entry = |i: usize, s: &State, t: Turn| {
            std::cmp::Reverse((s.turns, rank[i], i, t.test, t.visit, t.schedules, t.fair))
        };
        let mut waiting = std::collections::BinaryHeap::new();
        for &i in order {
            if let Some(t) = states[i].next_turn(budget) {
                waiting.push(entry(i, &states[i], t));
            }
        }
        let closed = waiting.is_empty();
        let queue = (std::sync::Mutex::new(Queue { waiting, running: 0, closed }), std::sync::Condvar::new());
        let cancelled = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<(usize, Turn, TurnResult)>();
        let jobs: Vec<Job<'_>> = (0..workers)
            .map(|_| {
                let (tx, queue, cancelled) = (tx.clone(), &queue, &cancelled);
                Box::new(move || {
                    let mut out = Flag(cancelled);
                    loop {
                        let next = {
                            let mut q = queue.0.lock().unwrap();
                            loop {
                                if let Some(std::cmp::Reverse((_, _, i, test, visit, schedules, fair))) =
                                    q.waiting.pop()
                                {
                                    q.running += 1;
                                    break Some((i, Turn { test, visit, schedules, fair }));
                                }
                                if q.closed {
                                    break None;
                                }
                                q = queue.1.wait(q).unwrap();
                            }
                        };
                        let Some((i, turn)) = next else { break };
                        match self.turn(i, &turn, &mut out) {
                            Some(r) => {
                                let _ = tx.send((i, turn, r));
                            }
                            None => break,
                        }
                    }
                }) as Job<'_>
            })
            .collect();
        drop(tx);
        let mut time_limit_reached = false;
        let mut main = || loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok((i, turn, r)) => {
                    *detected += usize::from(r.bug || r.step_bound);
                    states[i].record(turn, r);
                    let mut q = queue.0.lock().unwrap();
                    q.running -= 1;
                    for j in std::iter::once(i).chain(self.release(i, states)) {
                        if let Some(t) = states[j].next_turn(budget) {
                            q.waiting.push(entry(j, &states[j], t));
                            queue.1.notify_one();
                        }
                    }
                    if q.waiting.is_empty() && q.running == 0 {
                        q.closed = true;
                        queue.1.notify_all();
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            report_detected(host, progress, *detected);
            if host.now() >= deadline && !cancelled.load(Ordering::Relaxed) {
                let mut q = queue.0.lock().unwrap();
                if !q.closed {
                    time_limit_reached = true;
                    q.closed = true;
                    cancelled.store(true, Ordering::Relaxed);
                    queue.1.notify_all();
                }
            }
        };
        (threads.run)(jobs, &mut main);
        time_limit_reached
    }
}

/// Engine output that counts the schedules that run to the step bound, and stops the
/// exploration once they show a runaway (see [`runaway`]) against `baseline`.
struct Watch<'a> {
    inner: Box<dyn Output + 'a>,
    baseline: f64,
    schedules: usize,
    hits: usize,
    runaway: bool,
    /// Steps of the longest schedule that ended.
    longest: Option<usize>,
    /// Schedules at the bound before the first that ended.
    leading: usize,
}

impl<'a> Watch<'a> {
    fn new(inner: Box<dyn Output + 'a>, baseline: f64) -> Self {
        Watch { inner, baseline, schedules: 0, hits: 0, runaway: false, longest: None, leading: 0 }
    }
}

impl Output for Watch<'_> {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn schedule_bounded(&mut self, hit: bool, steps: usize) {
        self.schedules += 1;
        self.hits += usize::from(hit);
        if !hit {
            self.longest = self.longest.max(Some(steps));
        }
        if hit && self.leading + 1 == self.schedules {
            self.leading += 1;
        }
        self.runaway |= runaway(self.hits, self.schedules, self.leading, self.baseline);
    }
    fn cancelled(&mut self) -> bool {
        self.runaway || self.inner.cancelled()
    }
}

/// Engine output that passes cancellation through from another.
struct Forward<'a>(&'a mut dyn Output);

impl Output for Forward<'_> {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.0.cancelled()
    }
}

/// Engine output that is discarded, cancelled by a flag.
struct Flag<'a>(&'a AtomicBool);

impl Output for Flag<'_> {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Engine output that is discarded, cancelled at a deadline.
struct Deadline<'h> {
    host: &'h dyn Host,
    deadline: f64,
}

impl Output for Deadline<'_> {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.host.now() >= self.deadline
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(tests: usize) -> State {
        State::new((0..tests).collect(), vec![false; tests])
    }

    fn pass_with(state: &mut State, budget: usize, complete: bool) -> Turn {
        let turn = state.next_turn(budget).unwrap();
        state.record(
            turn,
            TurnResult { bug: false, step_bound: false, schedules: turn.schedules, complete, live_cycles: false },
        );
        turn
    }

    fn pass(state: &mut State, complete: bool) -> Turn {
        pass_with(state, SCHEDULES, complete)
    }

    #[test]
    fn turns_cycle_through_the_test_cases_and_grow_until_the_budget_is_spent() {
        let mut s = state(3);
        let mut turns = Vec::new();
        while s.next_turn(SCHEDULES).is_some() {
            let t = pass(&mut s, false);
            turns.push((t.test, t.visit, t.schedules));
        }
        assert_eq!(&turns[..7], [(0, 0, 10), (1, 0, 10), (2, 0, 10), (0, 1, 30), (1, 1, 30), (2, 1, 30), (0, 2, 90)]);
        // the budget is each test case's: a large one does not starve a small one
        assert_eq!(s.schedules, 3 * SCHEDULES);
        assert_eq!(s.planned, [SCHEDULES; 3]);
        let mut s = state(20);
        while s.next_turn(TURN).is_some() {
            pass_with(&mut s, TURN, false);
        }
        assert_eq!(s.planned, [TURN; 20]);
    }

    #[test]
    fn a_hot_cycle_left_unresolved_moves_the_test_case_to_fair_schedules() {
        let mut s = state(2);
        let turn = s.next_turn(0).unwrap();
        assert!(!turn.fair);
        s.record(turn, TurnResult { bug: false, step_bound: false, schedules: 10, complete: false, live_cycles: true });
        pass_with(&mut s, 0, false);
        let turn = s.next_turn(TURN).unwrap();
        assert_eq!((turn.test, turn.fair), (0, true));
        pass_with(&mut s, TURN, false);
        assert!(!s.next_turn(TURN).unwrap().fair, "only on the test case that pruned the cycle");
    }

    #[test]
    fn a_small_complete_test_case_runs_first_and_to_completion() {
        // test case 7 was complete without the change, test case 3 was not
        let mut s = State::new(vec![7, 3], vec![true, false]);
        let turn = s.next_turn(0).unwrap();
        assert_eq!((turn.test, turn.schedules), (0, SCHEDULES));
        s.record(turn, TurnResult { bug: false, step_bound: false, schedules: 40, complete: true, live_cycles: false });
        assert_eq!(pass_with(&mut s, 0, false).schedules, FIRST_TURN);
        // more turns only on the other one
        while let Some(t) = s.next_turn(TURN) {
            assert_eq!(t.test, 1);
            pass_with(&mut s, TURN, false);
        }
        assert!(!s.short(TURN));
        // run to completion, the search did not complete: no more turns there either,
        // and the budget it learns from is the other test cases'
        let mut s = State::new(vec![7, 3], vec![true, false]);
        let turn = s.next_turn(0).unwrap();
        s.record(
            turn,
            TurnResult { bug: false, step_bound: false, schedules: SCHEDULES, complete: false, live_cycles: false },
        );
        pass_with(&mut s, 0, false);
        assert_eq!(s.next_turn(TURN).unwrap().test, 1);
        let turn = s.next_turn(TURN).unwrap();
        s.record(turn, TurnResult { bug: true, step_bound: false, schedules: 4, complete: false, live_cycles: false });
        assert_eq!(s.needed, Some(FIRST_TURN + 4));
    }

    #[test]
    fn turn_lengths_triple_up_to_a_limit() {
        assert_eq!([0, 1, 2, 3, 9].map(turn_length), [FIRST_TURN, 30, 90, TURN, TURN]);
    }

    #[test]
    fn a_runaway_needs_nearly_every_schedule_at_the_bound_and_a_model_that_ends() {
        // too few schedules to tell
        assert!(!runaway(1, 1, 1, 0.0));
        assert!(runaway(9, 10, 0, 0.4));
        assert!(!runaway(8, 10, 0, 0.0));
        // the first ones in a row: 0.2^3 < 1%, 0.4^5 > 1%
        assert!(!runaway(2, 2, 2, 0.2));
        assert!(runaway(3, 3, 3, 0.2));
        assert!(!runaway(5, 5, 5, 0.4));
        assert!(runaway(6, 6, 6, 0.4));
        assert!(runaway(2, 2, 2, 0.0));
        // the unchanged model runs to the bound often too
        assert!(!runaway(10, 10, 10, 0.5));
    }

    #[test]
    fn a_step_bound_result_ends_the_turns_as_detected() {
        let mut s = state(2);
        let turn = s.next_turn(SCHEDULES).unwrap();
        s.record(
            turn,
            TurnResult { bug: false, step_bound: true, schedules: RUNAWAY_SAMPLE, complete: false, live_cycles: false },
        );
        assert!(s.next_turn(SCHEDULES).is_none());
        assert_eq!((s.detected, s.step_bound), (Some(0), true));
        // a bug in the same turn is the stronger result
        let mut s = state(1);
        let turn = s.next_turn(SCHEDULES).unwrap();
        s.record(
            turn,
            TurnResult { bug: true, step_bound: true, schedules: RUNAWAY_SAMPLE, complete: false, live_cycles: false },
        );
        assert_eq!((s.detected, s.step_bound), (Some(0), false));
    }

    #[test]
    fn a_complete_search_or_a_bug_ends_the_turns() {
        let mut s = state(2);
        assert_eq!(pass(&mut s, true).test, 0);
        // the complete test case is skipped
        assert_eq!(pass(&mut s, false).test, 1);
        assert_eq!(pass(&mut s, false).test, 1);
        let mut s = state(2);
        pass(&mut s, true);
        pass(&mut s, true);
        assert!(s.next_turn(SCHEDULES).is_none());
        let mut s = state(2);
        let turn = s.next_turn(SCHEDULES).unwrap();
        s.record(turn, TurnResult { bug: true, step_bound: false, schedules: 3, complete: false, live_cycles: false });
        assert!(s.next_turn(SCHEDULES).is_none());
        assert_eq!(s.detected, Some(0));
    }

    #[test]
    fn mutants_stop_at_ten_times_the_longest_schedule_that_ended() {
        assert_eq!(step_cap(Some(300), 100_000), 3000);
        assert_eq!(step_cap(Some(50_000), 100_000), 100_000);
        // no schedule ended: keep the bound
        assert_eq!(step_cap(None, 10_000), 10_000);
        assert_eq!(step_cap(Some(0), 10_000), STEP_FACTOR);
    }

    #[test]
    fn mutants_may_work_ten_times_the_most_the_model_did_in_one_step() {
        assert_eq!(work_cap(5_000, 20_000_000), 50_000);
        assert_eq!(work_cap(10, 20_000_000), 10_000);
        assert_eq!(work_cap(5_000_000, 20_000_000), 20_000_000);
        assert_eq!(work_cap(5_000, 0), 50_000);
    }

    #[test]
    fn the_first_phase_gives_one_short_turn_per_test_case() {
        let mut s = state(3);
        while s.next_turn(0).is_some() {
            pass_with(&mut s, 0, false);
        }
        assert_eq!((s.turns, s.schedules), (3, 3 * FIRST_TURN));
        // the budget adds more on each test case, and the last turn stops at it
        let turn = s.next_turn(25).unwrap();
        assert_eq!(turn.schedules, 15);
        for _ in 0..3 {
            pass_with(&mut s, 25, false);
        }
        assert!(s.next_turn(25).is_none());
        assert!(!s.short(25));
        assert!(s.short(26));
    }

    #[test]
    fn the_budget_is_a_multiple_of_what_most_detected_mutants_needed() {
        // too few to learn from
        assert_eq!(budget(&mut [1; BUDGET_SAMPLE - 1]), SCHEDULES);
        // 95% need at most 3 schedules: 9, at least one full turn
        let mut needed = vec![3; 19];
        needed.push(100);
        assert_eq!(budget(&mut needed), TURN);
        let mut needed = [80; 20];
        assert_eq!(budget(&mut needed), 240);
        assert_eq!(budget(&mut [400; 20]), SCHEDULES);
    }

    #[test]
    fn a_waiting_mutant_gets_only_its_first_turns_until_released() {
        let mut s = state(2);
        s.blocked = true;
        pass(&mut s, false);
        pass(&mut s, false);
        assert!(s.next_turn(SCHEDULES).is_none());
        s.blocked = false;
        assert!(s.next_turn(SCHEDULES).is_some());
    }

    #[test]
    fn mutants_inside_an_undetected_one_are_subsumed_by_it() {
        let mut sm = crate::syntax::SourceMap::new();
        sm.add("m.p", "machine Main { start state S { } }\ntest tc [main=Main]: { Main };\n".to_string());
        let prog = crate::sema::compile(&sm, &crate::sema::Options::default()).unwrap_or_else(|_| panic!()).program;
        let passed: Vec<(TestCase, Hits)> = vec![(prog.test_cases()[0].clone(), Hits::default())];
        // 0 is coarse and undetected; 1 is inside it, 2 inside 1; 3 inside 0 is detected;
        // 4 is inside 3, so it is a gap of its own
        let mut states = vec![state(1), state(1), state(1), state(1), state(1)];
        for s in &mut states {
            pass(s, false);
        }
        states[3].detected = Some(0);
        let parent = [None, Some(0), Some(1), Some(0), Some(3)];
        let v = verdicts(&states, &parent, &passed, SCHEDULES);
        assert!(matches!(v[0], Verdict::Undetected { .. }));
        assert!(matches!(v[1], Verdict::Subsumed { by: 0, .. }));
        assert!(matches!(v[2], Verdict::Subsumed { by: 0, .. }));
        assert!(matches!(v[3], Verdict::Detected { .. }));
        assert!(matches!(v[4], Verdict::Undetected { .. }));
        // a coarse mutant the time limit left untested leaves the ones inside as they are
        let mut states = vec![state(1), state(1)];
        pass(&mut states[1], false);
        let v = verdicts(&states, &[None, Some(0)], &passed, SCHEDULES);
        assert!(matches!(v[1], Verdict::Undetected { .. }));
    }
}
