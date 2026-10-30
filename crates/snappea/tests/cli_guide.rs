//! `snappea guide`: every topic prints, and the syntax guide's model passes `snappea check`.

use std::process::Command;

fn snappea(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_snappea")).current_dir(dir).args(args).output().unwrap()
}

#[test]
fn every_topic_prints_markdown() {
    let dir = std::env::temp_dir();
    for topic in ["", "workflow", "testing", "syntax"] {
        let args: Vec<&str> = std::iter::once("guide").chain((!topic.is_empty()).then_some(topic)).collect();
        let out = snappea(&args, &dir);
        assert!(out.status.success(), "snappea guide {topic} failed");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("# "), "guide {topic} is not Markdown");
    }
    let unknown = snappea(&["guide", "nope"], &dir);
    assert_eq!(unknown.status.code(), Some(2));
}

#[test]
fn syntax_guide_model_passes_check() {
    let guide = include_str!("../src/cli/guide/syntax.md");
    let start = guide.find("```p\n").expect("syntax guide has a P example") + 5;
    let end = start + guide[start..].find("```").unwrap();
    let dir = std::env::temp_dir().join(format!("snappea-guide-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Example.p"), &guide[start..end]).unwrap();
    let out = snappea(&["check", "Example.p", "--timeout", "5s", "--json"], &dir);
    let text = String::from_utf8_lossy(&out.stdout);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "the syntax guide's model must pass `snappea check`:\n{text}");
}
