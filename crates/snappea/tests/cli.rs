//! End-to-end tests of the `snappea` command line on an in-memory host.

use std::path::Path;

use snappea::host::{Host, Job, MemHost, Threads};

fn run_threads<'a>(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
        main();
    });
}

const COINS: &str = "machine M {\n\
       start state S {\n\
         entry {\n\
           var n: int;\n\
           while (n < 6) {\n\
             if ($) { n = n + 1; } else { goto Done; }\n\
           }\n\
           assert false, \"six heads\";\n\
         }\n\
       }\n\
       state Done { }\n\
     }\n\
     machine Fine { start state S { } }\n\
     // snappea-lint: allow no-model-machine -- CLI regression fixture keeps behavior in the entry machine\n\
     test tcCoins [main=M]: { M };\n\
     // snappea-lint: allow no-model-machine -- CLI regression fixture keeps behavior in the entry machine\n\
     test tcFine [main=Fine]: { Fine };\n";

/// A host with four threads and `coins.p`.
fn host() -> MemHost {
    let mut h = MemHost::new("/w");
    h.threads = Some(Threads { available: 4, run: run_threads });
    h.add_file("coins.p", COINS);
    h
}

fn snappea(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    snappea::cli::run(&args, host)
}

fn compat(host: &mut MemHost, args: &[&str]) -> i32 {
    host.stdout.clear();
    host.stderr.clear();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    snappea::compat::run(&args, host)
}

fn read(h: &MemHost, path: &str) -> String {
    h.read_to_string(Path::new(path)).unwrap_or_else(|e| panic!("{}: {}", path, e))
}

const OUT: &str = "PCheckerOutput/tcCoins";

/// The names of the files in `dir`.
fn names(h: &MemHost, dir: &str) -> Vec<String> {
    h.files_under(dir).iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect()
}

#[test]
fn check_finds_a_bug_that_replays() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["check", "tcCoins", "--seed", "2"]), 1, "{}", h.stdout);
    // one line per test case, the bug's report and how to replay it; no upstream progress
    assert!(
        h.stdout.starts_with("Checking generatedOutput: 1 test case\nFAIL  tcCoins (bug found after "),
        "{}",
        h.stdout
    );
    assert!(
        h.stdout.contains(
            "\n      Assertion Failed: coins.p:8:1 six heads\n      Replay: snappea replay PCheckerOutput/tcCoins/bug.schedule\n"
        ),
        "{}",
        h.stdout
    );
    assert!(h.stdout.ends_with("\n1 of 1 test case failed in 0.00s. Output: PCheckerOutput/\n"), "{}", h.stdout);
    assert!(!h.stdout.contains("Checker found a bug."), "{}", h.stdout);
    // the test case's folder, with fixed names; coverage and the summary go to the console
    assert_eq!(names(&h, OUT), ["bug.schedule", "bug.trace.json", "bug.txt"]);
    assert!(read(&h, &format!("{}/bug.txt", OUT)).contains("six heads"));
    let schedule = format!("{}/bug.schedule", OUT);
    assert!(read(&h, &schedule).contains("--test-method:tcCoins\n"));
    assert_eq!(snappea(&mut h, &["replay", &schedule]), 1, "{}{}", h.stdout, h.stderr);
    assert_eq!(
        h.stdout,
        "Replaying tcCoins from PCheckerOutput/tcCoins/bug.schedule\nReproduced the bug (0.00s):\n      Assertion Failed: coins.p:8:1 six heads\nRun with --verbose to see the schedule's log.\n"
    );
    // a run without a bug replaces the previous run's files
    h.add_file("coins.p", &COINS.replace("assert false, \"six heads\";", ""));
    assert_eq!(snappea(&mut h, &["check", "coins.p", "tcCoins"]), 0, "{}", h.stdout);
    assert!(names(&h, OUT).is_empty());
    assert!(h.stdout.contains("\nok    tcCoins (all "), "{}", h.stdout);
    assert!(h.stdout.ends_with("\n🫛 The test case passed in 0.00s. Output: PCheckerOutput/\n"), "{}", h.stdout);
}

#[test]
fn the_replay_command_names_the_sources() {
    let mut h = host();
    h.add_file("my model/coins.p", COINS);
    assert_eq!(snappea(&mut h, &["check", "my model/coins.p", "tcCoins", "--seed", "2"]), 1, "{}", h.stdout);
    let replay = "snappea replay PCheckerOutput/tcCoins/bug.schedule 'my model/coins.p'";
    assert!(h.stdout.contains(&format!("      Replay: {}\n", replay)), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["replay", "PCheckerOutput/tcCoins/bug.schedule", "my model/coins.p"]), 1);
}

#[test]
fn replay_accepts_step_options_for_existing_compat_traces() {
    let mut h = MemHost::new("/w");
    h.add_file("ping.p", "event ePing; machine Loop { start state S { entry { send this, ePing; } on ePing do { send this, ePing; } } } test tcLoop [main=Loop]: { Loop };");
    assert_eq!(
        compat(&mut h, &["check", "-tc", "tcLoop", "--schedules", "3", "--max-steps", "50", "--fail-on-maxsteps"]),
        1,
        "{}{}",
        h.stdout,
        h.stderr
    );
    let path = "PCheckerOutput/tcLoop/BugFinding/generatedOutput_0_0.schedule";
    assert_eq!(
        snappea(&mut h, &["replay", path, "ping.p", "--max-steps", "50", "--fail-on-max-steps"]),
        1,
        "{}{}",
        h.stdout,
        h.stderr
    );
    assert!(h.stdout.contains("Reproduced the bug"));
}

#[test]
fn verbose_shows_progress() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["check", "tcCoins", "--seed", "2", "-v"]), 1, "{}", h.stdout);
    for s in [".. Test case :: tcCoins", "Checker found a bug.", "..... Writing PCheckerOutput/tcCoins/bug.schedule"] {
        assert!(h.stdout.contains(s), "{}: {}", s, h.stdout);
    }
    assert!(h.stdout.contains("\nFAIL  tcCoins (bug found after "), "{}", h.stdout);
}

#[test]
fn warnings_show_without_verbose() {
    let mut h = host();
    h.add_file(
        "p.pproj",
        "<Project><ProjectName>P</ProjectName><InputFiles><PFile>./coins.p</PFile><PFile>./F.cs</PFile></InputFiles></Project>",
    );
    h.add_file("F.cs", "");
    assert_eq!(snappea(&mut h, &["check", "tcFine"]), 0, "{}", h.stdout);
    assert!(h.stdout.starts_with("Warning: foreign file /w/F.cs is ignored;"), "{}", h.stdout);
}

