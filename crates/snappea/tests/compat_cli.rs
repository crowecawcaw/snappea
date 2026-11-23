//! End-to-end tests of `p check` on an in-memory host.
mod common;

use common::compat as run;

use std::path::Path;

use snappea::host::{Host, MemHost};

fn upstream(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {}", p.display(), e))
}

/// `IsSubset` from upstream's `PCheckerLogGeneratorTests`.
fn json_subset(sub: &serde_json::Value, sup: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    if sub == sup {
        return true;
    }
    match (sub, sup) {
        (Object(a), Object(b)) => a.iter().all(|(k, v)| b.get(k).map(|w| json_subset(v, w)).unwrap_or(false)),
        (Array(a), Array(b)) => a.iter().all(|x| b.iter().any(|y| json_subset(x, y))),
        _ => false,
    }
}

#[test]
fn log_generator_matches_upstream_golden_files() {
    let mut h = MemHost::new("/work");
    h.add_file("bug2.p", &upstream("RegressionTests/Feature1SMLevelDecls/DynamicError/bug2/bug2.p"));
    let code = run(&mut h, &["check", "-pf", "bug2.p", "-pn", "bug2", "-o", "res"]);
    assert_eq!(code, 1, "{}", h.stdout);
    assert!(h.stdout.contains("Checker found a bug."));

    let txt = h.read_to_string(Path::new("res/BugFinding/bug2_0_0.txt")).unwrap();
    let expected_txt = upstream("CorrectLogs/bugs2/bug2_0_0.txt");
    assert!(txt.contains(&expected_txt), "trace:\n{}", txt);

    let json: serde_json::Value =
        serde_json::from_str(&h.read_to_string(Path::new("res/BugFinding/bug2_0_0.trace.json")).unwrap()).unwrap();
    let expected = upstream("CorrectLogs/bugs2/bug2_0_0.trace.json");
    let expected: serde_json::Value = serde_json::from_str(expected.trim_start_matches('\u{feff}')).unwrap();
    assert!(json_subset(&expected, &json), "json:\n{}", json);
    assert!(h.is_file(Path::new("res/BugFinding/bug2_0_0.schedule")));
    // exact match, as upstream's test requires for the coverage report
    let cov = h.read_to_string(Path::new("res/BugFinding/bug2.coverage.txt")).unwrap();
    assert_eq!(cov, upstream("CorrectLogs/bugs2/bug2.coverage.txt"));
    assert!(h.is_file(Path::new("res/BugFinding/bug2_pchecker_summary.txt")));
}

#[test]
fn replay_reproduces_the_bug() {
    let mut h = MemHost::new("/work");
    h.add_file("bug2.p", &upstream("RegressionTests/Feature1SMLevelDecls/DynamicError/bug2/bug2.p"));
    run(&mut h, &["check", "bug2.p", "-pn", "bug2"]);
    run(&mut h, &["check", "bug2.p", "-pn", "bug2", "--replay", "PCheckerOutput/BugFinding/bug2_0_0.schedule"]);
    assert!(h.stdout.contains("... Reproduced 1 bug."), "{}", h.stdout);
}

