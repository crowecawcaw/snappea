//! Workers (`WorkerPool`): their merged statistics are those of the schedules each explored.

use std::collections::BTreeSet;

use snappea::checker::engine::{CheckResult, Checker, CheckerOptions, WorkerPool};
use snappea::checker::strategy::StrategyKind;

const MODEL: &str = "
    event eA; event eB; event eNever;
    machine Main {
        start state Init {
            entry {
                var s: Server;
                var i: int;
                s = new Server();
                new Sender((s, eA));
                new Sender((s, eB));
                while (i < 3 && $) { send s, eA; i = i + 1; }
            }
        }
    }
    machine Sender {
        start state S { entry (p: (Server, event)) { send p.0, p.1; } }
    }
    machine Server {
        start state Serving {
            on eA do { if ($) { goto Other; } }
            ignore eB;
        }
        state Other { ignore eA, eB; }
    }
    scenario BBeforeA observes eA, eB {
        start hot state Start { on eB goto Satisfied; on eA goto Missed; }
        hot state Missed { ignore eA, eB; }
        cold state Satisfied { ignore eA, eB; }
    }
    scenario Never observes eNever {
        start hot state Waiting { on eNever goto Done; }
        cold state Done { }
    }
    test tcRace [main = Main]: { Main, Sender, Server };
";

fn run(strategy: StrategyKind, schedules: usize, workers: usize) -> (CheckResult, Vec<CheckResult>) {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", MODEL);
    let analysis = snappea::sema::compile(&sm, &snappea::sema::Options::default()).expect("compiles");
    let tc = analysis.program.test_cases().into_iter().find(|t| t.name == "tcRace").unwrap();
    let checker = Checker::new(&analysis.program);
    let opts = CheckerOptions {
        strategy,
        strategy_bound: 2,
        iterations: schedules,
        seed: 11,
        activity_coverage: true,
        ..Default::default()
    };
    // the workers, one after the other on this thread
    let pool = WorkerPool::new(workers);
    let parts = (0..workers).map(|w| checker.explore(&tc, &opts, &pool, w, &mut Vec::new()).unwrap()).collect();
    let merged = checker.finish(&tc, &opts, parts);
    // the schedules of each worker that has some, on their own
    let alone = (0..workers.min(schedules))
        .map(|w| checker.check(&tc, &pool.worker_options(w, &opts), &mut Vec::new()).unwrap())
        .collect();
    (merged, alone)
}

#[test]
fn merged_statistics_are_those_of_all_workers() {
    for strategy in [StrategyKind::Random, StrategyKind::Pct, StrategyKind::Pos] {
        let (merged, alone) = run(strategy, 203, 4);
        let m = &merged.report;
        let sum = |f: fn(&CheckResult) -> usize| alone.iter().map(f).sum::<usize>();
        assert_eq!(m.explored_fair_schedules + m.explored_unfair_schedules, 203);
        assert_eq!(
            alone
                .iter()
                .map(|r| r.report.explored_fair_schedules + r.report.explored_unfair_schedules)
                .collect::<Vec<_>>(),
            [51, 51, 51, 50]
        );
        assert_eq!(m.explored_fair_schedules, sum(|r| r.report.explored_fair_schedules));
        assert_eq!(m.total_fair_steps, sum(|r| r.report.total_fair_steps));
        assert_eq!(m.min_fair_steps, alone.iter().filter_map(|r| r.report.min_fair_steps).min());
        assert_eq!(m.max_fair_steps, alone.iter().filter_map(|r| r.report.max_fair_steps).max());
        let timelines: BTreeSet<u128> = alone.iter().flat_map(|r| r.report.timelines.clone()).collect();
        assert_eq!(m.timelines, timelines);
        // scenario coverage, in attachment order
        let names: Vec<&str> = m.scenarios.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["BBeforeA", "Never"]);
        let triggered = sum(|r| r.report.scenarios[0].trigger_count);
        assert!(triggered > 0 && triggered < 203, "{}", triggered);
        assert_eq!(m.scenarios[0].trigger_count, triggered);
        let satisfying: BTreeSet<u128> =
            alone.iter().flat_map(|r| r.report.scenarios[0].satisfying_timelines.clone()).collect();
        assert_eq!(m.scenarios[0].satisfying_timelines, satisfying);
        assert_eq!(m.scenarios[1].trigger_count, 0);
        assert_eq!(m.scenarios[1].total_states, alone[0].report.scenarios[1].total_states);
        // activity coverage
        let cov = merged.coverage.as_ref().unwrap();
        for r in &alone {
            let c = r.coverage.as_ref().unwrap();
            assert!(
                c.received.is_subset(&cov.received) && c.sent.is_subset(&cov.sent) && c.links.is_subset(&cov.links)
            );
        }
        let received: usize =
            alone.iter().flat_map(|r| r.coverage.as_ref().unwrap().received.iter()).collect::<BTreeSet<_>>().len();
        assert_eq!(cov.received.len(), received);
    }
}

