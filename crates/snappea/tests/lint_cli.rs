//! Lint/check parity over real parsed and type-checked P models, without scheduling
//! when a lint fails. Upstream compatibility remains independent of lint policy.
use snappea::host::MemHost;

fn run(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    snappea::cli::run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), host)
}
const MODEL: &str = "event E;\nspec Passive observes E { start state S { on E goto S; } }\nmachine Main { start state S {} } machine Worker { start state S {} }\ntest t [main=Main]: assert Passive in { Main, Worker };";

#[test]
fn lint_uses_the_shared_json_output_for_results_rules_and_failures() {
    let mut h = MemHost::new("/w");
    h.terminal = false;
    h.add_file("model.p", MODEL);
    for command in [&["check", "lint"][..], &["check"], &["check", "lint"]] {
        assert_eq!(run(&mut h, &[command, &["model.p"]].concat()), 2);
        let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
        assert_eq!(result["exit_code"], 2);
        assert!(result["error"].as_str().unwrap().contains("permissive-monitor"));
    }
    assert_eq!(run(&mut h, &["check", "lint", "--list-rules"]), 0);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["rules"].as_array().unwrap().len(), 8);
    h.add_file(
        "model.p",
        "// snappea-lint: allow no-model-machine -- CLI lint success fixture\nmachine Main { start state S {} }",
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--json-report", "lint.json"]), 0);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"], serde_json::json!([]));
    assert_eq!(result["exit_code"], 0);
}

#[test]
fn frozen_agent_models_type_check_and_report_the_expected_weaknesses() {
    for (source, expected) in [
        (include_str!("../../snappea-lint/tests/models/agent-p01.p"), "tautological-assert"),
        (include_str!("../../snappea-lint/tests/models/agent-p03.p"), "unused-payload-field"),
        (include_str!("../../snappea-lint/tests/models/agent-p06.p"), "permissive-monitor"),
    ] {
        let mut h = MemHost::new("/w");
        h.add_file("model.p", source);
        assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
        assert!(h.stderr.contains(&format!("error[{expected}]")), "{}{}", h.stdout, h.stderr);
        assert!(h.files_under("PCheckerOutput").is_empty());
    }
}

#[test]
fn lint_and_check_fail_with_same_findings_before_any_output_is_written() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    let lint = h.stderr.clone();
    assert!(lint.contains("error[permissive-monitor]: ") && lint.contains(" --> model.p:2:6\n"), "{lint}");
    assert_eq!(run(&mut h, &["check", "model.p"]), 2);
    // the same findings, then how to check anyway
    assert!(h.stderr.starts_with(&lint), "{}", h.stderr);
    assert!(h.stderr.ends_with("hint: to look for bugs before addressing these, pass --warn lint; the findings are then reported as warnings\n"), "{}", h.stderr);
    assert!(!h.stdout.contains("Checking "), "{}", h.stdout);
    assert!(h.files_under("PCheckerOutput").is_empty());
}

#[test]
fn warn_lint_reports_findings_as_warnings_and_checks_anyway() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint,coverage"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(h.stderr.contains("warning[permissive-monitor]: ") && !h.stderr.contains("error["), "{}", h.stderr);
    assert!(
        h.stderr.contains("Lint: 1 finding(s) reported as warnings (--warn lint); checking anyway."),
        "{}",
        h.stderr
    );
    assert!(h.stdout.contains("Checking "), "{}", h.stdout);
    assert!(h.stdout.contains("passed in ") && h.stdout.contains(", with warnings: 1 lint finding, "), "{}", h.stdout);
    // `all` warns on every check; JSON counts what was only warned about
    h.terminal = false;
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "all"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["warnings"]["lint"], 1, "{}", result);
    assert!(result["warnings"]["coverage"].as_u64().unwrap() > 0, "{}", result);
    h.terminal = true;
    // other checks keep their defaults, and a single check overrides `all`
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint"]), 1, "{}{}", h.stdout, h.stderr);
    assert!(h.stdout.contains("Coverage requirements were not met"), "{}", h.stdout);
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "all", "--deny", "lint"]), 2);
    assert!(h.stderr.contains("error[permissive-monitor]"), "{}", h.stderr);
    // bugs still fail
    h.add_file(
        "model.p",
        &MODEL
            .replace("machine Main { start state S {} }", "machine Main { start state S { entry { assert false; } } }"),
    );
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "all"]), 1, "{}{}", h.stdout, h.stderr);
    assert!(!h.stdout.contains("with warnings"), "{}", h.stdout);
}

