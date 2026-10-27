//! Checking test cases: the configuration, reading and type-checking the sources, running
//! the checker (on several threads when the host has them), replaying a schedule, and the
//! output files and console report of a run.
//!
//! The steps follow upstream's `TestingProcess` and `TestingEngine`: they print upstream's
//! progress and statistics and, with [`Style::Upstream`], write the output folder upstream's
//! `p check` writes. Both command lines, `snappea check` and `snappea-compat check`, are built on
//! these steps; `snappea` shows the progress only with `--verbose`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use crate::checker::engine::{CheckResult, Checker, CheckerOptions, Exploration, Output, WorkerPool};
use crate::checker::strategy::{ScheduleTrace, StrategyKind};

use crate::driver::format_0_00;
use crate::host::{
    full_path, report_and_exit, report_and_exit_hint, write_output, Color, Exit, Host, HostExt, Job, Stream, Threads,
};
use crate::project::{self, Sources};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BugFinding,
    PEx,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::BugFinding => "BugFinding",
            Mode::PEx => "PEx",
        }
    }
}

/// Which tool's output a run produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// Upstream's `p check`: `<out>/[<test>/]<Mode>/` with `<project>_0_<n>.schedule`,
    /// `.txt`, `.trace.json`, `<project>.coverage.txt`, `.sci` and
    /// `<project>_pchecker_summary.txt`, the ten previous runs kept as `<Mode>0`..`<Mode>9`,
    /// and elapsed times printed unrounded.
    #[default]
    Upstream,
    /// `snappea check`: `<out>/<test>/` with fixed names (`bug.schedule`, `bug.txt`,
    /// `bug.trace.json`, `scenario_coverage.json`), replaced by each run. Coverage and the
    /// summary go to the console only (no `coverage.txt`, `summary.txt` or `.sci` file).
    Snappea,
}

/// The files a [`Style::Snappea`] run may write, removed before each run (with those
/// earlier versions wrote).
const SNAPPEA_FILES: &[&str] = &[
    "bug.txt",
    "bug.trace.json",
    "bug.trace.xml",
    "bug.schedule",
    "verbose.trace.json",
    "coverage.txt",
    "summary.txt",
    crate::driver::scenario::SNAPPEA_FILE_NAME,
];

/// How to check a project (upstream: `CheckerConfiguration`).
#[derive(Debug, Clone)]
pub struct CheckConfig {
    /// Output folder layout and console details.
    pub style: Style,
    /// Where the P sources come from.
    pub input: project::Input,
    /// The project, resolved from `input`.
    pub project_name: String,
    pub p_files: Vec<PathBuf>,
    pub foreign_files: Vec<PathBuf>,
    pub output_path: String,
    /// Output directory with a trailing separator, as upstream builds it.
    pub output_directory: String,
    pub mode: Mode,
    pub test_case_name: String,
    pub list_test_cases: bool,
    pub strategy: StrategyKind,
    pub strategy_bound: usize,
    pub iterations: usize,
    /// Workers exploring schedules in parallel; `None` = one per CPU core. Upstream has
    /// one, so `p check` keeps the default of 1.
    pub workers: Option<usize>,
    /// `--schedules` (or `--timeout`) was given.
    pub user_set_iterations: bool,
    pub seed: Option<u32>,
    pub full_exploration: bool,
    pub max_unfair_steps: usize,
    pub max_fair_steps: usize,
    pub user_set_max_fair_steps: bool,
    pub fail_on_max_steps: bool,
    pub liveness_threshold: usize,
    /// Timeout in seconds; `None` disables it.
    pub timeout: Option<u32>,
    pub verbose: bool,
    /// `--xml-trace`: also write the buggy schedule's `.trace.xml`.
    pub xml_trace: bool,
    pub timeline: crate::checker::timeline::TimelineConfig,
    /// `--replay`: the schedule file to replay.
    pub schedule_file: Option<String>,
    /// Latency profiles and the profile to use (time tracking, see `design/time`).
    pub latency: Option<std::sync::Arc<crate::checker::time::ProfileSet>>,
    pub latency_profile: Option<String>,
    /// Time-aware scheduling.
    pub timed: bool,
    /// Record which statements, branches and handlers run ([`CaseOutcome::code_coverage`]).
    pub code_coverage: bool,
    /// Report what the driver did in every schedule ([`CaseOutcome::driver`]).
    pub driver_facts: bool,
}