#[test]
fn included_projects_are_found_beside_the_project_file() {
    let mut h = MemHost::new("/w");
    h.add_file(
        "app/App.pproj",
        "<Project><ProjectName>App</ProjectName><InputFiles><PFile>./coins.p</PFile></InputFiles>\
         <IncludeProject>../lib/Lib.pproj</IncludeProject></Project>",
    );
    h.add_file("app/coins.p", COINS);
    h.add_file(
        "lib/Lib.pproj",
        "<Project><ProjectName>Lib</ProjectName><InputFiles><PFile>./lib.p</PFile></InputFiles></Project>",
    );
    h.add_file("lib/lib.p", "machine Lib { start state S { } }\ntest tcLib [main=Lib]: { Lib };\n");
    assert_eq!(snappea(&mut h, &["tests", "app/App.pproj"]), 0, "{}{}", h.stdout, h.stderr);
    assert_eq!(h.stdout, "tcCoins\ntcFine\ntcLib\n");
    // upstream resolves them against the current folder
    assert_eq!(compat(&mut h, &["check", "-pp", "app/App.pproj", "--list-tests"]), 1);
    assert!(h.stderr.contains("Illegal P project file name ../lib/Lib.pproj"), "{}", h.stderr);
}

#[test]
fn tests_are_arguments() {
    let mut h = host();
    // all test cases by default; a bug in any fails the run
    assert_eq!(snappea(&mut h, &["check", "--seed", "2"]), 1, "{}", h.stdout);
    assert!(h.stdout.contains("\nFAIL  tcCoins (") && h.stdout.contains("\nok    tcFine (all "), "{}", h.stdout);
    assert!(h.stdout.ends_with("\n1 of 2 test cases failed in 0.00s. Output: PCheckerOutput/\n"), "{}", h.stdout);
    assert!(h.is_dir(Path::new("/w/PCheckerOutput/tcFine")));
    assert_eq!(snappea(&mut h, &["check", "coins.p", "tcF*"]), 0, "{}", h.stdout);
    assert!(!h.stdout.contains("tcCoins"), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["check", "tcNope"]), 2);
    assert_eq!(h.stderr, "error: no test case matches 'tcNope'. Test cases: tcCoins, tcFine\n");
}

#[test]
fn tests_lists_test_cases() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["tests"]), 0);
    assert_eq!(h.stdout, "tcCoins\ntcFine\n");
    h.add_file("coins.p", "machine M { start state S { entry { x = 1; } } }");
    assert_eq!(snappea(&mut h, &["tests", "coins.p"]), 2);
    assert!(h.stdout.starts_with("error: could not find variable 'x'\n  --> coins.p:1:37\n"), "{}", h.stdout);
    assert!(h.stdout.contains("= help: declare the variable before using it"), "{}", h.stdout);
    assert!(h.stdout.ends_with("The model has 1 error; fix it and run the command again.\n"), "{}", h.stdout);
}

#[test]
fn command_line_errors_exit_with_2() {
    let mut h = host();
    for (args, err) in [
        (&["check", "--bogus"][..], "error: unknown option '--bogus'\n"),
        (&["check", "--timeout", "5d"], "error: bad duration '5d' (examples: 90, 90s, 5m, 1h)\n"),
        (&["check", "-j", "0"], "error: '--workers' needs at least 1 worker\n"),
        (&["replay"], "error: no .schedule file given\n"),
        (&["frobnicate"], "error: unknown command 'frobnicate'\n"),
    ] {
        assert_eq!(snappea(&mut h, args), 2, "{:?}", args);
        assert!(h.stderr.starts_with(err), "{:?}: {}", args, h.stderr);
    }
    assert_eq!(snappea(&mut h, &["check", "--help"]), 0);
    assert!(h.stdout.starts_with("Usage: snappea check"), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &[]), 0);
    assert!(h.stdout.starts_with("🫛 snappea checks P models."), "{}", h.stdout);
}

#[test]
fn native_scheduling_controls_are_removed_and_compat_keeps_them() {
    let mut h = host();
    for option in [
        "--schedules",
        "--strategy",
        "--exhaustive",
        "--max-steps",
        "--fair-max-steps",
        "--fail-on-max-steps",
        "--timeline",
        "--timeline-payload",
    ] {
        assert_eq!(snappea(&mut h, &["check", option]), 2, "{option}: {}", h.stderr);
        assert!(h.stderr.contains("unknown option"), "{option}: {}", h.stderr);
    }
    for strategy in ["--sch-random", "--sch-feedback", "--sch-rl"] {
        assert_eq!(
            compat(&mut h, &["check", "-tc", "tcCoins", "--schedules", "1000", "--seed", "3", strategy]),
            if strategy == "--sch-rl" { 0 } else { 1 },
            "{}{}",
            h.stdout,
            h.stderr
        );
    }
    assert_eq!(compat(&mut h, &["check", "-tc", "tcFine", "--mode", "pex"]), 0, "{}{}", h.stdout, h.stderr);
    assert_eq!(snappea(&mut h, &["check", "--help"]), 0);
    assert!(h.stdout.contains("default: 10s"));
    assert!(!h.stdout.contains("--strategy"));
}

/// The schedule count of the `FAIL`/`ok` line of `test`.
fn schedules(stdout: &str, test: &str) -> usize {
    let line = stdout.lines().find(|l| l.contains(&format!(" {} (", test))).unwrap();
    let words: Vec<&str> = line.split([' ', '(']).filter(|w| !w.is_empty()).collect();
    let i = words.iter().position(|w| w.starts_with("schedule")).unwrap();
    words[i - 1].parse().unwrap()
}

#[test]
fn workers_find_a_bug_that_replays() {
    for seed in ["1", "2", "3", "4", "5"] {
        let mut h = host();
        assert_eq!(snappea(&mut h, &["check", "tcCoins", "--seed", seed]), 1, "{}", h.stdout);
        assert!(h.stdout.contains(" schedules, 4 workers, "), "{}", h.stdout);
        let txt = read(&h, &format!("{}/bug.txt", OUT));
        assert!(txt.contains("<ErrorLog> Assertion Failed: coins.p:8:1 six heads"), "{}", txt);
        // the trace ends with the statistics of all workers, as the console reports them
        let n = schedules(&h.stdout, "tcCoins");
        assert!(txt.contains(&format!("<StrategyLog> Explored {} schedules", n)), "{}\n{}", n, txt);
        assert!(n < 5000);
        let schedule = format!("{}/bug.schedule", OUT);
        assert_eq!(snappea(&mut h, &["replay", &schedule]), 1, "{}", h.stdout);
        assert!(h.stdout.contains("Reproduced the bug"), "{}", h.stdout);
    }
}

