//! `snappea guide`: the modeling workflow and runnable examples, built into the CLI so
//! a person or agent with only the binary can learn to use it.

use std::path::Path;

use crate::host::{Host, HostExt, Stream};

use super::command::{error, usage_error, ERROR, OK};

const TOPICS: &[(&str, &str, &str)] = &[
    (
        "workflow",
        "(default) Model the design, check it, then test the code against it",
        include_str!("guide/workflow.md"),
    ),
    (
        "testing",
        "Force the model's interleavings in implementation tests; check real calls",
        include_str!("guide/testing.md"),
    ),
    ("modeling", "What to model: boundaries, atomic steps, interference", include_str!("guide/modeling.md")),
    ("scenarios", "Choose scenarios and independent oracles that can fail", include_str!("guide/scenarios.md")),
    ("recovery", "Crashes, retries and durable state", include_str!("guide/recovery.md")),
    ("python", "Capture real Python calls and check them against P specs", include_str!("guide/python.md")),
    ("syntax", "P syntax and a complete small model", include_str!("guide/syntax.md")),
];

pub(super) struct Example {
    pub name: &'static str,
    /// The file to read first: its header explains the example and how to run it.
    pub main: &'static str,
    pub about: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

pub(super) const EXAMPLES: &[Example] = &[
    Example {
        name: "cas-retry",
        main: "Balance.p",
        about: "Compare-and-swap with retries: two clients, generated inputs, an independent oracle.",
        files: &[
            ("Balance.p", include_str!("guide/examples/cas-retry/Balance.p")),
            ("Balance.pproj", include_str!("guide/examples/cas-retry/Balance.pproj")),
            ("check_examples.py", include_str!("guide/examples/cas-retry/check_examples.py")),
            ("invalid-retry.jsonl", include_str!("guide/examples/cas-retry/invalid-retry.jsonl")),
            ("valid-concurrent.jsonl", include_str!("guide/examples/cas-retry/valid-concurrent.jsonl")),
        ],
    },
    Example {
        name: "forced-interleaving",
        main: "test_wallet.py",
        about: "Python tests that force the interleavings a model explores, and check real calls.",
        files: &[
            ("Wallet.p", include_str!("guide/examples/forced-interleaving/Wallet.p")),
            ("test_wallet.py", include_str!("guide/examples/forced-interleaving/test_wallet.py")),
            ("wallet.py", include_str!("guide/examples/forced-interleaving/wallet.py")),
        ],
    },
    Example {
        name: "reservation",
        main: "Reservation.p",
        about: "Atomic conditional update versus a stale read/write, with Python observation tests.",
        files: &[
            ("Reservation.p", include_str!("guide/examples/reservation/Reservation.p")),
            ("storage.py", include_str!("guide/examples/reservation/storage.py")),
            ("test_storage.py", include_str!("guide/examples/reservation/test_storage.py")),
        ],
    },
    Example {
        name: "bounded-history",
        main: "bounded-history.p",
        about: "Bounded generated input histories, with hand-made valid and invalid traces.",
        files: &[
            ("bounded-history.p", include_str!("guide/examples/bounded-history/bounded-history.p")),
            ("incomplete.jsonl", include_str!("guide/examples/bounded-history/incomplete.jsonl")),
            ("invalid-reply.jsonl", include_str!("guide/examples/bounded-history/invalid-reply.jsonl")),
            ("invalid-state.jsonl", include_str!("guide/examples/bounded-history/invalid-state.jsonl")),
            ("invalid.jsonl", include_str!("guide/examples/bounded-history/invalid.jsonl")),
            ("valid-concurrent.jsonl", include_str!("guide/examples/bounded-history/valid-concurrent.jsonl")),
            ("valid.jsonl", include_str!("guide/examples/bounded-history/valid.jsonl")),
            ("zero-work.jsonl", include_str!("guide/examples/bounded-history/zero-work.jsonl")),
        ],
    },
    Example {
        name: "durable-retry",
        main: "DurableRetry.p",
        about: "Retries across crashes: durable state, split commits, reapplied effects.",
        files: &[
            ("DurableRetry.p", include_str!("guide/examples/durable-retry/DurableRetry.p")),
            ("DurableRetry.pproj", include_str!("guide/examples/durable-retry/DurableRetry.pproj")),
            ("invalid-duplicate.jsonl", include_str!("guide/examples/durable-retry/invalid-duplicate.jsonl")),
            ("invalid-incomplete.jsonl", include_str!("guide/examples/durable-retry/invalid-incomplete.jsonl")),
            ("invalid-premature-done.jsonl", include_str!("guide/examples/durable-retry/invalid-premature-done.jsonl")),
            ("invalid-reverted-total.jsonl", include_str!("guide/examples/durable-retry/invalid-reverted-total.jsonl")),
            ("invalid-zero-work.jsonl", include_str!("guide/examples/durable-retry/invalid-zero-work.jsonl")),
            ("test_model_guards.py", include_str!("guide/examples/durable-retry/test_model_guards.py")),
            ("valid-concurrent.jsonl", include_str!("guide/examples/durable-retry/valid-concurrent.jsonl")),
        ],
    },
    Example {
        name: "versioned-publication",
        main: "Revalidated.p",
        about: "Publish only what was validated for the current version.",
        files: &[
            ("Revalidated.p", include_str!("guide/examples/versioned-publication/Revalidated.p")),
            ("StaleRetry.p", include_str!("guide/examples/versioned-publication/StaleRetry.p")),
            ("test_completion.py", include_str!("guide/examples/versioned-publication/test_completion.py")),
        ],
    },
];

const HELP: &str = "\
Usage: snappea guide [TOPIC]
       snappea guide examples
       snappea guide example NAME [FILE | --out DIR]

Prints a short guide, as Markdown. Start with `snappea guide`.

Topics:
  workflow   (default) Model the design, check it, then test the code against it
  testing    Force the model's interleavings in implementation tests; check real calls
  modeling   What to model: boundaries, atomic steps, interference
  scenarios  Choose scenarios and independent oracles that can fail
  recovery   Crashes, retries and durable state
  python     Capture real Python calls and check them against P specs
  syntax     P syntax and a complete small model

Examples are complete, checked models, some with Python tests. `example NAME` prints
its main file, whose header explains it and how to run it; `example NAME FILE` prints
another of its files; `--out DIR` writes them all to DIR/NAME so you can run them.

Examples:
  snappea guide testing
  snappea guide example cas-retry --out /tmp && cd /tmp/cas-retry && snappea check
";

pub(super) fn guide(args: &[String], host: &mut dyn Host) -> i32 {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => print(host, TOPICS[0].2),
        ["--help" | "-h"] => print(host, HELP),
        ["examples"] => print(host, &list()),
        ["example", name] => match find(name) {
            Some(e) => print(host, &show(e)),
            None => unknown_example(host, name),
        },
        ["example", name, file] if !file.starts_with('-') => match find(name) {
            Some(e) => match e.files.iter().find(|(f, _)| f == file) {
                Some((_, text)) => print(host, text),
                None => {
                    let names: Vec<&str> = e.files.iter().map(|(f, _)| *f).collect();
                    usage_error(
                        host,
                        &format!("example '{}' has no file '{}'; files: {}", name, file, names.join(", ")),
                        Some("guide"),
                    )
                }
            },
            None => unknown_example(host, name),
        },
        ["example", name, "--out", dir] | ["example", "--out", dir, name] => match find(name) {
            Some(e) => write(host, e, dir),
            None => unknown_example(host, name),
        },
        [topic] => match TOPICS.iter().find(|(n, _, _)| n == topic) {
            Some((_, _, text)) => print(host, text),
            None => {
                let names: Vec<&str> = TOPICS.iter().map(|(n, _, _)| *n).collect();
                usage_error(
                    host,
                    &format!("unknown guide topic '{}'; topics: {}, examples", topic, names.join(", ")),
                    Some("guide"),
                )
            }
        },
        _ => usage_error(host, "expected a topic, `examples`, or `example NAME [FILE | --out DIR]`", Some("guide")),
    }
}

