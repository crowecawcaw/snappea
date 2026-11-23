use snappea::test::{json, CheckError, ErrorKind, Model, UnknownEvents};
use std::path::PathBuf;
use std::process::Command;

const SPEC: &str = r#"
event Stock: int;
event Pair: (order: string, units: int);
event Done;
event Unobserved;
spec Inventory observes Stock, Pair, Done {
  var stock: int;
  start state Ready {
    on Stock do (n: int) { assert n >= 0, "negative stock"; stock = stock + n; }
    on Pair do (p: (order: string, units: int)) {
      stock = stock - p.units;
      assert stock >= 0, "oversold";
    }
    on Done goto Complete;
  }
  state Complete { }
}
spec Finishes observes Stock, Done {
  start hot state Open { on Stock do {} on Done goto Closed; }
  state Closed { on Stock do {} }
}
"#;

fn model() -> Model {
    Model::compile(&[("inventory.p", SPEC)]).unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("snappea-test-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup_error(result: Result<snappea::test::CheckReport, CheckError>) -> snappea::test::SetupError {
    match result {
        Err(CheckError::Setup(e)) => e,
        other => panic!("expected a setup error, got {other:?}"),
    }
}

#[test]
fn payload_forms() {
    let model = model();
    for pair in [json!({"order":"a","units":1}), json!(["a", 1])] {
        let recording = model.recording();
        recording.emit("Stock", "coffee", 2);
        recording.emit("Pair", "coffee", pair);
        recording.emit("Done", "coffee", ());
        let checked = recording.verify().unwrap();
        assert_eq!(checked.specs(), ["Inventory", "Finishes"]);
        assert_eq!(checked.verified_events(), 5);
    }
    #[derive(serde::Serialize)]
    struct Pair {
        order: String,
        units: i64,
    }
    let recording = model.clone().specs(["Inventory"]).recording();
    recording.emit("Stock", "tea", 1);
    recording.emit("Pair", "tea", Pair { order: "o".into(), units: 1 });
    assert_eq!(recording.verify().unwrap().verified_events(), 2);
}

#[test]
fn invalid_payloads_are_parser_errors_naming_the_emit() {
    for (name, payload) in [("Stock", json!("wrong")), ("Stock", json!(null)), ("Done", json!(1))] {
        let recording = model().recording();
        recording.emit(name, "coffee", payload);
        let e = setup_error(recording.verify());
        assert_eq!(e.kind, ErrorKind::Parser);
        assert!(e.message.contains(name) && e.message.contains("tests/test_integration.rs:"), "{e}");
    }
}

#[test]
fn unknown_events_error_by_default_and_point_at_the_emit() {
    let recording = model().recording();
    recording.emit("Stock", "coffee", 1);
    recording.emit("Stok", "coffee", 1);
    let e = setup_error(recording.verify());
    assert_eq!(e.kind, ErrorKind::UnknownEvent);
    assert!(e.message.contains("'Stok'") && e.message.contains("did you mean Stock?"), "{e}");
    assert!(e.message.contains("first at crates/snappea/tests/test_integration.rs:"), "{e}");

    // Declared but unobserved events are always ignored.
    let recording = model().recording();
    recording.emit("Stock", "coffee", 1);
    recording.emit("Unobserved", "coffee", json!({"any": "thing"}));
    recording.verify().unwrap();

    for policy in [UnknownEvents::Warn, UnknownEvents::Ignore] {
        let recording = model().unknown_events(policy).recording();
        recording.emit("Stock", "coffee", 1);
        recording.emit("Stok", "coffee", 1);
        let report = recording.verify().unwrap();
        assert_eq!(report.warnings().len(), usize::from(policy == UnknownEvents::Warn));
    }
}

#[test]
fn spec_selection_and_strict_defaults() {
    // All specs, not strict: an empty recording passes without checking.
    assert!(model().recording().verify().unwrap().specs().is_empty());
    // Named specs are strict.
    let e = setup_error(model().specs(["Inventory"]).recording().verify());
    assert_eq!(e.kind, ErrorKind::Validation);
    assert!(e.message.contains("Inventory"));
    model().specs(["Inventory"]).strict(false).recording().verify().unwrap();
    // exclude, and unknown names.
    let recording = model().exclude(["Finishes"]).check_hot(true).recording();
    recording.emit("Stock", "k", 1);
    assert_eq!(recording.verify().unwrap().specs(), ["Inventory"]);
    for m in [model().specs(["Missing"]), model().exclude(["Missing"])] {
        let recording = m.recording();
        recording.emit("Stock", "k", 1);
        let e = setup_error(recording.verify());
        assert_eq!(e.kind, ErrorKind::Config);
        assert!(e.message.contains("Missing") && e.message.contains("Inventory, Finishes"), "{e}");
    }
}

#[test]
fn violations_are_structured_and_formatted_like_python() {
    let recording = model().recording();
    recording.emit("Stock", "coffee", 1);
    recording.emit("Stock", "tea", 1);
    recording.emit_at(snappea::test::Time::now(), "Pair", "coffee", json!({"order": "o1", "units": 2}));
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    assert_eq!((v.spec.as_str(), v.key.as_str(), v.message.as_str()), ("Inventory", "coffee", "oversold"));
    assert_eq!(v.location.as_deref(), Some("inventory.p:12"));
    assert!(!v.liveness && v.others.is_empty());
    assert_eq!(v.events.iter().map(|e| e.event.as_str()).collect::<Vec<_>>(), ["Stock", "Pair"]);
    assert_eq!(v.events[1].payload, json!({"order": "o1", "units": 2}));
    assert!(v.events[1].at);
    let src = v.events[1].src.as_deref().unwrap();
    assert!(src.starts_with("crates/snappea/tests/test_integration.rs:"), "{src}");
    let text = v.to_string();
    assert!(text.starts_with("P spec Inventory was broken: oversold\n\n  Events (key \"coffee\"):\n"), "{text}");
    assert!(text.contains("order=o1 units=2") && text.contains("<- broke the spec here"), "{text}");
    assert!(text.contains("(time set with emit_at)") && text.contains("  Spec: inventory.p:12"), "{text}");
}

#[test]
fn liveness_and_further_violations() {
    let recording = model().check_hot(true).recording();
    recording.emit("Stock", "a", 1);
    recording.emit("Stock", "b", -1);
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    let all: Vec<_> = std::iter::once(&*v).chain(&v.others).collect();
    assert_eq!(all.len(), 3, "{v}");
    let hot = all.iter().find(|v| v.liveness).unwrap();
    assert_eq!(hot.spec, "Finishes");
    assert_eq!(hot.message, "it ended in hot state 'Open'");
    assert!(v.to_string().contains("more violation(s)"), "{v}");
}

#[test]
fn expect_violation() {
    let recording = model().specs(["Inventory"]).recording();
    recording.emit("Pair", "coffee", json!(["o1", 1]));
    let v = recording.expect_violation("Inventory", "over.old");
    assert_eq!(v.key, "coffee");

    let passes = model().recording();
    passes.emit("Stock", "coffee", 1);
    let panic = std::panic::catch_unwind(|| passes.expect_violation("Inventory", "oversold")).unwrap_err();
    assert!(panic.downcast_ref::<String>().unwrap().contains("but verification passed"));

    let other = model().recording();
    other.emit("Pair", "coffee", json!(["o1", 1]));
    let panic = std::panic::catch_unwind(|| other.expect_violation("Inventory", "negative")).unwrap_err();
    assert!(panic.downcast_ref::<String>().unwrap().contains("no message matches"));
}

#[test]
fn late_events_panic_and_emitters_work_across_threads() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<snappea::test::Emitter>();
    require_send_sync::<snappea::test::Recording>();
    let recording = model().specs(["Inventory"]).recording();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let emitter = recording.emitter();
            std::thread::spawn(move || emitter.emit("Stock", format!("product-{i}"), 1))
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    std::thread::scope(|s| {
        s.spawn(|| recording.emit("Stock", "scoped", 1));
    });
    let emitter = recording.emitter();
    assert_eq!(recording.verify().unwrap().verified_events(), 5);
    assert!(std::panic::catch_unwind(|| emitter.emit("Stock", "coffee", 1)).is_err());
}

