//! Replays the shared helper conformance corpus (`tests/conformance/`) through the
//! `snappea::test` API: `emit_at` for `events.jsonl`, children files copied into the
//! recording's events folder. See `tests/conformance/README.md`.
use snappea::test::{CheckError, Model, Time, UnknownEvents, Value};
use std::path::{Path, PathBuf};

/// Every feature the corpus names; a case requiring anything else would be skipped.
const SUPPORTED: &[&str] = &["events_dir", "unknown_events"];

#[test]
fn conformance_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut cases: Vec<PathBuf> =
        std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    cases.sort();
    assert!(!cases.is_empty(), "no conformance cases in {}", root.display());
    let (mut failures, mut ran) = (Vec::new(), 0);
    for case in &cases {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let expect: Value = serde_json::from_str(&std::fs::read_to_string(case.join("expect.json")).unwrap()).unwrap();
        let missing: Vec<&str> = expect["requires"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|f| !SUPPORTED.contains(f))
            .collect();
        if !missing.is_empty() {
            eprintln!("skip {name}: snappea::test does not support {}", missing.join(", "));
            continue;
        }
        ran += 1;
        if let Err(e) = run_case(case, &expect) {
            failures.push(format!("{name}: {e}"));
        }
    }
    eprintln!("conformance: {ran} of {} ran", cases.len());
    assert_eq!(ran, cases.len(), "snappea::test must run every case");
    assert!(failures.is_empty(), "conformance failures:\n{}", failures.join("\n"));
}

fn strings(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
}

fn run_case(case: &Path, expect: &Value) -> Result<(), String> {
    let options = &expect["options"];
    let mut model = Model::from_files([case.join("spec.p")]).map_err(|e| e.to_string())?;
    if let Some(specs) = strings(&options["specs"]) {
        model = model.specs(specs);
    }
    if let Some(exclude) = strings(&options["exclude"]) {
        model = model.exclude(exclude);
    }
    if let Some(strict) = options["strict"].as_bool() {
        model = model.strict(strict);
    }
    if let Some(check_hot) = options["check_hot"].as_bool() {
        model = model.check_hot(check_hot);
    }
    if let Some(policy) = options["unknown_events"].as_str() {
        model = model.unknown_events(policy.parse::<UnknownEvents>()?);
    }

    let recording = model.recording();
    for line in std::fs::read_to_string(case.join("events.jsonl")).unwrap().lines() {
        let e: Value = serde_json::from_str(line).unwrap();
        let ts = snappea::checker::observe_json::parse_instant(e["timestamp"].as_str().unwrap()).unwrap();
        let at = Time::from_nanos(ts.secs * 1_000_000_000 + i64::from(ts.nanos));
        recording.emit_at(at, e["event"].as_str().unwrap(), e["key"].as_str().unwrap(), &e["payload"]);
    }
    if let Ok(children) = std::fs::read_dir(case.join("children")) {
        let dir = recording.events_dir();
        for child in children {
            let child = child.unwrap().path();
            std::fs::copy(&child, dir.join(child.file_name().unwrap())).unwrap();
        }
    }

    let want = &expect["expect"];
    let substring = |field: &str, got: &str| match want[field].as_str() {
        Some(w) if !got.contains(w) => Err(format!("{field}: expected to contain {w:?}, got {got:?}")),
        _ => Ok(()),
    };
    match (want["outcome"].as_str().unwrap(), recording.verify()) {
        ("pass", Ok(report)) => {
            let Some(coverage) = want["coverage"].as_object() else { return Ok(()) };
            let got = report.coverage().map_err(|e| e.to_string())?;
            for (spec, w) in coverage {
                let g = got.specs.get(spec).ok_or(format!("no coverage for {spec}"))?.to_json(false);
                for part in ["statements", "branches", "handlers"] {
                    for count in ["covered", "total"] {
                        if w[part].get(count).is_some() && w[part][count] != g[part][count] {
                            return Err(format!(
                                "{spec} {part}.{count}: expected {}, got {}",
                                w[part][count], g[part][count]
                            ));
                        }
                    }
                }
            }
            Ok(())
        }
        ("violation", Err(CheckError::Violation(v))) => {
            if want["spec"] != v.spec.as_str() {
                return Err(format!("spec: expected {}, got {}", want["spec"], v.spec));
            }
            if let Some(key) = want["key"].as_str() {
                if key != v.key {
                    return Err(format!("key: expected {key}, got {}", v.key));
                }
            }
            if want["liveness"].as_bool().unwrap_or(false) != v.liveness {
                return Err(format!("liveness: expected {}, got {}", want["liveness"], v.liveness));
            }
            substring("message", &v.message)
        }
        ("setup_error", Err(CheckError::Setup(e))) => {
            if want["kind"] != e.kind.as_str() {
                return Err(format!("kind: expected {}, got {} ({})", want["kind"], e.kind, e.message));
            }
            substring("message", &e.message)
        }
        (outcome, Ok(_)) => Err(format!("expected {outcome}, got a pass")),
        (outcome, Err(e)) => Err(format!("expected {outcome}, got:\n{e}")),
    }
}