#[test]
fn one_worker_is_check() {
    let (merged, alone) = run(StrategyKind::Random, 50, 1);
    assert_eq!(format!("{:?}", merged.report), format!("{:?}", alone[0].report));
    assert_eq!(merged.coverage.as_ref().unwrap().received, alone[0].coverage.as_ref().unwrap().received);
}

#[test]
fn workers_without_a_schedule_explore_none() {
    // (a share of 0 schedules would mean no bound if run alone)
    let (merged, alone) = run(StrategyKind::Random, 2, 4);
    assert_eq!(merged.report.explored_fair_schedules + merged.report.explored_unfair_schedules, 2);
    assert_eq!(alone.len(), 2);
}

/// Two workers that hit a bug at the same moment, before either sees the other stop: the run
/// still reports the one bug it ends at.
#[test]
fn simultaneous_bugs_report_one() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add(
        "model.p",
        "machine Main { start state S { entry { assert false, \"always\"; } } } test tc [main = Main]: { Main };",
    );
    let analysis = snappea::sema::compile(&sm, &snappea::sema::Options::default()).expect("compiles");
    let tc = analysis.program.test_cases().into_iter().find(|t| t.name == "tc").unwrap();
    let checker = Checker::new(&analysis.program);
    let opts = CheckerOptions { iterations: 10, ..Default::default() };
    // a pool per worker, so that neither is stopped by the other's bug
    let parts = (0..2).map(|w| checker.explore(&tc, &opts, &WorkerPool::new(2), w, &mut Vec::new()).unwrap()).collect();
    let result = checker.finish(&tc, &opts, parts);
    assert_eq!(result.report.num_bugs, 1);
    assert_eq!(result.report.bug_reports.len(), 1);
    assert!(result.readable_trace.is_some());
}

/// Two clients each pick one of five values and send it; the server fails on one
/// order of values (`bug`), or never (`None`). Small enough for auto mode to exhaust.
fn split_model(bug: Option<(i32, i32)>) -> String {
    let (a, b) = bug.unwrap_or((-1, -1));
    format!(
        "
        event eV: int;
        machine Main {{
            start state Init {{
                entry {{
                    var s: Server;
                    s = new Server();
                    new Client((s, 1));
                    new Client((s, 2));
                }}
            }}
        }}
        machine Client {{
            start state S {{ entry (p: (Server, int)) {{ send p.0, eV, p.1 * 10 + choose(5); }} }}
        }}
        machine Server {{
            var log: seq[int];
            start state R {{
                on eV do (v: int) {{
                    log += (sizeof(log), v);
                    if (sizeof(log) == 2) {{
                        assert !(log[0] == {a} && log[1] == {b}), \"the planted bug\";
                    }}
                }}
            }}
        }}
        test tc [main = Main]: {{ Main, Client, Server }};
        "
    )
}

fn split_checker_run(
    bug: Option<(i32, i32)>,
    f: impl FnOnce(&Checker, &snappea::sema::TestCase) -> CheckResult,
) -> CheckResult {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", split_model(bug));
    let analysis = snappea::sema::compile(&sm, &snappea::sema::Options::default()).expect("compiles");
    let tc = analysis.program.test_cases().remove(0);
    f(&Checker::new(&analysis.program), &tc)
}