#[test]
fn unserializable_payloads_fail_verify_naming_the_emit() {
    let recording = model().recording();
    let mut bad = std::collections::HashMap::new();
    bad.insert((1, 2), 3);
    recording.emit("Stock", "coffee", bad);
    let e = setup_error(recording.verify());
    assert_eq!(e.kind, ErrorKind::Parser);
    assert!(e.message.contains("Stock") && e.message.contains("tests/test_integration.rs:"), "{e}");
}

#[test]
fn emitter_records_in_process() {
    let recording = model().recording();
    let emitter = recording.emitter();
    emitter.emit("Stock", "coffee", 1);
    emitter.emit("Pair", "coffee", json!({"order": "o", "units": 2}));
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    assert!(v.events[1].src.as_deref().unwrap().contains("integration.rs"));
}

#[test]
fn unrecordable_events_keep_the_folder_like_other_failures() {
    let recording = model().recording();
    let dir = recording.events_dir();
    let mut bad = std::collections::HashMap::new();
    bad.insert((1, 2), 3);
    recording.emitter().emit("Stock", "coffee", bad);
    let e = setup_error(recording.verify());
    assert_eq!(e.kind, ErrorKind::Parser);
    let artifacts = e.artifacts.expect("a failed check keeps its folder");
    assert_eq!(artifacts.join("events"), dir);
    assert!(artifacts.join("recording.jsonl").is_file());
    std::fs::remove_dir_all(artifacts).unwrap();
}

