//! `snappea guide`: topics print, examples write out completely, and every example's
//! models behave as documented (correct cases pass, broken cases fail for the stated
//! reason, oracle probes accept or reject). The Python parts of the examples run in
//! scripts/check-guide-examples.py.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn snappea(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_snappea")).current_dir(dir).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("snappea-guide-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// (example, spec file, spec, [(trace, expected error)])
type Probe<'a> = (&'a str, &'a str, &'a str, &'a [(&'a str, Option<&'a str>)]);

const EXAMPLES: &[&str] =
    &["cas-retry", "forced-interleaving", "reservation", "bounded-history", "durable-retry", "versioned-publication"];

#[test]
fn every_topic_prints_markdown() {
    let dir = std::env::temp_dir();
    for topic in ["", "workflow", "testing", "modeling", "scenarios", "recovery", "python", "syntax", "examples"] {
        let args: Vec<&str> = std::iter::once("guide").chain((!topic.is_empty()).then_some(topic)).collect();
        let out = snappea(&args, &dir);
        assert!(out.status.success(), "snappea guide {topic} failed");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("# "), "guide {topic} is not Markdown");
    }
    for bad in [&["guide", "nope"][..], &["guide", "example", "nope"]] {
        assert_eq!(snappea(bad, &dir).status.code(), Some(2), "{bad:?}");
    }
}

#[test]
fn guide_text_points_only_to_existing_topics_and_examples() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/guide");
    let topics = ["workflow", "testing", "modeling", "scenarios", "recovery", "python", "syntax", "examples"];
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().path()).collect();
    for e in EXAMPLES {
        pages.extend(std::fs::read_dir(src.join("examples").join(e)).unwrap().map(|e| e.unwrap().path()));
    }
    for page in pages.iter().filter(|p| p.is_file()) {
        let body = std::fs::read_to_string(page).unwrap();
        for (i, _) in body.match_indices("snappea guide ") {
            let rest: String =
                body[i + 14..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ' ').collect();
            let mut words = rest.split_whitespace();
            match words.next() {
                Some("example") => {
                    let name = words.next().unwrap_or("");
                    assert!(EXAMPLES.contains(&name) || name == "NAME", "{}: unknown example '{name}'", page.display());
                }
                Some(t) if t.starts_with('-') => {}
                Some(t) => assert!(topics.contains(&t), "{}: unknown topic '{t}'", page.display()),
                None => {}
            }
        }
        assert!(!body.contains("](../"), "{}: relative link outside the example", page.display());
    }
}