#[test]
fn workers_share_the_schedule_count() {
    use snappea::driver::check::{self, CheckConfig, Style};
    let mut h = host();
    h.add_file("coins.p", &COINS.replace("assert false", "assert true"));
    // the driver's progress is numbered across workers, each number once
    h.stdout.clear();
    let mut cfg = CheckConfig {
        style: Style::Snappea,
        workers: Some(4),
        iterations: 1003,
        user_set_iterations: true,
        seed: Some(9),
        test_case_name: "tcCoins".into(),
        ..CheckConfig::default()
    };
    cfg.input.pfiles = vec!["coins.p".into()];
    check::finish_options(&mut cfg);
    check::resolve_sources(&mut h, &mut cfg).unwrap();
    let loaded = check::load(&mut h, &cfg).unwrap();
    check::set_output_directory(&mut h, &mut cfg).unwrap();
    let case = check::testing_process(&mut h, &loaded, &cfg).unwrap();
    assert_eq!((case.schedules, case.workers, case.bugs), (1003, 4, 0));
    assert!(h.stdout.contains("..... Explored 1003 schedules\n"), "{}", h.stdout);
    let progress: Vec<&str> = h.stdout.lines().filter(|l| l.starts_with("..... Schedule #")).collect();
    let mut sorted = progress.clone();
    sorted.sort_by_key(|l| l[16..].parse::<usize>().unwrap());
    sorted.dedup();
    assert_eq!(sorted.len(), 28, "{:?}", progress);
}

#[test]
fn some_runs_use_one_worker() {
    let mut h = host();
    for args in [&["-j", "1"][..], &["-v"]] {
        let mut all = vec!["check", "tcCoins"];
        all.extend_from_slice(args);
        snappea(&mut h, &all);
        assert!(!h.stdout.contains("workers"), "{:?}: {}", args, h.stdout);
    }
}

#[test]
fn exhaustive_search_writes_pex_output() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["check", "tcCoins"]), 1, "{}", h.stdout);
    assert!(h.is_file(Path::new("/w/PCheckerOutput/tcCoins/bug.schedule")));
    assert_eq!(snappea(&mut h, &["check", "tcFine"]), 0, "{}", h.stdout);
    assert!(
        h.stdout.contains(
            "
ok    tcFine (all "
        ),
        "{}",
        h.stdout
    );
}

/// The time-tracking example of `design/time`.
fn retry_budget() -> MemHost {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design/time/RetryBudget");
    let mut h = MemHost::new("/w");
    for rel in [
        "RetryBudget.pproj",
        "PSrc/Timer.p",
        "PSrc/Time.p",
        "PSrc/RetryClient.p",
        "PSpec/Budget.p",
        "PTst/Test.p",
        "PTst/latency.toml",
    ] {
        h.add_file(rel, &std::fs::read_to_string(dir.join(rel)).unwrap());
    }
    h
}

#[test]
fn time_tracking() {
    let mut h = retry_budget();
    let schedule = "PCheckerOutput/tcRetry/bug.schedule";
    for (latency, header) in [
        ("PTst/latency.toml", "--snappea-time:timed=1;profile=same-az\n"),
        ("PTst/latency.toml:cross-region", "--snappea-time:timed=1;profile=cross-region\n"),
    ] {
        let code = snappea(&mut h, &["check", "tcRetry", "--seed", "1", "--latency", latency, "--timed"]);
        assert_eq!(code, 1, "{}{}", h.stdout, h.stderr);
        assert!(read(&h, schedule).contains(header), "{}", read(&h, schedule));
    }
    assert_eq!(snappea(&mut h, &["replay", schedule]), 1, "{}{}", h.stdout, h.stderr);
    assert_eq!(snappea(&mut h, &["check", "--latency", "missing.toml"]), 2);
    assert!(h.stderr.contains("Cannot read latency file 'missing.toml'"), "{}", h.stderr);
}

#[test]
fn coverage_merge() {
    let mut h = host();
    // snappea's report files; snappea-compat reads only upstream's names
    let report = r#"{"version": 1, "testCase": "tcA", "scenarios": [{"name": "S", "satisfied": true, "satisfyingSchedules": 1, "distinctSatisfyingTimelines": 1, "maxStatesVisited": 1, "monitorStates": 1}]}"#;
    h.add_file("out/tcA/scenario_coverage.json", report);
    assert_eq!(snappea(&mut h, &["coverage", "merge", "out"]), 0, "{}", h.stderr);
    assert!(h.stdout.contains("tcA"), "{}", h.stdout);
    assert_eq!(compat(&mut h, &["merge-scenario-coverage", "out"]), 0);
    assert!(!h.stdout.contains("tcA"), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["coverage", "merge", "nowhere"]), 2);
    assert_eq!(h.stderr, "error: directory not found: nowhere\n");
    assert_eq!(snappea(&mut h, &["coverage", "merge"]), 0, "{}", h.stderr);
    assert_eq!(snappea(&mut h, &["coverage"]), 2);
}

#[test]
fn observe_has_its_own_options() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["observe", "--help"]), 0);
    assert!(h.stdout.starts_with("Usage: snappea observe [SOURCE...] [LOG...] --spec <NAME>"), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["observe"]), 2);
    assert!(h.stderr.starts_with("error: name the specs to check with --spec"), "{}", h.stderr);
    // PObserve's options are snappea-compat's
    assert_eq!(snappea(&mut h, &["observe", "--spec", "S", "--outputDir", "o"]), 2);
    assert!(h.stderr.starts_with("error: unknown option '--outputDir'"), "{}", h.stderr);
}

#[test]
fn live_observe_rejects_incompatible_inputs_before_loading_project() {
    let mut h = host();
    h.add_file("a.log", "");
    for (args, err) in [
        (&["observe", "--spec", "Inventory", "--stdin", "a.log"][..], "cannot be combined with log files"),
        (&["observe", "--spec", "Inventory", "--stdin", "--p-trace"], "'--p-trace' cannot be combined with live input"),
        (&["observe", "--spec", "Inventory", "--live", "a", "--live", "b"], "several live inputs need --sequence"),
        (&["observe", "--spec", "Inventory", "--stdin", "--live", "-"], "stdin can be read only once"),
        (&["observe", "--spec", "Inventory", "a.log", "--record", "r"], "apply to live input"),
        (&["observe", "--spec", "Inventory"], "no logs given"),
        (&["observe", "--spec", "A", "--all-specs", "a.log"], "cannot be combined"),
    ] {
        assert_eq!(snappea(&mut h, args), 2, "{:?}", args);
        assert!(h.stderr.contains(err), "{:?}: {}", args, h.stderr);
    }
}

