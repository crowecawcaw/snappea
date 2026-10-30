//! The testing engine: runs a test case for a number of schedules under a strategy and
//! collects the statistics, bug reports and traces (`TestingEngine` + `TestReport`).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::sema::{Program, TestCase};

use crate::checker::compile::{compile_program, Code};
use crate::checker::coverage::Coverage;
use crate::checker::random::RandomValueGenerator;
use crate::checker::runtime::{Config, Runtime, Setup};
use crate::checker::strategy::{
    ComboStrategy, PctStrategy, ProbabilisticRandomStrategy, RandomStrategy, ReplayStrategy, ScheduleTrace,
    SharedRandom, Strategy, StrategyKind,
};

/// Options of a checking run (the subset of `CheckerConfiguration` that affects results).
#[derive(Debug, Clone)]
pub struct CheckerOptions {
    pub strategy: StrategyKind,
    /// Seed of the random value generator (`--seed`). Must be given: the checker has no
    /// source of entropy.
    pub seed: u32,
    /// Number of schedules to explore (`--schedules`); 0 = unbounded.
    pub iterations: usize,
    pub max_unfair_steps: usize,
    pub max_fair_steps: usize,
    /// Whether the user set `--max-steps` explicitly (affects the report text).
    pub user_set_max_fair_steps: bool,
    /// `None` = the upstream default, half of the fair step bound.
    pub liveness_temperature_threshold: Option<usize>,
    pub fail_on_max_steps: bool,
    /// Priority switch points for `pct`/`fairpct`, coin flips for `probabilistic`.
    pub strategy_bound: usize,
    /// Preserve upstream's zero-length feedback mutations and unfair step bound for
    /// seed compatibility. Native feedback uses productive mutations and the fair bound.
    pub upstream_feedback: bool,
    /// Preserve prefixes at increasing event counts in native feedback and auto's
    /// feedback phase. Enabled by default; ignored by upstream compatibility mode.
    pub feedback_frontier_guidance: bool,
    /// Keep exploring after the first bug (`--explore`).
    pub full_exploration: bool,
    /// The schedule to replay with [`StrategyKind::Replay`].
    pub replay: Option<ScheduleTrace>,
    /// Replaying, report the step bound as reached at the end of the schedule, so a bug
    /// found by `fail_on_max_steps` reproduces. Upstream's replays never reach the bound.
    pub replay_bound_at_end: bool,
    /// See [`Config::max_work_per_step`](crate::checker::runtime::Config::max_work_per_step).
    pub max_work_per_step: u64,
    /// Write `--test-method:<name>` into the reproducible trace (upstream does so only when
    /// a test case was named with `--testcase`).
    pub record_test_name: bool,
    /// Stream every schedule's runtime log to the output instead of keeping it in memory
    /// (`--verbose`); no readable trace is produced then.
    pub verbose: bool,
    /// Keep the runtime log of every schedule (`CheckResult::last_log` is then the last
    /// schedule's log). Off by default: exploring without logging is much faster, and the
    /// first buggy schedule is replayed to produce its log.
    pub log_every_schedule: bool,
    /// Collect activity coverage (`CheckResult::coverage`), as upstream does by default.
    pub activity_coverage: bool,
    /// Record what the test's driver does (`CheckResult::driver_facts`): per machine,
    /// whether it drives the model (see [`crate::checker::driver_facts::driver_machines`]).
    pub driver_machines: Option<Vec<bool>>,
    /// Produce the JSON trace of the first buggy schedule (by replaying it with JSON
    /// logging on, so exploration itself pays nothing).
    pub json_trace: bool,
    /// Produce the XML trace of the first buggy schedule (`--xml-trace`), also by replaying it.
    pub xml_trace: bool,
    /// `--timeline-repr`, `--timeline-kgram`, `--timeline-payload`.
    pub timeline: crate::checker::timeline::TimelineConfig,
    /// Name of the compiled project, the source upstream reports for exceptions raised by
    /// the model's own code.
    pub assembly_name: String,
    /// Latency profiles (`--latency`); see `design/time/PROPOSAL.md`.
    pub latency: Option<Arc<crate::checker::time::ProfileSet>>,
    /// The profile to use instead of the `[tests]` entry (`--latency <file>:<profile>`).
    pub latency_profile: Option<String>,
    /// `--timed`: time-aware scheduling.
    pub timed: bool,
}

impl Default for CheckerOptions {
    fn default() -> Self {
        CheckerOptions {
            strategy: StrategyKind::Random,
            seed: 0,
            iterations: 1,
            max_unfair_steps: 10000,
            max_fair_steps: 100000,
            user_set_max_fair_steps: false,
            liveness_temperature_threshold: None,
            fail_on_max_steps: false,
            strategy_bound: 0,
            upstream_feedback: false,
            feedback_frontier_guidance: true,
            full_exploration: false,
            replay: None,
            replay_bound_at_end: false,
            max_work_per_step: crate::checker::runtime::DEFAULT_MAX_WORK_PER_STEP,
            record_test_name: true,
            verbose: false,
            log_every_schedule: false,
            activity_coverage: false,
            driver_machines: None,
            json_trace: false,
            xml_trace: false,
            timeline: Default::default(),
            assembly_name: "PImplementation".into(),
            latency: None,
            latency_profile: None,
            timed: false,
        }
    }
}

impl CheckerOptions {
    pub fn effective_liveness_threshold(&self) -> usize {
        self.liveness_temperature_threshold.unwrap_or(self.max_fair_steps / 2)
    }
}

/// Accumulated statistics of a run (`TestReport`).
#[derive(Debug, Clone, Default)]
pub struct TestReport {
    pub num_bugs: usize,
    pub bug_reports: Vec<String>,
    pub explored_fair_schedules: usize,
    pub explored_unfair_schedules: usize,
    pub total_fair_steps: usize,
    pub min_fair_steps: Option<usize>,
    pub max_fair_steps: Option<usize>,
    pub max_fair_steps_hit_in_fair: usize,
    pub max_unfair_steps_hit_in_fair: usize,
    pub max_unfair_steps_hit_in_unfair: usize,
    pub timelines: BTreeSet<String>,
    /// Scenario coverage, per scenario (in attachment order).
    pub scenarios: Vec<ScenarioCoverage>,
}

