//! Every `--flag` a doc passes to `snappea <cmd>` or `snappea-compat <cmd>` is one that
//! command's `--help` lists, so renamed or removed flags cannot linger in the docs.
mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use snappea::host::MemHost;

/// Flags the scan reads as passed to a command that does not take them, with why.
/// Keep it short: fix the doc rather than adding an entry.
const ALLOWED: &[(&str, &str, &str)] = &[
    // (file, command, flag)
];

/// What `snappea args --help` (or `snappea-compat ...`) prints.
fn help(compat: bool, args: &[&str]) -> String {
    let mut h = MemHost::new("/w");
    let mut all = args.to_vec();
    all.push("--help");
    if compat {
        common::compat(&mut h, &all);
    } else {
        common::snappea(&mut h, &all);
    }
    format!("{}{}", h.stdout, h.stderr)
}

/// The `--name` tokens in `text`.
fn flags(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, _) in text.match_indices("--") {
        let rest = &text[i + 2..];
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(rest.len());
        let name = &rest[..end];
        if name.starts_with(|c: char| c.is_ascii_alphabetic()) && !text[..i].ends_with(|c: char| c.is_alphanumeric()) {
            out.insert(format!("--{name}"));
        }
    }
    out
}

/// Accepted flags per command (`check`, `check lint`, ...) for one binary.
type Commands = BTreeMap<String, BTreeSet<String>>;

/// `snappea`: commands from the top-level help, subcommands from each command's
/// `Usage:` block. Output options in the top-level help apply to every command.
fn snappea_commands() -> Commands {
    let top = help(false, &[]);
    let global = flags(&top);
    let names: Vec<String> = top
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.strip_prefix("  ").filter(|r| !r.starts_with(' ')))
        .filter_map(|r| r.split_whitespace().next().map(str::to_string))
        .collect();
    assert!(names.iter().any(|n| n == "check"), "no commands in `snappea --help`:\n{top}");
    let mut out = Commands::new();
    for name in names {
        let text = help(false, &[&name]);
        let mut accepted = flags(&text);
        accepted.extend(global.iter().cloned());
        accepted.insert("--help".into());
        let usage = text.lines().take_while(|l| !l.trim().is_empty());
        let prefix = format!("snappea {name} ");
        for line in usage {
            let Some(rest) = line.find(&prefix).map(|i| &line[i + prefix.len()..]) else { continue };
            let sub = rest.split_whitespace().next().unwrap_or("");
            if !sub.is_empty() && sub.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                let mut sub_flags = flags(&help(false, &[&name, sub]));
                sub_flags.extend(global.iter().cloned());
                sub_flags.insert("--help".into());
                out.insert(format!("{name} {sub}"), sub_flags);
            }
        }
        out.insert(name, accepted);
    }
    out
}