impl Default for CheckConfig {
    fn default() -> CheckConfig {
        CheckConfig {
            style: Style::Upstream,
            input: Default::default(),
            project_name: project::DEFAULT_NAME.into(),
            p_files: Vec::new(),
            foreign_files: Vec::new(),
            output_path: "PCheckerOutput".into(),
            output_directory: String::new(),
            mode: Mode::BugFinding,
            test_case_name: String::new(),
            list_test_cases: false,
            strategy: StrategyKind::Random,
            strategy_bound: 0,
            iterations: 1,
            workers: Some(1),
            user_set_iterations: false,
            seed: None,
            full_exploration: false,
            max_unfair_steps: 10000,
            max_fair_steps: 100000,
            user_set_max_fair_steps: false,
            fail_on_max_steps: false,
            liveness_threshold: 0,
            timeout: None,
            verbose: false,
            xml_trace: false,
            timeline: Default::default(),
            schedule_file: None,
            latency: None,
            latency_profile: None,
            timed: false,
            code_coverage: false,
            driver_facts: false,
        }
    }
}

/// Resolves `cfg.input` to the project name and files.
pub fn resolve_sources(host: &mut dyn Host, cfg: &mut CheckConfig) -> Result<(), Exit> {
    let resolved = project::resolve(host, &cfg.input)?;
    cfg.project_name = resolved.project_name;
    cfg.p_files = resolved.p_files;
    cfg.foreign_files = resolved.foreign_files;
    Ok(())
}

/// The strategy and limits implied by the mode and step bounds (`SanitizeConfiguration`).
pub fn finish_options(cfg: &mut CheckConfig) {
    // PEx mode searches exhaustively: `--sch-pex dfs` in order, otherwise (`random`, the
    // default) trying the alternatives at each choice point in a random order, with no
    // schedule limit unless one is given
    if cfg.mode == Mode::PEx && cfg.strategy != StrategyKind::Replay {
        if cfg.strategy != StrategyKind::Dfs {
            cfg.strategy = StrategyKind::RandomDfs;
        }
        if !cfg.user_set_iterations {
            cfg.iterations = 0;
        }
    }
    if cfg.liveness_threshold == 0 && cfg.max_fair_steps > 0 {
        cfg.liveness_threshold = cfg.max_fair_steps / 2;
    }
}

/// Reads and parses a latency profile file (time tracking).
pub fn load_latency(host: &mut dyn Host, file: &str) -> Result<std::sync::Arc<crate::checker::time::ProfileSet>, Exit> {
    let path = full_path(&host.cwd(), Path::new(file));
    let text = match host.read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return Err(report_and_exit_hint(
                host,
                &format!("Cannot read latency file '{}': {}", file, e),
                "check the path given to --latency; it is relative to the current folder",
            ))
        }
    };
    match crate::checker::time::ProfileSet::parse(&text) {
        Ok(set) => Ok(std::sync::Arc::new(set)),
        Err(e) => Err(report_and_exit_hint(
            host,
            &format!("Latency file '{}': {}", file, e),
            "fix the file at the place named; see docs/checking-models.md for the latency file format",
        )),
    }
}

fn sanitize_segment(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
}

