//! Queue-policy mutations must exercise the actual deferred/discarded event, not just a state entry.
use snappea::checker::{
    code_coverage::Hits,
    engine::{Checker, CheckerOptions},
    mutation::{self, Change},
    strategy::StrategyKind,
};
use snappea::host::MemHost;

const MODEL: &str = r#"
event Data: machine; event Ready; event Ack; event Noise; event Never; event Plan;
machine Server {
    start state Waiting {
        defer Data;
        ignore Noise, Never;
        on Ready goto Serving;
    }
    state Serving {
        on Data do (client: machine) { send client, Ack; }
        ignore Noise, Never;
    }
}
machine Main {
    start state Start { entry {
        var server: Server;
        announce Plan;
        server = new Server();
        send server, Data, this;
        send server, Noise;
        send server, Ready;
        receive { case Ack: { } }
    }
    ignore Never;
    }
}
machine Unselected { start state S { ignore Noise; } }
spec Completion observes Plan, Ack {
    start state Initial { on Plan goto Pending; }
    hot state Pending { on Ack goto Done; }
    cold state Done { }
}
test tcQueue [main=Main]: assert Completion in { Main, Server };
"#;

fn program(source: &str) -> snappea::sema::Program {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("queue.p", source.to_string());
    snappea::sema::compile(&sm, &Default::default()).unwrap().program
}

#[test]
fn queue_policy_mutants_detect_loss_and_unhandled_events_without_touching_functions() {
    let prog = program(MODEL);
    let options =
        CheckerOptions { strategy: StrategyKind::Dfs, iterations: 0, activity_coverage: true, ..Default::default() };
    let checker = Checker::with_code_coverage(&prog);
    let baseline = checker.check(&prog.test_cases()[0], &options, &mut Vec::new()).unwrap();
    assert_eq!(baseline.report.num_bugs, 0);
    assert!(baseline.exhaustive.as_ref().unwrap().complete());
    let hits = Hits::from_info(&prog, &checker.coverage_tables(), baseline.coverage.as_ref().unwrap());
    let mutants = mutation::queue_mutants(&prog, &prog.test_cases(), &[]);
    assert!(mutants.iter().all(|m| m.context.starts_with("Server.")));
    let reached: Vec<_> = mutants.iter().filter(|m| m.reached(&hits)).collect();
    assert_eq!(reached.len(), 3, "{reached:?}");
    for kind in [Change::RemoveDefer, Change::DeferToIgnore, Change::RemoveIgnore] {
        let m = reached.iter().find(|m| m.change == kind).unwrap();
        assert!(m.changed(&hits));
        let mutated = mutation::apply(&prog, m);
        assert_eq!(format!("{:?}", prog.funs), format!("{:?}", mutated.funs));
        let result = Checker::new(&mutated).check(&mutated.test_cases()[0], &options, &mut Vec::new()).unwrap();
        assert!(result.report.num_bugs > 0, "{}: {:?}", m.description, result.report);
        if kind == Change::DeferToIgnore {
            assert!(
                result
                    .report
                    .bug_reports
                    .iter()
                    .any(|b| b.contains("hot") || b.to_ascii_lowercase().contains("deadlock")),
                "{:?}",
                result.report.bug_reports
            );
        } else {
            assert!(
                result.report.bug_reports.iter().any(|b| b.contains("Unhandled") || b.contains("cannot be handled")),
                "{:?}",
                result.report.bug_reports
            );
        }
    }
    // Applying each mutant uses a clone: all original queue declarations remain intact.
    assert_eq!(mutation::queue_mutants(&prog, &prog.test_cases(), &[]).len(), mutants.len());
    let server = prog.machines.iter().position(|m| m.name == "Server").unwrap();
    let mut drivers = vec![false; prog.machines.len()];
    drivers[server] = true;
    assert!(mutation::queue_mutants(&prog, &prog.test_cases(), &drivers).is_empty());
}

