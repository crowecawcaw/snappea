//! Lint/check parity over real parsed and type-checked P models. A failing lint fails
//! `check` but does not hide its bugs. Upstream compatibility remains independent of
//! lint policy.
mod common;

use common::snappea as run;
use snappea::host::MemHost;

const MODEL: &str = "event E;\nspec Passive observes E { start state S { on E goto S; } }\nmachine Main { start state S {} } machine Worker { start state S {} }\ntest t [main=Main]: assert Passive in { Main, Worker };";

#[test]
fn lint_uses_the_shared_json_output_for_results_rules_and_failures() {
    let mut h = MemHost::new("/w");
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    h.add_file("model.p", MODEL);
    for command in [&["check", "lint"][..], &["check"], &["check", "lint"]] {
        assert_eq!(run(&mut h, &[command, &["model.p"]].concat()), 2);
        let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
        assert_eq!(result["exit_code"], 2);
        assert!(result["error"].as_str().unwrap().starts_with("Lint failed"), "{}", h.stdout);
        let findings = if command == ["check"] { &result["lint"]["findings"] } else { &result["findings"] };
        assert!(findings.as_array().unwrap().iter().any(|f| f["rule"] == "permissive-monitor"), "{}", h.stdout);
    }
    assert_eq!(run(&mut h, &["check", "lint", "--list-rules"]), 0);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["rules"].as_array().unwrap().len(), 21);
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
fn lint_and_check_fail_with_same_findings_and_check_still_runs_the_tests() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    let lint = h.stderr.clone();
    assert!(lint.contains("error[permissive-monitor]: ") && lint.contains(" --> model.p:2:6\n"), "{lint}");
    assert_eq!(run(&mut h, &["check", "model.p"]), 2);
    // the same findings, after the test cases ran
    assert_eq!(h.stderr, lint);
    assert!(h.stdout.contains("Checking ") && h.stdout.contains("ok    t "), "{}", h.stdout);
    assert!(
        h.stdout.ends_with("but lint failed; coverage skipped. Fix or suppress each finding above, then check again. Output: PCheckerOutput/\n"),
        "{}",
        h.stdout
    );
    // mutation testing still runs, so one run lists every gap; coverage does not
    assert!(!h.stdout.contains("Coverage") && h.stdout.contains("Mutation testing ("), "{}", h.stdout);
}

#[test]
fn lint_failure_still_reports_bugs_first_and_exits_with_1() {
    let mut h = MemHost::new("/w");
    let buggy = MODEL
        .replace("machine Main { start state S {} }", "machine Main { start state S { entry { assert false; } } }");
    h.add_file("model.p", &buggy);
    assert_eq!(run(&mut h, &["check", "model.p"]), 1, "{}{}", h.stdout, h.stderr);
    // the bug and its replay on stdout, the lint finding on stderr, then the summary
    let fail = h.stdout.find("FAIL  t ").expect(&h.stdout);
    let replay = h.stdout.find("Replay: snappea replay ").expect(&h.stdout);
    assert!(fail < replay, "{}", h.stdout);
    assert!(
        h.stderr.contains("error[permissive-monitor]: ")
            && h.stderr.contains("\nLint failed. Each finding's help says how to fix it")
    );
    assert!(
        h.stdout.lines().last().unwrap().starts_with("1 of 1 test case failed and lint failed in "),
        "{}",
        h.stdout
    );
    assert!(h.stdout.contains("Fix the bugs first"), "{}", h.stdout);
    assert!(!h.stderr.contains("--warn lint"), "{}", h.stderr);

    // JSON: the bug, the findings, and what was skipped
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    assert_eq!(run(&mut h, &["check", "model.p"]), 1, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["exit_code"], 1);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["test_cases"][0]["result"], "bug", "{}", result);
    assert!(result["test_cases"][0]["bug"]["replay"].as_str().is_some(), "{}", result);
    assert_eq!(result["lint"]["failed"], true);
    assert_eq!(result["lint"]["errors"], 1);
    assert_eq!(result["lint"]["findings"][0]["rule"], "permissive-monitor", "{}", result);
    assert!(result["error"].as_str().unwrap().starts_with("Lint failed. Each finding's help"), "{}", result);
    assert_eq!(result["summary"]["status"], "tests_failed");
    assert_eq!(result["summary"]["lint_failed"], true);
    assert_eq!(result["summary"]["message"], "1 of 1 test case failed; lint failed");
    assert_eq!(result["coverage"], serde_json::json!({"skipped": "lint failed"}));
    assert_eq!(result["mutation"], serde_json::json!({"skipped": "lint failed"}));
}

