//! Search strategies: coverage feedback and its progress checkpoints, auto mode, the
//! exhaustive search (`dfs`, `random-dfs`) and the Q-learning strategy (`rl`).

use snappea::checker::engine::{CheckResult, Checker, CheckerOptions, Output};
use snappea::checker::strategy::{ScheduleTrace, StrategyKind};

struct Quiet;
impl Output for Quiet {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
}

#[test]
fn frontier_feedback_reaches_repeated_data_and_delivery_stages_and_replays() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("stages.p", include_str!("models/guided_prefix.p").to_string());
    let compiled = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let checker = Checker::new(&compiled.program);
    for case in compiled.program.test_cases() {
        for seed in 0..3 {
            let options = CheckerOptions {
                strategy: StrategyKind::Feedback,
                seed,
                iterations: 200,
                max_fair_steps: 1000,
                ..Default::default()
            };
            let result = checker.check(&case, &options, &mut Quiet).unwrap();
            assert_eq!(result.report.num_bugs, 1, "{} seed {seed}", case.name);
            let replay = checker
                .check(
                    &case,
                    &CheckerOptions {
                        strategy: StrategyKind::Replay,
                        replay: Some(ScheduleTrace::parse(result.schedule_trace.as_ref().unwrap()).unwrap()),
                        ..options
                    },
                    &mut Quiet,
                )
                .unwrap();
            assert!(replay.replay_error.is_none());
            assert_eq!(replay.report.bug_reports, result.report.bug_reports);
        }
    }
}

fn check(src: &str, opts: &CheckerOptions) -> CheckResult {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", src.to_string());
    let a =
        snappea::sema::compile(&sm, &snappea::sema::Options::default()).unwrap_or_else(|_| panic!("does not compile"));
    let tc = a.program.test_cases().into_iter().find(|t| t.name == "tcMain").unwrap();
    Checker::new(&a.program).check(&tc, opts, &mut Quiet).unwrap()
}

fn opts(strategy: StrategyKind, iterations: usize) -> CheckerOptions {
    CheckerOptions { strategy, iterations, seed: 7, ..Default::default() }
}

/// Three clients send to a server; `ORDER` decides which delivery order fails.
fn clients(check: &str) -> String {
    format!(
        "event eHello: int;
         machine Server {{
           var got: seq[int];
           start state Init {{
             on eHello do (x: int) {{
               got += (sizeof(got), x);
               if (sizeof(got) == 3) {{ {check} }}
             }}
           }}
         }}
         machine Client {{
           start state Init {{
             entry (p: (s: Server, x: int)) {{ send p.s, eHello, p.x; }}
           }}
         }}
         machine Main {{
           start state Init {{
             entry {{
               var s: Server;
               s = new Server();
               new Client((s = s, x = 1));
               new Client((s = s, x = 2));
               new Client((s = s, x = 3));
             }}
           }}
         }}
         test tcMain [main=Main]: {{ Main, Server, Client }};"
    )
}

#[test]
fn dfs_explores_everything_of_a_correct_test() {
    let r = check(&clients("assert sizeof(got) == 3;"), &opts(StrategyKind::Dfs, 0));
    assert_eq!(r.report.num_bugs, 0);
    let x = r.exhaustive.expect("search stats");
    assert!(x.finished && x.depth_bound_hits == 0, "{:?}", x);
    assert_eq!(x.cycle_prunes, 0, "joining another path is not an ancestor cycle");
    assert!(r.report.explored_unfair_schedules > 6, "{:?}", r.report.explored_unfair_schedules);
    // caching: far fewer schedules than interleavings (every pruned one ends early)
    assert!(x.pruned_schedules > 0, "{:?}", x);
    // the random order explores the same state space
    let rr = check(&clients("assert sizeof(got) == 3;"), &opts(StrategyKind::RandomDfs, 0));
    let y = rr.exhaustive.unwrap();
    assert!(y.finished);
    assert_eq!(x.distinct_states, y.distinct_states);
}

