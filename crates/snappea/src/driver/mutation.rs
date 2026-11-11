//! Mutation testing for `snappea check`: runs the passing test cases against mutants of
//! the model (see [`crate::checker::mutation`]) and reports the mutants no test case
//! detects.
//!
//! A mutant runs only the test cases that ran the code it changes. The schedules are
//! explored in rounds: each round gives every mutant still undetected one turn of
//! [`TURN`] schedules on the next of its test cases, so when the time limit stops the run,
//! every mutant has had about the same share. The first phase gives each mutant one turn
//! per test case; the second gives the undetected ones more, up to a [`budget`] learned
//! from how many schedules the first phase's detections needed. With several workers there is no wait at
//! the end of a round: a free worker takes the next turn of the mutant with the fewest
//! turns, so one slow turn holds up only its own mutant. A mutant's turns depend only on its own
//! results and a fixed seed, so its verdict for a given number of turns does not depend
//! on the machine; the time limit only decides how many turns it gets.
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

/// Most schedules a mutant gets in all, shared by the test cases that run its code; the
/// learned budget (see [`budget`]) is often less.
pub const SCHEDULES: usize = 1000;
/// Schedules of one turn: one test case, for one mutant, in one round.
pub const TURN: usize = 100;
/// Schedules of a turn before a mutant can be stopped as a runaway (see [`runaway`]).
pub const RUNAWAY_SAMPLE: usize = 10;

/// Whether a mutant whose `hits` of `schedules` schedules stopped at the step bound keeps
/// the model from finishing: at least [`RUNAWAY_SAMPLE`] schedules, at least 90% of them at
/// the bound, while at most 40% of the unchanged model's (`baseline`) were. A model whose
/// schedules mostly run to the bound anyway (a server that never stops) has no runaways.
pub fn runaway(hits: usize, schedules: usize, baseline: f64) -> bool {
    schedules >= RUNAWAY_SAMPLE && hits * 10 >= schedules * 9 && baseline <= 0.4
}

/// Detected mutants needed before the budget is learned from them (see [`budget`]).
pub const BUDGET_SAMPLE: usize = 20;
/// How many times the schedules most detected mutants needed an undetected mutant gets
/// (see [`budget`]).
pub const BUDGET_FACTOR: usize = 3;