fn print(host: &mut dyn Host, text: &str) -> i32 {
    host.write(Stream::Out, text, None);
    OK
}

fn find(name: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.name == name)
}

fn unknown_example(host: &mut dyn Host, name: &str) -> i32 {
    let names: Vec<&str> = EXAMPLES.iter().map(|e| e.name).collect();
    usage_error(host, &format!("unknown example '{}'; examples: {}", name, names.join(", ")), Some("guide"))
}

fn list() -> String {
    let mut s =
        String::from("# Examples\n\nComplete, checked models. `snappea guide example NAME` prints the main file;\n");
    s.push_str("`snappea guide example NAME --out DIR` writes all its files so you can run them.\n\n");
    for e in EXAMPLES {
        s.push_str(&format!("- `{}`: {}\n", e.name, e.about));
    }
    s
}

/// The example's main file, then the other files and how to get them.
fn show(e: &Example) -> String {
    let main = e.files.iter().find(|(f, _)| *f == e.main).map_or("", |(_, t)| *t);
    let c = if e.main.ends_with(".py") { "#" } else { "//" };
    let mut s = format!("{c} snappea guide example {}: {}\n{c} {}\n\n{}", e.name, e.main, e.about, main);
    if !s.ends_with('\n') {
        s.push('\n');
    }
    let others: Vec<&str> = e.files.iter().map(|(f, _)| *f).filter(|f| *f != e.main).collect();
    if !others.is_empty() {
        s.push_str(&format!(
            "\n{c} Other files: {}.\n{c} Print one with `snappea guide example {} FILE`; write all with `snappea guide example {} --out DIR`.\n",
            others.join(", "),
            e.name,
            e.name
        ));
    }
    s
}

fn write(host: &mut dyn Host, e: &Example, dir: &str) -> i32 {
    let target = Path::new(dir).join(e.name);
    let base = host.cwd();
    for (file, text) in e.files {
        let path = crate::host::full_path(&base, &target.join(file));
        let made = path.parent().map_or(Ok(()), |p| host.create_dir_all(p));
        if let Err(err) = made.and_then(|()| host.write_file(&path, text)) {
            error(host, &format!("cannot write {}: {}", path.display(), err));
            return ERROR;
        }
    }
    host.out_line(&format!(
        "Wrote {} files to {}. Next: cd there; the header of {} lists the commands to run, \
         including the deliberately broken cases.",
        e.files.len(),
        target.display(),
        e.main
    ));
    OK
}