#[test]
fn lint_failure_without_a_bug_exits_with_2_and_reports_the_skipped_stages() {
    let mut h = MemHost::new("/w");
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    h.add_file("model.p", MODEL);
    for (command, skipped) in [(&["check"][..], "; coverage skipped"), (&["check", "mutation"], "")] {
        assert_eq!(run(&mut h, &[command, &["model.p"]].concat()), 2, "{}{}", h.stdout, h.stderr);
        let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
        assert_eq!(result["exit_code"], 2);
        assert_eq!(result["failed"], 0);
        assert_eq!(result["test_cases"][0]["result"], "ok", "{}", result);
        assert_eq!(result["lint"]["findings"][0]["rule"], "permissive-monitor", "{}", result);
        assert_eq!(result["summary"]["status"], "lint_failed");
        assert_eq!(result["summary"]["lint_failed"], true);
        assert_eq!(result["summary"]["coverage_gaps"], 0);
        assert!(result["mutation"]["skipped"].is_null(), "{}", result["mutation"]);
        h.env.remove("SNAPPEA_FORMAT");
        assert_eq!(run(&mut h, &[command, &["model.p"]].concat()), 2);
        assert!(h.stdout.contains(&format!("but lint failed{skipped}. Fix")), "{}", h.stdout);
        h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    }
    // --warn lint still runs the full pipeline
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint"]), 1);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["summary"]["status"], "coverage_failed", "{}", result);
    assert_eq!(result["summary"]["lint_failed"], false);
    assert_eq!(result["warnings"]["lint"], 1);
}

