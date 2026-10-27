//! Mutation testing for `snappea check`: runs the passing test cases against mutants of
//! the model (see [`crate::checker::mutation`]) and reports the mutants no test case
//! detects.
//!
//! A mutant runs only the test cases that ran the code it changes. The schedules are
//! explored in rounds: each round gives every mutant still undetected one turn of
//! [`TURN`] schedules on the next of its test cases, so when the time limit stops the run,
//! every mutant has had about the same share. A mutant's turns depend only on its own
//! results and a fixed seed, so its verdict for a given number of turns does not depend
//! on the machine; the time limit only decides how many turns it gets.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::checker::code_coverage::Hits;
use crate::checker::engine::{Checker, CheckerOptions, Output};
use crate::checker::mutation::{self, Mutant};
use crate::checker::strategy::StrategyKind;
use crate::driver::check::{engine_options, CheckConfig};
use crate::host::{Host, Job};
use crate::sema::{Program, TestCase};
use crate::syntax::Span;

/// Schedules a mutant gets in all, shared by the test cases that run its code.
pub const SCHEDULES: usize = 1000;
/// Schedules of one turn: one test case, for one mutant, in one round.
pub const TURN: usize = 100;

/// What the test cases found for one mutant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A test case failed with the change.
    Detected { test: String },
    /// Every test case that ran the changed code passed.
    Undetected {
        tests: Vec<String>,
        schedules: usize,
        /// Every test case explored all of its schedules.
        search_complete: bool,
    },
    /// The time limit came before its first turn.
    NotTested,
    /// A `// snappea-lint: allow mutation` comment excludes it.
    Suppressed,
}

/// The mutants of a suite and their verdicts.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub mutants: Vec<Mutant>,
    pub verdicts: Vec<Verdict>,
    pub seconds: f64,
    pub time_limit_reached: bool,
}

/// An undetected mutant: the test cases it ran, its schedules, and whether their
/// searches were complete.
pub type Undetected<'a> = (&'a Mutant, &'a [String], usize, bool);

impl Report {
    fn count(&self, f: impl Fn(&Verdict) -> bool) -> usize {
        self.verdicts.iter().filter(|v| f(v)).count()
    }

    pub fn detected(&self) -> usize {
        self.count(|v| matches!(v, Verdict::Detected { .. }))
    }

    pub fn not_tested(&self) -> usize {
        self.count(|v| *v == Verdict::NotTested)
    }

    pub fn suppressed(&self) -> usize {
        self.count(|v| *v == Verdict::Suppressed)
    }

    /// Mutants that ran.
    pub fn tested(&self) -> usize {
        self.mutants.len() - self.not_tested() - self.suppressed()
    }

    /// Undetected mutants the time limit left with fewer than [`SCHEDULES`] schedules and
    /// an incomplete search.
    pub fn short(&self) -> usize {
        self.undetected().iter().filter(|(_, _, n, complete)| *n < SCHEDULES && !complete).count()
    }

    /// The mutants no test case detected, by file and line.
    pub fn undetected(&self) -> Vec<Undetected<'_>> {
        let mut out: Vec<_> = self
            .mutants
            .iter()
            .zip(&self.verdicts)
            .filter_map(|(m, v)| match v {
                Verdict::Undetected { tests, schedules, search_complete } => {
                    Some((m, tests.as_slice(), *schedules, *search_complete))
                }
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| (&a.0.file, a.0.line).cmp(&(&b.0.file, b.0.line)));
        out
    }

    /// The console lines: the counts, then the first `shown` undetected mutants.
    pub fn lines(&self, shown: usize) -> Vec<String> {
        let tested = self.tested();
        let suppressed = match self.suppressed() {
            0 => String::new(),
            n => format!(", {} suppressed", n),
        };
        let mut out = vec![format!(
            "Mutation testing: {} of {} mutant{} detected{} ({:.1}s)",
            self.detected(),
            tested,
            plural(tested),
            suppressed,
            self.seconds
        )];
        let undetected = self.undetected();
        if !undetected.is_empty() {
            out.push("Not detected by any test case (every test case still passes with the change):".into());
            let location = |m: &Mutant| format!("{}:{}", m.file, m.line);
            let list = &undetected[..undetected.len().min(shown)];
            let width = list.iter().map(|(m, ..)| location(m).len()).max().unwrap_or(0);
            for (m, _, schedules, complete) in list {
                // the time limit left it a lighter check
                let short = if *schedules < SCHEDULES && !complete {
                    format!(" (only {} schedules)", schedules)
                } else {
                    String::new()
                };
                out.push(format!("  {:width$}  {}: {}{}", location(m), m.context, m.description, short, width = width));
            }
            if undetected.len() > list.len() {
                out.push(format!("  ... {} more (--json lists all)", undetected.len() - list.len()));
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
                    SCHEDULES
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
            "undetected": self.undetected().iter().map(|(m, tests, schedules, complete)| json!({
                "file": m.file,
                "line": m.line,
                "context": m.context,
                "mutation": m.change.name(),
                "change": m.description,
                "test_cases": tests,
                "schedules": schedules,
                "search_complete": complete,
            })).collect::<Vec<_>>(),
            "suppressed": self.suppressed(),
            "not_tested": self.not_tested(),
            "time_limit_reached": self.time_limit_reached,
            "schedules_per_mutant": SCHEDULES,
            "seconds": (self.seconds * 1000.0).round() / 1000.0,
        })
    }
}

