//! Runs the checker over the upstream regression corpus the way upstream's unit-test harness
//! does: `Correct` tests must check clean, `DynamicError` tests must find a bug, with 1000
//! schedules of at most 1000 steps. The `DefaultImpl` test case is checked, or the single
//! expected test of a parametric test directory.
//!
//! Exits with status 1 when any test disagrees with upstream.
//!
//! Usage: corpus [-v] [--trace] [--seed N] [--schedules N] [--strategy NAME] [--bound N]
//! [--filter SUBSTR] ROOT...
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use snappea::checker::engine::{Checker, CheckerOptions, Output};
use snappea::checker::strategy::StrategyKind;

struct Quiet;
impl Output for Quiet {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
}

#[derive(Default)]
struct Stats {
    ok: usize,
    mismatched: usize,
    skipped: usize,
}

struct Args {
    verbose: bool,
    seed: u32,
    schedules: usize,
    filter: Option<String>,
    trace: bool,
    strategy: StrategyKind,
    bound: usize,
}

const USAGE: &str = "usage: corpus [-v] [--trace] [--seed N] [--schedules N] [--strategy NAME] \
                     [--bound N] [--filter SUBSTR] ROOT...";

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2);
}

/// The value of the flag at `raw[*i]`, advancing `i` past it.
fn value<T: FromStr>(raw: &[String], i: &mut usize) -> T {
    *i += 1;
    raw.get(*i).and_then(|v| v.parse().ok()).unwrap_or_else(|| usage())
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut args = Args {
        verbose: false,
        seed: 0,
        schedules: 1000,
        filter: None,
        trace: false,
        strategy: StrategyKind::Random,
        bound: 0,
    };
    let mut roots = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "-v" => args.verbose = true,
            "--trace" => args.trace = true,
            "--seed" => args.seed = value(&raw, &mut i),
            "--schedules" => args.schedules = value(&raw, &mut i),
            "--strategy" => args.strategy = value(&raw, &mut i),
            "--bound" => args.bound = value(&raw, &mut i),
            "--filter" => args.filter = Some(value(&raw, &mut i)),
            r if r.starts_with('-') => usage(),
            r => roots.push(r.to_string()),
        }
        i += 1;
    }
    if roots.is_empty() {
        usage();
    }
    let mut stats = Stats::default();
    for root in &roots {
        find_tests(Path::new(root), &mut stats, &args);
    }
    println!("ok={} mismatched={} skipped={}", stats.ok, stats.mismatched, stats.skipped);
    if stats.mismatched > 0 {
        std::process::exit(1);
    }
}

fn p_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            p_files(&e, out);
        } else if e.extension().map(|x| x == "p").unwrap_or(false) {
            out.push(e);
        }
    }
}

fn is_category(n: &str) -> bool {
    matches!(n, "Correct" | "DynamicError" | "StaticError")
}

fn find_tests(dir: &Path, stats: &mut Stats, args: &Args) {
    let parent_is_category =
        dir.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(is_category).unwrap_or(false);
    if !parent_is_category {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for e in entries.into_iter().filter(|e| e.is_dir()) {
            find_tests(&e, stats, args);
        }
        return;
    }
    let category = dir.parent().unwrap().file_name().unwrap().to_str().unwrap().to_string();
    if category == "StaticError" {
        return;
    }
    if let Some(f) = &args.filter {
        if !dir.display().to_string().contains(f.as_str()) {
            return;
        }
    }
    let mut files = Vec::new();
    p_files(dir, &mut files);
    if files.is_empty() {
        return;
    }
    let mut sm = snappea::syntax::SourceMap::new();
    for f in &files {
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).to_string();
        sm.add(f.display().to_string(), text);
    }
    let analysis = match snappea::sema::compile(&sm, &snappea::sema::Options::default()) {
        Ok(a) => a,
        Err(_) => {
            stats.mismatched += 1;
            println!("MISMATCH {} ({}): does not compile", dir.display(), category);
            return;
        }
    };
    let cases = analysis.program.test_cases();
    let dir_name = dir.file_name().unwrap().to_str().unwrap().to_lowercase();
    let wanted: Option<String> = if dir_name.contains("param") {
        let expected = std::fs::read_to_string(dir.join("ExpectedParametricTests.txt")).unwrap_or_default();
        let lines: Vec<&str> =
            expected.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        if lines.len() != 1 {
            stats.skipped += 1;
            return;
        }
        Some(lines[0].to_string())
    } else {
        Some("DefaultImpl".to_string())
    };
    let tc = cases.iter().find(|c| Some(&c.name) == wanted.as_ref());
    let Some(tc) = tc else {
        // upstream fails to load the test class: an error exit, which counts as a bug
        let ok = category == "DynamicError";
        record(stats, ok, dir, &category, "no test case", &format!("no test case {:?}", wanted));
        return;
    };
    if args.verbose {
        eprintln!("RUN {}", dir.display());
    }
    let checker = Checker::new(&analysis.program);
    let opts = CheckerOptions {
        seed: args.seed,
        iterations: args.schedules,
        steps: snappea::checker::runtime::StepBounds { unfair: 1000, fair: 1000, fair_explicit: true },
        strategy: args.strategy,
        strategy_bound: args.bound,
        ..Default::default()
    };
    let start = Instant::now();
    let r = checker.check(tc, &opts, &mut Quiet);
    let elapsed = start.elapsed().as_secs_f64();
    match r {
        Err(e) => record(stats, false, dir, &category, "engine error", &e),
        Ok(res) => {
            let found = res.report.num_bugs > 0;
            let ok = found == (category == "DynamicError");
            let what = if found { "bug" } else { "no bug" };
            let detail = res.report.bug_reports.first().cloned().unwrap_or_default();
            if args.trace {
                for l in &res.last_log {
                    println!("  | {}", l);
                }
            }
            if let (true, Some(x)) = (args.verbose, &res.exhaustive) {
                let n = res.report.explored_fair_schedules + res.report.explored_unfair_schedules;
                println!(
                    "SEARCH {} finished={} states={} schedules={} pruned={} bound_hits={} depth={}",
                    dir.display(),
                    x.finished,
                    x.distinct_states,
                    n,
                    x.pruned_schedules,
                    x.depth_bound_hits,
                    x.max_depth
                );
            }
            record(stats, ok, dir, &category, what, &format!("{} [{:.2}s]", detail, elapsed));
        }
    }
    if args.verbose && elapsed > 5.0 {
        println!("SLOW {} {:.1}s", dir.display(), elapsed);
    }
}

fn record(stats: &mut Stats, ok: bool, dir: &Path, category: &str, what: &str, detail: &str) {
    if ok {
        stats.ok += 1;
    } else {
        stats.mismatched += 1;
        println!("MISMATCH {} ({}): {} {}", dir.display(), category, what, detail);
    }
}