#[test]
fn warn_and_deny_reject_unknown_and_conflicting_checks() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lints"]), 2);
    assert!(
        h.stderr.contains("unknown check 'lints' for '--warn': use lint, coverage, mutation or all"),
        "{}",
        h.stderr
    );
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint", "--deny", "lint"]), 2);
    assert!(h.stderr.contains("'--warn lint' and '--deny lint' conflict: use one"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--no-mutation", "--deny", "mutation"]), 2);
    assert!(h.stderr.contains("use only one of '--no-mutation' and '--deny mutation'"), "{}", h.stderr);
}

#[test]
fn scoped_suppression_allows_check_but_does_not_hide_model_bugs() {
    let mut h = MemHost::new("/w");
    let model = MODEL.replace(
        "spec Passive",
        "// snappea-lint: allow permissive-monitor, coverage -- intentionally passive fixture\nspec Passive",
    );
    h.add_file("model.p", &model);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0, "{}{}", h.stdout, h.stderr);
    assert_eq!(h.stdout, "Lint passed.\n");
    assert!(h.files_under("PCheckerOutput").is_empty());
    assert_eq!(run(&mut h, &["check", "model.p"]), 0, "{}{}", h.stdout, h.stderr);
    h.add_file(
        "model.p",
        &model
            .replace("machine Main { start state S {} }", "machine Main { start state S { entry { assert false; } } }"),
    );
    assert_eq!(run(&mut h, &["check", "model.p"]), 1, "{}{}", h.stdout, h.stderr);
}

#[test]
fn no_model_machine_reports_the_entry_and_suggests_a_separate_machine() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", "machine Main { start state S {} }\ntest t [main=Main]: { Main };");
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(h.stderr.contains("error[no-model-machine]"), "{}", h.stderr);
    assert!(h.stderr.contains("--> model.p:2:14"), "{}", h.stderr);
    assert!(h.stderr.contains("put modeled behavior in a separate `machine`"), "{}", h.stderr);

    h.add_file(
        "model.p",
        "machine Main { start state S {} }\n// snappea-lint: allow no-model-machine -- fixed trace\ntest t [main=Main]: { Main };",
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0);
}

#[test]
fn source_discovery_pproj_and_parse_errors_work_without_running_schedules() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "Model.pproj",
        "<Project><ProjectName>Model</ProjectName><InputFiles><PFile>model.p</PFile></InputFiles></Project>",
    );
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "lint"]), 2);
    assert!(h.stderr.contains("permissive-monitor"));
    assert_eq!(run(&mut h, &["check", "lint", "Model.pproj"]), 2);
    assert!(h.stderr.contains("permissive-monitor"));
    h.add_file("model.p", "machine Main { broken");
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(!h.stdout.contains("Lint passed"));
    h.add_file("model.p", "machine Main { start state S { entry { assert missing; } } }");
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(!h.stdout.contains("Lint passed"));
}

#[test]
fn help_rules_and_bad_options_do_not_need_sources() {
    let mut h = MemHost::new("/empty");
    assert_eq!(run(&mut h, &["check", "lint", "--list-rules"]), 0);
    assert_eq!(h.stdout.lines().count(), 8);
    assert!(h.stdout.contains("unused-payload-field"));
    assert_eq!(run(&mut h, &["check", "lint", "--help"]), 0);
    assert!(h.stdout.contains("Usage: snappea check lint"));
    // output options before the subcommand
    assert_eq!(run(&mut h, &["--json", "check", "lint", "--list-rules"]), 0);
    assert!(h.stdout.starts_with('{'), "{}", h.stdout);
    assert_eq!(run(&mut h, &["check", "lint", "--schedules", "1"]), 2);
    assert_eq!(run(&mut h, &["check", "lint", "testName"]), 2);
    assert_eq!(run(&mut h, &["check", "lint", "--list-rules", "model.p"]), 2);
}