/// Runs the passing test cases `passed` (each with the code it ran) against the mutants
/// of the code they ran, for at most `time_limit` seconds. `suppressed` lists the code
/// suppression comments exclude, with the ID each allows (see
/// [`snappea_lint::mutation_suppressions`]).
pub fn run(
    host: &mut dyn Host,
    prog: &Program,
    cfg: &CheckConfig,
    passed: &[(TestCase, Hits)],
    suppressed: &[(Span, String)],
    drivers: &[bool],
    time_limit: f64,
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
    let mut states: Vec<State> = mutants
        .iter()
        .map(|m| {
            let allowed = |id: &str| id == "mutation" || id.strip_prefix("mutation:") == Some(m.change.name());
            let inside = |s: &Span| s.file == m.span.file && s.start <= m.span.start && m.span.end <= s.end;
            let covering: Vec<usize> = (0..passed.len()).filter(|t| passed[*t].1.ran(m.probe)).collect();
            State {
                suppressed: suppressed.iter().any(|(s, id)| inside(s) && allowed(id)),
                complete: vec![false; covering.len()],
                covering,
                ..State::default()
            }
        })
        .collect();
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
    let threads = host.threads();
    let mut time_limit_reached = false;
    loop {
        let round: Vec<(usize, Turn)> = order.iter().filter_map(|&i| states[i].next_turn().map(|t| (i, t))).collect();
        if round.is_empty() {
            break;
        }
        let job = Mutants { prog, mutants: &mutants, states: &states, passed, opts: &opts };
        let workers = threads.map_or(1, |t| cfg.workers.unwrap_or(t.available)).clamp(1, round.len());
        let results = match threads {
            Some(threads) if workers > 1 => job.round_threads(host, threads, workers, deadline, &round),
            _ => {
                let mut out = Deadline { host, deadline };
                let mut results = Vec::new();
                for (k, (i, turn)) in round.iter().enumerate() {
                    match job.turn(*i, turn, &mut out) {
                        Some(r) => results.push((k, r)),
                        None => break,
                    }
                }
                results
            }
        };
        let finished = results.len() == round.len();
        for (k, r) in results {
            let (i, turn) = round[k];
            states[i].record(turn, r);
        }
        if !finished || host.now() >= deadline {
            time_limit_reached = true;
            break;
        }
    }
    let verdicts = states.iter().map(|s| s.verdict(passed)).collect();
    Report { mutants, verdicts, seconds: host.now() - start, time_limit_reached }
}

/// Where a mutant is in its rounds.
#[derive(Debug, Default)]
struct State {
    suppressed: bool,
    /// The test cases that ran its code, as indexes into the passing test cases.
    covering: Vec<usize>,
    /// Whether each of them explored all its schedules.
    complete: Vec<bool>,
    /// Turns taken.
    turns: usize,
    schedules: usize,
    /// The test case that detected it.
    detected: Option<usize>,
    /// Test cases that ran it.
    ran: Vec<usize>,
}

/// One turn: which of the covering test cases (an index into [`State::covering`]), and
/// how many times it ran before.
#[derive(Debug, Clone, Copy)]
struct Turn {
    test: usize,
    visit: u32,
}

/// What a turn found.
#[derive(Debug, Clone, Copy)]
struct TurnResult {
    bug: bool,
    schedules: usize,
    complete: bool,
}

impl State {
    /// The next turn, or none when the mutant is done: detected, suppressed, out of
    /// schedules, or every search complete. A mutant gets at least one turn per test case.
    fn next_turn(&self) -> Option<Turn> {
        let k = self.covering.len();
        let turns = (SCHEDULES / TURN).max(k);
        if self.suppressed || self.detected.is_some() || self.turns >= turns || self.complete.iter().all(|c| *c) {
            return None;
        }
        // round robin over the test cases whose search is not complete
        let test = (0..k).map(|d| (self.turns + d) % k).find(|p| !self.complete[*p])?;
        Some(Turn { test, visit: (self.turns / k) as u32 })
    }

    fn record(&mut self, turn: Turn, r: TurnResult) {
        self.turns += 1;
        self.schedules += r.schedules;
        let t = self.covering[turn.test];
        if !self.ran.contains(&t) {
            self.ran.push(t);
        }
        if r.bug {
            self.detected = Some(t);
        }
        if r.complete {
            self.complete[turn.test] = true;
        }
    }