/// Picks and prepares the output folder for `cfg.test_case_name`. Upstream
/// (`CheckerConfiguration.SetOutputDirectory`): `<outdir>/[<testcase>/]<Mode>/`, rotating up
/// to ten previous runs to `<Mode>0` .. `<Mode>9`. snappea: `<outdir>/<testcase>/`, removing the
/// files of the previous run.
pub fn set_output_directory(host: &mut dyn Host, cfg: &mut CheckConfig) -> Result<(), Exit> {
    let fail = |host: &mut dyn Host, path: &str, e: std::io::Error| {
        report_and_exit_hint(
            host,
            &format!("Cannot prepare output directory '{}': {}", path, e),
            "check that the folder is writable, or pick another with --out",
        )
    };
    if cfg.style == Style::Snappea {
        let out = if cfg.output_path.is_empty() { "." } else { cfg.output_path.trim_end_matches('/') };
        cfg.output_directory = if cfg.test_case_name.is_empty() {
            format!("{}/", out)
        } else {
            format!("{}/{}/", out, sanitize_segment(&cfg.test_case_name))
        };
        let dir = full_path(&host.cwd(), Path::new(&cfg.output_directory));
        host.create_dir_all(&dir).map_err(|e| fail(host, &cfg.output_directory, e))?;
        if cfg.strategy != StrategyKind::Replay {
            for f in SNAPPEA_FILES {
                let p = dir.join(f);
                if host.is_file(&p) {
                    host.remove_file(&p).map_err(|e| fail(host, &cfg.output_directory, e))?;
                }
            }
        }
        return Ok(());
    }
    let suffix = if cfg.test_case_name.is_empty() {
        cfg.mode.name().to_string()
    } else {
        format!("{}/{}", sanitize_segment(&cfg.test_case_name), cfg.mode.name())
    };
    let base = if cfg.output_path.is_empty() {
        format!("./Output/{}/", cfg.project_name)
    } else {
        format!("{}/", cfg.output_path)
    };
    cfg.output_directory = format!("{}{}/", base, suffix);
    let cwd = host.cwd();
    let abs = |s: &str| full_path(&cwd, Path::new(s));
    let stem = &cfg.output_directory[..cfg.output_directory.len() - 1];
    let history = |h: usize| format!("{}{}", stem, h);
    const MAX_HISTORY: usize = 10;
    let mut older = history(MAX_HISTORY - 1);
    if host.is_dir(&abs(&older)) {
        host.remove_dir_all(&abs(&older)).map_err(|e| fail(host, &older, e))?;
    }
    if cfg.strategy != StrategyKind::Replay {
        for h in (0..MAX_HISTORY - 1).rev() {
            let newer = history(h);
            if host.is_dir(&abs(&newer)) {
                host.rename(&abs(&newer), &abs(&older)).map_err(|e| fail(host, &newer, e))?;
            }
            older = newer;
        }
        if host.is_dir(&abs(&cfg.output_directory)) {
            host.rename(&abs(&cfg.output_directory), &abs(&older)).map_err(|e| fail(host, &cfg.output_directory, e))?;
        }
    }
    host.create_dir_all(&abs(&cfg.output_directory)).map_err(|e| fail(host, &cfg.output_directory, e))
}

/// Type-checked sources.
pub struct Loaded {
    pub sources: Sources,
    pub analysis: crate::sema::Analysis,
}

/// Reads and type-checks the project's sources (upstream: `TestingEngine.LoadAssembly`);
/// type errors are reported as upstream's compiler reports them.
pub fn load(host: &mut dyn Host, cfg: &CheckConfig) -> Result<Loaded, Exit> {
    let sources = match project::read(host, &cfg.project_name, &cfg.p_files, &host.cwd()) {
        Ok(s) => s,
        Err(e) => return Err(report_and_exit(host, &e)),
    };
    let sm = sources.source_map();
    let r = crate::sema::compile(&sm, &crate::sema::Options::default());
    if !project::report_result(host, &sources, &sm, &r, cfg.style == Style::Upstream) {
        return Err(Exit(1));
    }
    for f in &cfg.foreign_files {
        host.out_colored_line(
            &format!(
                "Warning: foreign file {} is ignored; snappea runs P models directly, so foreign functions must be provided to the checker in Rust.",
                f.display()
            ),
            Color::Yellow,
        );
    }
    let analysis = r.map_err(|_| Exit(1))?;
    Ok(Loaded { sources, analysis })
}

