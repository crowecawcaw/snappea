use snappea::checker::code_coverage::{CodeCoverage, CodeMap, Hits, PointKind, Scope};
use snappea::checker::coverage::CoverageTables;
use snappea::checker::engine::{Checker, CheckerOptions};

fn coverage(source: &str) -> CodeCoverage {
    let mut sources = snappea::syntax::SourceMap::new();
    sources.add("model.p", source);
    let analysis = snappea::sema::compile(&sources, &Default::default()).expect("compiles");
    let prog = &analysis.program;
    let tc = prog.test_cases().remove(0);
    let checker = Checker::with_code_coverage(prog);
    let options = CheckerOptions { iterations: 200, seed: 42, activity_coverage: true, ..Default::default() };
    let result = checker.check(&tc, &options, &mut Vec::new()).expect("runs");
    assert_eq!(result.report.num_bugs, 0, "{:?}", result.report.bug_reports);
    let hits = Hits::from_info(prog, &CoverageTables::new(prog), result.coverage.as_ref().unwrap());
    CodeMap::new(prog, &checker.code.probes, &Scope::test_case(prog, &tc)).coverage(&hits)
}

#[test]
fn full_branch_coverage_can_leave_a_condition_outcome_missing() {
    let c = coverage(
        r#"
        machine Main {
            start state S {
                entry {
                    var a: bool;
                    var b: bool;
                    a = $;
                    b = true;
                    if (a && b) { print "both"; } else { print "other"; }
                }
            }
        }
    "#,
    );
    let s = c.summary();
    assert_eq!((s.branches.covered, s.branches.total), (2, 2));
    assert_eq!((s.conditions.covered, s.conditions.total), (3, 4));
    assert!(c.uncovered().iter().any(|u| u.what == "condition b never false"));
    assert_eq!(CodeCoverage::from_json(&c.to_json(true)).unwrap(), c);
}

#[test]
fn probes_preserve_short_circuit_and_evaluate_calls_once() {
    let c = coverage(
        r#"
        machine Main {
            var calls: int;
            fun Tick(): bool { calls = calls + 1; return true; }
            start state S {
                entry {
                    if (false && Tick()) { print "never"; }
                    if (true || Tick()) { print "short circuit"; }
                    if (!(Tick() ==> false)) { print "negation and implication"; }
                    if (Tick() <==> true) { print "equivalence"; }
                    while (calls < 3) { Tick(); }
                    assert calls == 3;
                }
            }
        }
    "#,
    );
    let s = c.summary();
    // Four Tick conditions (two never evaluated), and one while condition.
    assert_eq!((s.conditions.covered, s.conditions.total), (4, 10));
    assert!(!c.points.iter().any(|p| p.missed.contains("condition calls == 3")));
}

#[test]
fn transition_pairs_must_occur_together_not_just_cover_both_edges() {
    let c = coverage(
        r#"
        machine Main {
            var viaA: bool;
            start state S {
                entry { if ($) { viaA = true; goto A; } else { goto B; } }
            }
            state A { entry { GoB(); } }
            fun GoB() { goto B; }
            state B { entry { if (viaA) { goto D; } else { goto C; } } }
            state C { }
            state D { }
        }
    "#,
    );
    let s = c.summary();
    assert_eq!((s.transition_pairs.covered, s.transition_pairs.total), (3, 5));
    let gaps: Vec<_> = c.uncovered().into_iter().filter(|u| u.what.contains("consecutively")).map(|u| u.what).collect();
    assert_eq!(gaps, ["A -> B -> C never taken consecutively", "S -> B -> D never taken consecutively"]);
}

#[test]
fn transition_history_is_not_combined_across_instances_or_schedules() {
    let c = coverage(
        r#"
        machine Main {
            start state S { entry { new Worker(true); new Worker(false); } }
        }
        machine Worker {
            var first: bool;
            start state S {
                entry (p: bool) { first = p; if (first) { goto A; } else { goto B; } }
            }
            state A { entry { goto B; } }
            state B { entry { if (!first) { goto C; } } }
            state C { }
        }
    "#,
    );
    let s = c.summary();
    assert_eq!((s.transition_pairs.covered, s.transition_pairs.total), (2, 3));
    let missing = c
        .points
        .iter()
        .enumerate()
        .find(|(_, p)| p.kind == PointKind::TransitionPair && p.missed.starts_with("A -> B -> C"))
        .unwrap()
        .0;
    assert!(!c.hit.contains(&(missing as u32)));
    assert!(c.uncovered().iter().any(|u| u.what == "A -> B -> C never taken consecutively"));
}

#[test]
fn observe_merges_witnessed_pairs_without_joining_separate_executions() {
    use snappea::checker::observe::{ObserveOptions, ObservedEvent, Parser, Spec, Timestamp};
    use snappea::checker::value::Value;
    struct Events;
    impl Parser for Events {
        fn name(&self) -> &str {
            "Events"
        }
        fn parse(&self, entry: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
            Ok(vec![spec.event("key", Timestamp::default(), entry, Value::Null, None)?])
        }
    }
    let mut sources = snappea::syntax::SourceMap::new();
    sources.add(
        "spec.p",
        r#"
        event eA; event eB; event eStep;
        spec Paths observes eA, eB, eStep {
            start state S { on eA goto A; on eB goto B; }
            state A { on eStep goto B; }
            state B { on eStep goto C; }
            state C { }
        }
    "#,
    );
    let analysis = snappea::sema::compile(&sources, &Default::default()).unwrap();
    let spec = Spec::new(&analysis.program, "Paths").unwrap();
    let run = |events: &[&str]| {
        let entries = events.iter().map(|e| e.to_string()).collect();
        let report = spec.observe(&[entries], &Events, &ObserveOptions::default(), &mut || 0.0);
        assert!(report.errors.is_empty());
        report.coverage
    };
    let mut merged = run(&["eA", "eStep"]);
    let right = run(&["eB", "eStep"]);
    merged.merge(&right).unwrap();
    assert_eq!((merged.summary().transition_pairs.covered, merged.summary().transition_pairs.total), (2, 3));
    assert!(merged.uncovered().iter().any(|u| u.what == "A -> B -> C never taken consecutively"));
    let restored = CodeCoverage::from_json(&merged.to_json(true)).unwrap();
    assert_eq!(restored, merged);
    merged.merge(&run(&["eA", "eStep", "eStep"])).unwrap();
    assert_eq!(merged.summary().transition_pairs.covered, 3);
}

#[test]
fn return_after_endless_loop_is_not_required_coverage() {
    // P requires a `return` after `while (true)`; it can never run, so it is not a gap.
    let c = coverage(
        r#"
        fun f(): bool {
            var i: int;
            while (true) {
                i = i + 1;
                if (i > 2) { return true; }
            }
            return false;
        }
        fun g(): int {
            var i: int;
            while (true) {
                i = i + 1;
                if (i > 2) { break; }
            }
            return i;
        }
        fun h(): int {
            var i: int;
            while (true) {
                i = i + 1;
                if (i > 2) { return i; }
            }
            if (i > 5) { i = 0; }
            return i;
        }
        event eGo;
        machine Main {
            var x: int;
            start state S {
                entry {
                    assert f();
                    assert g() == 3;
                    assert h() == 3;
                    send this, eGo;
                    receive {
                        case eGo: {
                            while (true) {
                                x = x + 1;
                                if (x > 2) { return; }
                            }
                            x = 0;
                        }
                    }
                }
            }
        }
    "#,
    );
    // nothing after the loops is required: no statement, branch or condition gaps
    assert!(c.uncovered().is_empty(), "{:?}", c.uncovered());
}