#[test]
fn observe_prints_each_spec_and_its_errors() {
    let mut h = host();
    h.add_file("inventory.p", include_str!("../../../examples/python/specs/inventory.p"));
    h.add_file(
        "logs/a.jsonl",
        "{\"event\":\"eStocked\",\"key\":\"c\",\"payload\":{\"units\":1},\"timestamp\":1}\n\
         {\"event\":\"eReserved\",\"key\":\"c\",\"payload\":{\"order\":\"o\",\"units\":5},\"timestamp\":2}\n",
    );
    let code = snappea(&mut h, &["observe", "inventory.p", "logs", "--spec", "InventoryCorrect"]);
    assert_eq!(code, 1, "{}{}", h.stdout, h.stderr);
    let mut lines = h.stdout.lines();
    assert_eq!(lines.next(), Some("FAIL  InventoryCorrect (1 error; 2 events, 1 key)"), "{}", h.stdout);
    assert!(lines.next().unwrap().starts_with("      key c: "), "{}", h.stdout);
    // the events leading up to it, with --verbose
    assert_eq!(snappea(&mut h, &["observe", "inventory.p", "logs", "--spec", "InventoryCorrect", "-v"]), 1);
    assert!(h.stdout.contains("        | {\"event\":\"eReserved\""), "{}", h.stdout);
    // a spec that does not exist
    assert_eq!(snappea(&mut h, &["observe", "inventory.p", "logs", "--spec", "Nope"]), 2);
    assert!(h.stderr.starts_with("error: no spec named Nope. Specs: InventoryCorrect"), "{}", h.stderr);
    // a log that is not a P trace, with one spec
    h.add_file("bad.json", "{not json");
    assert_eq!(snappea(&mut h, &["observe", "inventory.p", "bad.json", "--p-trace", "--spec", "InventoryCorrect"]), 2);
    assert!(h.stderr.starts_with("error: cannot read log /w/bad.json: "), "{}", h.stderr);
    assert!(h.stdout.is_empty(), "{}", h.stdout);
}

#[test]
fn observe_reports_coverage_and_merges_it_without_the_sources() {
    let mut h = host();
    h.add_file("inventory.p", include_str!("../../../examples/python/specs/inventory.p"));
    let stocked = "{\"event\":\"eStocked\",\"key\":\"c\",\"payload\":{\"units\":3},\"timestamp\":1}\n";
    let reserved =
        "{\"event\":\"eReserved\",\"key\":\"c\",\"payload\":{\"order\":\"o\",\"units\":2},\"timestamp\":2}\n";
    let released =
        "{\"event\":\"eReleased\",\"key\":\"c\",\"payload\":{\"order\":\"o\",\"units\":2},\"timestamp\":3}\n";
    h.add_file("a.jsonl", &format!("{stocked}{reserved}"));
    h.add_file("b.jsonl", &format!("{stocked}{reserved}{released}"));
    let observe = |h: &mut MemHost, log: &str, report: &str| {
        snappea(h, &["observe", "inventory.p", log, "--spec", "InventoryCorrect", "--json-report", report])
    };
    assert_eq!(observe(&mut h, "a.jsonl", "a.json"), 0, "{}", h.stderr);
    // `assert false` handlers must never run, so they are not counted
    assert!(h.stdout.contains("\n      Coverage: 8/12 statements, 0/0 branches, 2/3 event handlers\n"), "{}", h.stdout);
    assert!(
        h.stdout.contains(
            "Not covered by the events:\n  inventory.p:30  InventoryCorrect.Ready, on eReleased: never handled\n"
        ),
        "{}",
        h.stdout
    );
    assert_eq!(observe(&mut h, "b.jsonl", "b.json"), 0, "{}", h.stderr);
    assert!(!h.stdout.contains("Not covered"), "{}", h.stdout);
    // merged without the sources: the union of what ran
    h.add_file("inventory.p", "");
    assert_eq!(
        snappea(&mut h, &["coverage", "merge-observe", "a.json", "--json-report", "suite.json"]),
        0,
        "{}",
        h.stderr
    );
    assert!(h.stdout.contains("InventoryCorrect: 8/12 statements"), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["coverage", "merge-observe", "suite.json", "b.json", "--format", "text"]), 0);
    assert_eq!(h.stdout, "Coverage:\n  InventoryCorrect: 12/12 statements, 0/0 branches, 3/3 event handlers\n");
    // merging again changes nothing
    let merged = read(&h, "suite.json");
    h.add_file("again.json", &merged);
    assert_eq!(snappea(&mut h, &["coverage", "merge-observe", "suite.json", "again.json", "--json"]), 0);
    let value = |t: &str| serde_json::from_str::<serde_json::Value>(t).unwrap();
    assert_eq!(value(&h.stdout), value(&merged));
    h.add_file("bad.json", "{}");
    assert_eq!(snappea(&mut h, &["coverage", "merge-observe", "bad.json", "--format", "text"]), 2);
    assert!(h.stderr.contains("specs must be an array"), "{}", h.stderr);
}

#[test]
fn output_is_json_for_programs_and_text_on_terminals_and_in_ci() {
    let mut h = host();
    h.terminal = false;
    assert_eq!(snappea(&mut h, &["tests"]), 0);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v, serde_json::json!({"exit_code": 0, "test_cases": ["tcCoins", "tcFine"]}));
    assert_eq!(snappea(&mut h, &["tests", "--format", "text"]), 0);
    assert_eq!(h.stdout, "tcCoins\ntcFine\n");
    h.env.insert("CI".into(), "true".into());
    assert_eq!(snappea(&mut h, &["tests"]), 0);
    assert_eq!(h.stdout, "tcCoins\ntcFine\n");
    h.env.insert("SNAPPEA_FORMAT".into(), "json".into());
    assert_eq!(snappea(&mut h, &["tests"]), 0);
    assert!(h.stdout.starts_with('{'), "{}", h.stdout);
    h.terminal = true;
    assert_eq!(snappea(&mut h, &["tests", "--json"]), 0);
    assert!(h.stdout.starts_with('{'), "{}", h.stdout);
    assert_eq!(snappea(&mut h, &["tests", "--json", "--format", "text"]), 2);
    // a failure reports its error in the JSON
    assert_eq!(snappea(&mut h, &["check", "tcNope", "--json"]), 2);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["exit_code"], 2);
    assert!(v["error"].as_str().unwrap().starts_with("no test case matches 'tcNope'"), "{}", v);
    // a check: each test case, and the code no test case ran
    assert_eq!(snappea(&mut h, &["check", "tcCoins", "--seed", "2", "--json"]), 1);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["exit_code"], 1);
    let case = &v["test_cases"][0];
    assert_eq!(case["result"], "bug");
    assert_eq!(case["bug"]["report"], "Assertion Failed: coins.p:8:1 six heads");
    assert_eq!(case["bug"]["replay"], "snappea replay PCheckerOutput/tcCoins/bug.schedule");
    assert_eq!(case["coverage"]["branches"], serde_json::json!({"covered": 2, "total": 2}));
    assert!(v["coverage"]["uncovered"].is_array(), "{}", v);
}

