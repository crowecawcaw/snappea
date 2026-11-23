//! Replays the shared helper conformance corpus (`tests/conformance/`) through the
//! `snappea observe` CLI. Language helpers run the same corpus through their own APIs; this
//! runner checks that the corpus itself agrees with the core. See
//! `tests/conformance/README.md` for the case layout and the `expect.json` schema.
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The core supports every feature a case can require, so every case runs.
const KNOWN_FEATURES: &[&str] = &["events_dir", "unknown_events"];
const CASE_KEYS: &[&str] = &["description", "requires", "options", "expect"];
const OPTION_KEYS: &[&str] = &["specs", "exclude", "strict", "check_hot", "unknown_events"];
const EXPECT_KEYS: &[&str] = &["outcome", "spec", "key", "message", "liveness", "kind", "coverage"];

#[derive(Debug, PartialEq)]
enum Outcome {
    Pass,
    Violation { spec: String, key: Option<String>, message: String, liveness: bool },
    SetupError { kind: String, message: String },
}

#[test]
fn conformance_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut cases: Vec<PathBuf> =
        std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    cases.sort();
    assert!(!cases.is_empty(), "no conformance cases in {}", root.display());
    let mut failures = Vec::new();
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let expect: Value = serde_json::from_str(&std::fs::read_to_string(case.join("expect.json")).unwrap())
            .unwrap_or_else(|e| panic!("{name}/expect.json: {e}"));
        if let Err(e) = validate(&expect) {
            failures.push(format!("{name}: invalid expect.json: {e}"));
            continue;
        }
        if let Err(e) = run_case(case, &expect) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(failures.is_empty(), "conformance failures:\n{}", failures.join("\n"));
}

fn check_keys(value: &Value, allowed: &[&str], what: &str) -> Result<(), String> {
    let Some(map) = value.as_object() else { return Err(format!("{what} must be an object")) };
    match map.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("unknown key {k:?} in {what}")),
        None => Ok(()),
    }
}

fn validate(case: &Value) -> Result<(), String> {
    check_keys(case, CASE_KEYS, "the case")?;
    if !case["description"].is_string() {
        return Err("description is required".into());
    }
    if let Some(requires) = case.get("requires") {
        for f in requires.as_array().ok_or("requires must be a list")? {
            let f = f.as_str().ok_or("requires must list strings")?;
            if !KNOWN_FEATURES.contains(&f) {
                return Err(format!("unknown feature {f:?} in requires"));
            }
        }
    }
    if let Some(options) = case.get("options") {
        check_keys(options, OPTION_KEYS, "options")?;
        if let Some(policy) = options.get("unknown_events") {
            if !matches!(policy.as_str(), Some("ignore" | "warn" | "error")) {
                return Err("unknown_events must be ignore, warn or error".into());
            }
        }
    }
    let expect = &case["expect"];
    check_keys(expect, EXPECT_KEYS, "expect")?;
    match expect["outcome"].as_str() {
        Some("pass") => {}
        Some("violation") if expect["spec"].is_string() && expect["message"].is_string() => {}
        Some("violation") => return Err("a violation needs spec and message".into()),
        Some("setup_error") if expect["kind"].is_string() => {}
        Some("setup_error") => return Err("a setup_error needs kind".into()),
        _ => return Err("outcome must be pass, violation or setup_error".into()),
    }
    Ok(())
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect()
}