#[test]
fn dfs_finds_a_specific_interleaving_and_its_schedule_replays() {
    let src = clients("assert !(got[0] == 3 && got[1] == 2 && got[2] == 1), \"reversed\";");
    for strategy in [StrategyKind::Dfs, StrategyKind::RandomDfs] {
        let r = check(&src, &opts(strategy, 0));
        assert_eq!(r.report.num_bugs, 1, "{strategy}");
        assert!(r.report.bug_reports[0].contains("reversed"));
        assert!(!r.exhaustive.unwrap().finished);
        let trace = ScheduleTrace::parse(r.schedule_trace.as_ref().unwrap()).unwrap();
        let replay = check(&src, &CheckerOptions { replay: Some(trace), ..opts(StrategyKind::Replay, 1) });
        assert_eq!(replay.report.num_bugs, 1, "{strategy}");
        assert!(replay.replay_error.is_none());
    }
}

#[test]
fn dfs_branches_on_nondeterministic_choices() {
    let src = "machine Main {
                 start state Init {
                   entry {
                     var i: int;
                     var n: int;
                     while (i < 4) { if ($) { n = n + 1; } i = i + 1; }
                     assert n + choose(3) != 6, \"all\";
                   }
                 }
               }
               test tcMain [main=Main]: { Main };";
    let r = check(src, &opts(StrategyKind::Dfs, 0));
    assert_eq!(r.report.num_bugs, 1);
    let ok = check(&src.replace("!= 6", "!= 7"), &opts(StrategyKind::Dfs, 0));
    assert_eq!(ok.report.num_bugs, 0);
    assert!(ok.exhaustive.unwrap().finished);
}

#[test]
fn dfs_reports_a_hot_monitor_at_the_end() {
    let src = "event eReq; event eAck;
               spec Live observes eReq, eAck {
                 start cold state Idle { on eReq goto Waiting; }
                 hot state Waiting { on eAck goto Idle; }
               }
               machine Main {
                 start state Init {
                   entry { announce eReq; if ($) { announce eAck; } }
                 }
               }
               test tcMain [main=Main]: assert Live in { Main };";
    let r = check(src, &opts(StrategyKind::Dfs, 0));
    assert_eq!(r.report.num_bugs, 1);
    assert!(r.report.bug_reports[0].contains("hot state 'Waiting'"), "{}", r.report.bug_reports[0]);
}

#[test]
fn dfs_distinguishes_ancestor_cycles_from_acyclic_monitor_progress() {
    let cycle = "event eTick;
        machine Main { start state Running {
            entry { send this, eTick; }
            on eTick do { send this, eTick; }
        } }
        test tcMain [main=Main]: { Main };";
    for strategy in [StrategyKind::Dfs, StrategyKind::RandomDfs] {
        let result = check(cycle, &opts(strategy, 0));
        let stats = result.exhaustive.unwrap();
        assert!(stats.finished && stats.depth_bound_hits == 0);
        assert!(stats.cycle_prunes > 0);
        let finite = "event eBegin; event eEnd;
            spec Live observes eBegin, eEnd {
                start cold state Idle { on eBegin goto Waiting; }
                hot state Waiting { on eEnd goto Finished; }
                cold state Finished { }
            }
            machine Main { start state Init { entry { announce eBegin; announce eEnd; } } }
            test tcMain [main=Main]: assert Live in { Main };";
        let result = check(finite, &opts(strategy, 0));
        assert_eq!(result.report.num_bugs, 0);
        let stats = result.exhaustive.unwrap();
        assert!(stats.finished && stats.depth_bound_hits == 0);
        assert_eq!(stats.cycle_prunes, 0);
    }
}

#[test]
fn dfs_reports_when_the_depth_bound_cut_the_search() {
    let src = "event eTick;
               machine Main {
                 var n: int;
                 start state Init {
                   entry { send this, eTick; }
                   on eTick do { n = n + 1; send this, eTick; }
                 }
               }
               test tcMain [main=Main]: { Main };";
    let r = check(src, &CheckerOptions { max_unfair_steps: 50, ..opts(StrategyKind::Dfs, 0) });
    assert_eq!(r.report.num_bugs, 0);
    let x = r.exhaustive.unwrap();
    assert!(x.finished);
    assert_eq!(x.depth_bound_hits, 1);
}

#[test]
fn dfs_statistics_count_the_schedule_that_found_the_bug() {
    let src = "machine Main { start state Init { entry { assert $, \"first\"; } } }
               test tcMain [main=Main]: { Main };";
    let r = check(src, &opts(StrategyKind::Dfs, 0));
    assert_eq!((r.report.num_bugs, r.report.explored_unfair_schedules), (1, 1));
    let trace = ScheduleTrace::parse(r.schedule_trace.as_ref().unwrap()).unwrap();
    assert_eq!(r.exhaustive.unwrap().max_depth, trace.steps.len());
}