#[test]
fn project_file_and_test_case_listing() {
    let mut h = MemHost::new("/proj");
    h.add_file(
        "Demo.pproj",
        "<Project><ProjectName>Demo</ProjectName><InputFiles><PFile>./PSrc/</PFile></InputFiles><OutputDir>./PGenerated/</OutputDir></Project>",
    );
    h.add_file(
        "PSrc/demo.p",
        "machine M { start state S { entry { assert false, \"boom\"; } } }\n\
         machine N { start state S { } }\n\
         test tcFails [main=M]: { M };\n\
         test tcPasses [main=N]: { N };\n",
    );
    assert_eq!(run(&mut h, &["check", "--list-tests"]), 0, "{}{}", h.stdout, h.stderr);
    assert!(h.stdout.contains("....... includes p file: /proj/PSrc/demo.p"));
    // nothing is generated
    assert!(!h.is_dir(Path::new("PGenerated")));
    assert!(h.stdout.contains(".. List of test cases (total 2)\ntcFails\ntcPasses\n"), "{}", h.stdout);

    // ambiguous: upstream lists the options on stderr and exits with 1
    assert_eq!(run(&mut h, &["check"]), 1);
    assert!(h.stderr.contains("Possible options are: \ntcFails\ntcPasses\n"), "{}", h.stderr);

    assert_eq!(run(&mut h, &["check", "-tc", "tcPasses", "-s", "10"]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("..... Explored 10 schedules"));
    assert_eq!(run(&mut h, &["check", "-tc", "tcFails"]), 1);
    let trace = h.read_to_string(Path::new("PCheckerOutput/tcFails/BugFinding/Demo_0_0.txt")).unwrap();
    assert!(trace.contains("<ErrorLog> Assertion Failed: PSrc/demo.p:1:37 boom"), "{}", trace);
    // the bug line is only echoed to the console in verbose mode, as upstream
    assert!(!h.stdout.contains("Error: Assertion"));
    assert_eq!(run(&mut h, &["check", "-tc", "tcFails", "-v"]), 1);
    assert!(h.stdout.contains("Error: Assertion Failed: PSrc/demo.p:1:37 boom"), "{}", h.stdout);
}

#[test]
fn compat_check_keeps_upstream_exit_behavior_for_coverage_gaps() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "gap.p",
        "event eNever; machine Main { start state S { on eNever do { print \"never\"; } } }\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(run(&mut h, &["check", "-pf", "gap.p", "-pn", "Gap", "-tc", "tcMain", "-s", "10"]), 0, "{}", h.stdout);
    assert!(!h.stdout.contains("Required coverage was not reached"), "{}", h.stdout);
}

#[test]
fn type_errors_are_reported() {
    let mut h = MemHost::new("/w");
    h.add_file("bad.p", "machine M { start state S { entry { x = 1; } } }");
    assert_eq!(run(&mut h, &["check", "-pf", "bad.p"]), 1);
    assert!(h.stdout.contains("[Error:]"), "{}", h.stdout);
    assert!(h.stdout.ends_with("~~ [PTool]: Thanks for using P! ~~\n"));
}

#[test]
fn there_is_no_compile_command() {
    let mut h = MemHost::new("/w");
    h.add_file("m.p", "machine M { start state S { } }");
    assert_eq!(run(&mut h, &["compile", "-pf", "m.p"]), 0);
    assert!(
        h.stdout.starts_with(
            "Expected (check | observe | merge-scenario-coverage) as the command input but received `compile`"
        ),
        "{}",
        h.stdout
    );
    assert_eq!(h.files_under("/w").len(), 1, "compile wrote files");
    assert_eq!(run(&mut h, &["check", "out/PChecker/net8.0/X.dll"]), 1);
    assert!(h.stderr.contains("there is no compiled file"), "{}", h.stderr);
}

#[test]
fn bad_option_prints_help_and_error() {
    let mut h = MemHost::new("/w");
    assert_eq!(run(&mut h, &["check", "--bogus"]), 1);
    assert!(h.stdout.starts_with("usage: p check path [--help]"), "{}", h.stdout);
    assert_eq!(h.stderr, "Error: Unexpected argument: '--bogus'\n");
}

#[test]
fn output_directory_history_rotates() {
    let mut h = MemHost::new("/w");
    h.add_file("bug2.p", &upstream("RegressionTests/Feature1SMLevelDecls/DynamicError/bug2/bug2.p"));
    for _ in 0..3 {
        run(&mut h, &["check", "-pf", "bug2.p", "-pn", "bug2"]);
    }
    assert!(h.is_dir(Path::new("PCheckerOutput/BugFinding")));
    assert!(h.is_dir(Path::new("PCheckerOutput/BugFinding0")));
    assert!(h.is_dir(Path::new("PCheckerOutput/BugFinding1")));
    assert!(!h.is_dir(Path::new("PCheckerOutput/BugFinding2")));
}