    fn verdict(&self, passed: &[(TestCase, Hits)]) -> Verdict {
        if self.suppressed {
            return Verdict::Suppressed;
        }
        if let Some(t) = self.detected {
            return Verdict::Detected { test: passed[t].0.name.clone() };
        }
        if self.turns == 0 {
            return Verdict::NotTested;
        }
        let mut ran = self.ran.clone();
        ran.sort();
        Verdict::Undetected {
            tests: ran.iter().map(|t| passed[*t].0.name.clone()).collect(),
            schedules: self.schedules,
            search_complete: self.complete.iter().all(|c| *c),
        }
    }
}

#[derive(Clone, Copy)]
struct Mutants<'a> {
    prog: &'a Program,
    mutants: &'a [Mutant],
    states: &'a [State],
    passed: &'a [(TestCase, Hits)],
    opts: &'a CheckerOptions,
}

impl Mutants<'_> {
    /// Takes `turn` for mutant `i`; `None` when `out` was cancelled first.
    fn turn(&self, i: usize, turn: &Turn, out: &mut dyn Output) -> Option<TurnResult> {
        if out.cancelled() {
            return None;
        }
        let prog = mutation::apply(self.prog, &self.mutants[i]);
        let checker = Checker::new(&prog);
        let tc = &self.passed[self.states[i].covering[turn.test]].0;
        // each visit to a test case explores other schedules
        let seed = self.opts.seed.wrapping_add(turn.visit.wrapping_mul(7919));
        let opts = CheckerOptions { seed, ..self.opts.clone() };
        let res = match checker.check(tc, &opts, out) {
            Ok(res) => res,
            // a change the checker cannot run is one a test case notices
            Err(_) => return Some(TurnResult { bug: true, schedules: 0, complete: false }),
        };
        if res.report.num_bugs == 0 && res.cancelled {
            return None;
        }
        Some(TurnResult {
            bug: res.report.num_bugs > 0,
            schedules: res.report.explored_fair_schedules + res.report.explored_unfair_schedules,
            complete: res.exhaustive.as_ref().is_some_and(|x| x.complete()),
        })
    }

    /// Takes the turns of `round` on `workers` threads, until `deadline`; returns the
    /// results by position in `round`.
    fn round_threads(
        &self,
        host: &mut dyn Host,
        threads: crate::host::Threads,
        workers: usize,
        deadline: f64,
        round: &[(usize, Turn)],
    ) -> Vec<(usize, TurnResult)> {
        let next = AtomicUsize::new(0);
        let cancelled = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<(usize, TurnResult)>();
        let jobs: Vec<Job<'_>> = (0..workers)
            .map(|_| {
                let (tx, next, cancelled) = (tx.clone(), &next, &cancelled);
                Box::new(move || {
                    let mut out = Flag(cancelled);
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some((i, turn)) = round.get(k) else { break };
                        match self.turn(*i, turn, &mut out) {
                            Some(r) => {
                                let _ = tx.send((k, r));
                            }
                            None => break,
                        }
                    }
                }) as Job<'_>
            })
            .collect();
        drop(tx);
        let mut results = Vec::new();
        let mut main = || loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(r) => results.push(r),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if host.now() >= deadline {
                cancelled.store(true, Ordering::Relaxed);
            }
        };
        (threads.run)(jobs, &mut main);
        results
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
        State { covering: (0..tests).collect(), complete: vec![false; tests], ..State::default() }
    }

    fn pass(state: &mut State, complete: bool) -> Turn {
        let turn = state.next_turn().unwrap();
        state.record(turn, TurnResult { bug: false, schedules: TURN, complete });
        turn
    }

    #[test]
    fn turns_cycle_through_the_test_cases_until_the_budget_is_spent() {
        let mut s = state(3);
        let mut turns = Vec::new();
        while s.next_turn().is_some() {
            let t = pass(&mut s, false);
            turns.push((t.test, t.visit));
        }
        assert_eq!(turns.len(), SCHEDULES / TURN);
        assert_eq!(&turns[..4], [(0, 0), (1, 0), (2, 0), (0, 1)]);
        assert_eq!(s.schedules, SCHEDULES);
        // more test cases than turns: one turn each
        let mut s = state(SCHEDULES / TURN + 2);
        while s.next_turn().is_some() {
            pass(&mut s, false);
        }
        assert_eq!(s.ran.len(), SCHEDULES / TURN + 2);
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
        assert!(s.next_turn().is_none());
        let mut s = state(2);
        let turn = s.next_turn().unwrap();
        s.record(turn, TurnResult { bug: true, schedules: 3, complete: false });
        assert!(s.next_turn().is_none());
        assert_eq!(s.detected, Some(0));
    }
}