/// Coverage of one scenario monitor across a run (`TestReport.Scenario*`).
#[derive(Debug, Clone, Default)]
pub struct ScenarioCoverage {
    pub name: String,
    /// Schedules that satisfied the scenario.
    pub trigger_count: usize,
    /// Distinct abstract timelines of the satisfying schedules.
    pub satisfying_timelines: BTreeSet<String>,
    /// Most distinct states reached in one schedule.
    pub max_states_reached: usize,
    pub total_states: usize,
}

/// String ordering of .NET's default comparer (`OrderBy(k => k)`, `List<string>.Sort()`) as
/// the P tool runs it: with `InvariantGlobalization`, comparisons are ordinal.
pub fn culture_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

impl TestReport {
    fn scenario(&mut self, name: &str) -> &mut ScenarioCoverage {
        if let Some(i) = self.scenarios.iter().position(|s| s.name == name) {
            return &mut self.scenarios[i];
        }
        self.scenarios.push(ScenarioCoverage { name: name.to_string(), ..Default::default() });
        self.scenarios.last_mut().unwrap()
    }

    /// Adds the statistics of another worker's schedules.
    pub fn merge(&mut self, other: TestReport) {
        self.num_bugs += other.num_bugs;
        self.bug_reports.extend(other.bug_reports);
        self.explored_fair_schedules += other.explored_fair_schedules;
        self.explored_unfair_schedules += other.explored_unfair_schedules;
        self.total_fair_steps += other.total_fair_steps;
        self.min_fair_steps = match (self.min_fair_steps, other.min_fair_steps) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.max_fair_steps = self.max_fair_steps.max(other.max_fair_steps);
        self.max_fair_steps_hit_in_fair += other.max_fair_steps_hit_in_fair;
        self.max_unfair_steps_hit_in_fair += other.max_unfair_steps_hit_in_fair;
        self.max_unfair_steps_hit_in_unfair += other.max_unfair_steps_hit_in_unfair;
        self.timelines.extend(other.timelines);
        for s in other.scenarios {
            let cov = self.scenario(&s.name);
            cov.trigger_count += s.trigger_count;
            cov.satisfying_timelines.extend(s.satisfying_timelines);
            cov.max_states_reached = cov.max_states_reached.max(s.max_states_reached);
            if s.total_states > 0 {
                cov.total_states = s.total_states;
            }
        }
    }

    /// Scenarios sorted the way upstream lists them.
    pub fn sorted_scenarios(&self) -> Vec<&ScenarioCoverage> {
        let mut v: Vec<&ScenarioCoverage> = self.scenarios.iter().collect();
        v.sort_by(|a, b| culture_order(&a.name, &b.name));
        v
    }

