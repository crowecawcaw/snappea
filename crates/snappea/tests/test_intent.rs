//! Declared negative controls and boundary tests keep their meaning in a mixed suite.
use snappea::host::MemHost;

fn run(src: &str, args: &[&str]) -> (i32, serde_json::Value, String) {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", src);
    let mut command = vec!["check", "model.p", "--no-mutation", "--json", "--warn", "lint"];
    command.extend(args);
    let code = snappea::cli::run(&command.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut h);
    let json = serde_json::from_str(&h.stdout).unwrap_or_else(|_| panic!("{} {}", h.stdout, h.stderr));
    (code, json, h.stderr)
}

#[test]
fn expected_bug_must_match_and_missing_complete_bug_is_a_failure() {
    let source = |body: &str| {
        format!(
            r#"
        machine Main {{ start state S {{ entry {{ {body} }} }} }}
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Main]: {{ Main }};
    "#
        )
    };
    let (code, report, _) = run(&source("assert false, \"known flaw\";"), &[]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["test_cases"][0]["result"], "expected_bug");
    assert_eq!(report["test_cases"][0]["expected_bug"]["matched"], true);
    assert_eq!(report["summary"]["status"], "passed");
    for body in ["assert false, \"another flaw\";", "assert true;"] {
        let (code, report, _) = run(&source(body), &[]);
        assert_eq!(code, 1, "{report}");
        assert_eq!(report["test_cases"][0]["result"], "expectation_failed");
        assert_eq!(report["test_cases"][0]["expected_bug"]["matched"], false);
    }
    let (_, report, _) = run(&source("assert true;"), &[]);
    assert_eq!(report["test_cases"][0]["search_complete"], true);
    assert_eq!(report["test_cases"][0]["expected_bug"]["outcome"], "unreachable");
}

#[test]
fn boundary_suppresses_only_the_low_diversity_note() {
    let source = r#"
        machine Main { start state S { entry { assert true; } } }
        // snappea: boundary -- lifecycle end point
        test tcBoundary [main=Main]: { Main };
    "#;
    let (_, report, _) = run(source, &[]);
    let case = &report["test_cases"][0];
    assert_eq!(case["boundary"], "lifecycle end point");
    assert!(case.get("little_to_explore").is_none());
    let (_, plain, _) = run(&source.replace("// snappea: boundary -- lifecycle end point", ""), &[]);
    assert_eq!(plain["test_cases"][0]["little_to_explore"], true);
    assert_eq!(case["coverage"], plain["test_cases"][0]["coverage"]);
    assert_eq!(case["schedules"], plain["test_cases"][0]["schedules"]);
    let (code, failed, _) = run(&source.replace("assert true", "assert false"), &[]);
    assert_eq!(code, 1);
    assert_eq!(failed["test_cases"][0]["result"], "bug");
}

#[test]
fn negative_driver_coverage_cannot_fill_positive_design_or_shared_helper_gaps() {
    let source = r#"
        fun Shared(b: bool): int { if (b) { return 1; } return 0; }
        fun OnlyNegative(): int { return 2; }
        machine Model { start state S { entry (b: bool) { assert Shared(b) >= 0; } } }
        machine Positive { start state S { entry { new Model(false); } } }
        machine Negative { start state S { entry {
            assert OnlyNegative() == 2;
            assert Shared(true) == 1;
            new Model(true);
            assert false, "known flaw";
        } } }
        test tcPositive [main=Positive]: { Positive, Model };
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Negative]: { Negative, Model };
    "#;
    let (code, report, _) = run(source, &[]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["summary"]["status"], "coverage_failed", "{report}");
    let gaps = report["coverage"]["uncovered"].as_array().unwrap();
    assert!(gaps.iter().any(|g| g["context"].as_str().unwrap().contains("Shared")), "{report}");
    assert!(!gaps.iter().any(|g| g["context"].as_str().unwrap().contains("OnlyNegative")), "{report}");
    let negative = report["test_cases"].as_array().unwrap().iter().find(|t| t["name"] == "tcNegative").unwrap();
    assert!(negative["expected_bug_design_coverage"].is_object());
    assert_eq!(negative["result"], "expected_bug");
    let (_, only_negative, _) = run(source, &["tcNegative"]);
    let excluded = only_negative["coverage"]["excluded_components"].as_array().unwrap();
    assert!(!excluded.iter().any(|g| g["context"] == "Model"), "{only_negative}");
}