/// Reads a `.schedule` file.
pub fn read_schedule(host: &mut dyn Host, schedule_file: &str) -> Result<ScheduleTrace, Exit> {
    let text = match host.read_to_string(&full_path(&host.cwd(), Path::new(schedule_file))) {
        Ok(t) => t,
        Err(e) => {
            return Err(report_and_exit_hint(
                host,
                &format!("Cannot read schedule file '{}': {}", schedule_file, e),
                "pass a bug's schedule as `snappea check` prints it, e.g. PCheckerOutput/<test>/bug.schedule",
            ))
        }
    };
    ScheduleTrace::parse(&text).map_err(|e| {
        report_and_exit_hint(
            host,
            &e,
            "the file is not a schedule `snappea check` wrote, or was edited; re-run `snappea check` to write a fresh one",
        )
    })
}

/// What replaying a schedule found.
#[derive(Debug, Clone)]
pub struct Replayed {
    /// Bugs reproduced (0 or 1).
    pub bugs: usize,
    /// The bug's report, e.g. `Assertion Failed: model.p:8:1 six heads`.
    pub bug_report: Option<String>,
    /// Seconds the replay took.
    pub elapsed: f64,
}

/// Replays `trace` on `tc`.
pub fn replay(
    host: &mut dyn Host,
    loaded: &Loaded,
    tc: &crate::sema::TestCase,
    cfg: &mut CheckConfig,
    trace: ScheduleTrace,
) -> Result<Replayed, Exit> {
    if let Some(t) = trace.liveness_temperature_threshold {
        cfg.liveness_threshold = t;
    }
    let mut opts = engine_options(cfg, 0);
    opts.replay = Some(trace);
    opts.replay_bound_at_end = cfg.style == Style::Snappea && cfg.fail_on_max_steps;
    let start = host.now();
    let (res, _) = run_engine(host, loaded, tc, &opts, None, Some(1), false)?;
    // `ReplayStrategy` reports a schedule it cannot follow with `Error.ReportAndExit`
    if let Some(e) = &res.replay_error {
        return Err(report_and_exit_hint(
            host,
            e,
            "the model changed since the schedule was written; re-run `snappea check` to find the bug again",
        ));
    }
    let elapsed = host.now() - start;
    let n = res.report.num_bugs;
    if cfg.style == Style::Upstream {
        let text = format!(
            "... Reproduced {} bug{}.\n... Elapsed {} sec and used {} GB.",
            n,
            if n == 1 { "" } else { "s" },
            format_0_00(elapsed),
            format_0_00(host.max_memory_gb())
        );
        host.write(Stream::Out, &text, Some(Color::Yellow));
    }
    Ok(Replayed { bugs: n, bug_report: res.report.bug_reports.first().cloned(), elapsed })
}

/// `TestMethodInfo.GetTestMethod`: exact name, else the unique test case containing it.
pub fn select_test_case(host: &mut dyn Host, loaded: &Loaded, name: &str) -> Result<crate::sema::TestCase, Exit> {
    let all = loaded.analysis.program.test_cases();
    let exact: Vec<_> = all.iter().filter(|t| t.name == name).cloned().collect();
    let filtered: Vec<_> =
        if !exact.is_empty() { exact } else { all.iter().filter(|t| t.name.contains(name)).cloned().collect() };
    let options: String = all.iter().map(|t| format!("{}\n", t.name)).collect();
    match filtered.as_slice() {
        [tc] => {
            host.out_line(&format!(".. Test case :: {}", tc.name));
            Ok(tc.clone())
        }
        [_, _, ..] => Err(report_and_exit(
            host,
            &format!(
                "We found '{}' test cases. Please provide a more precise name of the test case you wish to check using (--testcase | -tc).\nPossible options are: \n{}",
                all.len(),
                options
            ),
        )),
        [] if !all.is_empty() => Err(report_and_exit(
            host,
            &format!("Cannot find a P test case that contains {}. Possible options are: \n{}", name, options),
        )),
        [] => Err(report_and_exit_hint(
            host,
            "Cannot detect a P test case to check.\n -- If you have a test case specified and P checker is not finding it, please report this error to the P team --",
            "declare a test case, e.g. `test tcMain [main=Main]: { Main };`",
        )),
    }
}

