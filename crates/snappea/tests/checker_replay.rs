//! Replaying schedules (`--replay`), the `.schedule` format, and the program-state hash
//! that searches rely on.

use std::cell::RefCell;
use std::rc::Rc;

use snappea::checker::engine::{CheckResult, Checker, CheckerOptions, Output};
use snappea::checker::random::RandomValueGenerator;
use snappea::checker::runtime::{Config, Runtime, Setup};
use snappea::checker::strategy::{OpInfo, RandomStrategy, ScheduleStep, ScheduleTrace, Strategy, StrategyKind};

struct Quiet;
impl Output for Quiet {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
}

fn compile(src: &str) -> snappea::sema::Analysis {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", src.to_string());
    snappea::sema::compile(&sm, &snappea::sema::Options::default()).unwrap_or_else(|_| panic!("does not compile"))
}

fn check(src: &str, opts: &CheckerOptions) -> CheckResult {
    let a = compile(src);
    let tc = a.program.test_cases().into_iter().next().unwrap();
    Checker::new(&a.program).check(&tc, opts, &mut Quiet).unwrap()
}

fn replay(src: &str, trace: ScheduleTrace) -> CheckResult {
    check(src, &CheckerOptions { strategy: StrategyKind::Replay, replay: Some(trace), ..Default::default() })
}

/// The schedule of the first bug `src` has.
fn buggy_schedule(src: &str) -> ScheduleTrace {
    let r = check(src, &CheckerOptions { iterations: 10, ..Default::default() });
    assert_eq!(r.report.num_bugs, 1, "{:?}", r.report.bug_reports);
    ScheduleTrace::parse(r.schedule_trace.as_ref().unwrap()).unwrap()
}

/// `B` blocks in a `receive` that is never satisfied, so a replay that stops early would
/// look like a deadlock.
const BLOCKED: &str = "event E; event F;
    machine B { start state S { entry { receive { case E: {} } } } }
    machine Main {
      start state S {
        entry {
          var b: B;
          b = new B();
          send b, F;
          send b, F;
          assert false, \"real bug\";
        }
      }
    }
    test tc [main=Main]: { Main, B };";

const CHOOSE: &str = "machine Main {
      start state S {
        entry {
          var s: seq[int];
          s += (0, 10); s += (1, 20);
          assert choose(s) == 100, \"boom\";
        }
      }
    }
    test tc [main=Main]: { Main };";

fn assert_diverged(r: &CheckResult, error: &str) {
    assert_eq!(r.replay_error.as_deref(), Some(error));
    assert_eq!(r.report.num_bugs, 0, "{:?}", r.report.bug_reports);
}

#[test]
fn a_buggy_schedule_replays() {
    for src in [BLOCKED, CHOOSE] {
        let r = replay(src, buggy_schedule(src));
        assert_eq!(r.report.num_bugs, 1);
        assert!(r.replay_error.is_none());
    }
}

#[test]
fn replay_stops_at_an_operation_that_is_not_enabled() {
    let mut trace = buggy_schedule(BLOCKED);
    let last = trace.steps.len() - 1;
    trace.steps[last] = ScheduleStep::Operation(9);
    assert_diverged(&replay(BLOCKED, trace), "Trace is not reproducible: cannot detect id '9'.");
}

#[test]
fn replay_stops_where_the_trace_ends() {
    let mut trace = buggy_schedule(BLOCKED);
    trace.steps.pop();
    assert_diverged(&replay(BLOCKED, trace), "Trace is not reproducible: execution is longer than trace.");
}

#[test]
fn replay_stops_at_a_step_of_the_wrong_kind() {
    let trace = buggy_schedule(CHOOSE);
    let at = trace.steps.iter().position(|s| matches!(s, ScheduleStep::Int(_))).unwrap();
    let with = |step| {
        let mut t = trace.clone();
        t.steps[at] = step;
        t
    };
    assert_diverged(
        &replay(CHOOSE, with(ScheduleStep::Bool(true))),
        "Trace is not reproducible: next step is not a nondeterministic integer choice.",
    );
    assert_diverged(
        &replay(CHOOSE, with(ScheduleStep::Operation(1))),
        "Trace is not reproducible: next step is not a nondeterministic choice.",
    );
    let mut t = trace.clone();
    t.steps[at - 1] = ScheduleStep::Int(0);
    assert_diverged(&replay(CHOOSE, t), "Trace is not reproducible: next step is not a scheduling choice.");
}