#[test]
fn intent_directives_validate_values_and_bind_to_test_declarations() {
    use snappea::syntax::SourceMap;
    let parse = |directives: &str| {
        let mut sources = SourceMap::new();
        sources.add("model.p", format!("machine M {{ start state S {{}} }}\n{directives}\ntest tc [main=M]: {{M}};"));
        let findings = snappea_lint::lint(&sources).unwrap();
        (snappea_lint::test_intents(&sources).unwrap(), findings)
    };
    let (intents, findings) =
        parse("// snappea: expect-bug \"text with -- and \\\"quotes\\\"\"\n// snappea: boundary -- deliberate");
    assert!(!findings.iter().any(|f| f.rule == "invalid-directive"), "{findings:?}");
    assert_eq!(intents[0].expected_bug.as_deref(), Some("text with -- and \"quotes\""));
    assert_eq!(intents[0].boundary.as_deref(), Some("deliberate"));
    for directive in [
        "// snappea: expect-bug \"\"",
        "// snappea: expect-bug unquoted",
        "// snappea: boundary",
        "// snappea: boundary -- ",
        "// snappea: boundary -- a\n// snappea: boundary -- b",
    ] {
        assert!(parse(directive).1.iter().any(|f| f.rule == "invalid-directive"), "{directive}");
    }
}

#[test]
fn progress_lints_follow_selected_positive_tests() {
    let source = r#"
        event ePing: Client; event ePong;
        machine Counter { start state S { on ePing do (c: Client) { send c, ePong; } } }
        machine Client {
            start state Init { entry (c: Counter) { send c, ePing, this; goto Waiting; } }
            state Waiting { on ePong goto Done; }
            state Done { }
        }
        machine Driver { start state S { entry { new Client(new Counter()); } } }
        machine Idle { start state S { } }
        test tcWait [main=Driver]: { Driver, Client, Counter };
        test tcIdle [main=Idle]: { Idle, Counter };
    "#;
    let (_, selected, stderr) = run(source, &["tcIdle"]);
    assert!(!stderr.contains("no-progress-spec"), "{selected}\n{stderr}");
    let (_, _, stderr) = run(source, &["tcWait"]);
    assert!(stderr.contains("no-progress-spec"), "{stderr}");
    let negative = source.replace("test tcWait", "// snappea: expect-bug \"known\"\ntest tcWait");
    let (_, _, stderr) = run(&negative, &["tcWait"]);
    assert!(!stderr.contains("no-progress-spec"), "{stderr}");
}

#[test]
fn negative_scenarios_do_not_close_positive_scenario_gaps() {
    let source = r#"
        event eSpecial; event eOrdinary;
        machine Model { start state S { } }
        machine Positive { start state S { entry { new Model(); announce eOrdinary; } } }
        machine Negative { start state S { entry { new Model(); announce eSpecial; assert false, "known"; } } }
        scenario Special observes eSpecial, eOrdinary {
            start hot state Waiting { on eSpecial goto Accepted; ignore eOrdinary; }
            cold state Accepted { ignore eSpecial, eOrdinary; }
        }
        test tcPositive [main=Positive]: { Positive, Model };
        // snappea: expect-bug "known"
        test tcNegative [main=Negative]: { Negative, Model };
    "#;
    let (code, report, _) = run(source, &[]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["summary"]["scenario_gaps"], 1);
    let negative = report["test_cases"].as_array().unwrap().iter().find(|t| t["name"] == "tcNegative").unwrap();
    assert_eq!(negative["scenarios"]["scenarios"][0]["satisfied"], true);
    assert_eq!(report["scenarios"].as_array().unwrap().len(), 1);
}