#[test]
fn rl_is_reproducible_finds_bugs_and_replays() {
    let src = clients("assert !(got[0] == 3 && got[1] == 2 && got[2] == 1), \"reversed\";");
    let a = check(&src, &opts(StrategyKind::Rl, 1000));
    let b = check(&src, &opts(StrategyKind::Rl, 1000));
    assert_eq!(a.report.num_bugs, 1);
    assert_eq!(a.schedule_trace, b.schedule_trace);
    assert_eq!(a.report.explored_fair_schedules, b.report.explored_fair_schedules);
    assert!(a.strategy_description.starts_with("rl[seed '7']"));
    let trace = ScheduleTrace::parse(a.schedule_trace.as_ref().unwrap()).unwrap();
    let replay = check(&src, &CheckerOptions { replay: Some(trace), ..opts(StrategyKind::Replay, 1) });
    assert_eq!(replay.report.num_bugs, 1);
    assert!(replay.replay_error.is_none());
    // a correct test stays clean
    let ok = check(&clients("assert sizeof(got) == 3;"), &opts(StrategyKind::Rl, 200));
    assert_eq!(ok.report.num_bugs, 0);
    assert_eq!(ok.report.explored_fair_schedules, 200);
}

#[test]
fn native_feedback_uses_the_fair_bound_and_detects_hot_cycles() {
    let src = "event eTick;
        spec Live observes eTick { start hot state Waiting { on eTick do {} } }
        machine Main { start state Running {
            entry { send this, eTick; }
            on eTick do { announce eTick; send this, eTick; }
        } }
        test tcMain [main=Main]: assert Live in { Main };";
    for strategy in [StrategyKind::Feedback, StrategyKind::FeedbackPct, StrategyKind::FeedbackPos] {
        let options = CheckerOptions {
            max_unfair_steps: 100,
            max_fair_steps: 1000,
            liveness_temperature_threshold: Some(500),
            strategy_bound: 10,
            ..opts(strategy, 1)
        };
        let corrected = check(src, &options);
        assert_eq!(corrected.report.num_bugs, 1, "{strategy}");
        assert!(corrected.report.bug_reports[0].contains("potential liveness bug"));
        let trace = ScheduleTrace::parse(corrected.schedule_trace.as_ref().unwrap()).unwrap();
        let replay =
            check(src, &CheckerOptions { strategy: StrategyKind::Replay, replay: Some(trace), ..options.clone() });
        assert_eq!(replay.report.bug_reports, corrected.report.bug_reports);
        // The compatibility CLI preserves upstream's shorter bound and random stream.
        let upstream = check(src, &CheckerOptions { upstream_feedback: true, ..options });
        assert_eq!(upstream.report.num_bugs, 0);
        assert_eq!(upstream.report.max_fair_steps, Some(100));
    }
}

#[test]
fn corrected_feedback_is_reproducible_with_fresh_exploration_and_replays() {
    let src = clients("assert !(got[0] == 3 && got[1] == 2 && got[2] == 1), \"reversed\";");
    let options = opts(StrategyKind::Feedback, 1000);
    let a = check(&src, &options);
    let b = check(&src, &options);
    assert_eq!(a.report.num_bugs, 1);
    assert_eq!(a.schedule_trace, b.schedule_trace);
    let trace = ScheduleTrace::parse(a.schedule_trace.as_ref().unwrap()).unwrap();
    let replay = check(&src, &CheckerOptions { strategy: StrategyKind::Replay, replay: Some(trace), ..options });
    assert_eq!(replay.report.bug_reports, a.report.bug_reports);
    assert!(replay.replay_error.is_none());
}