#[test]
fn timed_delivery_records_deferred_and_ignored_policies_too() {
    let prog = program(MODEL);
    let options = CheckerOptions {
        strategy: StrategyKind::Random,
        iterations: 8,
        timed: true,
        activity_coverage: true,
        ..Default::default()
    };
    let checker = Checker::with_code_coverage(&prog);
    let result = checker.check(&prog.test_cases()[0], &options, &mut Vec::new()).unwrap();
    assert_eq!(result.report.num_bugs, 0);
    let hits = Hits::from_info(&prog, &checker.coverage_tables(), result.coverage.as_ref().unwrap());
    let reached: Vec<_> =
        mutation::queue_mutants(&prog, &prog.test_cases(), &[]).into_iter().filter(|m| m.reached(&hits)).collect();
    assert_eq!(reached.len(), 3, "{reached:?}");
}

#[test]
fn queue_mutation_cli_reports_reached_policies_and_honors_kind_suppressions() {
    let source = MODEL.replace("defer Data;", "// snappea-lint: allow mutation:defer-to-ignore -- intentional delayed-data loss for this fixture\n        defer Data;");
    let mut host = MemHost::new("/w");
    host.add_file("queue.p", &source);
    let args = ["check", "mutation", "queue.p", "--json", "--warn", "all", "--list-suppressions"].map(str::to_owned);
    assert_eq!(snappea::cli::run(&args, &mut host), 0, "{}{}", host.stdout, host.stderr);
    let output: serde_json::Value = serde_json::from_str(&host.stdout).unwrap();
    let report = &output["mutation"];
    let text = report.to_string();
    assert_eq!(report["mutants"], 3, "{report}");
    assert_eq!(report["detected"], 3, "{report}");
    assert!(!text.contains("Never"), "an event never queued must not create a run: {report}");
    assert!(
        report["suppressed_details"].as_array().unwrap().iter().any(|m| m["mutation"] == "defer-to-ignore"),
        "{report}"
    );
    assert_eq!(report["stale_suppressions"].as_array().unwrap().len(), 0, "{report}");
}

#[test]
fn bodyless_design_states_and_raised_events_have_real_mutation_targets() {
    for body in ["", "entry { raise E; }"] {
        let source = format!(
            r#"
            event E;
            machine Sink {{ start state S {{ {body} ignore E; }} }}
            machine Main {{ start state S {{ entry {{ var sink: Sink; sink = new Sink(); send sink, E; }} }} }}
            test tc [main=Main]: {{ Main, Sink }};
        "#
        );
        let prog = program(&source);
        let options = CheckerOptions {
            strategy: StrategyKind::Dfs,
            iterations: 0,
            activity_coverage: true,
            ..Default::default()
        };
        let checker = Checker::with_code_coverage(&prog);
        let baseline = checker.check(&prog.test_cases()[0], &options, &mut Vec::new()).unwrap();
        assert_eq!(baseline.report.num_bugs, 0);
        let hits = Hits::from_info(&prog, &checker.coverage_tables(), baseline.coverage.as_ref().unwrap());
        let mutants = mutation::queue_mutants(&prog, &prog.test_cases(), &[]);
        assert_eq!(mutants.len(), 1);
        assert!(mutants[0].reached(&hits));
        assert!(mutants[0].fun.is_none());
        let mutated = mutation::apply(&prog, &mutants[0]);
        let result = Checker::new(&mutated).check(&mutated.test_cases()[0], &options, &mut Vec::new()).unwrap();
        assert!(result.report.num_bugs > 0);
    }
}

#[test]
fn queue_policies_in_ptst_are_not_mutated_even_without_driver_annotations() {
    let mut sm = snappea::syntax::SourceMap::new();
    sm.add("pTsT/Queue.p", MODEL.to_string());
    let prog = snappea::sema::compile(&sm, &Default::default()).unwrap().program;
    assert!(mutation::queue_mutants(&prog, &prog.test_cases(), &[]).is_empty());
}
