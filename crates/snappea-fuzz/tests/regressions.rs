//! Replays every saved fuzz input in `fuzz/regressions/<harness>/` through its harness.
//!
//! When a fuzz run finds a crash, minimize it (`cargo +nightly fuzz tmin <target> <file>`),
//! copy the result here under a name that says what it found, and fix the crash. This test
//! keeps it fixed, on stable, in every `cargo test --workspace`. See docs/fuzzing.md.

use std::path::Path;

#[test]
fn saved_fuzz_inputs_do_not_crash() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/regressions");
    let mut ran = 0;
    for (name, harness) in snappea_fuzz::HARNESSES {
        let dir = root.join(name);
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
        files.sort();
        for file in files {
            if file.file_name().is_some_and(|n| n == ".gitkeep") {
                continue;
            }
            let data = std::fs::read(&file).unwrap();
            let result = std::panic::catch_unwind(|| harness(&data));
            assert!(result.is_ok(), "{name} harness panicked on {}", file.display());
            ran += 1;
        }
    }
    eprintln!("replayed {ran} saved fuzz inputs");
}

/// Each harness accepts the inputs the fuzz targets start from.
#[test]
fn harnesses_run_on_seed_inputs() {
    let model = snappea_fuzz::OBSERVE_SPEC.to_string()
        + r#"
machine Shop {
  start state Run {
    entry {
      var items: seq[Item];
      announce eOpen, (id = 1, owner = "a", items = items, tags = default(set[string]));
      if ($) { announce eClose, (id = 1, status = FAILED, ok = true); }
      else { announce eClose, (id = 1, status = OK, ok = true); }
    }
  }
}
test tcShop [main = Shop]: assert OrdersComplete in { Shop };
"#;
    snappea_fuzz::parse(model.as_bytes());
    snappea_fuzz::compile(model.as_bytes());
    snappea_fuzz::check(model.as_bytes());
    let log = include_str!("../../../fuzz/seeds/observe/valid.jsonl");
    for mode in 0..32u8 {
        let mut data = vec![mode];
        data.extend_from_slice(log.as_bytes());
        snappea_fuzz::observe(&data);
    }
    snappea_fuzz::observe(b"");
    snappea_fuzz::parse(&[0xff, 0xfe]);
}
