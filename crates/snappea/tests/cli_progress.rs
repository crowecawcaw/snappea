//! Progress reporting of `snappea check` (`--progress`) on an in-memory host.

use std::cell::{Cell, RefCell};

use serde_json::Value;
use snappea::driver::progress::{Phase, Progress};
use snappea::host::{Job, MemHost, Threads};

const MODEL: &str = "machine Flips {\n\
       start state S {\n\
         entry {\n\
           var n: int;\n\
           var k: int;\n\
           while (k < 12) {\n\
             if ($) { n = n + 1; }\n\
             k = k + 1;\n\
           }\n\
           assert n <= 12, \"too many\";\n\
         }\n\
       }\n\
     }\n\
     machine Coins {\n\
       start state S {\n\
         entry {\n\
           var n: int;\n\
           while (n < 3) {\n\
             if ($) { n = n + 1; } else { goto Done; }\n\
           }\n\
           assert false, \"three heads\";\n\
         }\n\
       }\n\
       state Done { }\n\
     }\n\
     // snappea-lint: allow no-model-machine -- progress fixture keeps behavior in the entry machine\n\
     test tcFlips [main=Flips]: { Flips };\n\
     // snappea-lint: allow no-model-machine -- progress fixture keeps behavior in the entry machine\n\
     test tcCoins [main=Coins]: { Coins };\n";

thread_local! {
    static NOW: Cell<f64> = const { Cell::new(0.0) };
    static SEEN: RefCell<Vec<Progress>> = const { RefCell::new(Vec::new()) };
}

/// A clock that moves 10ms each time it is read, so snapshots come due.
fn clock() -> f64 {
    NOW.with(|t| {
        t.set(t.get() + 0.01);
        t.get()
    })
}

fn record(p: &Progress) {
    SEEN.with(|s| s.borrow_mut().push(p.clone()));
}

fn seen() -> Vec<Progress> {
    SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

fn run_threads<'a>(jobs: Vec<Job<'a>>, main: &mut (dyn FnMut() + 'a)) {
    std::thread::scope(|s| {
        for job in jobs {
            s.spawn(job);
        }
        main();
    });
}

fn host(threads: bool) -> MemHost {
    let mut h = MemHost::new("/w");
    h.clock = clock;
    h.on_progress = Some(record);
    if threads {
        h.threads = Some(Threads { available: 4, run: run_threads });
    }
    h.add_file("m.p", MODEL);
    seen();
    h
}

fn snappea(h: &mut MemHost, args: &[&str]) -> i32 {
    h.stdout.clear();
    h.stderr.clear();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    snappea::cli::run(&args, h)
}

/// The JSON progress lines on stderr.
fn json_lines(stderr: &str) -> Vec<Value> {
    stderr.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{}: {}", l, e))).collect()
}

/// For each worker count: the last snapshot of each test case has the counts the result
/// reports, and the counts before it never go down.
#[test]
fn json_progress_ends_with_the_reported_counts() {
    for threads in [false, true] {
        let mut h = host(threads);
        let code = snappea(
            &mut h,
            &["check", "--json", "--progress", "json", "--no-mutation", "--timeout", "3s", "--seed", "1"],
        );
        assert_eq!(code, 1, "tcCoins has a bug: {}", h.stdout);
        let result: Value = serde_json::from_str(&h.stdout).unwrap();
        let lines = json_lines(&h.stderr);
        assert!(lines.len() > 2, "{}", h.stderr);
        for (i, case) in result["test_cases"].as_array().unwrap().iter().enumerate() {
            let name = case["name"].as_str().unwrap();
            let mine: Vec<&Value> = lines.iter().filter(|l| l["test_case"] == name).collect();
            assert_eq!(mine.first().unwrap()["schedules"], 0, "{}", h.stderr);
            let last = mine.last().unwrap();
            assert_eq!(last["done"], true, "{}", h.stderr);
            assert_eq!(last["schedules"], case["schedules"], "{}", h.stderr);
            assert_eq!(last["bugs"].as_u64().unwrap() > 0, case["result"] == "bug");
            assert_eq!(last["workers"], case["workers"]);
            assert_eq!(last["test"], i + 1);
            assert_eq!(last["tests"], 2);
            assert_eq!(last["time_limit"], 3.0);
            let counts: Vec<u64> = mine.iter().map(|l| l["schedules"].as_u64().unwrap()).collect();
            assert!(counts.windows(2).all(|w| w[0] <= w[1]), "{:?}", counts);
            assert_eq!(mine.iter().filter(|l| l["done"] == true).count(), 1);
        }
        // the host received the same snapshots
        assert_eq!(seen().len(), lines.len());
    }
}

#[test]
fn mutation_testing_reports_its_progress() {
    let mut h = host(false);
    snappea(
        &mut h,
        &["check", "tcFlips", "--json", "--progress", "json", "--timeout", "1s", "--mutation-timeout", "2s"],
    );
    let result: Value = serde_json::from_str(&h.stdout).unwrap();
    let mutation: Vec<Value> = json_lines(&h.stderr).into_iter().filter(|l| l["phase"] == "mutation").collect();
    let last = mutation.last().unwrap_or_else(|| panic!("no mutation progress: {}", h.stderr));
    assert_eq!(last["done"], true);
    assert_eq!(last["detected"], result["mutation"]["detected"]);
    assert_eq!(last["mutants"], result["mutation"]["mutants"]);
    assert_eq!(last["time_limit"], 2.0);
}

/// Agents and scripts (stderr not a terminal, not in CI) see nothing on stderr, and
/// `--progress off` turns the snapshots off altogether.
#[test]
fn auto_progress_is_silent_off_a_terminal() {
    let mut h = host(false);
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "1s"]);
    assert_eq!(h.stderr, "");
    assert!(!seen().is_empty(), "the host still receives snapshots");
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "1s", "--progress", "off"]);
    assert_eq!(h.stderr, "");
    assert!(seen().is_empty());
}