    /// `TestReport.GetText`.
    pub fn text(&self, opts: &CheckerOptions, prefix: &str) -> String {
        let sub = if prefix == "..." { "....." } else { prefix };
        let mut out = String::new();
        out.push_str(&format!("{} Checking statistics:", prefix));
        out.push_str(&format!("\n{} Found {} bug{}.", sub, self.num_bugs, plural(self.num_bugs)));
        out.push_str(&format!("\n{} Scheduling statistics:", prefix));
        let total = self.explored_fair_schedules + self.explored_unfair_schedules;
        out.push_str(&format!("\n{} Explored {} schedule{}", sub, total, plural(total)));
        let tl = self.timelines.len();
        out.push_str(&format!("\n{} Explored {} timeline{}", sub, tl, plural(tl)));
        if !self.scenarios.is_empty() {
            let names = self.sorted_scenarios();
            let covered: Vec<&&ScenarioCoverage> = names.iter().filter(|s| s.trigger_count > 0).collect();
            let gaps: Vec<&&ScenarioCoverage> = names.iter().filter(|s| s.trigger_count == 0).collect();
            let gap_text =
                if gaps.is_empty() { String::new() } else { format!(" ({} gap{})", gaps.len(), plural(gaps.len())) };
            out.push_str(&format!(
                "\n{} Scenario coverage: {}/{} scenarios covered{}",
                sub,
                covered.len(),
                names.len(),
                gap_text
            ));
            for s in covered {
                let u = s.satisfying_timelines.len();
                out.push_str(&format!(
                    "\n{}   [covered] {}: triggered in {} schedule{}, {} unique satisfying timeline{}",
                    sub,
                    s.name,
                    s.trigger_count,
                    plural(s.trigger_count),
                    u,
                    plural(u)
                ));
            }
            for s in gaps {
                out.push_str(&format!("\n{}   [  GAP  ] {}: not covered", sub, s.name));
                if s.total_states > 0 {
                    out.push_str(&format!(
                        " (best partial progress: {}/{} states)",
                        s.max_states_reached, s.total_states
                    ));
                }
            }
        }
        if total > 0 && self.num_bugs > 0 {
            out.push_str(&format!(
                "\n{} Found {} buggy schedules.",
                sub,
                percent(self.num_bugs as f64 * 100.0 / total as f64)
            ));
        }
        if let Some(avg) = self.total_fair_steps.checked_div(self.explored_fair_schedules) {
            out.push_str(&format!(
                "\n{} Number of scheduling points in terminating schedules: {} (min), {} (avg), {} (max).",
                sub,
                self.min_fair_steps.unwrap_or(0),
                avg,
                self.max_fair_steps.unwrap_or(0)
            ));
            if opts.max_unfair_steps > 0 && self.max_unfair_steps_hit_in_fair > 0 {
                out.push_str(&format!(
                    "\n{} Exceeded the max-steps bound of '{}' in {} of the fair schedules.",
                    sub,
                    opts.max_unfair_steps,
                    percent(self.max_unfair_steps_hit_in_fair as f64 / self.explored_fair_schedules as f64 * 100.0)
                ));
            }
            if opts.user_set_max_fair_steps && opts.max_fair_steps > 0 && self.max_fair_steps_hit_in_fair > 0 {
                out.push_str(&format!(
                    "\n{} Hit the max-steps bound of '{}' in {} of schedules.",
                    sub,
                    opts.max_fair_steps,
                    percent(self.max_fair_steps_hit_in_fair as f64 / self.explored_fair_schedules as f64 * 100.0)
                ));
            }
        }
        if self.explored_unfair_schedules > 0 && opts.max_unfair_steps > 0 && self.max_unfair_steps_hit_in_unfair > 0 {
            out.push_str(&format!(
                "\n{} Hit the max-steps bound of '{}' in {} of the unfair schedules.",
                sub,
                opts.max_unfair_steps,
                percent(self.max_unfair_steps_hit_in_unfair as f64 / self.explored_unfair_schedules as f64 * 100.0)
            ));
        }
        out
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `{0:F2}%`.
fn percent(v: f64) -> String {
    format!("{:.2}%", v)
}

/// A stable failure identity for discovery metrics. Assertions use their source site
/// without formatted values. Other errors ignore numeric machine instance suffixes,
/// preserving event/type names such as `e1` and `e2` as distinct issues.
pub fn failure_site(bug: &str) -> String {
    if bug.starts_with("Assertion Failed: ") {
        return crate::checker::feedback::failure_class(bug);
    }
    let mut key = String::new();
    for part in bug.split_inclusive(')') {
        if let Some(start) = part.rfind('(').filter(|_| part.ends_with(')')) {
            let id = &part[start + 1..part.len() - 1];
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
                key.push_str(&part[..start]);
                key.push_str("(#)");
                continue;
            }
        }
        key.push_str(part);
    }
    key
}

/// Receives the console output of a run as it is produced.
pub trait Output {
    /// A normal line (`Logger.WriteLine`).
    fn line(&mut self, s: &str);
    /// A bug report (`ErrorReporter.WriteErrorLine`).
    fn error(&mut self, s: &str);
    /// A completed exploration schedule (one-based across workers), including its bug.
    /// Replays used to render traces do not emit this callback.
    fn schedule_finished(&mut self, _schedule: usize, _bug: Option<&str>) {}
    /// Polled before every schedule; returning true stops exploration (timeouts).
    fn cancelled(&mut self) -> bool {
        false
    }
}

impl Output for Vec<String> {
    fn line(&mut self, s: &str) {
        self.push(s.to_string());
    }
    fn error(&mut self, s: &str) {
        self.push(format!("Error: {}", s));
    }
}

/// The first buggy schedule, kept for the logged replay.
struct BugSchedule {
    trace: ScheduleTrace,
    bound_hit_at: Option<usize>,
    strategy_description: Option<String>,
    bug: Option<String>,
    report_text: String,
    /// `Task.CurrentId` of the buggy schedule's test task.
    test_task_id: u64,
}

/// What a checking run produced.
#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    pub report: TestReport,
    /// The runtime log of the first buggy schedule plus the report (`ReadableTrace`).
    pub readable_trace: Option<String>,
    /// The reproducible schedule of the first buggy schedule (`.schedule` file).
    pub schedule_trace: Option<String>,
    /// The runtime log of the first buggy schedule; with `log_every_schedule` (or
    /// `verbose`), the log of the last schedule explored.
    pub last_log: Vec<String>,
    pub strategy_description: String,
    /// Set when a replay could not follow its trace.
    pub replay_error: Option<String>,
    /// Exploration was stopped by [`Output::cancelled`].
    pub cancelled: bool,
    /// The `.trace.json` contents of the first buggy schedule (see `json_trace`).
    pub json_trace: Option<String>,
    /// The `.trace.xml` contents of the first buggy schedule (see `xml_trace`).
    pub xml_trace: Option<String>,
    /// With `verbose` and `json_trace`: the JSON logs of every schedule explored, as one
    /// array of arrays (upstream's `_verbose.trace.json`).
    pub verbose_json_trace: Option<String>,
    /// Activity coverage over all schedules (see `activity_coverage`).
    pub coverage: Option<crate::checker::coverage::CoverageInfo>,
    /// What the test's driver did over all schedules (see `driver_machines`).
    pub driver_facts: Option<crate::checker::driver_facts::DriverFacts>,
    /// Statistics of an exhaustive search (`dfs`, `random-dfs`).
    pub exhaustive: Option<crate::checker::exhaustive::SearchStats>,
}