pub(crate) fn engine_options(cfg: &CheckConfig, seed: u32) -> CheckerOptions {
    CheckerOptions {
        strategy: cfg.strategy,
        seed,
        iterations: cfg.iterations,
        max_unfair_steps: cfg.max_unfair_steps,
        max_fair_steps: cfg.max_fair_steps,
        user_set_max_fair_steps: cfg.user_set_max_fair_steps,
        liveness_temperature_threshold: Some(cfg.liveness_threshold),
        fail_on_max_steps: cfg.fail_on_max_steps,
        strategy_bound: cfg.strategy_bound,
        upstream_feedback: cfg.style == Style::Upstream,
        full_exploration: cfg.full_exploration,
        replay: None,
        record_test_name: !cfg.test_case_name.is_empty(),
        verbose: cfg.verbose,
        json_trace: true,
        xml_trace: cfg.xml_trace,
        timeline: cfg.timeline,
        activity_coverage: true,
        latency: cfg.latency.clone(),
        latency_profile: cfg.latency_profile.clone(),
        timed: cfg.timed,
        ..CheckerOptions::default()
    }
}

/// Engine output on the console, with an optional deadline (`--timeout`).
struct ConsoleOut<'h> {
    host: &'h mut dyn Host,
    deadline: Option<f64>,
}

impl Output for ConsoleOut<'_> {
    fn line(&mut self, s: &str) {
        self.host.out_line(s);
    }
    fn error(&mut self, s: &str) {
        self.host.write(Stream::Out, "Error: ", Some(Color::Red));
        self.host.write(Stream::Out, s, Some(Color::Yellow));
        self.host.write(Stream::Out, "\n", None);
    }
    fn cancelled(&mut self) -> bool {
        self.deadline.map(|d| self.host.now() >= d).unwrap_or(false)
    }
}

/// Runs the engine on `tc` with up to `workers` workers (`None` = one per CPU core).
fn run_engine(
    host: &mut dyn Host,
    loaded: &Loaded,
    tc: &crate::sema::TestCase,
    opts: &CheckerOptions,
    deadline: Option<f64>,
    workers: Option<usize>,
    code_coverage: bool,
) -> Result<(CheckResult, usize), Exit> {
    let prog = &loaded.analysis.program;
    let checker = if code_coverage { Checker::with_code_coverage(prog) } else { Checker::new(prog) };
    let opts = &CheckerOptions { assembly_name: loaded.sources.project_name.clone(), ..opts.clone() };
    // a host without threads runs one worker
    let threads = host.threads();
    let requested = threads.map_or(1, |t| workers.unwrap_or(t.available));
    let workers = WorkerPool::workers_for(opts, requested);
    let mut out = ConsoleOut { host, deadline };
    let result = match threads {
        Some(threads) if workers > 1 => run_workers(&checker, tc, opts, &WorkerPool::new(workers), threads, &mut out),
        _ => checker.check(tc, opts, &mut out),
    };
    result.map(|r| (r, workers)).map_err(|e| report_and_exit(host, &e))
}

/// A worker's output, sent to the thread that writes the console.
enum Message {
    Line(String),
    Error(String),
    Done(usize, Box<Result<Exploration, String>>),
}

struct WorkerOut<'a> {
    tx: mpsc::Sender<Message>,
    timed_out: &'a AtomicBool,
}

impl Output for WorkerOut<'_> {
    fn line(&mut self, s: &str) {
        let _ = self.tx.send(Message::Line(s.to_string()));
    }
    fn error(&mut self, s: &str) {
        let _ = self.tx.send(Message::Error(s.to_string()));
    }
    fn cancelled(&mut self) -> bool {
        self.timed_out.load(Ordering::Relaxed)
    }
}

