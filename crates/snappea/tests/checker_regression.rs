//! Dynamic regression tests over the vendored upstream corpus (the suites upstream's own
//! harness runs): every `Correct` test must check clean and every `DynamicError` test must
//! find a bug. Uses fewer schedules than upstream's 1000 to keep debug builds fast.
use std::path::{Path, PathBuf};

use snappea::checker::engine::{Checker, CheckerOptions, Output};

const SUITES: &[&str] =
    &["Combined", "Feature1SMLevelDecls", "Feature2Stmts", "Feature3Exprs", "Feature4DataTypes", "Integration"];

struct Quiet;
impl Output for Quiet {
    fn line(&mut self, _: &str) {}
    fn error(&mut self, _: &str) {}
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

/// Checks one test directory; returns a failure description on mismatch.
fn check_dir(dir: &Path, expect_bug: bool) -> Option<String> {
    let mut files = Vec::new();
    p_files(dir, &mut files);
    let mut sm = snappea::syntax::SourceMap::new();
    for f in &files {
        sm.add(f.display().to_string(), String::from_utf8_lossy(&std::fs::read(f).unwrap()).to_string());
    }
    let a = match snappea::sema::compile(&sm, &snappea::sema::Options::default()) {
        Ok(a) => a,
        Err(_) => return Some("does not compile".into()),
    };
    let name = dir.file_name().unwrap().to_str().unwrap().to_lowercase();
    let wanted = if name.contains("param") {
        let text = std::fs::read_to_string(dir.join("ExpectedParametricTests.txt")).unwrap_or_default();
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        if lines.len() != 1 {
            return None; // upstream only lists these
        }
        lines[0].to_string()
    } else {
        "DefaultImpl".to_string()
    };
    let cases = a.program.test_cases();
    let Some(tc) = cases.iter().find(|c| c.name == wanted) else {
        return if expect_bug { None } else { Some(format!("no test case {}", wanted)) };
    };
    let opts = CheckerOptions {
        iterations: 100,
        steps: snappea::checker::runtime::StepBounds { unfair: 1000, fair: 1000, fair_explicit: true },
        ..Default::default()
    };
    match Checker::new(&a.program).check(tc, &opts, &mut Quiet) {
        Err(e) => Some(format!("engine error: {}", e)),
        Ok(r) if (r.report.num_bugs > 0) != expect_bug => Some(match r.report.bug_reports.first() {
            Some(b) => format!("unexpected bug: {}", b),
            None => "no bug found".into(),
        }),
        Ok(_) => None,
    }
}

#[test]
fn upstream_regression_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/RegressionTests");
    let mut failures = Vec::new();
    let mut count = 0;
    for suite in SUITES {
        for (category, expect_bug) in [("Correct", false), ("DynamicError", true)] {
            let dir = root.join(suite).join(category);
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut tests: Vec<_> = rd.map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
            tests.sort();
            for t in tests {
                count += 1;
                if let Some(f) = check_dir(&t, expect_bug) {
                    failures.push(format!("{}: {}", t.display(), f));
                }
            }
        }
    }
    assert!(count > 200, "corpus not found under {}", root.display());
    assert!(failures.is_empty(), "{} of {} tests mismatched:\n{}", failures.len(), count, failures.join("\n"));
}