/// Schedules an undetected mutant gets in all, learned from `needed`: the schedules each
/// mutant detected in its first turns ran before a test case failed. It is
/// [`BUDGET_FACTOR`] times what 95% of them needed, in whole turns, at most [`SCHEDULES`];
/// with fewer than [`BUDGET_SAMPLE`] it is [`SCHEDULES`]. Most mutants a test case detects
/// at all, it detects early; the rest of the schedules mostly confirm survivors.
pub fn budget(needed: &mut [usize]) -> usize {
    if needed.len() < BUDGET_SAMPLE {
        return SCHEDULES;
    }
    needed.sort_unstable();
    let p95 = needed[(needed.len() * 95).div_ceil(100) - 1];
    (p95 * BUDGET_FACTOR).div_ceil(TURN).clamp(1, SCHEDULES / TURN) * TURN
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
    /// Schedules each undetected mutant could get (see [`budget`]).
    pub budget: usize,
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

    /// Mutants that ran.
    pub fn tested(&self) -> usize {
        self.mutants.len() - self.not_tested() - self.suppressed()
    }

    /// Undetected mutants the time limit left with fewer schedules than the budget and an
    /// incomplete search.
    pub fn short(&self) -> usize {
        self.undetected().iter().filter(|(_, _, n, complete)| *n < self.budget && !complete).count()
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
        let bound = self.step_bound();
        if bound > 0 {
            out.push(format!(
                "{} of them only by running every schedule to the step bound: no spec checks that the model makes progress.",
                bound
            ));
        }
        let undetected = self.undetected();
        if !undetected.is_empty() {
            out.push("Not detected by any test case (every test case still passes with the change):".into());
            let location = |m: &Mutant| format!("{}:{}", m.file, m.line);
            let list = &undetected[..undetected.len().min(shown)];
            let width = list.iter().map(|(m, ..)| location(m).len()).max().unwrap_or(0);
            for (m, _, schedules, complete) in list {
                // the time limit left it a lighter check
                let short = if *schedules < self.budget && !complete {
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
            "detected_by_step_bound": self.step_bound(),
            "suppressed": self.suppressed(),
            "not_tested": self.not_tested(),
            "time_limit_reached": self.time_limit_reached,
            "schedules_per_mutant": self.budget,
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
    // how often each test case's schedules run to the step bound without a change, for
    // the test cases a mutant runs
    let mut baseline = vec![1.0; passed.len()];
    let base = Checker::new(prog);
    for t in 0..passed.len() {
        if states.iter().any(|s| !s.suppressed && s.covering.contains(&t)) {
            let mut out = Watch::new(Box::new(Deadline { host: &*host, deadline }), 1.0);
            if let Ok(res) = base.check(&passed[t].0, &opts, &mut out) {
                if !res.cancelled && out.schedules > 0 {
                    baseline[t] = out.hits as f64 / out.schedules as f64;
                }
            }
        }
    }
    let threads = host.threads();
    let mut time_limit_reached = false;
    let testable = states.iter().filter(|s| !s.suppressed).count();
    let mut progress =
        cfg.progress.map(|_| Tracker::new(Phase::Mutation { detected: 0, mutants: testable }, start, Some(time_limit)));
    let mut detected = 0;
    if let Some(t) = progress.as_mut() {
        t.tick(host, |_| {});
    }
    let covering: Vec<Vec<usize>> = states.iter().map(|s| s.covering.clone()).collect();
    let job = Mutants { prog, mutants: &mutants, covering: &covering, passed, opts: &opts, baseline: &baseline };
    let pending = states.iter().filter(|s| s.next_turn(0).is_some()).count();
    let workers = threads.map_or(1, |t| cfg.workers.unwrap_or(t.available)).clamp(1, pending.max(1));
    // first one turn on each covering test case, then more turns up to a budget learned
    // from how many schedules the detected mutants needed
    let mut max_turns = 0;
    for phase in 0..2 {
        if phase == 1 {
            let mut needed: Vec<usize> = states.iter().filter_map(|s| s.needed).collect();
            max_turns = budget(&mut needed) / TURN;
        }
        time_limit_reached = match threads {
            Some(threads) if workers > 1 => job.run_threads(
                host,
                threads,
                workers,
                deadline,
                max_turns,
                &order,
                &mut states,
                &mut progress,
                &mut detected,
            ),
            _ => job.run_rounds(host, deadline, max_turns, &order, &mut states, &mut progress, &mut detected),
        };
        if time_limit_reached {
            break;
        }
    }
    let verdicts = states.iter().map(|s| s.verdict(passed)).collect();
    let report =
        Report { mutants, verdicts, seconds: host.now() - start, time_limit_reached, budget: max_turns * TURN };
    if let Some(t) = progress.as_mut() {
        let (detected, tested) = (report.detected(), report.tested());
        t.finish(host, |phase| *phase = Phase::Mutation { detected, mutants: tested });
    }
    report
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
    /// The test cases that ran its code, as indexes into the passing test cases.
    covering: Vec<usize>,
    /// Whether each of them explored all its schedules.
    complete: Vec<bool>,
    /// Turns taken.
    turns: usize,
    schedules: usize,
    /// The test case that detected it.
    detected: Option<usize>,
    /// It was detected by running its schedules to the step bound.
    step_bound: bool,
    /// The schedules it ran until a test case failed, if one did after running some.
    needed: Option<usize>,
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
    /// Its schedules ran to the step bound (see [`runaway`]); counts as a bug.
    step_bound: bool,
    schedules: usize,
    complete: bool,
}

impl State {
    /// The next turn, or none when the mutant is done: detected, suppressed, out of its
    /// `max_turns` turns, or every search complete. A mutant gets at least one turn per
    /// test case.
    fn next_turn(&self, max_turns: usize) -> Option<Turn> {
        let k = self.covering.len();
        let turns = max_turns.max(k);
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
        if r.bug || r.step_bound {
            self.detected = Some(t);
            self.step_bound = r.step_bound && !r.bug;
            if r.bug && r.schedules > 0 {
                self.needed = Some(self.schedules);
            }
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
            return Verdict::Detected { test: passed[t].0.name.clone(), step_bound: self.step_bound };
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
    /// The test cases that ran each mutant's code (see [`State::covering`]).
    covering: &'a [Vec<usize>],
    passed: &'a [(TestCase, Hits)],
    opts: &'a CheckerOptions,
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
        max_turns: usize,
        order: &[usize],
        states: &mut [State],
        progress: &mut Option<Tracker>,
        detected: &mut usize,
    ) -> bool {
        loop {
            let round: Vec<(usize, Turn)> =
                order.iter().filter_map(|&i| states[i].next_turn(max_turns).map(|t| (i, t))).collect();
            if round.is_empty() {
                return false;
            }
            for (i, turn) in round {
                let mut out = Deadline { host: &*host, deadline };
                let Some(r) = self.turn(i, &turn, &mut out) else { return true };
                *detected += usize::from(r.bug || r.step_bound);
                states[i].record(turn, r);
                report_detected(host, progress, *detected);
            }
            if host.now() >= deadline {
                return true;
            }
        }
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
        let seed = self.opts.seed.wrapping_add(turn.visit.wrapping_mul(7919));
        let opts = CheckerOptions { seed, ..self.opts.clone() };
        let mut watch = Watch::new(Box::new(Forward(out)), self.baseline[t]);
        let res = match checker.check(tc, &opts, &mut watch) {
            Ok(res) => res,
            // a change the checker cannot run is one a test case notices
            Err(_) => return Some(TurnResult { bug: true, step_bound: false, schedules: 0, complete: false }),
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
        max_turns: usize,
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
        type Waiting = std::cmp::Reverse<(usize, usize, usize, usize, u32)>;
        struct Queue {
            waiting: std::collections::BinaryHeap<Waiting>,
            running: usize,
            closed: bool,
        }
        let entry = |i: usize, s: &State, t: Turn| std::cmp::Reverse((s.turns, rank[i], i, t.test, t.visit));
        let mut waiting = std::collections::BinaryHeap::new();
        for &i in order {
            if let Some(t) = states[i].next_turn(max_turns) {
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
                                if let Some(std::cmp::Reverse((_, _, i, test, visit))) = q.waiting.pop() {
                                    q.running += 1;
                                    break Some((i, Turn { test, visit }));
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
                    if let Some(t) = states[i].next_turn(max_turns) {
                        q.waiting.push(entry(i, &states[i], t));
                        queue.1.notify_one();
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
}

impl<'a> Watch<'a> {
    fn new(inner: Box<dyn Output + 'a>, baseline: f64) -> Self {
        Watch { inner, baseline, schedules: 0, hits: 0, runaway: false }
    }
}

impl Output for Watch<'_> {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
    fn schedule_bounded(&mut self, hit: bool) {
        self.schedules += 1;
        self.hits += usize::from(hit);
        self.runaway |= runaway(self.hits, self.schedules, self.baseline);
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
        State { covering: (0..tests).collect(), complete: vec![false; tests], ..State::default() }
    }

    fn pass(state: &mut State, complete: bool) -> Turn {
        let turn = state.next_turn(SCHEDULES / TURN).unwrap();
        state.record(turn, TurnResult { bug: false, step_bound: false, schedules: TURN, complete });
        turn
    }

    #[test]
    fn turns_cycle_through_the_test_cases_until_the_budget_is_spent() {
        let mut s = state(3);
        let mut turns = Vec::new();
        while s.next_turn(SCHEDULES / TURN).is_some() {
            let t = pass(&mut s, false);
            turns.push((t.test, t.visit));
        }
        assert_eq!(turns.len(), SCHEDULES / TURN);
        assert_eq!(&turns[..4], [(0, 0), (1, 0), (2, 0), (0, 1)]);
        assert_eq!(s.schedules, SCHEDULES);
        // more test cases than turns: one turn each
        let mut s = state(SCHEDULES / TURN + 2);
        while s.next_turn(SCHEDULES / TURN).is_some() {
            pass(&mut s, false);
        }
        assert_eq!(s.ran.len(), SCHEDULES / TURN + 2);
    }

    #[test]
    fn a_runaway_needs_nearly_every_schedule_at_the_bound_and_a_model_that_ends() {
        // too few schedules to tell
        assert!(!runaway(RUNAWAY_SAMPLE - 1, RUNAWAY_SAMPLE - 1, 0.0));
        assert!(runaway(RUNAWAY_SAMPLE, RUNAWAY_SAMPLE, 0.0));
        assert!(runaway(9, 10, 0.4));
        assert!(!runaway(8, 10, 0.0));
        // the unchanged model runs to the bound often too
        assert!(!runaway(10, 10, 0.5));
    }

    #[test]
    fn a_step_bound_result_ends_the_turns_as_detected() {
        let mut s = state(2);
        let turn = s.next_turn(SCHEDULES / TURN).unwrap();
        s.record(turn, TurnResult { bug: false, step_bound: true, schedules: RUNAWAY_SAMPLE, complete: false });
        assert!(s.next_turn(SCHEDULES / TURN).is_none());
        assert_eq!((s.detected, s.step_bound), (Some(0), true));
        // a bug in the same turn is the stronger result
        let mut s = state(1);
        let turn = s.next_turn(SCHEDULES / TURN).unwrap();
        s.record(turn, TurnResult { bug: true, step_bound: true, schedules: RUNAWAY_SAMPLE, complete: false });
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
        assert!(s.next_turn(SCHEDULES / TURN).is_none());
        let mut s = state(2);
        let turn = s.next_turn(SCHEDULES / TURN).unwrap();
        s.record(turn, TurnResult { bug: true, step_bound: false, schedules: 3, complete: false });
        assert!(s.next_turn(SCHEDULES / TURN).is_none());
        assert_eq!(s.detected, Some(0));
    }

    #[test]
    fn the_first_phase_gives_one_turn_per_test_case() {
        let mut s = state(3);
        while s.next_turn(0).is_some() {
            pass(&mut s, false);
        }
        assert_eq!(s.turns, 3);
        // and the budget adds more
        assert!(s.next_turn(5).is_some());
    }

    #[test]
    fn the_budget_is_a_multiple_of_what_most_detected_mutants_needed() {
        // too few to learn from
        assert_eq!(budget(&mut [1; BUDGET_SAMPLE - 1]), SCHEDULES);
        // 95% need at most 3 schedules: 9, in one turn
        let mut needed = vec![3; 19];
        needed.push(100);
        assert_eq!(budget(&mut needed), TURN);
        let mut needed = [80; 20];
        assert_eq!(budget(&mut needed), 300);
        assert_eq!(budget(&mut [400; 20]), SCHEDULES);
    }
}