/// Explores `tc` with the workers of `pool`, each on its own thread with its own
/// `Checker`; this thread writes their output and watches the deadline.
fn run_workers(
    checker: &Checker,
    tc: &crate::sema::TestCase,
    opts: &CheckerOptions,
    pool: &WorkerPool,
    threads: Threads,
    out: &mut ConsoleOut,
) -> Result<CheckResult, String> {
    checker.announce(tc, opts, out)?;
    out.line(&format!("... Checker is using {} workers.", pool.count()));
    let (prog, code_coverage) = (checker.prog, !checker.code.probes.is_empty());
    let timed_out = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel();
    let jobs: Vec<Job<'_>> = (0..pool.count())
        .map(|index| {
            let tx = tx.clone();
            let timed_out = &timed_out;
            Box::new(move || {
                let checker = if code_coverage { Checker::with_code_coverage(prog) } else { Checker::new(prog) };
                let mut out = WorkerOut { tx, timed_out };
                let part = checker.explore(tc, opts, pool, index, &mut out);
                let _ = out.tx.send(Message::Done(index, Box::new(part)));
            }) as Job<'_>
        })
        .collect();
    drop(tx);
    let mut parts: Vec<Option<Result<Exploration, String>>> = (0..pool.count()).map(|_| None).collect();
    let mut main = || loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Message::Line(s)) => out.line(&s),
            Ok(Message::Error(s)) => out.error(&s),
            Ok(Message::Done(index, part)) => parts[index] = Some(*part),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !timed_out.load(Ordering::Relaxed) && out.cancelled() {
            timed_out.store(true, Ordering::Relaxed);
        }
    };
    (threads.run)(jobs, &mut main);
    let parts = parts.into_iter().map(|p| p.expect("every worker reports")).collect::<Result<Vec<_>, String>>()?;
    Ok(checker.finish(tc, opts, parts))
}

/// What checking one test case found.
#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub test_case: String,
    pub bugs: usize,
    /// The first bug's report, e.g. `Assertion Failed: model.p:8:1 six heads`.
    pub bug_report: Option<String>,
    /// Schedules explored.
    pub schedules: usize,
    /// Workers that explored them.
    pub workers: usize,
    /// The time limit stopped the exploration.
    pub timed_out: bool,
    /// For an exhaustive search: whether every alternative was explored.
    pub exhaustive_finished: Option<bool>,
    /// Seconds the test case took.
    pub elapsed: f64,
    /// The `.schedule` file written for the bug (as `cfg.output_directory` names it).
    pub schedule_file: Option<String>,
    /// What the schedules ran, and the code they could have run, with `cfg.code_coverage`.
    pub code_coverage: Option<(crate::checker::code_coverage::Scope, crate::checker::code_coverage::Hits)>,
    /// Scenario results, also written to the per-test artifact for cross-run merging.
    pub scenario_coverage: crate::driver::scenario::Artifact,
    /// What the driver did in every schedule, with `cfg.driver_facts`.
    pub driver: Option<DriverReport>,
}

/// What a test case's driver did in every schedule it explored.
#[derive(Debug, Clone)]
pub struct DriverReport {
    /// The machines treated as the driver.
    pub machines: Vec<String>,
    /// What held in every schedule, most telling first.
    pub facts: Vec<crate::checker::driver_facts::Fact>,
    /// The driver may repeat without limit (the search found cycles, or hit the step
    /// bound), so no counts or orders are claimed.
    pub repeats: bool,
}

impl CaseOutcome {
    /// Upstream's exit code for the test case: 1 when a bug was found.
    pub fn exit_code(&self) -> i32 {
        i32::from(self.bugs > 0)
    }
}