/// Issue #47: a misspelled event name is silently dropped unless `--unknown-events` asks.
#[test]
fn observe_reports_a_misspelled_event_name() {
    let mut h = host();
    let spec = include_str!("../../../examples/python/specs/inventory.p");
    h.add_file("inventory.p", spec);
    h.add_file(
        "typo.jsonl",
        "{\"event\":\"eStocked\",\"key\":\"c\",\"payload\":{\"units\":1},\"timestamp\":1}\n\
         {\"event\":\"eReserverd\",\"key\":\"c\",\"payload\":{\"order\":\"o\",\"units\":5},\"timestamp\":2}\n",
    );
    let args = |policy: &'static str| {
        let mut a = vec!["observe", "inventory.p", "--spec", "InventoryCorrect", "-l", "typo.jsonl"];
        a.extend(["--strict", "--json-report", "r.json"]);
        if !policy.is_empty() {
            a.extend(["--unknown-events", policy]);
        }
        a
    };
    assert_eq!(snappea(&mut h, &args("")), 0);
    assert!(h.stderr.is_empty());
    assert!(!read(&h, "r.json").contains("eReserverd"));

    assert_eq!(snappea(&mut h, &args("warn")), 0);
    assert_eq!(
        h.stderr,
        "warning: Unknown event 'eReserverd': not declared in the P sources (1 occurrence, first at typo.jsonl:2); did you mean eReserved?\n"
    );
    let report: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    assert_eq!(report["specs"][0]["errors"].as_array().unwrap().len(), 0);
    assert_eq!(report["specs"][0]["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(report["specs"][0]["unknown_events"][0]["suggestion"], "eReserved");

    assert_eq!(snappea(&mut h, &args("error")), 2);
    assert!(h.stdout.contains("did you mean eReserved?"), "{}", h.stdout);
    let report: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    assert_eq!(report["exit_code"], 2);
    let error = &report["specs"][0]["errors"][0];
    assert_eq!(error["kind"], "unknown_event");
    assert!(error["message"].as_str().unwrap().contains("'eReserverd'"));
    assert_eq!(report["specs"][0]["unknown_events"][0]["location"], "typo.jsonl:2");

    assert_eq!(snappea(&mut h, &args("loud")), 2);
    assert!(h.stderr.contains("ignore, warn or error"), "{}", h.stderr);
}

const PING: &str = "event ePing: machine;\n\
     event ePong;\n\
     event eStop;\n\
     machine Server {\n\
       start state Ready {\n\
         on ePing do (from: machine) {\n\
           if (from == this) { print \"self\"; }\n\
           send from, ePong;\n\
         }\n\
         on eStop goto Stopped;\n\
         on ePong do { assert false, \"unexpected\"; }\n\
       }\n\
       state Stopped { entry { print \"stopped\"; } }\n\
     }\n\
     machine Client {\n\
       start state Init {\n\
         entry {\n\
           var i: int;\n\
           send new Server(), ePing, this;\n\
           while (i > 0) {\n\
             i = i - 1;\n\
             print \"never\";\n\
           }\n\
         }\n\
         ignore ePong;\n\
       }\n\
     }\n\
     test tcPing [main=Client]: { Client, Server };\n";

#[test]
fn check_reports_what_never_ran() {
    let mut h = host();
    h.add_file("ping.p", PING);
    assert_eq!(
        snappea(&mut h, &["check", "ping.p", "--allow-incomplete-coverage", "--json-report", "result.json"]),
        0,
        "{}",
        h.stdout
    );
    // `assert false` marks code that must never run: it is neither counted nor listed
    assert!(
        h.stdout
            .contains("\n      Coverage: 4/8 statements, 1/2 branches, 1/2 event handlers, 2/4 condition outcomes\n"),
        "{}",
        h.stdout
    );
    let expected = "Not covered by the test case:\n  \
         ping.p:7      Server.Ready, on ePing: condition from == this never true\n  \
         ping.p:7      Server.Ready, on ePing: if (from == this) never true\n  \
         ping.p:10     Server.Ready, on eStop: goto Stopped never taken\n  \
         ping.p:13     Server.Stopped entry: never ran\n  \
         ping.p:20     Client.Init entry: condition i > 0 never true\n  \
         ping.p:21-22  Client.Init entry: not reached: i = i - 1;\n";
    assert!(h.stdout.contains(expected), "{}", h.stdout);
    // the JSON result has the same, and all of it
    let result: serde_json::Value = serde_json::from_str(&read(&h, "result.json")).unwrap();
    assert_eq!(
        result["coverage"]["uncovered"][5],
        serde_json::json!({"file": "ping.p", "line": 21, "end_line": 22, "context": "Client.Init entry",
            "what": "not reached: i = i - 1;"})
    );
    // no coverage files
    assert!(names(&h, "PCheckerOutput/tcPing").is_empty());
}

#[test]
fn check_requires_aggregate_coverage_and_supports_scoped_suppressions() {
    let mut h = host();
    h.add_file(
        "coverage.p",
        "event ePing;\n\
         machine Main { start state S { entry { print \"entry\"; } on ePing do { print \"ping\"; } } }\n\
         // snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "coverage.p", "--no-mutation", "--json-report", "r.json"]), 1);
    let json = read(&h, "r.json");
    let result: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(result["coverage_failed"], true);
    assert_eq!(result["coverage"]["statements"], serde_json::json!({"covered": 1, "total": 2}));
    assert_eq!(result["coverage"]["handlers"], serde_json::json!({"covered": 0, "total": 1}));
    assert_eq!(result["coverage"]["required"]["handlers"], serde_json::json!({"covered": 0, "total": 1}));
    assert_eq!(result["coverage"]["uncovered_required"].as_array().unwrap().len(), 2);
    assert_eq!(result["summary"]["status"], "coverage_failed");
    assert_eq!(result["summary"]["coverage_gaps"], 2);
    assert_eq!(result["summary"]["mutation_time_limit_reached"], false);
    assert_eq!(result["summary"]["uncovered"][0]["file"], "coverage.p");
    assert!(json.find("\"summary\"").unwrap() < json.find("\"coverage\"").unwrap());
    assert_eq!(result["coverage"]["failure"]["message"], "Required coverage is missing (2 points).");
    assert!(result["coverage"]["failure"]["help"].as_str().unwrap().contains("Add scenarios"));
    assert!(h.stdout.contains("Required coverage is missing (2 points)."), "{}", h.stdout);
    assert!(h.stdout.contains("Add scenarios that reach the listed sites"), "{}", h.stdout);

    assert_eq!(
        snappea(&mut h, &["check", "coverage.p", "--no-mutation", "--allow-incomplete-coverage", "--format", "text"]),
        0
    );
    assert!(h.stdout.contains("Coverage gate bypassed:"), "{}", h.stdout);

    h.add_file(
        "coverage.p",
        "event ePing;\n\
         machine Main { start state S { entry { print \"entry\"; }\n\
           // snappea-lint: allow coverage, unused-payload-field -- exercised by a separate integration suite\n\
           on ePing do { print \"ping\"; } } }\n\
         // snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "coverage.p", "--no-mutation", "--json"]), 0, "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage"]["handlers"], serde_json::json!({"covered": 0, "total": 1}));
    assert_eq!(result["coverage"]["required"]["handlers"], serde_json::json!({"covered": 0, "total": 0}));
    assert_eq!(result["coverage"]["uncovered_required"].as_array().unwrap().len(), 0);

    h.add_file(
        "helper.p",
        "machine Main { fun Helper(): int { print \"helper\"; return 1; }\n\
         start state S { entry { print \"entry\"; } } }\n\
         // snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "helper.p", "--no-mutation", "--json"]), 1);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage_failed"], true);
    assert!(result["coverage"]["uncovered_required"].as_array().unwrap().iter().any(|p| p["context"] == "Main.Helper"));

    h.add_file("coverage.p", "event ePing;\n// snappea-lint: allow coverage:nope -- typo\nmachine Main { start state S { } }\n// snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n");
    assert_eq!(snappea(&mut h, &["check", "coverage.p", "--no-mutation"]), 2);
    assert!(h.stderr.contains("unknown or empty lint rule ID"), "{}", h.stderr);
    h.add_file(
        "coverage.p",
        "// snappea-lint: allow coverage\nmachine Main { start state S { } }\n// snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "coverage.p", "--no-mutation"]), 2);
    assert!(h.stderr.contains("coverage suppression needs a reason"), "{}", h.stderr);
}

#[test]
fn selected_test_coverage_aggregates_and_forbidden_paths_stay_excluded() {
    let mut h = host();
    h.add_file(
        "coverage.p",
        "machine A { start state S { entry { print \"a\"; } } }\n\
         machine B { start state S { entry { print \"b\"; } } }\n\
         // snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcA [main=A]: { A };\n\
         // snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcB [main=B]: { B };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "coverage.p", "--no-mutation", "--json"]), 0, "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage"]["statements"], serde_json::json!({"covered": 2, "total": 2}));
    assert_eq!(result["coverage"]["required"]["statements"], serde_json::json!({"covered": 2, "total": 2}));

    h.add_file(
        "forbidden.p",
        "event eNever;\nmachine Main { start state S { on eNever do { assert false, \"forbidden\"; } } }\n// snappea-lint: allow no-model-machine -- single-machine fixture isolates coverage behavior\n\
         test tcMain [main=Main]: { Main };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "forbidden.p", "--no-mutation", "--json"]), 0, "{}", h.stderr);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage"]["required"]["handlers"], serde_json::json!({"covered": 0, "total": 0}));
}

#[test]
fn check_lists_loaded_components_outside_selected_tests() {
    let mut h = host();
    h.add_file(
        "model.p",
        "machine Main { start state S { } }\n\
        machine Other { start state S { entry { print \"other\"; } } }\n\
        machine MainModel { start state S { } }\n\
        test tcMain [main=Main]: { Main, MainModel };\n\
        test tcOther [main=Other]: { Other, MainModel };\n",
    );
    assert_eq!(snappea(&mut h, &["check", "model.p", "tcMain", "--json-report", "r.json"]), 0, "{}", h.stdout);
    assert!(h.stdout.contains("Outside the selected tests (excluded from coverage totals):"));
    let v: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    let excluded = v["coverage"]["excluded_components"].as_array().unwrap();
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0]["context"], "Other");
    assert_eq!(v["coverage"]["statements"]["total"], 0);
    assert_eq!(snappea(&mut h, &["check", "model.p", "--json-report", "r.json"]), 0);
    let v: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    assert_eq!(v["coverage"]["excluded_components"], serde_json::json!([]));
    assert_eq!(v["coverage"]["statements"]["covered"], 1);
}

#[test]
fn output_options_work_before_commands_and_usage_errors_write_reports() {
    let mut h = host();
    for args in
        [vec!["--json", "tests"], vec!["--format=json", "tests"], vec!["--json-report", "r.json", "tests", "--json"]]
    {
        assert_eq!(snappea(&mut h, &args), 0, "{}", h.stderr);
        let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
        assert_eq!(result["test_cases"], serde_json::json!(["tcCoins", "tcFine"]));
    }
    // Options also work between the command and subcommand.
    assert_eq!(snappea(&mut h, &["coverage", "--json", "merge-observe", "--stdin"]), 2);
    assert!(!h.stderr.contains("unknown subcommand"));
    for args in [
        vec!["check", "--bogus", "--json-report", "r.json", "--json"],
        vec!["tests", "--workers", "--json-report=r.json", "--json"],
        vec!["--json-report=r.json", "--json", "check", "--workers"],
        vec!["check", "--workers", "0", "--json-report", "r.json", "--json"],
        vec!["observe", "--bogus", "--json-report", "r.json", "--json"],
        vec!["--json", "--json-report=r.json", "typo"],
        vec!["tests", "--format=invalid", "--json-report=r.json", "--json"],
    ] {
        assert_eq!(snappea(&mut h, &args), 2, "{:?}: {}", args, h.stderr);
        let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
        assert_eq!(result["exit_code"], 2);
        assert!(result["error"].is_string());
        assert_eq!(h.stdout, read(&h, "r.json"));
    }
    assert_eq!(snappea(&mut h, &["--json-report=r.json", "check", "--bogus", "--format=text"]), 2);
    assert!(h.stdout.is_empty());
    assert!(read(&h, "r.json").contains("unknown option"));
    // A command option's value and everything after `--` stay positional.
    assert_eq!(snappea(&mut h, &["check", "--out", "--json", "tcFine"]), 0);
    assert!(h.stdout.starts_with("Checking"));
    assert_eq!(snappea(&mut h, &["tests", "--", "--json"]), 2);
    assert!(!h.stdout.starts_with('{'));
}

#[test]
fn json_keeps_insertion_order() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["--json", "version"]), 0);
    assert!(h.stdout.find("\"version\"").unwrap() < h.stdout.find("\"exit_code\"").unwrap());
}