fn run_case(case: &Path, expect: &Value) -> Result<(), String> {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let work = std::env::temp_dir().join(format!(
        "snappea-conformance-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&work);
    // One events folder, as a helper builds it: in-process events first, then the files
    // child processes wrote, each in name order (ties keep this order).
    let events = work.join("events");
    std::fs::create_dir_all(&events).unwrap();
    std::fs::copy(case.join("events.jsonl"), events.join("0-events.jsonl")).unwrap();
    if let Ok(children) = std::fs::read_dir(case.join("children")) {
        for child in children {
            let child = child.unwrap().path();
            let name = child.file_name().unwrap().to_string_lossy().into_owned();
            std::fs::copy(&child, events.join(format!("1-{name}"))).unwrap();
        }
    }
    std::fs::copy(case.join("spec.p"), work.join("spec.p")).unwrap();

    let empty = Map::new();
    let options = expect["options"].as_object().unwrap_or(&empty);
    let expected = &expect["expect"];
    let specs = strings(&options.get("specs").cloned().unwrap_or_default());
    let exclude = strings(&options.get("exclude").cloned().unwrap_or_default());
    // strict defaults to true when specs are named, false when they default to all.
    let strict = options.get("strict").and_then(Value::as_bool).unwrap_or(!specs.is_empty());
    let check_hot = options.get("check_hot").and_then(Value::as_bool).unwrap_or(false);
    let unknown_events = options.get("unknown_events").and_then(Value::as_str).unwrap_or("error");

    let mut args: Vec<String> = vec!["observe".into(), "spec.p".into(), "events".into()];
    if specs.is_empty() {
        args.push("--all-specs".into());
        if !exclude.is_empty() {
            args.push("--exclude-spec".into());
            args.push(exclude.join(","));
        }
    } else {
        args.push("--spec".into());
        args.push(specs.join(","));
    }
    args.extend(["--json-report", "report.json"].map(String::from));
    if strict {
        args.push("--strict".into());
    }
    if check_hot {
        args.push("--check-hot".into());
    }
    args.extend(["--unknown-events".into(), unknown_events.into()]);
    let output = Command::new(env!("CARGO_BIN_EXE_snappea")).current_dir(&work).args(&args).output().unwrap();
    let report: Option<Value> =
        std::fs::read_to_string(work.join("report.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
    let _ = std::fs::remove_dir_all(&work);
    let context = || {
        format!(
            "\n  snappea {}\n  exit {:?}\n  report {}",
            args.join(" "),
            output.status.code(),
            report.as_ref().map(Value::to_string).unwrap_or_else(|| String::from_utf8_lossy(&output.stdout).into())
        )
    };

    let actual = outcome(output.status.code(), report.as_ref());
    compare(expected, &actual).map_err(|e| format!("{e}{}", context()))?;
    if let Some(cov) = expected.get("coverage").and_then(Value::as_object) {
        let report = report.as_ref().ok_or_else(|| format!("no report for coverage{}", context()))?;
        for (spec, want) in cov {
            let got = report["specs"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|s| s["spec"] == spec.as_str())
                .map(|s| &s["coverage"])
                .ok_or_else(|| format!("no coverage for {spec}{}", context()))?;
            for part in ["statements", "branches", "handlers"] {
                for count in ["covered", "total"] {
                    if want[part].get(count).is_some() && want[part][count] != got[part][count] {
                        return Err(format!(
                            "{spec} {part}.{count}: expected {}, got {}{}",
                            want[part][count],
                            got[part][count],
                            context()
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Classifies a CLI run the way a helper does: a spec violation if any spec reports
/// one, otherwise a setup error of the first error's kind.
fn outcome(status: Option<i32>, report: Option<&Value>) -> Outcome {
    if status == Some(0) {
        return Outcome::Pass;
    }
    let Some(report) = report else {
        return Outcome::SetupError { kind: "config".into(), message: String::new() };
    };
    let errors: Vec<(String, &Value)> = report["specs"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| {
            let spec = s["spec"].as_str().unwrap_or_default().to_string();
            s["errors"].as_array().into_iter().flatten().map(move |e| (spec.clone(), e))
        })
        .collect();
    if let Some((spec, e)) = errors.iter().find(|(_, e)| e["kind"] == "spec_violation") {
        let message = e["message"].as_str().unwrap_or_default().to_string();
        return Outcome::Violation {
            spec: spec.clone(),
            key: e["key"].as_str().map(String::from),
            liveness: message.contains("detected liveness bug"),
            message,
        };
    }
    match errors.first() {
        Some((_, e)) => Outcome::SetupError {
            kind: e["kind"].as_str().unwrap_or("unknown").into(),
            message: format!(
                "{} {}",
                e["message"].as_str().unwrap_or_default(),
                e["cause"].as_str().unwrap_or_default()
            ),
        },
        None => Outcome::SetupError { kind: "config".into(), message: String::new() },
    }
}

fn compare(expected: &Value, actual: &Outcome) -> Result<(), String> {
    let substring = |field: &str, got: &str| match expected[field].as_str() {
        Some(want) if !got.contains(want) => Err(format!("{field}: expected to contain {want:?}, got {got:?}")),
        _ => Ok(()),
    };
    match (expected["outcome"].as_str().unwrap(), actual) {
        ("pass", Outcome::Pass) => Ok(()),
        ("violation", Outcome::Violation { spec, key, message, liveness }) => {
            if expected["spec"] != spec.as_str() {
                return Err(format!("spec: expected {}, got {spec}", expected["spec"]));
            }
            if let Some(want) = expected["key"].as_str() {
                if key.as_deref() != Some(want) {
                    return Err(format!("key: expected {want}, got {key:?}"));
                }
            }
            if expected["liveness"].as_bool().unwrap_or(false) != *liveness {
                return Err(format!("liveness: expected {}, got {liveness}", expected["liveness"]));
            }
            substring("message", message)
        }
        ("setup_error", Outcome::SetupError { kind, message }) => {
            if expected["kind"] != kind.as_str() {
                return Err(format!("kind: expected {}, got {kind}", expected["kind"]));
            }
            substring("message", message)
        }
        (want, got) => Err(format!("expected {want}, got {got:?}")),
    }
}