/// `TestingProcess.Run` for one test case.
pub fn testing_process(host: &mut dyn Host, loaded: &Loaded, cfg: &CheckConfig) -> Result<CaseOutcome, Exit> {
    let process_start = host.now();
    let seed = match cfg.seed {
        Some(s) => s, // + 673 * TestingProcessId: see `WorkerPool::worker_options`
        None => host.random_seed(),
    };
    let tc = select_test_case(host, loaded, &cfg.test_case_name)?;
    let mut opts = engine_options(cfg, seed);
    if cfg.driver_facts {
        let sources = loaded.sources.source_map();
        let marked = snappea_lint::driver_machines(&sources).unwrap_or_default();
        opts.driver_machines =
            Some(crate::checker::driver_facts::driver_machines(&loaded.analysis.program, &sources, &marked));
    }
    let engine_start = host.now();
    let deadline = cfg.timeout.map(|t| engine_start + f64::from(t));
    let (res, workers) = run_engine(host, loaded, &tc, &opts, deadline, cfg.workers, cfg.code_coverage)?;
    let engine_elapsed = host.now() - engine_start;
    if res.cancelled {
        host.out_line("... Checker timed out.");
    }
    let file = cfg.project_name.clone();
    let snappea = cfg.style == Style::Snappea;
    // an output file's path: upstream's name, or snappea's
    let path = |upstream: String, snappea_name: &str| {
        format!("{}{}", cfg.output_directory, if snappea { snappea_name.to_string() } else { upstream })
    };
    if let Some(j) = &res.verbose_json_trace {
        host.out_line("... Emitting verbose logs:");
        let path = path(format!("{}_0_verbose.trace.json", file), "verbose.trace.json");
        host.out_line(&format!("..... Writing {}", path));
        write_output(host, &path, j)?;
    }
    let mut schedule_file = None;
    if !cfg.full_exploration && res.report.num_bugs > 0 {
        host.out_line("Checker found a bug.");
        host.out_line("... Emitting traces:");
        schedule_file = emit_traces(host, cfg, &res, &format!("{}_0", file), engine_elapsed)?;
    }
    if cfg.verbose {
        host.out_line("... ### Process 0 is terminating");
    }
    let prog = &loaded.analysis.program;
    let code_coverage = res.coverage.as_ref().filter(|_| cfg.code_coverage).map(|cov| {
        use crate::checker::code_coverage::{Hits, Scope};
        let tables = crate::checker::coverage::CoverageTables::new(prog);
        (Scope::test_case(prog, &tc), Hits::from_info(prog, &tables, cov))
    });
    // snappea reports coverage on the console
    if let Some(cov) = res.coverage.as_ref().filter(|_| !snappea) {
        // ReportActivityCoverage (on by default upstream)
        host.out_line("... Emitting coverage report:");
        let checker = Checker::new(&loaded.analysis.program);
        let text = checker.coverage_report(cov);
        let txt = path(format!("{}.coverage.txt", file), "coverage.txt");
        host.out_line(&format!("..... Writing {}", txt));
        write_output(host, &txt, &text)?;
        let sci = path(format!("{}.sci", file), "");
        host.out_line(&format!("..... Writing {}", sci));
        let xml = crate::driver::sci::coverage_xml(prog, &checker.coverage_tables(), cov);
        write_output(host, &sci, &xml)?;
    }
    let driver = res.driver_facts.as_ref().zip(opts.driver_machines.as_ref()).map(|(d, machines)| {
        let repeats = res.exhaustive.as_ref().is_some_and(|x| x.cycle_prunes > 0) || d.truncated > 0;
        let (module, main) = prog.test_case_module(&tc);
        let mut names: Vec<String> = module
            .interface_def
            .iter()
            .map(|(_, m)| *m)
            .chain(main)
            .filter(|m| machines[m.idx()] || Some(*m) == main)
            .map(|m| prog.machine(m).name.clone())
            .collect();
        names.sort();
        names.dedup();
        DriverReport { machines: names, facts: d.facts(prog, repeats), repeats }
    });
    let process_elapsed = host.now() - process_start;
    host.out_line(&res.report.text(&opts, "..."));
    if let Some(x) = &res.exhaustive {
        let schedules = res.report.explored_fair_schedules + res.report.explored_unfair_schedules;
        host.out_line(&x.text(schedules, opts.max_unfair_steps, "....."));
    }
    if !snappea {
        let summary_path = path(format!("{}_pchecker_summary.txt", file), "summary.txt");
        host.out_line(&format!("..... Writing {}", summary_path));
        let summary = format!(
            "bugs:{}\nschedules:{}\nmax_depth:{}\ntime_seconds:{}\nmemory_max_mb:{}",
            res.report.num_bugs,
            res.report.explored_fair_schedules + res.report.explored_unfair_schedules,
            res.report.max_fair_steps.unwrap_or(0),
            format_0_00(process_elapsed),
            format_0_00(host.max_memory_gb())
        );
        write_output(host, &summary_path, &summary)?;
    }
    if !res.report.scenarios.is_empty() {
        let suffix = if cfg.test_case_name.is_empty() { String::new() } else { format!("_{}", cfg.test_case_name) };
        let path = path(
            format!("{}{}{}", file, suffix, crate::driver::scenario::FILE_SUFFIX),
            crate::driver::scenario::SNAPPEA_FILE_NAME,
        );
        host.out_line(&format!("..... Writing {}", path));
        let art = crate::driver::scenario::from_report(&res.report, &cfg.test_case_name);
        write_output(host, &path, &art.to_json())?;
    }
    if snappea {
        host.out_line(&format!("... Elapsed {} sec.", format_0_00(process_elapsed)));
    } else {
        host.out_line(&format!("... Elapsed {} sec.", process_elapsed));
    }
    Ok(CaseOutcome {
        test_case: tc.name.clone(),
        bugs: res.report.num_bugs,
        bug_report: res.report.bug_reports.first().cloned(),
        schedules: res.report.explored_fair_schedules + res.report.explored_unfair_schedules,
        workers,
        timed_out: res.cancelled,
        exhaustive_finished: res.exhaustive.as_ref().map(|x| x.complete()),
        elapsed: process_elapsed,
        schedule_file,
        code_coverage,
        scenario_coverage: crate::driver::scenario::from_report(&res.report, &cfg.test_case_name),
        driver,
    })
}