#[test]
fn failed_case_coverage_is_marked_partial_in_case_and_suite() {
    let mut h = host();
    assert_eq!(snappea(&mut h, &["check", "--seed=2", "--json"]), 1);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage"]["partial"], true);
    assert_eq!(result["test_cases"][0]["coverage"]["partial"], true);
    assert_eq!(result["test_cases"][1]["coverage"]["partial"], false);
    assert_eq!(snappea(&mut h, &["check", "tcFine", "--json"]), 0);
    let result: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(result["coverage"]["partial"], false);
}

#[test]
fn check_includes_scenario_coverage_in_console_and_json() {
    let mut h = host();
    h.add_file("scenario.p", concat!(
        include_str!("../../../tests/upstream/RegressionTests/Feature1SMLevelDecls/Correct/ScenarioCoverageBasic/ScenarioCoverageBasic.p"),
        "\ntest tcScenario [main=Main]: { Main, Server };"
    ));
    assert_eq!(snappea(&mut h, &["check", "scenario.p", "--json-report=r.json"]), 0, "{}", h.stderr);
    assert!(h.stdout.contains("1/2 scenarios covered"), "{}", h.stdout);
    assert!(h.stdout.contains("[  GAP  ] NeverCovered"));
    let result: serde_json::Value = serde_json::from_str(&read(&h, "r.json")).unwrap();
    let artifact: serde_json::Value =
        serde_json::from_str(&read(&h, "PCheckerOutput/tcScenario/scenario_coverage.json")).unwrap();
    assert_eq!(result["scenarios"], serde_json::json!([artifact]));
    assert_eq!(result["test_cases"][0]["scenarios"], result["scenarios"][0]);
}