/// `snappea-compat`: the commands its help lists between `command :` and `options:`.
fn compat_commands() -> Commands {
    let top = help(true, &[]);
    let names: Vec<String> = top
        .lines()
        .skip_while(|l| !l.contains("command :"))
        .take_while(|l| !l.contains("options:"))
        .filter_map(|l| l.split(" : ").last().unwrap_or(l).split_whitespace().next().map(str::to_string))
        .collect();
    assert!(names.iter().any(|n| n == "observe"), "no commands in `snappea-compat --help`:\n{top}");
    names
        .into_iter()
        .map(|name| {
            let mut accepted = flags(&help(true, &[&name]));
            accepted.insert("--help".into());
            (name, accepted)
        })
        .collect()
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn walk(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !matches!(name.to_str(), Some("node_modules" | "target" | "dist" | ".git")) {
                walk(&path, keep, out);
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
}

fn doc_files() -> Vec<PathBuf> {
    let r = root();
    let ext = |e: &'static str| move |p: &Path| p.extension().is_some_and(|x| x == e);
    let readme = |p: &Path| p.file_name().is_some_and(|n| n == "README.md");
    let mut out = Vec::new();
    for f in [
        "README.md",
        "python/README.md",
        "go/README.md",
        "java/README.md",
        "crates/snappea/README.md",
        "crates/snappea/src/emit/README.md",
        "crates/snappea/src/test/README.md",
        "examples/model/github-workflow.yml",
    ] {
        assert!(r.join(f).exists(), "{f} moved: update the list in this test");
        out.push(r.join(f));
    }
    walk(&r.join("docs"), &ext("md"), &mut out);
    walk(&r.join("examples"), &readme, &mut out);
    walk(&r.join("crates/snappea/src/cli/guide"), &ext("md"), &mut out);
    walk(&r.join("web/src/site/docs"), &ext("tsx"), &mut out);
    walk(&r.join(".github/workflows"), &ext("yml"), &mut out);
    out.sort();
    out.dedup();
    out
}

/// Code in `text` as (byte offset, code): fenced blocks and `inline` spans in Markdown,
/// `<code>` and `<Code>` elements in TSX, and the whole of a workflow.
fn code(path: &Path, text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => {
            let mut fence: Option<(&str, usize, String)> = None;
            let mut offset = 0;
            for line in text.split_inclusive('\n') {
                let t = line.trim_start();
                let marker = ["```", "~~~"].into_iter().find(|m| t.starts_with(m));
                match (&mut fence, marker) {
                    (Some((m, start, body)), Some(n)) if *m == n => {
                        out.push((*start, std::mem::take(body)));
                        fence = None;
                    }
                    (Some((_, _, body)), _) => body.push_str(line),
                    (None, Some(m)) => fence = Some((m, offset + line.len(), String::new())),
                    (None, None) => {
                        let mut at = offset;
                        for (i, part) in line.split('`').enumerate() {
                            if i % 2 == 1 {
                                out.push((at, part.to_string()));
                            }
                            at += part.len() + 1;
                        }
                    }
                }
                offset += line.len();
            }
        }
        Some("tsx") => {
            for (open, close) in [("<code>", "</code>"), ("<Code ", "</Code>")] {
                let mut from = 0;
                while let Some(i) = text[from..].find(open) {
                    let start = from + i + open.len();
                    let Some(len) = text[start..].find(close) else { break };
                    let body = text[start..start + len].replace("{\" \"}", " ").replace("{\"", "").replace("\"}", "");
                    out.push((start, body));
                    from = start + len;
                }
            }
        }
        _ => out.push((0, text.to_string())),
    }
    out
}

/// Each `snappea`/`snappea-compat` invocation on a line: (compat, command words, flags passed).
fn invocations(line: &str) -> Vec<(bool, Vec<String>, Vec<String>)> {
    let tokens: Vec<&str> =
        line.split_whitespace().map(|t| t.trim_matches(|c| matches!(c, '"' | '\'' | ','))).collect();
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let bin = t.rsplit('/').next().unwrap_or(t);
        let compat = match bin {
            "snappea" => false,
            "snappea-compat" => true,
            _ => continue,
        };
        // Arguments up to the end of this shell command.
        let mut args = Vec::new();
        for a in &tokens[i + 1..] {
            if matches!(*a, "|" | "||" | "&&" | ";" | ">" | ">>" | "2>&1" | "2>") || a.starts_with('#') {
                break;
            }
            let end = a.ends_with([';', ')']);
            args.push(a.trim_end_matches([';', ')']).to_string());
            if end {
                break;
            }
        }
        let flags = args
            .iter()
            .filter(|a| a.starts_with("--") && a.len() > 2)
            .map(|a| a.split('=').next().unwrap().to_string())
            .collect();
        // The command, then the argument right after it: a subcommand only there.
        let words = match args.iter().position(|a| !a.starts_with('-')) {
            Some(c) => args[c..].iter().take(2).cloned().collect(),
            None => Vec::new(),
        };
        out.push((compat, words, flags));
    }
    out
}

#[test]
fn docs_pass_only_flags_each_command_accepts() {
    let snappea = snappea_commands();
    let compat = compat_commands();
    let root = root();
    let mut stale = Vec::new();
    let mut seen = 0;
    for path in doc_files() {
        let text = fs::read_to_string(&path).unwrap();
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        for (start, block) in code(&path, &text) {
            let joined = block.replace("\\\n", " \n").replace("\\\\\n", " \n");
            let mut at = start;
            for line in joined.split('\n') {
                let line_no = text[..at.min(text.len())].matches('\n').count() + 1;
                at += line.len() + 1;
                for (is_compat, words, passed) in invocations(line) {
                    let commands = if is_compat { &compat } else { &snappea };
                    let bin = if is_compat { "snappea-compat" } else { "snappea" };
                    let Some(first) = words.first() else { continue };
                    if !commands.contains_key(first) {
                        continue;
                    }
                    let cmd = match words.get(1) {
                        Some(sub) if commands.contains_key(&format!("{first} {sub}")) => format!("{first} {sub}"),
                        _ => first.clone(),
                    };
                    seen += 1;
                    for flag in passed {
                        let allowed = ALLOWED.iter().any(|(f, c, g)| *f == rel && *c == cmd && *g == flag);
                        if !commands[&cmd].contains(&flag) && !allowed {
                            stale.push(format!(
                                "{rel}:{line_no}: `{bin} {cmd}` does not accept {flag}\n    {}",
                                line.trim()
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(seen > 100, "found only {seen} invocations: is the scan broken?");
    assert!(
        stale.is_empty(),
        "docs pass flags their command does not accept:\n{}\nhelp: fix the doc to use a flag from `snappea <command> --help`; \
         if the scan misreads the line, add it to ALLOWED in this test",
        stale.join("\n")
    );
}

#[test]
fn scan_reads_commands_and_flags() {
    let got = invocations("run: snappea check lint --strict x.p && snappea-compat observe --spec S | jq . # --no");
    assert_eq!(
        got,
        [
            (false, vec!["check".into(), "lint".into()], vec!["--strict".into()]),
            (true, vec!["observe".into(), "--spec".into()], vec!["--spec".into()]),
        ]
    );
    let commands = snappea_commands();
    assert!(commands["check lint"].contains("--list-rules"));
    assert!(!commands["tests"].contains("--timeout"));
    assert!(compat_commands()["check"].contains("--testcase"));
}