#[test]
fn an_expected_bug_test_is_never_a_mutation_killer() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", "machine M { start state S { entry { assert false, \"known\"; } } }\n// snappea: expect-bug \"known\"\ntest tc [main=M]: {M};");
    let args = ["check", "mutation", "model.p", "--warn", "lint", "--json"];
    let code = snappea::cli::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut h);
    let report: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["mutation"]["skipped"], "no passing positive test case");
    assert!(report["summary"]["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str().unwrap().contains("no passing positive")));
}

#[test]
fn a_budget_stop_is_not_reported_as_an_unreachable_expected_bug() {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let mut h = MemHost::new("/w");
    h.clock = || START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64();
    h.add_file("model.p", "event E; machine M { var n: int; start state S { entry { send this, E; } on E do { n = n + 1; send this, E; } } }\n// snappea: expect-bug \"known\"\ntest tc [main=M]: {M};");
    let args = ["check", "model.p", "--no-mutation", "--warn", "lint", "--timeout", "1s", "--json"];
    let code = snappea::cli::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut h);
    let report: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["test_cases"][0]["expected_bug"]["outcome"], "not_found");
    assert_eq!(report["test_cases"][0]["search_complete"], false);
}

#[test]
fn expected_text_in_a_source_excerpt_does_not_match_an_unrelated_failure() {
    let source = r#"
        event E;
        machine Main { var target: machine; start state S { entry {
            // known flaw
            send target, E;
        } } }
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Main]: { Main };
    "#;
    let (code, report, _) = run(source, &[]);
    assert_eq!(code, 1, "{report}");
    let case = &report["test_cases"][0];
    assert_eq!(case["expected_bug"]["outcome"], "wrong_failure");
    assert!(case["bug"]["report"].as_str().unwrap().contains("known flaw"));
    let (_, actual, _) = run(&source.replace("expect-bug \"known flaw\"", "expect-bug \"null target\""), &[]);
    assert_eq!(actual["test_cases"][0]["expected_bug"]["outcome"], "matched");
}

#[test]
fn assertion_source_path_is_not_an_expected_failure_message() {
    let source = r#"
        machine Main { start state S { entry { assert false, "different defect"; } } }
        // snappea: expect-bug "model.p"
        test tc [main=Main]: { Main };
    "#;
    let (code, report, _) = run(source, &[]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["test_cases"][0]["expected_bug"]["outcome"], "wrong_failure");
}

#[test]
fn explicit_seeds_repeat_only_negative_controls_and_keep_replays() {
    let src = r#"
        machine Positive { start state S { } }
        machine Negative { start state S { entry { assert false, "known flaw"; } } }
        test tcPositive [main=Positive]: { Positive };
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Negative]: { Negative };
    "#;
    let mut h = MemHost::new("/w");
    h.add_file("model.p", src);
    let command = ["check", "model.p", "--no-mutation", "--json", "--warn", "all", "--expect-bug-seeds", "1,2,3"];
    assert_eq!(
        snappea::cli::run(&command.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut h),
        0,
        "{}",
        h.stdout
    );
    let report: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    let cases = report["test_cases"].as_array().unwrap();
    assert_eq!(cases.iter().filter(|c| c["name"] == "tcPositive").count(), 1);
    assert_eq!(cases.iter().filter(|c| c["name"] == "tcNegative").count(), 3);
    assert_eq!(report["expected_bug_seed_checks"][0]["status"], "consistent");
    for seed in 1..=3 {
        let case = cases.iter().find(|c| c["seed"] == seed).unwrap();
        assert_eq!(case["result"], "expected_bug");
        assert_eq!(case["bug"]["identity"]["kind"], "assertion");
        assert_eq!(case["bug"]["identity"]["machine"], "Negative");
        assert_eq!(case["bug"]["identity"]["file"], "model.p");
        let schedule = case["bug"]["schedule"].as_str().unwrap();
        assert!(schedule.contains(&format!("tcNegative/seed-{seed}/")), "{schedule}");
        use snappea::host::Host;
        assert!(h.is_file(&std::path::Path::new("/w").join(schedule)));
    }
}