/// A counter whose spec only checks that the total stays non-negative.
const COUNTER: &str = "event eAdd: int;\n\
     event eTotal: int;\n\
     machine Counter {\n\
       var total: int;\n\
       start state S {\n\
         on eAdd do (n: int) {\n\
           total = total + n;\n\
           announce eTotal, total;\n\
         }\n\
       }\n\
     }\n\
     spec NonNegative observes eTotal {\n\
       start state S { on eTotal do (t: int) { assert t >= 0, \"negative total\"; } }\n\
     }\n\
     machine Driver {\n\
       start state S {\n\
         entry { var c: Counter; c = new Counter(); send c, eAdd, 1; send c, eAdd, 2; }\n\
       }\n\
     }\n\
     test tcCount [main=Driver]: assert NonNegative in { Counter, Driver };\n";

#[test]
fn check_reports_mutants_no_test_case_detects() {
    let mut h = host();
    h.add_file("counter.p", COUNTER);
    // `-` makes the total negative, which the spec detects; removing either statement of
    // the handler leaves the spec nothing to fail on. The spec and driver are not mutated.
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--json"]), 0, "{}{}", h.stdout, h.stderr);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    let m = &v["mutation"];
    assert_eq!((m["mutants"].as_u64(), m["detected"].as_u64()), (Some(3), Some(1)), "{}", m);
    let undetected: Vec<_> = m["undetected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["line"].as_u64().unwrap(), u["change"].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(
        undetected,
        [(7, "removed `total = total + n;`".to_string()), (8, "removed `announce eTotal, total;`".to_string())]
    );
    assert_eq!(m["undetected"][0]["test_cases"], serde_json::json!(["tcCount"]));
    assert_eq!(m["undetected"][0]["search_complete"], true);
    assert_eq!(m["undetected"][0]["schedules"], 5);
    assert_eq!(m["suppressed"], 0);
    assert_eq!(m["time_limit_reached"], false);
    // the same verdicts every run
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--json"]), 0);
    let again: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(again["mutation"]["undetected"], m["undetected"]);
    // the console lists them after the coverage
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--format", "text"]), 0);
    assert!(h.stdout.contains("Mutation testing: 1 of 3 mutants detected"), "{}", h.stdout);
    assert!(h.stdout.contains("  counter.p:7  Counter.S, on eAdd: removed `total = total + n;`\n"), "{}", h.stdout);
    // a suppression comment excludes the mutants of the next statement, of one kind or all
    let suppressed = COUNTER.replace(
        "announce eTotal",
        "// snappea-lint: allow mutation:remove-statement -- the spec reads every total\nannounce eTotal",
    );
    h.add_file("counter.p", &suppressed);
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--json"]), 0, "{}", h.stdout);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!((v["mutation"]["mutants"].as_u64(), v["mutation"]["suppressed"].as_u64()), (Some(2), Some(1)));
    assert_eq!(v["mutation"]["undetected"].as_array().unwrap().len(), 1);
    h.add_file("counter.p", &suppressed.replace("mutation:remove-statement", "mutation"));
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--json"]), 0, "{}", h.stdout);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["mutation"]["suppressed"], 1);
    // a misspelled kind fails linting rather than suppressing nothing
    h.add_file("counter.p", &suppressed.replace("remove-statement", "remove-statment"));
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--format", "text"]), 2);
    assert!(h.stderr.contains("error[invalid-suppression]"), "{}", h.stderr);
    h.add_file("counter.p", COUNTER);
    // `check mutation` runs it alone, without coverage, and fails on an undetected mutant
    assert_eq!(snappea(&mut h, &["check", "mutation", "counter.p", "--json"]), 1);
    let only: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(only["mutation"]["undetected"], m["undetected"]);
    assert!(only.get("coverage").is_none(), "{}", only);
    assert_eq!(snappea(&mut h, &["--format", "text", "check", "mutation", "counter.p"]), 1);
    assert!(!h.stdout.contains("Coverage"), "{}", h.stdout);
    assert!(h.stdout.contains("\n2 mutants not detected in "), "{}", h.stdout);
    h.add_file("counter.p", &suppressed.replace("mutation:remove-statement", "mutation"));
    assert_eq!(snappea(&mut h, &["check", "mutation", "counter.p", "--json"]), 1);
    h.add_file("counter.p", COUNTER);
    assert_eq!(snappea(&mut h, &["check", "mutation", "counter.p", "--no-mutation"]), 2);
    assert_eq!(snappea(&mut h, &["check", "mutation", "coins.p", "--seed", "2", "--json"]), 1);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["mutation"]["skipped"], "a test case failed");
    // skipped on request, and when a test case fails
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--no-mutation", "--json"]), 0);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["mutation"], serde_json::json!({"skipped": "--no-mutation"}));
    assert_eq!(snappea(&mut h, &["check", "counter.p", "--no-mutation", "--mutation-timeout", "5s"]), 2);
    assert_eq!(snappea(&mut h, &["check", "coins.p", "--seed", "2", "--format", "text"]), 1);
    assert!(h.stdout.contains("Mutation testing skipped: a test case failed."), "{}", h.stdout);
}

