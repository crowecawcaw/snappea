//! Fuzz harnesses for snappea.
//!
//! Each function takes arbitrary bytes and runs one part of snappea on them. None of them
//! may panic, hang or run out of memory, whatever the input: a bad input must come back as
//! an error, a diagnostic or a bug report. The `fuzz/` crate (cargo-fuzz, nightly) calls
//! these functions from its targets; `tests/regressions.rs` replays every saved crash
//! input on stable, so a fixed crash stays fixed.
//!
//! | Harness | Input | Runs |
//! |---|---|---|
//! | [`parse`] | P source | lexer, parser, error rendering |
//! | [`compile`] | P source | type checker, lint, bytecode compiler, mutant generation |
//! | [`check`] | P source | [`compile`], then a few bounded checker schedules per test case |
//! | [`observe`] | mode byte, then a log | `snappea observe` on a fixed spec |
//!
//! Example (what a fuzz target does):
//!
//! ```
//! snappea_fuzz::parse(b"machine M { start state S { } }");
//! ```

use snappea::checker::engine::{Checker, CheckerOptions};
use snappea::checker::mutation;
use snappea::checker::strategy::{ScheduleTrace, StrategyKind};
use snappea::host::MemHost;
use snappea::sema::diag::render_snippet;
use snappea::syntax::{SourceMap, Span};

/// A fuzz harness: runs one part of snappea on any bytes.
pub type Harness = fn(&[u8]);

/// The harnesses by name, as `fuzz/fuzz_targets/` and `fuzz/regressions/` name them.
pub const HARNESSES: &[(&str, Harness)] =
    &[("parse", parse), ("compile", compile), ("check", check), ("observe", observe)];

const FILE: &str = "fuzz.p";

/// Lexes and parses `data` as one P file, and renders the error if there is one.
///
/// Checks that the error's span lies inside the input, on character boundaries.
pub fn parse(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let mut sm = SourceMap::new();
    let file = sm.add(FILE, text);
    if let Err(e) = snappea::syntax::parse(file, text) {
        assert_span_in(text, e.span);
        let _ = render_snippet(&sm, Some(e.span), &e.message, None);
    }
}

/// Type-checks `data` as one P file and renders every diagnostic. When it type-checks,
/// also lints it, compiles it to bytecode, and generates and applies its mutants.
pub fn compile(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Some(program) = analyze(text) else { return };
    compile_program(&program);
}

/// [`compile`], then runs every test case (at most 4) for a few short schedules with the
/// random, PCT and depth-first strategies. The checker reports a model's bugs as bug
/// reports; any panic is a checker bug.
///
/// Also checks two promises of the checker: the same seed finds the same result, and the
/// `.schedule` of a bug found during the search replays to the same bug.
pub fn check(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Some(program) = analyze(text) else { return };
    compile_program(&program);
    let checker = Checker::new(&program);
    for tc in program.test_cases().iter().take(4) {
        for strategy in [StrategyKind::Random, StrategyKind::Pct, StrategyKind::Dfs] {
            let opts = bounded(strategy);
            let Ok(first) = checker.check(tc, &opts, &mut Vec::<String>::new()) else { continue };
            if strategy != StrategyKind::Random {
                continue;
            }
            let again = checker.check(tc, &opts, &mut Vec::<String>::new()).expect("the first run succeeded");
            assert_eq!(first.report.bug_reports, again.report.bug_reports, "same seed, different bugs");
            assert_eq!(first.schedule_trace, again.schedule_trace, "same seed, different schedule");
            // a bug found during the search (not by the liveness analysis after it) replays
            let (Some(schedule), Some(1..)) = (&first.schedule_trace, first.first_bug_schedule) else { continue };
            let trace = ScheduleTrace::parse(schedule).expect("the checker writes schedules it can read");
            let replay = CheckerOptions {
                strategy: StrategyKind::Replay,
                liveness_temperature_threshold: trace.liveness_temperature_threshold,
                replay: Some(trace),
                ..bounded(StrategyKind::Replay)
            };
            let replayed = checker.check(tc, &replay, &mut Vec::<String>::new()).expect("replay runs");
            assert_eq!(replayed.replay_error, None, "the bug's schedule does not replay");
            assert_eq!(
                replayed.report.bug_reports.first(),
                first.report.bug_reports.first(),
                "the bug's schedule replays to a different result"
            );
        }
    }
}