/// Auto mode on `workers` workers, one after the other.
fn split_run(bug: Option<(i32, i32)>, workers: usize) -> CheckResult {
    split_checker_run(bug, |checker, tc| {
        // enough schedules for each worker to finish its part
        let opts =
            CheckerOptions { strategy: StrategyKind::Auto, iterations: 4_000 * workers, seed: 3, ..Default::default() };
        let pool = WorkerPool::new(workers);
        let parts = (0..workers).map(|w| checker.explore(tc, &opts, &pool, w, &mut Vec::new()).unwrap()).collect();
        checker.finish(tc, &opts, parts)
    })
}

#[test]
fn split_exhaustive_search_completes_only_when_every_part_does() {
    for workers in [1, 2, 3, 4, 7] {
        let r = split_run(None, workers);
        assert_eq!(r.report.num_bugs, 0);
        let stats = r.exhaustive.unwrap();
        assert!(stats.complete(), "{workers} workers: {stats:?}");
    }
}

#[test]
fn split_exhaustive_search_finds_a_bug_in_any_part() {
    for workers in [1, 4] {
        for (p, q) in [(1, 2), (2, 1)] {
            for x in 0..5 {
                for y in 0..5 {
                    let bug = (p * 10 + x, q * 10 + y);
                    let r = split_run(Some(bug), workers);
                    assert_eq!(r.report.num_bugs, 1, "{workers} workers missed {bug:?}");
                }
            }
        }
    }
}

#[test]
fn split_exhaustive_search_explores_each_state_once() {
    let one = split_run(None, 1).exhaustive.unwrap().distinct_states;
    split_checker_run(None, |checker, tc| {
        let opts = CheckerOptions { strategy: StrategyKind::Auto, iterations: 16_000, seed: 3, ..Default::default() };
        let pool = WorkerPool::new(4);
        let mut total = 0;
        for w in 0..4 {
            let part = checker.explore(tc, &opts, &pool, w, &mut Vec::new()).unwrap();
            total += checker.finish(tc, &opts, vec![part]).exhaustive.unwrap().distinct_states;
        }
        // the workers share explored states, and the split walk records none
        assert!(total <= one, "4 workers explored {total} states, one worker {one}");
        assert!(total * 10 >= one * 9, "4 workers explored {total} states, one worker {one}");
        CheckResult::default()
    });
}

/// Auto mode on 4 workers, for two peers that ping-pong forever; with `announce`, the
/// liveness spec is cold before they start.
fn ping_pong(announce: &str) -> CheckResult {
    let src = format!(
        "event ePing: machine; event eDone;
         machine Peer {{ start state S {{ on ePing do (m: machine) {{ send m, ePing, this; }} }} }}
         machine Main {{ start state S {{ entry {{
           var a: machine; var b: machine;
           {announce}
           a = new Peer(); b = new Peer();
           send a, ePing, b;
         }} }} }}
         spec Progress observes eDone {{
           start hot state Waiting {{ on eDone goto Done; }}
           cold state Done {{ ignore eDone; }}
         }}
         test tc [main = Main]: assert Progress in {{ Main, Peer }};"
    );
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", src);
    let analysis = snappea::sema::compile(&sm, &snappea::sema::Options::default()).expect("compiles");
    let tc = analysis.program.test_cases().remove(0);
    let checker = Checker::new(&analysis.program);
    let opts = CheckerOptions { strategy: StrategyKind::Auto, iterations: 400, seed: 3, ..Default::default() };
    let pool = WorkerPool::new(4);
    let parts = (0..4).map(|w| checker.explore(&tc, &opts, &pool, w, &mut Vec::new()).unwrap()).collect();
    checker.finish(&tc, &opts, parts)
}

#[test]
fn split_search_verifies_liveness_when_cycles_repeat_only_while_specs_are_cold() {
    let r = ping_pong("announce eDone;");
    assert_eq!(r.report.num_bugs, 0, "{:?}", r.report.bug_reports);
    let stats = r.exhaustive.unwrap();
    assert!(stats.cycle_prunes > 0 && stats.live_cycle_prunes == 0, "{stats:?}");
    assert!(stats.complete() && stats.auto_handoffs.is_empty(), "{stats:?}");
}

#[test]
fn split_search_reports_a_fair_hot_cycle() {
    let r = ping_pong("");
    assert_eq!(r.report.num_bugs, 1);
    assert!(r.report.bug_reports[0].contains("repeat them forever"), "{:?}", r.report.bug_reports);
}