#[test]
fn scenario_coverage_is_reported_written_and_merged() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "s.p",
        &upstream("RegressionTests/Feature1SMLevelDecls/Correct/ScenarioCoverageBasic/ScenarioCoverageBasic.p"),
    );
    assert_eq!(run(&mut h, &["check", "-pf", "s.p", "-pn", "S", "-s", "5"]), 0, "{}", h.stdout);
    assert!(
        h.stdout.contains(
            "..... Scenario coverage: 1/2 scenarios covered (1 gap)\n\
         .....   [covered] ReadAfterWrite: triggered in 5 schedules, 1 unique satisfying timeline\n\
         .....   [  GAP  ] NeverCovered: not covered (best partial progress: 1/2 states)\n"
        ),
        "{}",
        h.stdout
    );
    assert!(h.stdout.contains("..... Writing PCheckerOutput/BugFinding/S_scenario_coverage.json"));
    assert_eq!(run(&mut h, &["merge-scenario-coverage", "PCheckerOutput"]), 0);
    assert!(h.stdout.starts_with(
        "Unified scenario coverage across 1 test case:\n  1/2 scenarios covered in >=1 test case; 1 coverage gap.\n"
    ), "{}", h.stdout);
}

#[test]
fn pex_mode_searches_exhaustively_and_rl_runs() {
    let mut h = MemHost::new("/proj");
    h.add_file(
        "Demo.pproj",
        "<Project><ProjectName>Demo</ProjectName><InputFiles><PFile>./PSrc/</PFile></InputFiles><OutputDir>./PGenerated/</OutputDir></Project>",
    );
    h.add_file(
        "PSrc/demo.p",
        "machine M { start state S { entry { var n: int; if ($) { n = 1; } if ($) { n = n + 2; } assert n != 3, \"three\"; } } }\n\
         machine N { start state S { entry { var n: int; if ($) { n = 1; } assert n < 2; } } }\n\
         test tcFails [main=M]: { M };\n\
         test tcPasses [main=N]: { N };\n",
    );
    assert_eq!(run(&mut h, &["check", "--mode", "pex", "-tc", "tcPasses"]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("'random-dfs' strategy"), "{}", h.stdout);
    assert!(h.stdout.contains("Search finished: every behaviour of the test was explored."), "{}", h.stdout);
    assert!(h.stdout.contains("..... Explored 2 schedules"), "{}", h.stdout);

    assert_eq!(run(&mut h, &["check", "--mode", "pex", "-tc", "tcFails", "--sch-pex", "dfs"]), 1, "{}", h.stdout);
    assert!(h.stdout.contains("'dfs' strategy"), "{}", h.stdout);
    assert!(h.is_file(Path::new("PCheckerOutput/tcFails/PEx/Demo_0_0.schedule")));
    run(
        &mut h,
        &["check", "--mode", "pex", "-tc", "tcFails", "--replay", "PCheckerOutput/tcFails/PEx/Demo_0_0.schedule"],
    );
    assert!(h.stdout.contains("... Reproduced 1 bug."), "{}", h.stdout);

    assert_eq!(run(&mut h, &["check", "-tc", "tcFails", "--sch-rl", "-s", "100", "--seed", "1"]), 1, "{}", h.stdout);
    assert!(h.stdout.contains("'rl' strategy (seed:1)"), "{}", h.stdout);
}

/// Two test cases sharing the prefix `tc`: the first finds a bug, the second passes.
fn failing_then_passing() -> MemHost {
    let mut h = MemHost::new("/w");
    h.add_file(
        "m.p",
        "machine M { start state S { entry { assert false, \"boom\"; } } }\n\
         machine N { start state S { } }\n\
         test tcA [main=M]: { M };\n\
         test tcB [main=N]: { N };\n",
    );
    h
}

#[test]
fn liveness_temperature_threshold_is_rejected_as_upstream_does() {
    let mut h = failing_then_passing();
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tcA", "--liveness-temperature-threshold", "7"]), 1);
    assert!(h.stdout.starts_with("usage: p check path [--help]"), "{}", h.stdout);
    assert_eq!(h.stderr, "Error: Unhandled parsed argument: 'liveness-temperature-threshold'\n");
}

#[test]
fn the_last_matching_test_case_sets_the_exit_code() {
    let mut h = failing_then_passing();
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tc"]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("Checker found a bug."), "{}", h.stdout);
    assert!(h.stdout.contains(".. Test case :: tcB"), "{}", h.stdout);
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tcA"]), 1, "{}", h.stdout);
}

#[test]
fn zero_timeout_means_no_timeout() {
    let mut h = failing_then_passing();
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tcB", "-t", "0", "-s", "3"]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("..... Explored 3 schedules"), "{}", h.stdout);
    assert!(!h.stdout.contains("timed out"), "{}", h.stdout);
}

#[test]
fn snappea_extensions_are_not_options() {
    let mut h = failing_then_passing();
    for flag in ["--workers", "--latency", "--latency-profile"] {
        assert_eq!(run(&mut h, &["check", "m.p", flag, "1"]), 1, "{}", flag);
        assert_eq!(h.stderr, format!("Error: Unexpected argument: '{}'\n", flag));
    }
    assert_eq!(run(&mut h, &["check", "m.p", "--timed"]), 1);
}

#[test]
fn unsupported_pex_strategy_is_rejected_before_checking() {
    let mut h = failing_then_passing();
    assert_eq!(run(&mut h, &["check", "m.p", "--mode", "pex", "--sch-pex", "astar"]), 1);
    assert_eq!(
        h.stderr,
        "Error: The 'astar' scheduling strategy is not supported by snappea. Run `p check --help` for usage help.\n"
    );
    assert!(!h.stdout.contains(".. Checking"), "{}", h.stdout);
}

#[test]
fn merge_scenario_coverage_help() {
    let mut h = MemHost::new("/w");
    for flag in ["-h", "--help"] {
        assert_eq!(run(&mut h, &["merge-scenario-coverage", flag]), 0);
        assert!(h.stdout.starts_with("usage: p merge-scenario-coverage [directory]\n"), "{}", h.stdout);
    }
}

#[test]
fn unusable_output_directory_is_an_error() {
    let mut h = failing_then_passing();
    h.add_file("afile", "");
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tcA", "-o", "afile/out"]), 1);
    assert_eq!(
        h.stderr,
        "Error: Cannot prepare output directory 'afile/out/tcA/BugFinding/': File exists: '/w/afile'\n"
    );
}

#[test]
fn missing_replay_file_is_named() {
    let mut h = failing_then_passing();
    assert_eq!(run(&mut h, &["check", "m.p", "-tc", "tcA", "--replay", "nope.schedule"]), 1);
    assert!(h.stderr.starts_with("Error: Cannot read schedule file 'nope.schedule': "), "{}", h.stderr);
}

#[test]
fn mem_host_file_system_matches_native_semantics() {
    use std::io::ErrorKind;
    let mut h = MemHost::new("/w");
    h.add_file("d/f.txt", "x");
    assert_eq!(h.remove_dir_all(Path::new("missing")).unwrap_err().kind(), ErrorKind::NotFound);
    assert!(h.remove_dir_all(Path::new("d/f.txt")).is_err());
    assert!(h.write_file(Path::new("d"), "y").is_err());
    assert_eq!(h.read_to_string(Path::new("d/f.txt")).unwrap(), "x", "children survive");
    assert!(h.read_to_string(Path::new("d")).is_err());
    assert!(h.write_file(Path::new("d/f.txt/g"), "z").is_err());
    h.remove_dir_all(Path::new("d")).unwrap();
    assert!(!h.is_dir(Path::new("d")) && !h.is_file(Path::new("d/f.txt")));
}

#[test]
#[should_panic(expected = "cannot add d")]
fn mem_host_add_file_panics_on_failure() {
    let mut h = MemHost::new("/w");
    h.add_file("d/f.txt", "x");
    h.add_file("d", "y");
}