/// Options for a short checker run: a few schedules of at most 200 steps.
fn bounded(strategy: StrategyKind) -> CheckerOptions {
    CheckerOptions {
        strategy,
        seed: 1,
        iterations: 4,
        steps: snappea::checker::runtime::StepBounds { unfair: 200, fair: 200, fair_explicit: true },
        strategy_bound: 2,
        max_bugs: 1,
        // keep each input fast: a long loop in the model becomes a bug report sooner
        limits: snappea::checker::runtime::RunLimits { max_work_per_step: 100_000, max_machines: 64, max_inbox: 256 },
        json_trace: true,
        ..Default::default()
    }
}

/// The spec [`observe`] checks logs against: a little of every payload type.
pub const OBSERVE_SPEC: &str = include_str!("observe_spec.p");

/// Runs `snappea observe` on [`OBSERVE_SPEC`] with the log `data[1..]`, in memory.
/// `data[0]` picks the options: bit 0 reads the log as a P JSON trace (`--p-trace`)
/// instead of JSON lines, bit 1 adds `--check-hot`, bit 2 adds `--strict`, bit 3 sets
/// `--unknown-events warn`, bit 4 adds `--sorted`.
pub fn observe(data: &[u8]) {
    let Some((&mode, log)) = data.split_first() else { return };
    let log = String::from_utf8_lossy(log);
    let mut host = MemHost::new("/work");
    let write = |host: &mut MemHost, path: &str, text: &str| {
        snappea::host::Host::write_file(host, std::path::Path::new(path), text).expect("in-memory write")
    };
    write(&mut host, "/work/spec.p", OBSERVE_SPEC);
    write(&mut host, "/work/log.jsonl", &log);
    let mut args: Vec<String> = ["observe", "spec.p", "log.jsonl", "--all-specs"].map(String::from).to_vec();
    let flags: [(u8, &[&str]); 5] = [
        (1, &["--p-trace"]),
        (2, &["--check-hot"]),
        (4, &["--strict"]),
        (8, &["--unknown-events", "warn"]),
        (16, &["--sorted"]),
    ];
    for (bit, flag) in flags {
        if mode & bit != 0 {
            args.extend(flag.iter().map(|s| s.to_string()));
        }
    }
    let code = snappea::cli::run(&args, &mut host);
    assert!((0..=2).contains(&code), "observe exited with {code}\n{}", host.stderr);
}

/// Type-checks `text`; renders the diagnostics and returns `None` if it does not
/// type-check.
fn analyze(text: &str) -> Option<snappea::sema::Program> {
    let mut sm = SourceMap::new();
    sm.add(FILE, text);
    match snappea::sema::compile(&sm, &Default::default()) {
        Ok(analysis) => {
            let linter = snappea_lint::Linter::parse(&sm).expect("sources that type-check parse");
            let _ = linter.lint(&sm, None);
            let _ = linter.lint_observation(&sm);
            Some(analysis.program)
        }
        Err(snappea::sema::CompileError::Parse { span, message, .. }) => {
            assert_span_in(text, span);
            let _ = render_snippet(&sm, Some(span), &message, None);
            None
        }
        Err(snappea::sema::CompileError::Analysis(e)) => {
            for d in e.diagnostics(&sm) {
                if let Some(span) = d.span {
                    assert_span_in(text, span);
                }
                let _ = render_snippet(&sm, d.span, &d.message, d.help.as_deref());
            }
            None
        }
        Err(_) => None,
    }
}

/// Compiles `program` and every mutant mutation testing would try (at most 64).
fn compile_program(program: &snappea::sema::Program) {
    let code = snappea::checker::compile::compile_program_with_probes(program);
    let tests = program.test_cases();
    let funs = mutation::mutable_funs(program, &tests, &[]);
    let mut mutants = mutation::queue_mutants(program, &tests, &[]);
    mutants.extend(mutation::mutants(program, &funs, &code.probes));
    for m in mutants.iter().take(64) {
        let mutant = mutation::apply(program, m);
        let _ = snappea::checker::compile::compile_program(&mutant);
    }
}

fn assert_span_in(text: &str, span: Span) {
    let (start, end) = (span.start as usize, span.end as usize);
    assert!(start <= end && end <= text.len(), "span {start}..{end} outside the {}-byte input", text.len());
    assert!(text.is_char_boundary(start) && text.is_char_boundary(end), "span {start}..{end} splits a character");
}