#[test]
fn explicit_times_order_events() {
    let recording = model().specs(["Inventory"]).recording();
    recording.emit_at(20, "Pair", "coffee", json!(["o", 1]));
    recording.emit_at(10, "Stock", "coffee", 1);
    recording.verify().unwrap();
}

#[test]
fn coverage_merges_into_one_file_from_many_threads() {
    let dir = temp("coverage");
    let path = dir.join("nested/coverage.json");
    let model = model().specs(["Inventory"]).coverage(&path);
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let model = model.clone();
            std::thread::spawn(move || {
                let recording = model.recording();
                recording.emit("Stock", "k", 1);
                if i % 2 == 0 {
                    recording.emit("Done", "k", ());
                }
                recording.verify().unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let suite = snappea::test::ObserveCoverageReport::from_json_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    // the union of every thread's check: half of them handle `Done`, none `Pair`
    let missed: Vec<String> = suite.specs["Inventory"].uncovered().iter().map(|u| u.context.clone()).collect();
    assert_eq!(missed, ["Inventory.Ready, on Pair"]);
    assert!(!dir.join("nested/coverage.json.lock").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn closure_checks_after_the_body_and_preserves_panics() {
    let model = model();
    let answer = model.check(|r| {
        r.emit("Stock", "coffee", 1);
        42
    });
    assert_eq!(answer, 42);
    let panic = std::panic::catch_unwind(|| model.check(|_| panic!("primary failure"))).unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"primary failure"));
    let panic = std::panic::catch_unwind(|| {
        model.check(|r| r.emit("Stock", "coffee", -1));
    })
    .unwrap_err();
    assert!(panic.downcast_ref::<String>().unwrap().starts_with("P spec Inventory was broken: negative stock"));
}

#[test]
fn compile_errors_name_the_file() {
    let e = Model::compile(&[("broken.p", "spec {")]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Config);
    assert!(e.message.contains("broken.p"), "{e}");
    let e = Model::compile(&[("types.p", "event E; spec S observes E { start state A { entry { absent = 1; } } }")])
        .unwrap_err();
    assert!(e.message.contains("types.p") && e.message.contains("absent"), "{e}");
}

#[test]
fn files_folders_globs_and_cargo_metadata() {
    let dir = temp("files");
    std::fs::create_dir_all(dir.join("specs")).unwrap();
    std::fs::write(dir.join("specs/inventory.p"), SPEC).unwrap();
    std::fs::write(dir.join("specs/notes.txt"), "not P").unwrap();
    for path in ["specs", "specs/*", "specs/inventory.p"] {
        let model = Model::from_files([dir.join(path)]).unwrap();
        let recording = model.recording();
        recording.emit("Stock", "k", 1);
        assert_eq!(recording.verify().unwrap().specs().len(), 2, "{path}");
    }
    assert_eq!(Model::from_files([dir.join("missing.p")]).unwrap_err().kind, ErrorKind::Config);

    let manifest = dir.join("Cargo.toml");
    std::fs::write(
        &manifest,
        "[package]\nname = \"x\"\n\n[package.metadata.snappea]\nspec_files = [\"specs\"]\nspecs = [\"Inventory\"]\n\
         coverage = \"target/cov.json\"\nmode = \"off\"\n",
    )
    .unwrap();
    let model = Model::load_manifest(&manifest).unwrap();
    // Named specs: strict. mode = "off": the fixture does not check.
    setup_error(model.recording().verify());
    model.check(|_| {});
    // The coverage path is relative to Cargo.toml.
    let recording = model.recording();
    recording.emit("Stock", "k", 1);
    recording.verify().unwrap();
    assert!(dir.join("target/cov.json").is_file());

    std::fs::write(&manifest, "[package.metadata.snappea]\nspec_files = [\"specs\"]\nspecz = []\n").unwrap();
    let e = Model::load_manifest(&manifest).unwrap_err();
    assert!(e.kind == ErrorKind::Config && e.message.contains("unknown key 'specz'"), "{e}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Runs this test binary again as a child process, running only `test` with `env`.
fn run_child(test: &str, env: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--test-threads=1", "--quiet"])
        .env_clear()
        .envs(env)
        .env("SNAPPEA_TEST_CHILD", test)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

/// Emits as a child process does: through `snappea::emit::emit`, to `SNAPPEA_EVENTS_DIR`.
#[test]
fn child_writer() {
    if std::env::var("SNAPPEA_TEST_CHILD").as_deref() != Ok("child_writer") {
        return;
    }
    snappea::emit::emit("Stock", "coffee", 1);
    snappea::emit::emit("Pair", "coffee", json!({"order": "child", "units": 2}));
}

#[test]
fn events_from_a_child_process() {
    let recording = model().specs(["Inventory"]).recording();
    recording.emit("Stock", "tea", 1);
    run_child("child_writer", recording.env());
    let files: Vec<_> = std::fs::read_dir(recording.events_dir()).unwrap().collect();
    assert_eq!(files.len(), 1);
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    assert_eq!(v.key, "coffee");
    assert_eq!(v.events[1].payload["order"], "child");
    assert!(v.events[1].src.as_deref().unwrap().contains("integration.rs"));
    assert!(v.events[1].pid != std::process::id());
    let artifacts = v.artifacts.clone().expect("a failed check keeps its folder");
    assert!(artifacts.join("recording.jsonl").is_file() && artifacts.join("events").is_dir());
    std::fs::remove_dir_all(artifacts).unwrap();
}

#[test]
fn spec_files_and_mode_from_the_environment() {
    if std::env::var("SNAPPEA_TEST_CHILD").as_deref() == Ok("spec_files_and_mode_from_the_environment") {
        let model = Model::load_manifest(std::env::var("MANIFEST").unwrap()).unwrap();
        // SNAPPEA_MODE=off overrides the file's mode = "auto".
        model.check(|r| r.emit("Stock", "k", -1));
        return;
    }
    let dir = temp("env");
    std::fs::write(dir.join("other.p"), SPEC).unwrap();
    let manifest = dir.join("Cargo.toml");
    std::fs::write(&manifest, "[package.metadata.snappea]\nspec_files = [\"missing.p\"]\nmode = \"auto\"\n").unwrap();
    let mut env: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().collect();
    env.push(("SNAPPEA_SPEC_FILES".into(), dir.join("other.p").into()));
    env.push(("SNAPPEA_MODE".into(), "off".into()));
    env.push(("MANIFEST".into(), manifest.into()));
    run_child("spec_files_and_mode_from_the_environment", env);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn added_folders_and_logs() {
    let dir = temp("inputs");
    std::fs::create_dir_all(dir.join("service")).unwrap();
    std::fs::write(
        dir.join("service/svc-1-abc.jsonl"),
        "{\"event\":\"Stock\",\"key\":\"k\",\"timestamp\":\"2026-01-01T00:00:00.000000001Z\",\"payload\":3}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("events.log"),
        "{\"event\":\"Pair\",\"key\":\"k\",\"timestamp\":\"2026-01-01T00:00:00.000000002Z\",\"payload\":[\"a\",1]}\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.log"), "INFO boot\nINFO reserved k 5\n").unwrap();
    let recording = model().specs(["Inventory"]).recording();
    recording.add_events_dir(dir.join("service")).add_log(dir.join("events.log"));
    recording.verify().unwrap();

    let recording = model().specs(["Inventory"]).recording();
    recording.add_events_dir(dir.join("service")).add_log_with(dir.join("app.log"), |line| {
        let units: i64 = line.strip_prefix("INFO reserved k ")?.parse().ok()?;
        Some(json!({"event": "Pair", "key": "k", "timestamp": "2026-01-01T00:00:00.000000005Z", "payload": ["o", units]}))
    });
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    assert!(v.events[1].src.as_deref().unwrap().ends_with("app.log:2"), "{v}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn global_emit_reaches_the_recording_bound_to_the_thread() {
    let recording = model().specs(["Inventory"]).recording();
    snappea::emit::emit("Stock", "k", 1);
    std::thread::scope(|s| {
        s.spawn(|| {
            let _bound = recording.bind();
            snappea::emit::emit("Pair", "k", json!(["o", 2]));
        });
    });
    let Err(CheckError::Violation(v)) = recording.verify() else { panic!("expected a violation") };
    assert_eq!(v.events.len(), 2);
    assert!(v.events[0].src.as_deref().unwrap().contains("integration.rs"));
}

#[test]
fn global_emit_on_an_unbound_thread_with_several_recordings_fails_them() {
    // In a child process: the failure would hit every recording open in this one.
    let name = "global_emit_on_an_unbound_thread_with_several_recordings_fails_them";
    if std::env::var("SNAPPEA_TEST_CHILD").as_deref() != Ok(name) {
        return run_child(name, std::env::vars_os());
    }
    let (a, b) = (model().recording(), model().recording());
    std::thread::spawn(|| snappea::emit::emit("Stock", "k", 1)).join().unwrap();
    for recording in [a, b] {
        let e = setup_error(recording.verify());
        assert_eq!(e.kind, ErrorKind::Config);
        assert!(e.message.contains("not bound to a recording") && e.message.contains(".bind()"), "{e}");
    }
}

#[test]
fn global_emit_on_an_unbound_thread_goes_to_the_only_recording() {
    if std::env::var("SNAPPEA_TEST_CHILD").as_deref()
        == Ok("global_emit_on_an_unbound_thread_goes_to_the_only_recording")
    {
        let recording = model().specs(["Inventory"]).recording();
        std::thread::spawn(|| snappea::emit::emit("Stock", "k", 1)).join().unwrap();
        assert_eq!(recording.verify().unwrap().verified_events(), 1);
        return;
    }
    run_child("global_emit_on_an_unbound_thread_goes_to_the_only_recording", std::env::vars_os());
}