#[test]
fn warn_lint_reports_findings_as_warnings_and_checks_anyway() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint,coverage"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(h.stderr.contains("warning[permissive-monitor]: ") && !h.stderr.contains("error["), "{}", h.stderr);
    assert!(h.stderr.contains("\nLint findings reported as warnings (--warn lint); checking anyway."), "{}", h.stderr);
    assert!(h.stdout.contains("Checking "), "{}", h.stdout);
    assert!(
        h.stdout.contains("passed in ") && h.stdout.contains(", with warnings: lint findings, coverage gaps."),
        "{}",
        h.stdout
    );
    // `all` warns on every check; JSON counts what was only warned about
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "all"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["warnings"]["lint"], 1, "{}", result);
    assert!(result["warnings"]["coverage"].as_u64().unwrap() > 0, "{}", result);
    h.env.remove("SNAPPEA_FORMAT");
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
fn warn_and_deny_reject_unknown_checks_and_apply_in_order() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", MODEL);
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lints"]), 2);
    assert!(
        h.stderr.contains("unknown check 'lints' for '--warn': use lint, coverage, mutation or all"),
        "{}",
        h.stderr
    );
    // the last flag naming a check wins
    assert_eq!(run(&mut h, &["check", "model.p", "--warn", "lint", "--deny", "lint"]), 2);
    assert!(h.stderr.contains("error[permissive-monitor]"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--deny", "lint", "--warn", "all"]), 0, "{}{}", h.stdout, h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--no-mutation", "--deny", "mutation"]), 2);
    assert!(h.stderr.contains("use only one of '--no-mutation' and '--deny mutation'"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--allow-incomplete-coverage"]), 2);
    assert!(
        h.stderr.contains("unknown option '--allow-incomplete-coverage': it was renamed, use '--warn coverage'"),
        "{}",
        h.stderr
    );
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
    assert_eq!(h.stdout.lines().count(), 21);
    assert!(h.stdout.contains("invalid-suppression") && h.stdout.contains("unused-suppression"));
    assert!(h.stdout.contains("unused-payload-field"));
    assert!(h.stdout.contains("announce-and-send"));
    assert!(h.stdout.contains("unchecked-state"));
    assert!(h.stdout.contains("unplanned-driver-stage") && h.stdout.contains("unreachable-statement"));
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
fn unobserved_handler_fails_check_and_the_fix_passes() {
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
    // the test case still runs, and the finding follows it
    assert!(h.stdout.contains("Checking ") && h.stdout.contains("but lint failed; coverage"), "{}", h.stdout);
    // upstream's warning text is not repeated next to the finding
    assert!(!h.stdout.contains("[Warning:]") && !h.stderr.contains("[Warning:]"), "{}{}", h.stdout, h.stderr);

    // JSON output carries the finding
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--json"]), 2);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["exit_code"], 2);
    // the findings themselves, not the report printed on stderr again
    assert!(result["error"].as_str().unwrap().starts_with("Lint failed. Each finding's help"), "{}", h.stdout);
    assert_eq!(result["findings"][0]["rule"], "unobserved-handler", "{}", h.stdout);
    assert_eq!(result["findings"][0]["severity"], "error");
    assert_eq!(result["findings"][0]["line"], 6);
    assert!(result["findings"][0]["help"].as_str().unwrap().starts_with("add the event"), "{}", h.stdout);

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

const SHARED_HELPER: &str = r#"
fun Compute(x: int): int { return x + 1; }
event Result: (input: int, actual: int);
machine Worker { start state S { entry { announce Result, (input = 1, actual = Compute(1)); } } }
machine Main { start state S { entry { new Worker(); } } }
spec Oracle observes Result { start state S { on Result do (p: (input: int, actual: int)) {
    assert p.actual == Compute(p.input);
} } }
test tcShared [main=Main]: assert Oracle in { Main, Worker };
"#;

#[test]
fn shared_spec_helpers_are_advisory_reported_and_failing_only_with_strict() {
    let mut h = MemHost::new("/w");
    h.add_file("shared.p", SHARED_HELPER);
    // an advisory rule is not reported by default, in lint or in check, even when lint is denied
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"], serde_json::json!([]));
    assert!(!h.stderr.contains("shared-spec-helper"), "{}", h.stderr);
    assert_eq!(
        run(&mut h, &["check", "shared.p", "--deny", "lint", "--no-mutation", "--json"]),
        0,
        "{}{}",
        h.stdout,
        h.stderr
    );
    assert!(!h.stderr.contains("shared-spec-helper"), "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert!(result["warnings"].get("lint").is_none(), "{result}");
    assert_eq!(result["test_cases"][0]["result"], "ok");
    // `--strict` reports it, and it fails lint
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--strict", "--json"]), 2, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"][0]["rule"], "shared-spec-helper");
    assert_eq!(result["findings"][0]["severity"], "advisory");
    assert_eq!(result["findings"][0]["file"], "shared.p");
    assert!(h.stderr.contains("warning[shared-spec-helper]"), "{}", h.stderr);
    assert!(h.stderr.contains("\nLint failed. "), "{}", h.stderr);
    assert_eq!(
        run(&mut h, &["check", "shared.p", "--strict", "--no-mutation", "--json"]),
        2,
        "{}{}",
        h.stdout,
        h.stderr
    );
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["summary"]["status"], "lint_failed");
    assert_eq!(result["lint"]["errors"], 1);
    assert_eq!(result["test_cases"][0]["result"], "ok");
    // `--strict --warn lint` reports it without failing
    assert_eq!(
        run(&mut h, &["check", "shared.p", "--strict", "--warn", "lint", "--no-mutation", "--json"]),
        0,
        "{}{}",
        h.stdout,
        h.stderr
    );
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["warnings"]["lint"], 1);

    h.add_file(
        "shared.p",
        &SHARED_HELPER
            .replace("assert p.actual == Compute(p.input);", "assert p.actual == Compute(p.input); assert true;"),
    );
    // beside a blocking finding, the advisory one is still shown only with `--strict`
    assert_eq!(run(&mut h, &["check", "shared.p", "--no-mutation"]), 2);
    assert!(h.stderr.contains("error[tautological-assert]"), "{}", h.stderr);
    assert!(!h.stderr.contains("shared-spec-helper"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "shared.p", "--no-mutation", "--strict"]), 2);
    assert!(h.stderr.contains("error[tautological-assert]"), "{}", h.stderr);
    assert!(h.stderr.contains("warning[shared-spec-helper]"), "{}", h.stderr);
}

