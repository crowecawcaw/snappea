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