#[test]
fn guided_search_develops_rare_data_and_scheduling_prefixes_and_replays_them() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("prefix.p", include_str!("models/guided_prefix.p").to_string());
    let analysis = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let checker = Checker::new(&analysis.program);
    for name in ["tcDataPrefix___depth_20", "tcSchedulePrefix___depth_12"] {
        let tc = analysis.program.test_cases().into_iter().find(|tc| tc.name == name).unwrap();
        let options = CheckerOptions { seed: 1, ..opts(StrategyKind::Feedback, 100_000) };
        let guided = checker.check(&tc, &options, &mut Quiet).unwrap();
        assert_eq!(guided.report.num_bugs, 1, "{name}");
        let schedules = guided.report.explored_fair_schedules + guided.report.explored_unfair_schedules;
        assert!(schedules < options.iterations, "{name}: {schedules}");

        // Equal attempt budget as well as a fixed seed: this is actual bug discovery,
        // not a comparison of incomparable timeline counts or wall-clock thresholds.
        let random = checker
            .check(
                &tc,
                &CheckerOptions { strategy: StrategyKind::Random, iterations: schedules, ..options.clone() },
                &mut Quiet,
            )
            .unwrap();
        assert_eq!(random.report.num_bugs, 0, "{name}");
        let trace = ScheduleTrace::parse(guided.schedule_trace.as_ref().unwrap()).unwrap();
        let replay = checker
            .check(&tc, &CheckerOptions { strategy: StrategyKind::Replay, replay: Some(trace), ..options }, &mut Quiet)
            .unwrap();
        assert_eq!(replay.report.bug_reports, guided.report.bug_reports);
        assert!(replay.replay_error.is_none());
    }
}

#[test]
fn auto_completes_finite_search_and_replays_failures() {
    let safe = check(&clients("assert sizeof(got) == 3;"), &opts(StrategyKind::Auto, 1000));
    assert_eq!(safe.report.num_bugs, 0);
    assert!(safe.exhaustive.unwrap().finished);
    let buggy = check(&clients("assert false, \"all clients delivered\";"), &opts(StrategyKind::Auto, 1000));
    assert!(buggy.report.num_bugs > 0);
    let replay = check(
        &clients("assert false, \"all clients delivered\";"),
        &CheckerOptions {
            strategy: StrategyKind::Replay,
            replay: Some(ScheduleTrace::parse(buggy.schedule_trace.as_ref().unwrap()).unwrap()),
            ..Default::default()
        },
    );
    assert_eq!(replay.report.bug_reports, buggy.report.bug_reports);
    assert!(replay.replay_error.is_none());
}

#[test]
fn auto_does_not_claim_cyclic_hot_monitors_are_exhausted() {
    let src = "event Tick; spec Progress observes Tick { start hot state Waiting { on Tick do {} } }
        machine Main { start state S { entry { send this, Tick; } on Tick do { announce Tick; send this, Tick; } } }
        test tcMain [main=Main]: assert Progress in { Main };";
    let result =
        check(src, &CheckerOptions { liveness_temperature_threshold: Some(50), ..opts(StrategyKind::Auto, 1000) });
    assert_eq!(result.report.num_bugs, 1);
    assert!(result.report.bug_reports[0].contains("potential liveness bug"));
    assert!(!result.exhaustive.unwrap().finished);
}

#[test]
fn auto_resource_limit_falls_back_without_claiming_exhaustion() {
    let src = "machine Main { start state S { entry { var i: int; var n: int; while (i < 200) { n = choose(10000); i = i + 1; } } } }
        test tcMain [main=Main]: { Main };";
    let result = check(src, &opts(StrategyKind::Auto, 30));
    assert_eq!(result.report.num_bugs, 0);
    assert!(!result.exhaustive.unwrap().finished);
    assert!(result.report.explored_fair_schedules > 0);
}

#[test]
fn auto_cancellation_interrupts_handlers_without_fabricating_completion_or_bugs() {
    struct Cancel {
        polls: usize,
    }
    impl Output for Cancel {
        fn line(&mut self, _: &str) {}
        fn error(&mut self, _: &str) {}
        fn cancelled(&mut self) -> bool {
            self.polls += 1;
            self.polls > 3
        }
    }
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("loop.p", "machine Main { fun Increment(n: int): int { return n + 1; } start state S { entry { var n: int; while (true) { n = Increment(n); } } } } test tcMain [main=Main]: { Main };");
    let analysis = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let tc = analysis.program.test_cases().into_iter().next().unwrap();
    let result = Checker::new(&analysis.program)
        .check(&tc, &CheckerOptions { max_work_per_step: 0, ..opts(StrategyKind::Auto, 0) }, &mut Cancel { polls: 0 })
        .unwrap();
    assert!(result.cancelled);
    assert_eq!(result.report.num_bugs, 0);
    assert!(!result.exhaustive.unwrap().finished);
}