/// A lock server whose release frees the lock even when it hands it to a waiting client;
/// `{client}` is how each client uses the lock.
fn lock_model(client: &str) -> String {
    format!(
        "event eAcquire: machine;\n\
         event eRelease: machine;\n\
         event eGranted: machine;\n\
         machine LockServer {{\n\
           var free: bool;\n\
           var waiting: seq[machine];\n\
           start state Serving {{\n\
             entry {{ free = true; }}\n\
             on eAcquire do (c: machine) {{\n\
               if (free) {{ free = false; send c, eGranted, c; }} else {{ waiting += (sizeof(waiting), c); }}\n\
             }}\n\
             on eRelease do (c: machine) {{\n\
               var next: machine;\n\
               if (sizeof(waiting) > 0) {{ next = waiting[0]; waiting -= (0); send next, eGranted, next; }}\n\
               free = true;\n\
             }}\n\
           }}\n\
         }}\n\
         spec MutualExclusion observes eGranted, eRelease {{\n\
           var held: bool;\n\
           start state S {{\n\
             on eGranted do {{ assert !held, \"two clients hold the lock\"; held = true; }}\n\
             on eRelease do {{ held = false; }}\n\
           }}\n\
         }}\n\
         // snappea: driver\n\
         machine Client {{\n\
           var server: machine;\n\
           start state Init {{\n\
             entry (s: machine) {{ server = s; send server, eAcquire, this; }}\n\
             on eGranted do {{ {client} }}\n\
           }}\n\
         }}\n\
         machine TestDriver {{\n\
           start state Init {{\n\
             entry {{ var server: machine; server = new LockServer(); new Client(server); new Client(server); }}\n\
           }}\n\
         }}\n\
         test tcTwoClients [main=TestDriver]: assert MutualExclusion in {{ TestDriver, Client, LockServer }};\n"
    )
}

#[test]
fn driver_limits_show_what_the_driver_never_did() {
    let mut h = host();
    h.add_file("lock.p", &lock_model("send server, eRelease, this;"));
    assert_eq!(snappea(&mut h, &["check", "lock.p", "--no-mutation", "--format", "text"]), 0, "{}", h.stderr);
    assert!(
        h.stdout.contains(
            "      Driver limits (held in all "
        ) && h.stdout.contains(
            " schedules):\n        each Client sends eAcquire, then eRelease, once each\n        creates 1 LockServer and 2 Client, never more\n      hint: if the real environment can do more, extend the driver to try it\n"
        ),
        "{}",
        h.stdout
    );
    assert_eq!(snappea(&mut h, &["check", "lock.p", "--no-mutation", "--json"]), 0, "{}", h.stderr);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    let d = &v["test_cases"][0]["driver"];
    assert_eq!(d["machines"], serde_json::json!(["Client", "TestDriver"]));
    assert_eq!(d["repeats"], false);
    assert_eq!(d["facts"][0]["kind"], "sequence");
    assert_eq!(d["facts"][0]["id"], "Client.sequence");
    assert!(d["facts"][0]["hint"].as_str().unwrap().contains("extend the driver"));
    assert_eq!(d["facts"][1]["kind"], "instances");

    // the limit was the gap: a client that acquires the lock again finds the bug
    h.add_file("lock.p", &lock_model("send server, eRelease, this; send server, eAcquire, this;"));
    assert_eq!(snappea(&mut h, &["check", "lock.p", "--no-mutation", "--json"]), 1, "{}", h.stderr);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert!(v["test_cases"][0]["bug"]["report"].as_str().unwrap().contains("two clients hold the lock"));
    // no limits for a failing test case
    assert!(v["test_cases"][0].get("driver").is_none());

    h.add_file("lock.p", &lock_model("send server, eRelease, this;"));
    assert_eq!(snappea(&mut h, &["check", "lock.p", "--no-mutation", "--no-driver-facts", "--format", "text"]), 0);
    assert!(!h.stdout.contains("Driver limits"), "{}", h.stdout);
}

#[test]
fn machines_under_ptst_are_drivers_and_are_not_mutated() {
    let mut h = MemHost::new("/w");
    h.threads = Some(Threads { available: 4, run: run_threads });
    let model = lock_model("send server, eRelease, this;").replace("// snappea: driver\n", "");
    let split = model.find("machine Client").unwrap();
    let test = model.find("test tcTwoClients").unwrap();
    h.add_file("PSrc/Lock.p", &format!("{}{}", &model[..split], &model[test..]));
    h.add_file("PTst/Driver.p", &model[split..test]);
    assert_eq!(snappea(&mut h, &["check", "--json"]), 0, "{}", h.stderr);
    let v: serde_json::Value = serde_json::from_str(&h.stdout).unwrap();
    assert_eq!(v["test_cases"][0]["driver"]["machines"], serde_json::json!(["Client", "TestDriver"]));
    assert_eq!(v["test_cases"][0]["driver"]["facts"][0]["id"], "Client.sequence");
    let mutants = v["mutation"]["undetected"].as_array().unwrap();
    assert!(!mutants.is_empty());
    assert!(mutants.iter().all(|m| !m["file"].as_str().unwrap().contains("PTst")), "{}", h.stdout);
}