#[test]
fn examples_write_out_every_source_file() {
    let out = temp("files");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/guide/examples");
    let names: Vec<String> =
        std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(names.len(), EXAMPLES.len(), "an example folder is not listed here: {names:?}");
    for e in EXAMPLES {
        let o = snappea(&["guide", "example", e, "--out", out.to_str().unwrap()], &out);
        assert!(o.status.success(), "{}", text(&o));
        for f in std::fs::read_dir(src.join(e)).unwrap() {
            let f = f.unwrap().path();
            if !f.is_file() || f.extension().is_some_and(|x| x == "pyc") {
                continue;
            }
            let written = out.join(e).join(f.file_name().unwrap());
            assert_eq!(
                std::fs::read_to_string(&f).unwrap(),
                std::fs::read_to_string(&written).unwrap_or_default(),
                "{} is not embedded in the binary (add it to cli/guide.rs)",
                f.display()
            );
        }
    }
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn syntax_guide_model_passes_and_catches_a_missing_effect() {
    let guide = include_str!("../src/cli/guide/syntax.md");
    let start = guide.find("```p\n").expect("syntax guide has a P example") + 5;
    let model = &guide[start..start + guide[start..].find("```").unwrap()];
    let dir = temp("syntax");
    std::fs::write(dir.join("Example.p"), model).unwrap();
    let ok = snappea(&["check", "Example.p", "--no-mutation", "--timeout", "5s"], &dir);
    assert!(ok.status.success(), "the syntax guide's model must pass:\n{}", text(&ok));
    let broken = model.replace("total = total + p.units;", "total = total;");
    assert_ne!(broken, model, "negative control did not change the model");
    std::fs::write(dir.join("Example.p"), broken).unwrap();
    let bad = snappea(&["check", "Example.p", "--no-mutation", "--timeout", "5s"], &dir);
    assert_eq!(bad.status.code(), Some(1), "{}", text(&bad));
    assert!(text(&bad).contains("reply disagrees with requested additions"), "{}", text(&bad));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn example_models_pass_and_their_broken_cases_fail_for_the_stated_reason() {
    let out = temp("models");
    for e in EXAMPLES {
        assert!(snappea(&["guide", "example", e, "--out", out.to_str().unwrap()], &out).status.success());
    }
    let checks: &[(&str, &[&str], Option<&str>)] = &[
        (
            "bounded-history",
            &[
                "check",
                "bounded-history.p",
                "tcFixedInputHistory",
                "tcSchedulerChoosesInterleaving",
                "tcGeneratedHistory",
                "tcConcurrentGeneratedHistory",
                "--no-mutation",
                "--timeout",
                "5s",
            ],
            None,
        ),
        (
            "bounded-history",
            &["check", "bounded-history.p", "tcBrokenInputHistory", "--no-mutation", "--timeout", "5s"],
            Some("accepted=True with"),
        ),
        (
            "bounded-history",
            &["check", "bounded-history.p", "tcBrokenGeneratedHistory", "--no-mutation", "--timeout", "5s"],
            Some("accepted=True with"),
        ),
        (
            "bounded-history",
            &["check", "bounded-history.p", "tcRejectedWriteChangesState", "--no-mutation", "--timeout", "5s"],
            Some("public stock 1, expected 0"),
        ),
        ("cas-retry", &["check", "Balance.p", "--no-mutation", "--timeout", "5s"], None),
        (
            "reservation",
            &["check", "Reservation.p", "tcAtomicConditionalReserve", "tcTwoSequentialSuccesses", "--timeout", "5s"],
            None,
        ),
        (
            "reservation",
            &["check", "Reservation.p", "tcReadCheckWrite", "--timeout", "5s"],
            Some("reservations exceed available capacity"),
        ),
        ("durable-retry", &["check", "tcDurableRetry", "tcCompletedReplay", "tcConcurrentRetries"], None),
        ("durable-retry", &["check", "tcSplitCommitRetry"], Some("one request caused more than one effect")),
        ("durable-retry", &["check", "tcReappliedReplay"], Some("one request caused more than one effect")),
        ("versioned-publication", &["check", "Revalidated.p"], None),
        ("versioned-publication", &["check", "StaleRetry.p"], Some("was validated for version")),
    ];
    for (example, args, expected) in checks {
        let o = snappea(args, &out.join(example));
        let expected_code = if expected.is_some() { 1 } else { 0 };
        assert_eq!(o.status.code(), Some(expected_code), "{example} {args:?}\n{}", text(&o));
        if let Some(msg) = expected {
            assert!(text(&o).contains(msg), "{example} {args:?}: expected '{msg}'\n{}", text(&o));
        }
    }
    let probes: &[Probe] = &[
        (
            "bounded-history",
            "bounded-history.p",
            "CapacityOracle",
            &[
                ("valid.jsonl", None),
                ("valid-concurrent.jsonl", None),
                ("invalid.jsonl", Some("accepted=true with")),
                ("invalid-state.jsonl", Some("public stock")),
                ("invalid-reply.jsonl", Some("reply disagrees")),
                ("incomplete.jsonl", Some("hot state")),
                ("zero-work.jsonl", Some("scenario must perform real work")),
            ],
        ),
        (
            "durable-retry",
            "DurableRetry.p",
            "RetryContract",
            &[
                ("valid-concurrent.jsonl", None),
                ("invalid-duplicate.jsonl", Some("one request caused more than one effect")),
                ("invalid-incomplete.jsonl", Some("hot state")),
                ("invalid-premature-done.jsonl", Some("some requested effects never committed")),
                ("invalid-reverted-total.jsonl", Some("expected current effect total")),
                ("invalid-zero-work.jsonl", Some("scenario completed without any requests")),
            ],
        ),
    ];
    for (example, spec_file, spec, traces) in probes {
        for (trace, expected) in *traces {
            let o =
                snappea(&["observe", spec_file, trace, "--spec", spec, "--strict", "--check-hot"], &out.join(example));
            let expected_code = if expected.is_some() { 1 } else { 0 };
            assert_eq!(o.status.code(), Some(expected_code), "{example}/{trace}\n{}", text(&o));
            if let Some(msg) = expected {
                assert!(
                    text(&o).to_lowercase().contains(&msg.to_lowercase()),
                    "{example}/{trace}: expected '{msg}'\n{}",
                    text(&o)
                );
            }
        }
    }
    std::fs::remove_dir_all(&out).ok();
}