#[test]
fn native_feedback_learns_code_branches_without_activity_coverage_requested() {
    let src = r#"
        machine Main {
            start state Init {
                entry {
                    if ($) { if ($) { if ($) { if ($) { if ($) {
                        if ($) { if ($) { if ($) { if ($) { if ($) {
                            assert false, "deep branch";
                        } } } } }
                    } } } } }
                }
            }
        }
        test tcMain [main=Main]: { Main };
    "#;
    let options = opts(StrategyKind::Feedback, 500);
    let a = check(src, &options);
    let b = check(src, &options);
    assert!(a.report.num_bugs > 0, "code progress should guide an event-free model");
    assert_eq!(a.schedule_trace, b.schedule_trace);
    let trace = ScheduleTrace::parse(a.schedule_trace.as_ref().unwrap()).unwrap();
    let replay = check(src, &CheckerOptions { strategy: StrategyKind::Replay, replay: Some(trace), ..options });
    assert_eq!(replay.report.num_bugs, 1);
}

#[test]
fn schedule_observer_counts_every_exploration_and_can_stop_at_all_bugs() {
    use std::collections::BTreeSet;
    #[derive(Default)]
    struct Observer {
        tested: usize,
        bugs: BTreeSet<String>,
    }
    impl Output for Observer {
        fn line(&mut self, _: &str) {}
        fn error(&mut self, _: &str) {}
        fn schedule_finished(&mut self, n: usize, bug: Option<&str>) {
            assert_eq!(n, self.tested + 1);
            self.tested = n;
            if let Some(b) = bug {
                self.bugs.insert(b.to_string());
            }
        }
        fn cancelled(&mut self) -> bool {
            self.bugs.len() == 2
        }
    }
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add(
        "model.p",
        r#"machine Main { start state Init { entry {
        if ($) { assert false, "one"; } else { assert false, "two"; }
    } } } test tcMain [main=Main]: { Main };"#,
    );
    let a = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let tc = a.program.test_cases().remove(0);
    let mut out = Observer::default();
    let r = Checker::new(&a.program)
        .check(&tc, &CheckerOptions { full_exploration: true, ..opts(StrategyKind::Dfs, 100) }, &mut out)
        .unwrap();
    assert_eq!(out.bugs.len(), 2);
    assert_eq!(out.tested, 2, "trace rendering must not count as exploration");
    assert_eq!(r.report.explored_unfair_schedules, out.tested);
}

#[test]
fn native_feedback_records_atomic_condition_probes_and_compat_does_not_enable_them() {
    use snappea::checker::bytecode::ProbeKind;
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add(
        "model.p",
        r#"machine Main { start state Init { entry {
        if ($ && $ && $) { print "all true"; }
    } } } test tcMain [main=Main]: { Main };"#,
    );
    let a = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let tc = a.program.test_cases().remove(0);
    let checker = Checker::with_code_coverage(&a.program);
    let options = CheckerOptions { activity_coverage: true, ..opts(StrategyKind::Feedback, 200) };
    let native = checker.check(&tc, &options, &mut Quiet).unwrap().coverage.unwrap();
    let conditions: Vec<_> = checker
        .code
        .probes
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.kind, ProbeKind::ConditionTrue | ProbeKind::ConditionFalse))
        .collect();
    assert_eq!(conditions.len(), 6, "two outcomes of each of three atomic conditions");
    assert!(conditions.iter().all(|(id, _)| native.probes.contains(*id as u32)));
    let legacy = Checker::new(&a.program)
        .check(&tc, &CheckerOptions { upstream_feedback: true, ..options }, &mut Quiet)
        .unwrap()
        .coverage
        .unwrap();
    assert!(
        checker.code.probes.iter().enumerate().all(|(id, _)| !legacy.probes.contains(id as u32)),
        "compatibility runs must not enable code instrumentation for feedback"
    );
}

#[test]
fn discovery_identity_ignores_instances_but_preserves_distinct_event_names() {
    use snappea::checker::engine::failure_site;
    assert_eq!(failure_site("Worker(12) received event e1"), failure_site("Worker(42) received event e1"));
    assert_ne!(failure_site("Worker(12) received event e1"), failure_site("Worker(12) received event e2"));
    assert_ne!(failure_site("Worker(12) received event e1"), failure_site("Worker2(12) received event e1"));
    assert_eq!(
        failure_site("Assertion Failed: dir with spaces/x.p:3:2 value 12"),
        failure_site("Assertion Failed: dir with spaces/x.p:3:2 value 99")
    );
}