#[test]
fn a_terminal_gets_a_status_line_that_is_cleared() {
    let mut h = host(false);
    h.stderr_terminal = true;
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "1s"]);
    assert!(h.stderr.starts_with("\r\x1b[2KChecking tcFlips: 0 schedules, 0.0s of 1s"), "{:?}", h.stderr);
    assert!(h.stderr.ends_with("\r\x1b[2K"), "{:?}", h.stderr);
    // --verbose logs every schedule instead
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "1s", "-v"]);
    assert_eq!(h.stderr, "");
}

#[test]
fn ci_gets_a_line_every_30_seconds() {
    let mut h = host(false);
    h.env.insert("CI".into(), "true".into());
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "100s"]);
    let lines: Vec<&str> = h.stderr.lines().collect();
    // the search may end before the time limit; it shows no line before 30s
    let elapsed = seen().iter().filter_map(|p| p.done.then_some(p.elapsed)).next().unwrap();
    assert_eq!(lines.len(), (elapsed / 30.0) as usize, "{:?}", h.stderr);
    assert!(!lines.is_empty(), "the check ended after {}s", elapsed);
    for l in lines {
        assert!(l.starts_with("Checking tcFlips: "), "{}", l);
    }
}

#[test]
fn a_bad_progress_value_is_a_usage_error() {
    let mut h = host(false);
    assert_eq!(snappea(&mut h, &["check", "--progress", "always"]), 2);
    assert!(h.stderr.contains("'--progress' takes auto, off or json, not 'always'"), "{}", h.stderr);
}

/// `snappea-compat` keeps upstream's output: no snapshots.
#[test]
fn compat_reports_no_progress() {
    let mut h = host(false);
    let args: Vec<String> = ["check", "m.p", "-tc", "tcFlips", "-s", "50"].iter().map(|s| s.to_string()).collect();
    snappea::compat::run(&args, &mut h);
    assert!(seen().is_empty());
}

#[test]
fn snapshots_name_the_phase() {
    let mut h = host(false);
    snappea(&mut h, &["check", "tcFlips", "--no-mutation", "--timeout", "1s"]);
    let all = seen();
    assert!(all.iter().all(|p| matches!(&p.phase, Phase::Check { test_case, .. } if test_case == "tcFlips")));
}