/// The trace file index: one more than the largest `<name>_<n>_<index>` already present.
fn next_trace_index(host: &dyn Host, dir: &Path, file: &str) -> usize {
    let mut index = 0;
    for p in host.read_dir(dir).unwrap_or_default() {
        if !host.is_file(&p) {
            continue;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !name.starts_with(file) {
            continue;
        }
        if let Some(i) = trailing_index(&name) {
            index = index.max(i + 1);
        }
    }
    index
}

/// Group 3 of `^(.*)_([0-9]+)_([0-9]+)` (greedy).
fn trailing_index(name: &str) -> Option<usize> {
    let b = name.as_bytes();
    for k in (0..b.len()).rev() {
        if b[k] != b'_' {
            continue;
        }
        let d2 = b[k + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if d2 == 0 || k + 1 + d2 >= b.len() || b[k + 1 + d2] != b'_' {
            continue;
        }
        let start = k + 2 + d2;
        let d3 = b[start..].iter().take_while(|c| c.is_ascii_digit()).count();
        if d3 == 0 {
            continue;
        }
        return name[start..start + d3].parse().ok();
    }
    None
}

/// `TestingEngine.TryEmitTraces`; returns the `.schedule` file written.
fn emit_traces(
    host: &mut dyn Host,
    cfg: &CheckConfig,
    res: &CheckResult,
    file: &str,
    engine_elapsed: f64,
) -> Result<Option<String>, Exit> {
    let base = if cfg.style == Style::Snappea {
        format!("{}bug", cfg.output_directory)
    } else {
        let dir = full_path(&host.cwd(), Path::new(&cfg.output_directory));
        let index = next_trace_index(host, &dir, file);
        format!("{}{}_{}", cfg.output_directory, file, index)
    };
    let mut schedule_file = None;
    if !cfg.full_exploration {
        if let Some(t) = res.readable_trace.as_ref().filter(|t| !t.is_empty()) {
            let path = format!("{}.txt", base);
            host.out_line(&format!("..... Writing {}", path));
            write_output(host, &path, t)?;
        }
    }
    if let Some(x) = &res.xml_trace {
        let path = format!("{}.trace.xml", base);
        host.out_line(&format!("..... Writing {}", path));
        write_output(host, &path, x)?;
    }
    if let Some(j) = &res.json_trace {
        let path = format!("{}.trace.json", base);
        host.out_line(&format!("..... Writing {}", path));
        write_output(host, &path, j)?;
    }
    if !cfg.full_exploration {
        if let Some(t) = res.schedule_trace.as_ref().filter(|t| !t.is_empty()) {
            let path = format!("{}.schedule", base);
            host.out_line(&format!("..... Writing {}", path));
            write_output(host, &path, t)?;
            schedule_file = Some(path);
        }
    }
    host.out_line(&format!(
        "... Elapsed {} sec and used {} GB.",
        format_0_00(engine_elapsed),
        format_0_00(host.max_memory_gb())
    ));
    Ok(schedule_file)
}