#[test]
fn auto_depth_bound_hands_off_to_fair_feedback_and_replays_the_bug() {
    let src = "machine Main { start state S { entry { if ($) { if ($) { if ($) { assert false; } } } } } } test tcMain [main=Main]: { Main };";
    let options = CheckerOptions { max_unfair_steps: 2, max_fair_steps: 100, ..opts(StrategyKind::Auto, 100) };
    let result = check(src, &options);
    assert_eq!(result.report.num_bugs, 1);
    assert_eq!(result.report.explored_unfair_schedules, 1);
    assert!(result.report.explored_fair_schedules > 0);
    let stats = result.exhaustive.unwrap();
    assert!(!stats.finished);
    assert_eq!(stats.depth_bound_hits, 1);
    let replay = check(
        src,
        &CheckerOptions {
            strategy: StrategyKind::Replay,
            replay: Some(ScheduleTrace::parse(result.schedule_trace.as_ref().unwrap()).unwrap()),
            ..options
        },
    );
    assert_eq!(replay.report.bug_reports, result.report.bug_reports);
    assert!(replay.replay_error.is_none());
}

#[test]
fn auto_keeps_coverage_from_both_sides_of_the_feedback_handoff() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("model.p", "machine Main { start state S { entry { var n: int; var i: int; if ($) { print \"left\"; } else { print \"right\"; } while (i < 200) { n = choose(10000); i = i + 1; } print \"done\"; } } } test tcMain [main=Main]: { Main };");
    let a = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let tc = a.program.test_cases().remove(0);
    let checker = Checker::with_code_coverage(&a.program);
    for seed in 0..20 {
        let options = CheckerOptions { seed, activity_coverage: true, ..opts(StrategyKind::Auto, 1) };
        let dfs = checker.check(&tc, &options, &mut Quiet).unwrap().coverage.unwrap();
        let feedback = checker
            .check(&tc, &CheckerOptions { strategy: StrategyKind::Feedback, ..options.clone() }, &mut Quiet)
            .unwrap()
            .coverage
            .unwrap();
        let mut expected = feedback.probes.clone();
        expected.union(&dfs.probes);
        if expected == feedback.probes {
            continue;
        }
        let both = checker.check(&tc, &CheckerOptions { iterations: 2, ..options }, &mut Quiet).unwrap();
        assert_eq!(both.report.explored_unfair_schedules, 1);
        assert_eq!(both.report.explored_fair_schedules, 1);
        assert_eq!(both.coverage.unwrap().probes, expected, "DFS-only hits must survive the fresh feedback history");
        return;
    }
    panic!("fixture did not exercise distinct DFS/feedback coverage");
}

#[test]
fn auto_keeps_small_recovery_models_in_exhaustive_search() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("prefix.p", include_str!("models/guided_prefix.p"));
    let a = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let checker = Checker::new(&a.program);
    for name in ["tcDataPrefix___depth_24", "tcSchedulePrefix___depth_12", "tcSchedulePrefix___depth_24"] {
        let tc = a.program.test_cases().into_iter().find(|tc| tc.name == name).unwrap();
        let result =
            checker.check(&tc, &CheckerOptions { seed: 1, ..opts(StrategyKind::Auto, 200) }, &mut Quiet).unwrap();
        assert_eq!(
            result.report.num_bugs, 1,
            "{name}: a small recovery model should stay exhaustive long enough to reach its deep bug"
        );
    }
}

#[test]
fn feedback_keeps_boolean_assignment_prefixes_in_addition_to_reported_conditions() {
    let condition = std::iter::repeat_n("$", 20).collect::<Vec<_>>().join(" && ");
    let src = format!("machine Main {{ start state S {{ entry {{ var ready: bool; ready = {condition}; if (ready) {{ assert false, \"rare Boolean assignment\"; }} }} }} }} test tcMain [main=Main]: {{ Main }};");
    let result = check(&src, &opts(StrategyKind::Feedback, 2000));
    assert_eq!(result.report.num_bugs, 1, "if/while probes alone cannot see progress inside the assignment");
}