#[test]
fn replay_stops_at_an_integer_choice_out_of_range() {
    let mut trace = buggy_schedule(CHOOSE);
    let at = trace.steps.iter().position(|s| matches!(s, ScheduleStep::Int(_))).unwrap();
    for (i, max) in [(2, 2), (-1, 2)] {
        trace.steps[at] = ScheduleStep::Int(i);
        assert_diverged(
            &replay(CHOOSE, trace.clone()),
            &format!("Trace is not reproducible: integer choice '{}' is not below the maximum value '{}'.", i, max),
        );
    }
}

#[test]
fn schedule_text_round_trips() {
    let trace = ScheduleTrace {
        is_fair: true,
        liveness_temperature_threshold: Some(50000),
        test_name: Some("tcMain".into()),
        time: Some("timed=1;profile=same-az".into()),
        latency: vec![("Client->Server.eReq".into(), "100..200".into()), ("cost.disk".into(), "5".into())],
        lasso: Some(2),
        steps: vec![
            ScheduleStep::Operation(0),
            ScheduleStep::Bool(true),
            ScheduleStep::Int(3),
            ScheduleStep::Bool(false),
            ScheduleStep::Operation(12),
        ],
    };
    let text = trace.to_text();
    assert_eq!(
        text,
        "--fair-scheduling\n--liveness-temperature-threshold:50000\n--test-method:tcMain\n\
         --snappea-time:timed=1;profile=same-az\n--snappea-latency:Client->Server.eReq=100..200\n\
         --snappea-latency:cost.disk=5\n--snappea-lasso:2\n(0)\nTrue\n3\nFalse\n(12)"
    );
    assert_eq!(ScheduleTrace::parse(&text).unwrap(), trace);
    let plain = ScheduleTrace { steps: vec![ScheduleStep::Operation(1)], ..Default::default() };
    assert_eq!(ScheduleTrace::parse(&plain.to_text()).unwrap(), plain);
}

/// Random scheduling that records the program-state hash at every scheduling point.
struct Hashes {
    inner: RandomStrategy,
    seen: Vec<u64>,
}

impl Strategy for Hashes {
    fn next_operation(&mut self, current: u64, ops: &[OpInfo]) -> Option<u64> {
        self.inner.next_operation(current, ops)
    }
    fn next_bool(&mut self, max: i32) -> bool {
        self.inner.next_bool(max)
    }
    fn next_int(&mut self, max: i32) -> i32 {
        self.inner.next_int(max)
    }
    fn prepare_next_iteration(&mut self) -> bool {
        self.inner.prepare_next_iteration()
    }
    fn scheduled_steps(&self) -> usize {
        self.inner.scheduled_steps()
    }
    fn has_reached_max_steps(&self) -> bool {
        self.inner.has_reached_max_steps()
    }
    fn is_fair(&self) -> bool {
        false
    }
    fn description(&self) -> String {
        "hashes".into()
    }
    fn wants_state_hash(&self) -> bool {
        true
    }
    fn observe_state(&mut self, hash: u64, _live: bool) -> bool {
        self.seen.push(hash);
        true
    }
}

#[test]
fn equal_program_states_hash_equal() {
    // `n` alternates between 0 and 1, so the program keeps returning to the same states
    let a = compile(
        "event eTick;
         machine Main {
           var n: int;
           start state S {
             entry { send this, eTick; }
             on eTick do { n = 1 - n; send this, eTick; }
           }
         }
         test tc [main=Main]: { Main };",
    );
    let tc = a.program.test_cases().into_iter().next().unwrap();
    let (module, main) = a.program.test_case_module(&tc);
    let code = snappea::checker::compile::compile_program(&a.program);
    let config = Config { logging: false, ..Default::default() };
    let run = || {
        let rng = Rc::new(RefCell::new(RandomValueGenerator::new(0)));
        let mut strategy = Hashes { inner: RandomStrategy::new(40, rng), seen: Vec::new() };
        let mut rt = Runtime::new(&a.program, &code, module, &config, &mut strategy, Setup::default());
        rt.run_test(main.unwrap(), &tc.name);
        strategy.seen
    };
    let first = run();
    assert_eq!(first.len(), 40);
    assert_eq!(first, run(), "the same schedule reaches the same states");
    let mut distinct = first.clone();
    distinct.sort_unstable();
    distinct.dedup();
    // start, create and the first send, then a cycle of two events (send and receive each)
    assert!((2..=8).contains(&distinct.len()), "{} distinct states: {:?}", distinct.len(), first);
    assert_eq!(first[first.len() - 1], first[first.len() - 5], "{:?}", first);
}