#[test]
fn upstream_compatibility_does_not_apply_lint_policy() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    let args = ["check", "--pfiles", "model.p", "--schedules", "1"].map(str::to_string);
    assert_eq!(snappea::compat::run(&args, &mut h), 0, "{}{}", h.stdout, h.stderr);
    assert!(!h.stdout.contains("permissive-monitor") && !h.stderr.contains("permissive-monitor"));
}

/// Issue #106: a spec handler for an event its `observes` list leaves out never runs, so a
/// passing check could hide a property that is never checked.
const DEAD_HANDLER: &str = r#"event eA: int;
event eB: int;
spec Watch observes eA {
  start state S {
    on eA do (n: int) { assert n >= 0, "negative"; }
    on eB do (n: int) { assert n < 10, "too big"; }
  }
}
machine Main { start state S { entry { new Worker(); } } }
machine Worker { start state S { entry { announce eA, 1; announce eB, 2; } } }
test t [main=Main]: assert Watch in { Main, Worker };
"#;

#[test]
fn unobserved_handler_fails_check_before_execution_and_the_fix_passes() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", DEAD_HANDLER);
    assert_eq!(run(&mut h, &["check", "model.p", "--no-mutation"]), 2);
    assert!(
        h.stderr.contains("error[unobserved-handler]: spec 'Watch' handles event 'eB' in state 'S'"),
        "{}",
        h.stderr
    );
    assert!(h.stderr.contains(" --> model.p:6:8\n"), "{}", h.stderr);
    assert!(h.stderr.contains("add the event to the monitor's `observes` list, or remove the handler"), "{}", h.stderr);
    assert!(h.stderr.contains("`// snappea-lint: allow unobserved-handler -- reason`"), "{}", h.stderr);
    assert!(!h.stdout.contains("Checking "), "{}", h.stdout);
    assert!(h.files_under("PCheckerOutput").is_empty());
    // upstream's warning text is not repeated next to the finding
    assert!(!h.stdout.contains("[Warning:]") && !h.stderr.contains("[Warning:]"), "{}{}", h.stdout, h.stderr);

    // JSON output carries the finding
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--json"]), 2);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["exit_code"], 2);
    assert!(result["error"].as_str().unwrap().contains("error[unobserved-handler]"), "{}", h.stdout);

    h.add_file("model.p", &DEAD_HANDLER.replace("observes eA {", "observes eA, eB {"));
    assert_eq!(run(&mut h, &["check", "model.p", "--no-mutation"]), 0, "{}{}", h.stdout, h.stderr);
}

#[test]
fn unobserved_handler_suppression_is_scoped_to_the_next_handler() {
    let mut h = MemHost::new("/w");
    let suppressed = DEAD_HANDLER.replace(
        "    on eB do",
        "    // snappea-lint: allow unobserved-handler, coverage -- kept for the next protocol version\n    on eB do",
    );
    h.add_file("model.p", &suppressed);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0, "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--no-mutation"]), 0, "{}{}", h.stdout, h.stderr);

    // a second dead handler in the same state is still reported
    h.add_file(
        "model.p",
        &suppressed.replace("  }\n}\nmachine Main", "    ignore eC;\n  }\n}\nevent eC;\nmachine Main"),
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(h.stderr.contains("handles event 'eC'") && !h.stderr.contains("handles event 'eB'"), "{}", h.stderr);
}