#[test]
fn different_assertions_with_identical_messages_are_reported_across_seeds() {
    let src = r#"
        machine Main { start state S { entry {
            if ($) { assert false, "known flaw"; } else { assert false, "known flaw"; }
        } } }
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Main]: { Main };
    "#;
    let (code, report, _) =
        run(src, &["--expect-bug-seeds", "1,2,3,4,5,6,7,8", "--workers", "1", "--warn", "coverage"]);
    assert_eq!(code, 0, "{report}");
    let check = &report["expected_bug_seed_checks"][0];
    assert_eq!(check["status"], "different_failures", "{report}");
    assert_eq!(check["all_matched"], true);
    assert_eq!(report["warnings"]["expected_bug_failure_identity"], 1);
    let columns: std::collections::BTreeSet<_> =
        check["runs"].as_array().unwrap().iter().map(|r| r["identity"]["column"].as_u64()).collect();
    assert_eq!(columns.len(), 2, "{check}");
    let mut host = MemHost::new("/w");
    host.add_file("model.p", src);
    let args = [
        "check",
        "model.p",
        "--no-mutation",
        "--expect-bug-seeds",
        "1,2,3,4,5,6,7,8",
        "--workers",
        "1",
        "--warn",
        "lint,coverage",
    ];
    let code = snappea::cli::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut host);
    assert_eq!(code, 0, "{} {}", host.stdout, host.stderr);
    assert!(host.stdout.contains("1 expected-bug test with differing failures"), "{}", host.stdout);
    assert!(!host.stdout.contains("undetected mutant"), "{}", host.stdout);
}

#[test]
fn every_seed_must_match_even_when_comparing_failures() {
    for body in ["assert false, \"wrong\";", "assert true;"] {
        let src = format!("machine Main {{ start state S {{ entry {{ {body} }} }} }}\n// snappea: expect-bug \"known flaw\"\ntest tcNegative [main=Main]: {{ Main }};");
        let (code, report, _) = run(&src, &["--expect-bug-seeds", "1,2"]);
        assert_eq!(code, 1, "{report}");
        assert_eq!(report["failed"], 2);
        assert_eq!(report["expected_bug_seed_checks"][0]["all_matched"], false);
        assert_eq!(report["expected_bug_seed_checks"][0]["status"], "incomplete");
    }
}

#[test]
fn failure_identity_ignores_formatted_values_at_the_same_assertion() {
    let src = r#"
        machine Main { start state S { entry {
            assert false, format("known flaw value {0}", choose(100));
        } } }
        // snappea: expect-bug "known flaw"
        test tcNegative [main=Main]: { Main };
    "#;
    let (code, report, _) = run(src, &["--expect-bug-seeds", "1,2,3,4", "--workers", "1"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["expected_bug_seed_checks"][0]["status"], "consistent", "{report}");
    let messages: std::collections::BTreeSet<_> =
        report["test_cases"].as_array().unwrap().iter().map(|c| c["bug"]["report"].as_str()).collect();
    assert!(messages.len() > 1, "fixture needs distinct formatted values: {report}");
}

#[test]
fn seed_matrix_options_reject_duplicates_and_unmarked_selection() {
    let src = "machine Main { start state S { } } test tc [main=Main]: { Main };";
    for seeds in ["1,1", "1", "1,no", "1,4294967296"] {
        let (code, _, _) = run(src, &["--expect-bug-seeds", seeds]);
        assert_eq!(code, 2, "{seeds}");
    }
    let (code, report, _) = run(src, &["--expect-bug-seeds", "1,2"]);
    assert_eq!(code, 2, "{report}");
}

#[test]
fn seed_comparison_captures_spec_liveness_and_runtime_exception_sites() {
    let liveness = r#"
        event eStarted;
        machine Main { start state S { entry { announce eStarted; } } }
        spec Progress observes eStarted {
            start state Idle { on eStarted goto RequestPending; }
            hot state RequestPending { ignore eStarted; }
        }
        // snappea: expect-bug "RequestPending"
        test tcNegative [main=Main]: assert Progress in { Main };
    "#;
    let (code, report, _) = run(liveness, &["--expect-bug-seeds", "1,2", "--workers", "1", "--warn", "coverage"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["expected_bug_seed_checks"][0]["status"], "consistent");
    let identity = &report["test_cases"][0]["bug"]["identity"];
    assert_eq!(identity["kind"], "liveness");
    assert_eq!(identity["machine"], "Progress");
    assert_eq!(identity["state"], "RequestPending");
    let exception = r#"
        event e;
        machine Main { start state S { entry { var target: machine; send target, e; } } }
        // snappea: expect-bug "null"
        test tcNegative [main=Main]: { Main };
    "#;
    let (code, report, _) = run(exception, &["--expect-bug-seeds", "1,2"]);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["expected_bug_seed_checks"][0]["status"], "consistent");
    assert_eq!(report["test_cases"][0]["bug"]["identity"]["kind"], "send-null-target");
}