fn build_strategy(
    opts: &CheckerOptions,
    rng: &SharedRandom,
    has_hot: bool,
    exhaustive: bool,
    split: Option<crate::checker::exhaustive::Split>,
) -> Result<Box<dyn Strategy>, String> {
    use crate::checker::feedback::{
        ControlledRandom, FeedbackGuidedStrategy, PctScheduler, PosScheduler, RandomScheduler, Scheduler, Source,
    };
    let feedback = |scheduler: Box<dyn Scheduler>| {
        let bound = if opts.upstream_feedback { opts.max_unfair_steps } else { opts.max_fair_steps };
        let mut strategy = FeedbackGuidedStrategy::new(bound, opts.seed, scheduler)
            .with_code_guidance(!opts.upstream_feedback)
            .with_legacy_mutations(opts.upstream_feedback)
            .with_fresh_exploration(if opts.upstream_feedback { 0 } else { 25 });
        if opts.feedback_frontier_guidance && !opts.upstream_feedback && opts.strategy == StrategyKind::Feedback {
            strategy = strategy.with_frontier_guidance();
        }
        Box::new(strategy)
    };
    let controlled =
        || Source::Controlled(ControlledRandom::new(opts.seed).with_legacy_mutations(opts.upstream_feedback));
    Ok(match opts.strategy {
        StrategyKind::Auto => Box::new(
            crate::checker::auto::AutoStrategy::new(
                opts.seed,
                opts.max_unfair_steps,
                opts.max_fair_steps,
                has_hot,
                exhaustive,
                split,
            )
            .with_frontier_guidance(opts.feedback_frontier_guidance && !opts.upstream_feedback),
        ),
        StrategyKind::Random => Box::new(RandomStrategy::new(opts.max_fair_steps, rng.clone())),
        StrategyKind::Pct => Box::new(PctStrategy::new(opts.max_unfair_steps, opts.strategy_bound, rng.clone())),
        StrategyKind::FairPct => Box::new(ComboStrategy::new(
            Box::new(PctStrategy::new(opts.max_unfair_steps, opts.strategy_bound, rng.clone())),
            Box::new(RandomStrategy::new(opts.max_fair_steps, rng.clone())),
        )),
        StrategyKind::Probabilistic => {
            Box::new(ProbabilisticRandomStrategy::new(opts.max_fair_steps, opts.strategy_bound, rng.clone()))
        }
        StrategyKind::Pos => Box::new(crate::checker::feedback::ScheduleAndInputStrategy::new(
            opts.max_unfair_steps,
            rng.clone(),
            Box::new(PosScheduler::new(Source::Shared(rng.clone()))),
        )),
        StrategyKind::Feedback => feedback(Box::new(RandomScheduler::new(controlled()))),
        StrategyKind::FeedbackPct => feedback(Box::new(PctScheduler::new(opts.strategy_bound, 0, controlled()))),
        StrategyKind::FeedbackPos => feedback(Box::new(PosScheduler::new(controlled()))),
        StrategyKind::Rl => Box::new(crate::checker::rl::QLearningStrategy::new(opts.max_unfair_steps, rng.clone())),
        StrategyKind::Dfs => Box::new(crate::checker::exhaustive::ExhaustiveStrategy::new(opts.max_unfair_steps, None)),
        StrategyKind::RandomDfs => {
            Box::new(crate::checker::exhaustive::ExhaustiveStrategy::new(opts.max_unfair_steps, Some(rng.clone())))
        }
        StrategyKind::Replay => match &opts.replay {
            Some(t) => {
                let mut s = ReplayStrategy::new(t.clone());
                if opts.replay_bound_at_end {
                    s.stop_at = Some(t.steps.len());
                }
                Box::new(s)
            }
            None => return Err("replay strategy needs a schedule".into()),
        },
    })
}

/// `ShouldPrintIteration`: schedules 1-10, then every 10th up to 100, every 100th up to
/// 1000, and so on.
fn should_print(schedule: usize) -> bool {
    let digits = schedule.to_string().len() - 1;
    schedule.is_multiple_of(10usize.pow(digits as u32))
}

/// Workers exploring one test case together, each on its own thread with its own
/// [`Checker`]: their shared state. A worker runs [`Checker::explore`] with its share of the
/// schedules and its own seed ([`WorkerPool::worker_options`]); [`Checker::finish`] merges
/// what the workers explored. The first bug found stops every worker before its next
/// schedule (unless `--explore` is given).
///
/// Worker 0 keeps the run's seed, so a pool of one worker explores exactly what
/// [`Checker::check`] does.
#[derive(Debug)]
pub struct WorkerPool {
    count: usize,
    /// A worker found the bug that ends the run.
    stop: AtomicBool,
    /// The worker whose bug is reported (`usize::MAX` = none yet).
    winner: AtomicUsize,
    /// Schedules started by all workers (numbers the progress lines).
    started: AtomicUsize,
    /// Buggy schedules found by all workers (numbers the `--explore` bug lines).
    bugs: AtomicUsize,
    /// Workers whose part of a split exhaustive search is complete.
    exhausted: AtomicUsize,
    /// What the workers of a split exhaustive search share.
    shared: std::sync::Arc<crate::checker::exhaustive::Shared>,
}