#[test]
fn scenario_states_that_cannot_report_coverage_fail_lint() {
    let base =
        "event eA;\nmachine Main { start state S { entry { announce eA; } } }\nmachine Worker { start state S {} }\n";
    let test = "test t [main=Main]: { Main, Worker };\n";
    let mut h = MemHost::new("/w");
    h.add_file(
        "model.p",
        &format!("{base}scenario Never observes eA {{ start state A {{ on eA goto B; }} state B {{}} }}\n{test}"),
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(
        h.stderr.contains("error[scenario-never-accepts]: scenario 'Never' has no accepting (cold) state"),
        "{}",
        h.stderr
    );
    assert!(h.stderr.contains("mark the state the scenario should reach `cold`"), "{}", h.stderr);

    h.add_file(
        "model.p",
        &format!("{base}scenario Early observes eA {{ start cold state A {{ on eA goto A; }} }}\n{test}"),
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(
        h.stderr.contains("error[scenario-cold-start]: scenario 'Early' starts in accepting (cold) state 'A'"),
        "{}",
        h.stderr
    );
    assert!(!h.stderr.contains("scenario-never-accepts"), "{}", h.stderr);

    h.add_file(
        "model.p",
        &format!("{base}scenario Done observes eA {{ start state A {{ on eA goto B; }} cold state B {{}} }}\n{test}"),
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0, "{}", h.stderr);
}

#[test]
fn observe_enforces_the_spec_rules_but_not_model_test_rules() {
    let spec = "event eA: int;\nevent eB: int;\nspec Watch observes eA {\n  start state S {\n    on eA do (n: int) { assert n >= 0; }\n    on eB do (n: int) { assert n < 10; }\n  }\n}\n";
    let mut h = MemHost::new("/w");
    h.add_file("spec.p", spec);
    h.add_file("log.jsonl", "{\"event\":\"eA\",\"key\":\"k\",\"payload\":1,\"timestamp\":1}\n");
    let args = ["observe", "spec.p", "--spec", "Watch", "-l", "log.jsonl"];
    assert_eq!(run(&mut h, &args), 2);
    assert!(h.stderr.contains("error[unobserved-handler]") && h.stderr.contains(" --> spec.p:6:8\n"), "{}", h.stderr);
    assert!(h.stderr.contains("The P specification has lint findings."), "{}", h.stderr);
    assert!(!h.stdout.contains("ok "), "{}", h.stdout);

    // observation-only specs are not unselected; a fixed spec checks the log
    h.add_file("spec.p", &spec.replace("observes eA {", "observes eA, eB {"));
    assert_eq!(run(&mut h, &args), 0, "{}{}", h.stdout, h.stderr);

    // a scoped suppression allows the intentional case
    h.add_file(
        "spec.p",
        &spec.replace("    on eB", "    // snappea-lint: allow unobserved-handler -- not emitted yet\n    on eB"),
    );
    assert_eq!(run(&mut h, &args), 0, "{}{}", h.stdout, h.stderr);
}

#[test]
fn test_library_rejects_unobserved_handlers() {
    let spec = "event eA: int;\nevent eB: int;\nspec Watch observes eA {\n  start state S {\n    on eA do (n: int) { assert n >= 0; }\n    on eB do (n: int) { assert n < 10; }\n  }\n}\n";
    let Err(e) = snappea::test::Model::compile(&[("spec.p", spec)]) else { panic!("expected a lint finding") };
    assert_eq!(e.kind, snappea::test::ErrorKind::Config);
    assert!(e.message.contains("error[unobserved-handler]") && e.message.contains("--> spec.p:6:8"), "{}", e.message);
    let fixed = spec.replace("observes eA {", "observes eA, eB {");
    assert!(snappea::test::Model::compile(&[("spec.p", &fixed)]).is_ok());
}

#[test]
fn upstream_compatibility_keeps_the_warnings() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", DEAD_HANDLER);
    let args = ["check", "--pfiles", "model.p", "--schedules", "1"].map(str::to_string);
    assert_eq!(snappea::compat::run(&args, &mut h), 0, "{}{}", h.stdout, h.stderr);
    assert!(
        h.stdout.contains("[Warning:] [model.p:6:8] event 'eB' is not in the observes list of spec monitor 'Watch'"),
        "{}",
        h.stdout
    );
    assert!(!h.stderr.contains("unobserved-handler"));
}