#[test]
fn shared_helper_advisories_follow_transitive_calls_and_scoped_suppressions() {
    let mut h = MemHost::new("/w");
    let src = SHARED_HELPER.replace(
        "fun Compute(x: int): int { return x + 1; }",
        "fun Compute(x: int): int { return Leaf(x); }\nfun Leaf(x: int): int { return x + 1; }",
    );
    h.add_file("shared.p", &src);
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--strict", "--json"]), 2, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"].as_array().unwrap().len(), 2);
    let src = src
        .replace("fun Compute", "// snappea-lint: allow shared-spec-helper -- reviewed adapter\nfun Compute")
        .replace("fun Leaf", "// snappea-lint: allow shared-spec-helper -- reviewed utility\nfun Leaf");
    h.add_file("shared.p", &src);
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--strict", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"], serde_json::json!([]));
}

#[test]
fn helper_sharing_between_unrelated_tests_or_only_drivers_is_not_reported() {
    let mut h = MemHost::new("/w");
    let src = SHARED_HELPER.replace("assert p.actual == Compute(p.input);", "assert p.actual == p.input + 1;")
        + r#"
        machine OtherWorker { start state S { entry { announce Result, (input = 1, actual = 2); } } }
        machine OtherMain { start state S { entry { new OtherWorker(); } } }
        spec OtherOracle observes Result { start state S { on Result do (p: (input: int, actual: int)) { assert p.actual == Compute(p.input); } } }
        test tcOther [main=OtherMain]: assert OtherOracle in { OtherMain, OtherWorker };
    "#;
    h.add_file("shared.p", &src);
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--strict", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"], serde_json::json!([]));
    // PTst and explicit driver annotations must not turn environment helpers into design oracles.
    h.add_file("shared.p", &SHARED_HELPER.replace("machine Worker", "// snappea: driver\nmachine Worker"));
    assert_eq!(run(&mut h, &["check", "lint", "shared.p", "--strict", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["findings"], serde_json::json!([]));
}

#[test]
fn shared_helper_warning_follows_check_test_selection() {
    let mut h = MemHost::new("/w");
    let src = SHARED_HELPER.to_string()
        + r#"
        machine OtherWorker { start state S { entry { announce Result, (input = 1, actual = 2); } } }
        machine OtherMain { start state S { entry { new OtherWorker(); } } }
        spec Independent observes Result { start state S { on Result do (p: (input: int, actual: int)) { assert p.actual == p.input + 1; } } }
        test tcIndependent [main=OtherMain]: assert Independent in { OtherMain, OtherWorker };
    "#;
    h.add_file("shared.p", &src);
    assert_eq!(
        run(&mut h, &["check", "tcIndependent", "shared.p", "--strict", "--no-mutation", "--json"]),
        0,
        "{}{}",
        h.stdout,
        h.stderr
    );
    assert!(!h.stderr.contains("shared-spec-helper"), "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert!(result["warnings"].get("lint").is_none(), "{result}");
    // the test case that shares the helper does report it
    assert_eq!(run(&mut h, &["check", "tcShared", "shared.p", "--strict", "--no-mutation", "--json"]), 2);
    assert!(h.stderr.contains("warning[shared-spec-helper]"), "{}", h.stderr);
}

fn payload_findings(body: &str, prefix: &str) -> Vec<serde_json::Value> {
    let mut h = MemHost::new("/w");
    h.add_file(
        "payload.p",
        &format!(
            r#"
        event Reply: (id: int, value: int);
        fun Consume(p: (id: int, value: int)) {{ assert p.id >= 0; }}
        {prefix}
        machine Worker {{ start state S {{ on Reply do (p: (id: int, value: int)) {{ {body} }} }} }}
        machine Main {{ start state S {{ entry {{ new Worker(); }} }} }}
        test tc [main=Main]: {{ Main, Worker }};
    "#
        ),
    );
    // an advisory rule: reported, and failing, only with `--strict`
    let code = run(&mut h, &["check", "lint", "payload.p", "--strict", "--json"]);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    let all = result["findings"].as_array().unwrap();
    assert_eq!(code, if all.is_empty() { 0 } else { 2 }, "{}{}", h.stdout, h.stderr);
    all.iter().filter(|f| f["rule"] == "unused-design-payload-field").cloned().collect()
}

#[test]
fn design_payload_advisory_distinguishes_behavior_from_diagnostic_reads() {
    for body in [
        "assert p.id >= 0;",
        "assert p.id >= 0, format(\"{0}\", p.value);",
        "print format(\"{0}\", p.value); assert p.id >= 0;",
        "p.value = 42; assert p.id >= 0;",
    ] {
        let findings = payload_findings(body, "");
        assert_eq!(findings.len(), 1, "{body}: {findings:?}");
        assert!(findings[0]["message"].as_str().unwrap().contains("p.value"));
        assert_eq!(findings[0]["severity"], "advisory");
    }
    for body in [
        "assert p.id >= p.value;",
        "Consume(p);",
        "var copy: (id: int, value: int); copy = p; assert copy.id >= 0;",
        "assert false;",
        "print format(\"{0}\", p.id); { assert false; }",
    ] {
        assert!(payload_findings(body, "").is_empty(), "{body}");
    }
    assert!(payload_findings("assert p.id >= 0;", "// snappea: driver").is_empty());
    assert!(payload_findings(
        "assert p.id >= 0;",
        "// snappea-lint: allow unused-design-payload-field -- intentional metadata"
    )
    .is_empty());
}

#[test]
fn design_payload_advisory_reads_nested_receive_captures() {
    let findings = payload_findings(
        "receive { case Reply: (next: (id: int, value: int)) { assert p.id == next.id; assert p.value == next.value; } }", "");
    assert!(findings.is_empty(), "{findings:?}");
    let findings = payload_findings(
        "assert p.id >= 0; receive { case Reply: (next: (id: int, value: int)) { assert next.id >= 0; } }",
        "",
    );
    assert_eq!(findings.len(), 2, "{findings:?}");
}

#[test]
fn design_payload_advisories_fail_checks_only_with_strict_and_skip_goto_plumbing() {
    let mut h = MemHost::new("/w");
    let source = r#"
        event Reply: (id: int, value: int);
        machine Worker {
            fun Handle(p: (id: int, value: int)) { assert p.id >= 0; }
            start state S { on Reply do Handle; }
        }
        machine Main { start state S { entry { var w: machine; w = new Worker(); send w, Reply, (id = 1, value = 2); } } }
        test tc [main=Main]: { Main, Worker };
    "#;
    h.add_file("payload.p", source);
    assert_eq!(
        run(&mut h, &["check", "payload.p", "--deny", "lint", "--no-mutation", "--json"]),
        0,
        "{}{}",
        h.stdout,
        h.stderr
    );
    // not reported by default
    assert!(!h.stderr.contains("unused-design-payload-field"), "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert!(result["warnings"].get("lint").is_none(), "{result}");
    // `--strict` reports it and fails lint; `--warn lint` keeps it a warning
    assert_eq!(run(&mut h, &["check", "payload.p", "--strict", "--no-mutation", "--json"]), 2);
    assert!(h.stderr.contains("warning[unused-design-payload-field]"), "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["summary"]["status"], "lint_failed");
    assert_eq!(run(&mut h, &["check", "payload.p", "--strict", "--warn", "lint", "--no-mutation", "--json"]), 0);
    assert!(h.stderr.contains("warning[unused-design-payload-field]"), "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["warnings"]["lint"], 1);
    h.add_file("payload.p", &source.replace("on Reply do Handle;", "on Reply goto S with Handle;"));
    assert_eq!(run(&mut h, &["check", "lint", "payload.p", "--strict", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(!h.stderr.contains("unused-design-payload-field"), "{}", h.stderr);
}

const COMPLEMENTARY_PAYLOAD_SPECS: &str = r#"
event Reading: (request: int, value: int);
spec RequestKnown observes Reading {
  start state Watching { on Reading do (p: (request: int, value: int)) { assert p.request > 0; } }
}
spec ValueValid observes Reading {
  start state Watching { on Reading do Check; }
  fun Check(p: (request: int, value: int)) { assert p.value > 0; }
}
machine Main { start state Start { entry { new Sensor(); } } }
machine Sensor { start state Ready { entry { announce Reading, (request=1, value=2); } } }
test together [main=Main]: assert RequestKnown, ValueValid in { Main, Sensor };
"#;

#[test]
fn complementary_specs_share_payload_checks_without_blocking_deny_lint() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", COMPLEMENTARY_PAYLOAD_SPECS);
    // a field a companion spec reads is no blocking finding, and the advisory one is not shown
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0, "{}{}", h.stdout, h.stderr);
    assert_eq!(h.stdout, "Lint passed.\n");
    assert!(h.stderr.is_empty(), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--deny", "lint", "--no-mutation"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(!h.stderr.contains("payload-field"), "{}", h.stderr);
    // `--strict` reports the advisory findings, and fails on them
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--strict"]), 2, "{}{}", h.stdout, h.stderr);
    assert!(h.stderr.contains("warning[payload-field-checked-by-other-spec]"), "{}", h.stderr);
    assert!(!h.stderr.contains("error[unused-payload-field]"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "--strict", "--no-mutation"]), 2, "{}{}", h.stdout, h.stderr);
    assert!(h.stdout.contains("but lint failed; coverage"), "{}", h.stdout);
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--strict"]), 2);
    let json: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(json["findings"].as_array().unwrap().len(), 2, "{json}");
    assert!(json["findings"].as_array().unwrap().iter().all(|f| f["severity"] == "advisory"), "{json}");
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0);
    let json: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(json["findings"], serde_json::json!([]));
}

#[test]
fn companion_reads_cannot_leak_across_tests_or_selections() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "model.p",
        &format!("{COMPLEMENTARY_PAYLOAD_SPECS}\ntest alone [main=Main]: assert RequestKnown in {{ Main, Sensor }};"),
    );
    assert_eq!(run(&mut h, &["check", "model.p", "together", "--no-mutation"]), 0, "{}{}", h.stdout, h.stderr);
    for args in [vec!["check", "lint", "model.p"], vec!["check", "model.p", "alone"], vec!["check", "model.p"]] {
        assert_eq!(run(&mut h, &args), 2, "{args:?}: {}{}", h.stdout, h.stderr);
        assert!(h.stderr.contains("error[unused-payload-field]") && h.stderr.contains("test(s) alone"), "{}", h.stderr);
    }
}

#[test]
fn diagnostic_only_companions_and_other_events_do_not_validate_payloads() {
    for source in [
        COMPLEMENTARY_PAYLOAD_SPECS.replace("assert p.value > 0;", "assert p.request > 0, format(\"value {0}\", p.value);"),
        COMPLEMENTARY_PAYLOAD_SPECS.replace("assert p.value > 0;", "print format(\"value {0}\", p.value); assert p.request > 0;"),
        COMPLEMENTARY_PAYLOAD_SPECS.replace("spec ValueValid observes Reading", "event Other: (request: int, value: int); spec ValueValid observes Other")
            .replace("on Reading do Check", "on Other do Check"),
        COMPLEMENTARY_PAYLOAD_SPECS.replace("spec ValueValid observes Reading", "event Other: (request: int, value: int); spec ValueValid observes Other")
            .replace("on Reading do Check", "// snappea-lint: allow unobserved-handler -- intentionally unreachable reader fixture\n on Reading do Check"),
        COMPLEMENTARY_PAYLOAD_SPECS.replace("spec ValueValid observes Reading", "scenario ValueValid observes Reading")
            .replace("start state Watching { on Reading do Check; }", "start state Watching { on Reading do Check; } cold state Seen {}"),
    ] {
        let mut h = MemHost::new("/w");
        h.add_file("model.p", &source);
        assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2, "{}{}", h.stdout, h.stderr);
        assert!(h.stderr.contains("error[unused-payload-field]") && h.stderr.contains("p.value"), "{}", h.stderr);
    }
}

#[test]
fn unused_and_misplaced_suppressions_say_where_to_go_with_strict() {
    let model = "event eReq: (id: int, amount: int, client: machine);\n\
        event eResp: (id: int);\n\
        spec Safety observes eReq, eResp {\n\
          start state S {\n\
            on eReq do (p: (id: int, amount: int, client: machine)) { assert p.id > 0; }\n\
            on eResp do (p: (id: int)) { assert p.id > 0; }\n\
          }\n\
        }\n\
        machine Server { start state S { on eReq do (p: (id: int, amount: int, client: machine)) { send p.client, eResp, (id = p.id,); } } }\n\
        machine Main { start state S { entry { send new Server(), eReq, (id = 1, amount = 2, client = this); } ignore eResp; } }\n\
        // snappea-lint: allow unused-payload-field -- misplaced\n\
        test tc [main=Main]: assert Safety in { Main, Server };\n";
    let mut h = MemHost::new("/w");
    h.add_file("model.p", model);
    // the misplaced suppression does not hide the finding; that it matches nothing is an
    // advisory finding, reported only with `--strict`
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(h.stderr.contains("error[unused-payload-field]: spec 'Safety'"), "{}", h.stderr);
    assert!(!h.stderr.contains("unused-suppression"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--strict"]), 2);
    // with it, the suppression says where it must go
    assert!(
        h.stderr.contains("error[unused-payload-field]: spec 'Safety' never reads payload field 'p.amount'"),
        "{}",
        h.stderr
    );
    assert!(!h.stderr.contains("payload field 'p.client'"), "{}", h.stderr);
    assert!(
        h.stderr.contains("warning[unused-suppression]: suppression of 'unused-payload-field' matches no finding"),
        "{}",
        h.stderr
    );
    assert!(h.stderr.contains("if it is meant for the `unused-payload-field` finding at model.p:5:"), "{}", h.stderr);
    assert!(h.stderr.contains("not the test that selects the spec"), "{}", h.stderr);
    // the likely fix first, then how to suppress, and never an empty assertion
    assert!(h.stderr.contains("= help: if the contract depends on the field, check it"), "{}", h.stderr);
    assert!(h.stderr.contains("do not add an assertion that cannot fail"), "{}", h.stderr);

    // in place, the suppression works and nothing is unused
    let fixed = model.replace("// snappea-lint: allow unused-payload-field -- misplaced\n", "").replace(
        "on eReq do (p: (id: int, amount: int, client: machine)) { assert",
        "// snappea-lint: allow unused-payload-field -- amounts are not part of this contract\non eReq do (p: (id: int, amount: int, client: machine)) { assert",
    );
    h.add_file("model.p", &fixed);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 0, "{}", h.stderr);
    // (`--strict` still fails, on another advisory finding: the server does not read the amount either)
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--strict"]), 2, "{}", h.stderr);
    assert!(h.stderr.contains("warning[unused-design-payload-field]: design machine 'Server'"), "{}", h.stderr);
    assert!(!h.stderr.contains("unused-suppression") && !h.stderr.contains("error["), "{}", h.stderr);

    // a block comment is not read, and says so
    h.add_file(
        "model.p",
        &fixed.replace(
            "// snappea-lint: allow unused-payload-field -- amounts are not part of this contract",
            "/* snappea-lint: allow unused-payload-field -- amounts */",
        ),
    );
    assert_eq!(run(&mut h, &["check", "lint", "model.p", "--strict"]), 2);
    assert!(h.stderr.contains("warning[unused-suppression]: this block comment is not read"), "{}", h.stderr);
}

#[test]
fn check_with_selected_tests_notes_the_findings_it_skipped() {
    let model = "event eReq: (id: int, client: machine);\n\
        event eResp: (id: int);\n\
        spec Safety observes eReq, eResp { start state S { on eReq do (p: (id: int, client: machine)) { assert p.id > 0; } on eResp do (p: (id: int)) { assert p.id > 0; } } }\n\
        spec Live observes eReq, eResp { start cold state Idle { on eReq goto Busy; ignore eResp; } hot state Busy { on eResp goto Idle; ignore eReq; } }\n\
        machine Server { start state S { on eReq do (p: (id: int, client: machine)) { send p.client, eResp, (id = p.id,); } } }\n\
        machine Main { start state S { entry { send new Server(), eReq, (id = 1, client = this); } on eResp goto Done; } state Done { ignore eResp; } }\n\
        test tcGood [main=Main]: assert Safety, Live in { Main, Server };\n\
        test tcWeak [main=Main]: assert Safety in { Main, Server };\n";
    let mut h = MemHost::new("/w");
    h.add_file("model.p", model);
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(h.stderr.contains("error[no-progress-spec]: test 'tcWeak'"), "{}", h.stderr);
    assert_eq!(run(&mut h, &["check", "model.p", "tcGood", "--no-mutation"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(
        h.stderr.contains(
            "note: lint findings only in tests not selected were skipped; run `snappea check lint` to see them"
        ),
        "{}",
        h.stderr
    );
}

#[test]
fn invalid_suppression_names_the_id_and_the_closest_rule() {
    let mut h = MemHost::new("/w");
    h.add_file("model.p", &format!("// snappea-lint: allow permissive-monitr -- typo\n{}", MODEL));
    assert_eq!(run(&mut h, &["check", "lint", "model.p"]), 2);
    assert!(
        h.stderr.contains("error[invalid-suppression]: unknown lint rule ID 'permissive-monitr' in suppression"),
        "{}",
        h.stderr
    );
    assert!(h.stderr.contains("= help: did you mean `permissive-monitor`? Otherwise write"), "{}", h.stderr);
    // the whole comment is underlined, and the fix is never to allow the diagnostic itself
    assert!(
        h.stderr.contains(&format!("| {}\n", "^".repeat("// snappea-lint: allow permissive-monitr -- typo".len()))),
        "{}",
        h.stderr
    );
    assert!(!h.stderr.contains("allow invalid-suppression"), "{}", h.stderr);
}

#[test]
fn design_receivers_may_ignore_reply_addresses() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "votes.p",
        "event eVote: (id: int, from: machine, voters: seq[Participant]);\n\
         machine Participant { start state S {} }\n\
         machine Coordinator { var n: int; start state S { on eVote do (v: (id: int, from: machine, voters: seq[Participant])) { n = n + v.id; assert n > 0; } } }\n\
         machine Main { start state S { entry { new Coordinator(); } } }\n\
         test tc [main=Main]: { Main, Coordinator, Participant };\n",
    );
    assert_eq!(run(&mut h, &["check", "lint", "votes.p", "--strict", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(!h.stdout.contains("unused-design-payload-field"), "{}", h.stdout);
}