impl WorkerPool {
    pub fn new(count: usize) -> WorkerPool {
        WorkerPool {
            count: count.max(1),
            stop: AtomicBool::new(false),
            winner: AtomicUsize::new(usize::MAX),
            started: AtomicUsize::new(0),
            bugs: AtomicUsize::new(0),
            exhausted: AtomicUsize::new(0),
            shared: Default::default(),
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Schedules the workers have started so far.
    pub fn started(&self) -> usize {
        self.started.load(Ordering::Relaxed)
    }

    /// Bugs the workers have found so far; counted only with `full_exploration`, since
    /// otherwise the first bug ends the run.
    pub fn bugs(&self) -> usize {
        self.bugs.load(Ordering::Relaxed)
    }

    /// How many workers a run with `opts` can use when `requested` are asked for: one for
    /// strategies that learn across schedules (feedback, rl), `dfs`/`random-dfs`, replays
    /// and verbose runs (which stream every schedule's log); otherwise at most one per
    /// schedule. Auto mode's workers split its exhaustive phase.
    pub fn workers_for(opts: &CheckerOptions, requested: usize) -> usize {
        if !opts.strategy.explores_independently() || opts.replay.is_some() || opts.verbose || opts.log_every_schedule {
            return 1;
        }
        match opts.iterations {
            0 => requested.max(1),
            n => requested.clamp(1, n),
        }
    }

    /// The options of worker `index`: its share of the schedules (0 = unbounded, as for
    /// the run) and its seed. Worker 0 keeps the seed; worker `w` uses `seed + 673 * w`,
    /// as upstream's `TestingProcess` derives a seed from its process id.
    pub fn worker_options(&self, index: usize, opts: &CheckerOptions) -> CheckerOptions {
        let n = self.count;
        let iterations = match opts.iterations {
            0 => 0,
            s => s / n + usize::from(index < s % n),
        };
        let seed = opts.seed.wrapping_add(673u32.wrapping_mul(index as u32));
        CheckerOptions { seed, iterations, ..opts.clone() }
    }

    /// Records that a worker completed its part of the exhaustive search; when every
    /// worker has, the whole search is complete and the run ends.
    fn exhausted(&self) {
        if self.exhausted.fetch_add(1, Ordering::AcqRel) + 1 == self.count {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Whether another worker found the bug that ends the run.
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Records that worker `index` found a bug; true if it is the one reported.
    fn claim(&self, index: usize, full_exploration: bool) -> bool {
        if !full_exploration {
            self.stop.store(true, Ordering::Relaxed);
        }
        self.winner.compare_exchange(usize::MAX, index, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }
}

/// What one worker explored ([`Checker::explore`]).
pub struct Exploration {
    result: CheckResult,
    first_bug: Option<BugSchedule>,
    /// This worker's bug is the one reported.
    won: bool,
    time: Option<crate::checker::time::TimeOptions>,
}

/// What every schedule of a test case needs, resolved once per worker.
struct Prepared<'p> {
    module: &'p crate::sema::ModuleInfo,
    main: crate::sema::MachineId,
    assignment: Option<&'p crate::sema::ParamAssignment>,
    time: Option<crate::checker::time::TimeOptions>,
    config: Config,
}

/// A compiled program ready to be checked.
pub struct Checker<'p> {
    pub prog: &'p Program,
    pub code: Code,
    /// Implementations of the program's foreign functions.
    pub foreign: Option<Rc<dyn crate::checker::foreign::ForeignFunctions>>,
    coverage_tables: std::cell::OnceCell<Rc<crate::checker::coverage::CoverageTables>>,
    guided_code: std::cell::OnceCell<Code>,
}

impl<'p> Checker<'p> {
    pub fn new(prog: &'p Program) -> Checker<'p> {
        Checker {
            prog,
            code: compile_program(prog),
            foreign: None,
            coverage_tables: std::cell::OnceCell::new(),
            guided_code: std::cell::OnceCell::new(),
        }
    }

    /// A checker whose code records which statements and branches run: activity coverage
    /// (`activity_coverage`) then also holds code coverage (see
    /// [`crate::checker::code_coverage`]).
    pub fn with_code_coverage(prog: &'p Program) -> Checker<'p> {
        let code = crate::checker::compile::compile_program_with_probes(prog);
        Checker {
            prog,
            code,
            foreign: None,
            coverage_tables: std::cell::OnceCell::new(),
            guided_code: std::cell::OnceCell::new(),
        }
    }

    /// Node/event tables for activity coverage (built on first use).
    pub fn coverage_tables(&self) -> Rc<crate::checker::coverage::CoverageTables> {
        self.coverage_tables.get_or_init(|| Rc::new(crate::checker::coverage::CoverageTables::new(self.prog))).clone()
    }

    /// The `.coverage.txt` report of a run's activity coverage.
    pub fn coverage_report(&self, info: &crate::checker::coverage::CoverageInfo) -> String {
        crate::checker::coverage::coverage_report(self.prog, &self.coverage_tables(), info)
    }

    /// Registers implementations for the program's foreign functions.
    pub fn with_foreign(mut self, f: Rc<dyn crate::checker::foreign::ForeignFunctions>) -> Checker<'p> {
        self.foreign = Some(f);
        self
    }

    /// Time tracking for `tc`: active when the program declares one of the time
    /// functions or a latency file or `--timed` is given. Checks the profile's names.
    fn time_options(
        &self,
        tc: &TestCase,
        opts: &CheckerOptions,
    ) -> Result<Option<crate::checker::time::TimeOptions>, String> {
        let declares = crate::checker::time::declares_time(self.prog)?;
        if !declares && opts.latency.is_none() && !opts.timed {
            return Ok(None);
        }
        let profile = match &opts.latency {
            Some(set) => match set.for_test(&tc.name, opts.latency_profile.as_deref())? {
                Some(p) => Some(p.clone()),
                None => {
                    return Err(format!(
                        "test case '{}' has no latency profile: add it to [tests] in the latency file or name one with --latency <file>:<profile>",
                        tc.name
                    ))
                }
            },
            None if opts.latency_profile.is_some() => return Err("--latency-profile needs --latency".into()),
            None => None,
        };
        self.check_profile(profile.as_ref())?;
        Ok(Some(crate::checker::time::TimeOptions { profile, timed: opts.timed }))
    }

    fn check_profile(&self, profile: Option<&crate::checker::time::Profile>) -> Result<(), String> {
        let Some(p) = profile else { return Ok(()) };
        let machines: Vec<&str> = self.prog.machines.iter().map(|m| m.name.as_str()).collect();
        let events: Vec<&str> = self.prog.events.iter().map(|e| e.name.as_str()).collect();
        p.check_names(&machines, &events)
    }

    /// Replays a buggy schedule with text (and optionally JSON) logging enabled; returns
    /// the text log and the JSON trace.
    #[allow(clippy::too_many_arguments)]
    fn logged_replay(
        &self,
        tc: &TestCase,
        main: crate::sema::MachineId,
        module: &crate::sema::ModuleInfo,
        assignment: Option<&crate::sema::ParamAssignment>,
        config: &Config,
        bug: &BugSchedule,
        json: bool,
        xml: bool,
        time: Option<&crate::checker::time::TimeOptions>,
    ) -> (Vec<String>, Option<String>, Option<String>) {
        let mut strategy = ReplayStrategy::new(bug.trace.clone());
        strategy.stop_at = bug.bound_hit_at;
        let mut config = config.clone();
        config.strategy_description = bug.strategy_description.clone();
        config.logging = true;
        let setup = Setup {
            assignment,
            foreign: self.foreign.clone(),
            time: time.cloned(),
            json_trace: json,
            xml_trace: xml.then_some(bug.test_task_id),
            ..Setup::default()
        };
        let mut rt = Runtime::new(self.prog, &self.code, module, &config, &mut strategy, setup);
        rt.run_test(main, &tc.name);
        if rt.bug.is_none() {
            rt.check_hot_monitors_at_termination();
        }
        debug_assert_eq!(rt.bug, bug.bug, "replaying a buggy schedule must reproduce its bug");
        (std::mem::take(&mut rt.log), rt.json.take().map(|j| j.to_json_text()), rt.xml.take().map(|x| x.finish()))
    }

    /// Runs one test case (`TestingEngine.Run`).
    pub fn check(&self, tc: &TestCase, opts: &CheckerOptions, out: &mut dyn Output) -> Result<CheckResult, String> {
        self.announce(tc, opts, out)?;
        let pool = WorkerPool::new(1);
        let part = self.explore(tc, opts, &pool, 0, out)?;
        Ok(self.finish(tc, opts, vec![part]))
    }

    /// Checks that `tc` can run with `opts` and prints which strategy explores it: the
    /// start of [`Checker::check`], done once for all workers.
    pub fn announce(&self, tc: &TestCase, opts: &CheckerOptions, out: &mut dyn Output) -> Result<(), String> {
        self.prepare(tc, opts)?;
        let seed_opt = if opts.strategy.reports_seed() { format!(" (seed:{})", opts.seed) } else { String::new() };
        out.line(&format!("... Checker is using '{}' strategy{}.", opts.strategy, seed_opt));
        Ok(())
    }

    fn prepare(&self, tc: &TestCase, opts: &CheckerOptions) -> Result<Prepared<'p>, String> {
        let prog = self.prog;
        let (module, main) = prog.test_case_module(tc);
        let Some(main) = main else {
            return Err(format!("test case '{}' has no main machine", tc.name));
        };
        let assignment = prog.test_case_assignment(tc);
        // reports a replay without a schedule before anything else
        build_strategy(opts, &Rc::new(RefCell::new(RandomValueGenerator::new(opts.seed))), false, false, None)?;
        let is_replay = opts.strategy == StrategyKind::Replay;
        // a replay takes its time settings from the schedule file
        let time = match opts.replay.as_ref().filter(|_| is_replay).and_then(|t| t.time.as_ref().map(|h| (h, t))) {
            Some((header, t)) => {
                let o = crate::checker::time::TimeOptions::from_schedule(header, &t.latency)
                    .map_err(|e| format!("the schedule's time settings: {}", e))?;
                crate::checker::time::declares_time(self.prog)?;
                self.check_profile(o.profile.as_ref())?;
                Some(o)
            }
            // a schedule without time settings was recorded untimed, without a profile
            None if is_replay => {
                let plain = CheckerOptions { latency: None, latency_profile: None, timed: false, ..opts.clone() };
                self.time_options(tc, &plain)?
            }
            None => self.time_options(tc, opts)?,
        };
        if time.is_some() && opts.strategy.is_exhaustive() {
            return Err("exhaustive search (PEx mode) does not support time tracking yet".into());
        }
        Ok(Prepared { module, main, assignment, time, config: run_config(opts) })
    }

    /// Explores worker `index`'s share of the schedules of `tc` (see [`WorkerPool`]);
    /// `opts` are the options of the whole run. Stops early when `out` is cancelled or
    /// another worker of `pool` finds a bug.
    pub fn explore(
        &self,
        tc: &TestCase,
        opts: &CheckerOptions,
        pool: &WorkerPool,
        index: usize,
        out: &mut dyn Output,
    ) -> Result<Exploration, String> {
        let out = RefCell::new(out);
        let interrupted = || out.borrow_mut().cancelled() || pool.stopped();
        // a worker of a pool larger than the schedule count may have none to explore
        let worker_opts = pool.worker_options(index, opts);
        // every worker of a split exhaustive search orders the shared choices alike
        let run_seed = opts.seed;
        let idle = opts.iterations > 0 && worker_opts.iterations == 0;
        let opts = &worker_opts;
        let prog = self.prog;
        let Prepared { module, main, assignment, time, config } = self.prepare(tc, opts)?;
        let rng: SharedRandom = Rc::new(RefCell::new(RandomValueGenerator::new(opts.seed)));
        let has_hot = prog.states.iter().any(|s| s.temperature == crate::sema::Temperature::Hot);
        let searchable = self.foreign.as_ref().is_none_or(|f| f.searchable());
        let split = (pool.count > 1)
            .then(|| crate::checker::exhaustive::Split::new(pool.count, index, run_seed, pool.shared.clone()));
        // auto mode skips exhaustive search when it could not prove anything
        let mut strategy = build_strategy(opts, &rng, has_hot, time.is_none() && searchable, split)?;
        let is_replay = opts.strategy == StrategyKind::Replay;

        let mut result = CheckResult { strategy_description: strategy.description(), ..Default::default() };
        let max_iterations = if is_replay { 1 } else { opts.iterations };
        let mut i = 0usize;
        let mut first_bug: Option<BugSchedule> = None;
        let mut won = false;
        // `Task.Id`s are handed out in order: one for the engine, then per schedule one for
        // the test task and one per machine event handler run
        let mut tasks_used = 1u64;
        let guided = !opts.upstream_feedback
            && matches!(
                opts.strategy,
                StrategyKind::Auto | StrategyKind::Feedback | StrategyKind::FeedbackPct | StrategyKind::FeedbackPos
            );
        let code = if guided && self.code.probes.is_empty() {
            self.guided_code.get_or_init(|| crate::checker::compile::compile_program_with_probes(prog))
        } else {
            &self.code
        };
        let guidance_probes = guided.then(|| {
            use crate::checker::bytecode::ProbeKind;
            let mut mask = crate::checker::coverage::ProbeSet::default();
            for (id, probe) in code.probes.iter().enumerate() {
                if !matches!(probe.kind, ProbeKind::ConditionTrue | ProbeKind::ConditionFalse) {
                    mask.insert(id as u32);
                }
            }
            Rc::new(mask)
        });
        let coverage_tables = (opts.activity_coverage || guided).then(|| self.coverage_tables());
        let mut verbose_json = (opts.verbose && opts.json_trace && !is_replay).then(Vec::new);
        let mut depth_bound_hits = 0usize;
        let mut coverage_epoch = strategy.coverage_epoch();
        let mut earlier_coverage: Option<crate::checker::coverage::CoverageInfo> = None;
        let mut reported_complete = false;
        let mut driver_facts = opts
            .driver_machines
            .clone()
            .filter(|_| !is_replay)
            .map(|d| crate::checker::driver_facts::DriverFacts::new(d, main));
        while !idle && (max_iterations == 0 || i < max_iterations) {
            if out.borrow_mut().cancelled() {
                result.cancelled = true;
                break;
            }
            if pool.stopped() {
                break;
            }
            if coverage_epoch != strategy.coverage_epoch() {
                if let Some(info) = result.coverage.take() {
                    earlier_coverage.get_or_insert_with(Default::default).merge(info);
                }
                coverage_epoch = strategy.coverage_epoch();
            }
            // the schedule's number among those of all workers
            let schedule = pool.started.fetch_add(1, Ordering::Relaxed) + 1;
            if !is_replay && should_print(schedule) {
                out.borrow_mut().line(&format!("..... Schedule #{}", schedule));
            }
            let bug_found;
            {
                let setup = Setup {
                    cancelled: (opts.strategy == StrategyKind::Auto).then_some(&interrupted as &dyn Fn() -> bool),
                    assignment,
                    foreign: self.foreign.clone(),
                    time: time.clone(),
                    json_trace: verbose_json.is_some(),
                    coverage: coverage_tables.as_ref().map(|t| {
                        let cov = Coverage::new(t.clone(), result.coverage.take().unwrap_or_default())
                            .with_condition_guidance(guided);
                        match &guidance_probes {
                            Some(mask) => cov.with_guidance_probes(mask.clone()),
                            None => cov,
                        }
                    }),
                    driver_facts: driver_facts.take(),
                    ..Setup::default()
                };
                let mut rt = Runtime::new(prog, code, module, &config, &mut *strategy, setup);
                let previous_coverage = rt.coverage.as_ref().map_or(0, |c| c.feature_count());
                rt.run_test(main, &tc.name);
                if rt.bug.is_none() {
                    rt.check_hot_monitors_at_termination();
                }
                let new_coverage =
                    rt.coverage.as_ref().map_or(0, |c| c.feature_count()).saturating_sub(previous_coverage);
                rt.strategy.observe_progress(new_coverage, rt.bug.as_deref());
                if rt.strategy.wants_feedback() {
                    let tokens = rt.timeline.tokens();
                    let timeline = tokens.join(";");
                    let minhash = if rt.strategy.wants_minhash() {
                        crate::checker::feedback::timeline_minhash(tokens.iter().map(String::as_str))
                    } else {
                        Vec::new()
                    };
                    let reached: Vec<(String, usize)> =
                        rt.scenarios.iter().map(|s| (s.name.clone(), s.reached.len())).collect();
                    let satisfied: Vec<String> =
                        rt.scenarios.iter().filter(|s| s.satisfied).map(|s| s.name.clone()).collect();
                    rt.strategy.observe_timeline(&timeline, &minhash, &reached, &satisfied);
                }

                bug_found = rt.bug.is_some();
                if !is_replay {
                    out.borrow_mut().schedule_finished(schedule, rt.bug.as_deref());
                }
                let test_task_id = tasks_used + 1;
                tasks_used += 1 + rt.handler_starts;
                if let (Some(all), Some(j)) = (verbose_json.as_mut(), rt.json.take()) {
                    all.push(j.to_json(false));
                }
                if opts.verbose {
                    for l in &rt.log {
                        out.borrow_mut().line(l);
                    }
                }
                // upstream reports the bug through a console that is redirected to a null
                // writer while a schedule runs, so it only shows in verbose mode
                if let Some(b) = rt.bug.as_ref().filter(|_| opts.verbose) {
                    out.borrow_mut().error(b);
                }
                // GatherTestingStatistics
                let steps = rt.strategy.scheduled_steps();
                let reached_max = rt.strategy.has_reached_max_steps();
                let fair = rt.strategy.is_fair();
                let r = &mut result.report;
                if let Some(b) = &rt.bug {
                    r.num_bugs += 1;
                    r.bug_reports.push(b.clone());
                }
                if fair {
                    r.explored_fair_schedules += 1;
                    r.total_fair_steps += steps;
                    r.min_fair_steps = Some(r.min_fair_steps.map_or(steps, |m| m.min(steps)));
                    r.max_fair_steps = Some(r.max_fair_steps.map_or(steps, |m| m.max(steps)));
                    if reached_max {
                        r.max_fair_steps_hit_in_fair += 1;
                    }
                    if opts.max_unfair_steps > 0 && steps >= opts.max_unfair_steps {
                        r.max_unfair_steps_hit_in_fair += 1;
                    }
                } else {
                    r.explored_unfair_schedules += 1;
                    if reached_max {
                        r.max_unfair_steps_hit_in_unfair += 1;
                    }
                }
                let abstract_timeline = rt.timeline.abstract_timeline();
                // scenario coverage (all attached scenarios are tracked, even if never reached)
                for sc in &rt.scenarios {
                    let cov = r.scenario(&sc.name);
                    cov.max_states_reached = cov.max_states_reached.max(sc.reached.len());
                    if sc.total_states > 0 {
                        cov.total_states = sc.total_states;
                    }
                    if sc.satisfied {
                        cov.trigger_count += 1;
                        cov.satisfying_timelines.insert(abstract_timeline.clone());
                    }
                }
                r.timelines.insert(abstract_timeline);
                if rt.bound_hit_at.is_some() {
                    depth_bound_hits += 1;
                }
                if let Some(e) = rt.strategy.replay_error() {
                    result.replay_error = Some(e);
                }
                if bug_found && !is_replay && result.schedule_trace.is_none() {
                    won = pool.claim(index, opts.full_exploration);
                    let mut trace = rt.trace.clone();
                    trace.is_fair = fair;
                    trace.liveness_temperature_threshold = Some(config.liveness_temperature_threshold);
                    trace.test_name = if opts.record_test_name { Some(tc.name.clone()) } else { None };
                    trace.time = time.as_ref().map(|t| t.schedule_header());
                    trace.latency =
                        time.as_ref().and_then(|t| t.profile.as_ref()).map(|p| p.entries()).unwrap_or_default();
                    result.schedule_trace = Some(trace.to_text());
                    first_bug = Some(BugSchedule {
                        trace,
                        bound_hit_at: rt.bound_hit_at,
                        strategy_description: rt.bug_strategy_description.clone(),
                        bug: rt.bug.clone(),
                        report_text: result.report.text(opts, "<StrategyLog>"),
                        test_task_id,
                    });
                }
                if bug_found && !is_replay && opts.full_exploration {
                    let n = pool.bugs.fetch_add(1, Ordering::Relaxed) + 1;
                    out.borrow_mut()
                        .line(&format!("..... Schedule #{} triggered bug #{} [task-{}]", schedule, n, index));
                }
                result.last_log = std::mem::take(&mut rt.log);
                if let Some(c) = rt.coverage.take() {
                    result.coverage = Some(c.into_info());
                }
                if let Some(mut d) = rt.driver_facts.take() {
                    let stopped = !rt.ran_to_end();
                    d.end_schedule(stopped, rt.bound_hit_at.is_some());
                    driver_facts = Some(d);
                }
            }
            // a bug that ends the run ends it, deadline or not
            let ended_by_bug = !opts.full_exploration && result.report.num_bugs > 0;
            if opts.strategy == StrategyKind::Auto && !ended_by_bug {
                // the deadline may have cut this schedule short, so it must not count as
                // an explored branch of the exhaustive search
                let cancelled = out.borrow_mut().cancelled();
                if cancelled || pool.stopped() {
                    result.cancelled = cancelled;
                    break;
                }
            }
            let more = !is_replay && !ended_by_bug && strategy.prepare_next_iteration();
            if !reported_complete && strategy.search_complete() {
                reported_complete = true;
                pool.exhausted();
            }
            if !more {
                break;
            }
            i += 1;
        }
        if let Some(mut earlier) = earlier_coverage {
            if let Some(current) = result.coverage.take() {
                earlier.merge(current);
            }
            result.coverage = Some(earlier);
        }
        let unverifiable = if !searchable {
            Some("the program's foreign functions may keep state or values the search cannot hash (see ForeignFunctions::searchable).".to_string())
        } else {
            result.replay_error.clone().filter(|_| !is_replay)
        };
        result.exhaustive = strategy.exhaustive_stats().map(|s| crate::checker::exhaustive::SearchStats {
            depth_bound_hits: if opts.strategy == StrategyKind::Auto { s.depth_bound_hits } else { depth_bound_hits },
            hot_monitors: has_hot,
            unverifiable,
            ..s
        });
        result.driver_facts = driver_facts;
        result.verbose_json_trace = verbose_json.map(|v| crate::checker::jsonlog::Json::Arr(v).to_pretty());
        Ok(Exploration { result, first_bug, won, time })
    }

    /// Merges what the workers explored (see [`WorkerPool`]) and replays the reported bug
    /// with logging on to produce its traces: the end of [`Checker::check`].
    pub fn finish(&self, tc: &TestCase, opts: &CheckerOptions, parts: Vec<Exploration>) -> CheckResult {
        let several = parts.len() > 1;
        let Exploration { mut result, first_bug, time, .. } = merge(parts);
        if several && !opts.full_exploration {
            // the run ends at the first bug; one another worker hit before it stopped is not
            // reported (the reporting worker's bug comes first)
            result.report.num_bugs = result.report.num_bugs.min(1);
            result.report.bug_reports.truncate(1);
        }
        let Some(mut bug) = first_bug else { return result };
        if several {
            // the report of all workers, not only the one that found the bug
            bug.report_text = result.report.text(opts, "<StrategyLog>");
        }
        let prog = self.prog;
        let (module, main) = prog.test_case_module(tc);
        let main = main.expect("a test case with a buggy schedule has a main machine");
        let assignment = prog.test_case_assignment(tc);
        let config = run_config(opts);
        let (log, json, xml) = self.logged_replay(
            tc,
            main,
            module,
            assignment,
            &config,
            &bug,
            opts.json_trace,
            opts.xml_trace,
            time.as_ref(),
        );
        if !opts.verbose {
            // `ReadableTrace`: the runtime log followed by the report so far
            let mut text = log.join("\n");
            text.push('\n');
            text.push_str(&bug.report_text);
            result.readable_trace = Some(text);
        }
        result.json_trace = json;
        result.xml_trace = xml;
        if !config.logging {
            result.last_log = log;
        }
        result
    }
}

/// The runtime configuration of a run with `opts`.
fn run_config(opts: &CheckerOptions) -> Config {
    Config {
        max_unfair_steps: opts.max_unfair_steps,
        max_fair_steps: opts.max_fair_steps,
        liveness_enabled: true,
        liveness_temperature_threshold: opts.effective_liveness_threshold(),
        fail_on_max_steps: opts.fail_on_max_steps,
        strategy: opts.strategy,
        max_work_per_step: opts.max_work_per_step,
        strategy_description: None,
        // buggy schedules are replayed with logging on afterwards
        logging: opts.strategy == StrategyKind::Replay || opts.verbose || opts.log_every_schedule,
        assembly_name: opts.assembly_name.clone(),
        timeline: opts.timeline,
    }
}

/// Merges the workers' explorations: the statistics and coverage of all of them; the bug,
/// traces, logs and strategy description of the worker whose bug is reported (else of
/// worker 0).
fn merge(parts: Vec<Exploration>) -> Exploration {
    let mut parts = parts;
    // the reporting worker first, so that its bug report comes first
    if let Some(w) = parts.iter().position(|p| p.won) {
        let winner = parts.remove(w);
        parts.insert(0, winner);
    }
    let mut parts = parts.into_iter();
    let mut merged = parts.next().expect("at least one worker");
    let result = &mut merged.result;
    for p in parts {
        let r = p.result;
        result.report.merge(r.report);
        result.cancelled |= r.cancelled;
        // workers split one exhaustive search: complete only when every part is
        match (&mut result.exhaustive, &r.exhaustive) {
            (Some(a), Some(b)) => a.combine(b),
            (Some(a), None) => a.finished = false,
            (None, Some(b)) => {
                result.exhaustive = Some(crate::checker::exhaustive::SearchStats { finished: false, ..b.clone() })
            }
            (None, None) => {}
        }
        result.driver_facts = match (result.driver_facts.take(), r.driver_facts) {
            (Some(mut a), Some(b)) => {
                a.merge(&b);
                Some(a)
            }
            (a, b) => a.or(b),
        };
        result.coverage = match (result.coverage.take(), r.coverage) {
            (Some(mut a), Some(b)) => {
                a.merge(b);
                Some(a)
            }
            (a, b) => a.or(b),
        };
    }
    merged
}