#[test]
fn auto_exhausts_atomic_inventory() {
    let mut sources = snappea::syntax::SourceMap::new();
    for (name, src) in [
        ("Inventory.p", include_str!("../../../examples/model/PSrc/Inventory.p")),
        ("inventory-spec.p", include_str!("../../../examples/model/PSpec/inventory.p")),
        ("TestDriver.p", include_str!("../../../examples/model/PTst/TestDriver.p")),
        ("TestScript.p", include_str!("../../../examples/model/PTst/TestScript.p")),
    ] {
        sources.add(name, src);
    }
    let analysis = snappea::sema::compile(&sources, &Default::default()).unwrap();
    let tc = analysis.program.test_cases().into_iter().find(|t| t.name == "tcAtomicReserve").unwrap();
    for strategy in [StrategyKind::RandomDfs, StrategyKind::Auto] {
        let result = Checker::new(&analysis.program)
            .check(
                &tc,
                &CheckerOptions { strategy, iterations: 30_000, full_exploration: true, seed: 0, ..Default::default() },
                &mut Quiet,
            )
            .unwrap();
        assert_eq!(result.report.num_bugs, 0);
        assert!(result.exhaustive.as_ref().is_some_and(|s| s.finished));
    }
}

#[test]
fn auto_handoff_uses_default_frontier_guidance_and_replays() {
    let src = include_str!("models/guided_prefix.p");
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("stages.p", src.to_string());
    let compiled = snappea::sema::compile(&sm, &Default::default()).unwrap();
    let case = compiled.program.test_cases().into_iter().find(|tc| tc.name == "tcDataPrefix___depth_24").unwrap();
    let checker = Checker::new(&compiled.program);
    let options = CheckerOptions {
        strategy: StrategyKind::Auto,
        iterations: 200,
        max_unfair_steps: 2,
        max_fair_steps: 1000,
        ..Default::default()
    };
    let result = checker.check(&case, &options, &mut Quiet).unwrap();
    assert_eq!(result.report.num_bugs, 1);
    assert!(result.report.explored_fair_schedules > 0);
    let replay = checker
        .check(
            &case,
            &CheckerOptions {
                strategy: StrategyKind::Replay,
                replay: Some(ScheduleTrace::parse(result.schedule_trace.as_ref().unwrap()).unwrap()),
                ..options
            },
            &mut Quiet,
        )
        .unwrap();
    assert!(replay.replay_error.is_none());
    assert_eq!(replay.report.bug_reports, result.report.bug_reports);
}

#[test]
fn auto_explores_long_actor_delays_and_replays_without_a_coverage_gradient() {
    for background in [false, true] {
        let source = include_str!("models/stalled_write.p.in")
            .replace("ROUNDS", "64")
            .replace("BACKGROUND", if background { "new Maintenance();" } else { "" });
        for seed in 0..10 {
            let options =
                CheckerOptions { seed, max_unfair_steps: 2000, max_fair_steps: 2000, ..opts(StrategyKind::Auto, 256) };
            let result = check(&source, &options);
            assert_eq!(result.report.num_bugs, 1, "background={background} seed={seed}");
            assert!(result.report.bug_reports[0].contains("write still pending"));
            let replay = check(
                &source,
                &CheckerOptions {
                    strategy: StrategyKind::Replay,
                    replay: Some(ScheduleTrace::parse(result.schedule_trace.as_ref().unwrap()).unwrap()),
                    ..options
                },
            );
            assert!(replay.replay_error.is_none());
            assert_eq!(replay.report.bug_reports, result.report.bug_reports);
            let allocated: usize = result.scheduler_stats.iter().map(|s| s.schedules).sum();
            assert!(allocated > 0);
            assert!(allocated <= result.report.explored_fair_schedules + result.report.explored_unfair_schedules);
        }
    }
}

#[test]
fn auto_does_not_reward_revisited_phase_coverage_as_new() {
    let source = include_str!("../../../benchmarks/checker/models/priority-switches-4.p");
    let result = check(
        source,
        &CheckerOptions {
            full_exploration: true,
            max_fair_steps: 1000,
            max_unfair_steps: 1000,
            ..opts(StrategyKind::Auto, 2000)
        },
    );
    assert!(result.scheduler_stats.iter().all(|a| a.schedules > 10));
    assert!(
        result.scheduler_stats.iter().all(|a| a.reward < 20.0),
        "revisiting a phase must restore coverage; repeated features otherwise earn unbounded false credit: {:?}",
        result.scheduler_stats
    );
}
