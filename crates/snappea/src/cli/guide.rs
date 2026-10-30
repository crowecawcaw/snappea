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
    pub about: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

pub(super) const EXAMPLES: &[Example] = &[
    Example {
        name: "cas-retry",
        about: "Compare-and-swap with retries: two clients, generated inputs, an independent oracle.",
        files: &[
            ("Balance.p", include_str!("guide/examples/cas-retry/Balance.p")),
            ("Balance.pproj", include_str!("guide/examples/cas-retry/Balance.pproj")),
            ("README.md", include_str!("guide/examples/cas-retry/README.md")),
            ("check_examples.py", include_str!("guide/examples/cas-retry/check_examples.py")),
            ("invalid-retry.jsonl", include_str!("guide/examples/cas-retry/invalid-retry.jsonl")),
            ("valid-concurrent.jsonl", include_str!("guide/examples/cas-retry/valid-concurrent.jsonl")),
        ],
    },
    Example {
        name: "forced-interleaving",
        about: "Python tests that force the interleavings a model explores, and check real calls.",
        files: &[
            ("README.md", include_str!("guide/examples/forced-interleaving/README.md")),
            ("Wallet.p", include_str!("guide/examples/forced-interleaving/Wallet.p")),
            ("test_wallet.py", include_str!("guide/examples/forced-interleaving/test_wallet.py")),
            ("wallet.py", include_str!("guide/examples/forced-interleaving/wallet.py")),
        ],
    },
    Example {
        name: "reservation",
        about: "Atomic conditional update versus a stale read/write, with Python observation tests.",
        files: &[
            ("README.md", include_str!("guide/examples/reservation/README.md")),
            ("Reservation.p", include_str!("guide/examples/reservation/Reservation.p")),
            ("storage.py", include_str!("guide/examples/reservation/storage.py")),
            ("test_storage.py", include_str!("guide/examples/reservation/test_storage.py")),
        ],
    },
    Example {
        name: "bounded-history",
        about: "Bounded generated input histories, with hand-made valid and invalid traces.",
        files: &[
            ("README.md", include_str!("guide/examples/bounded-history/README.md")),
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
        about: "Retries across crashes: durable state, split commits, reapplied effects.",
        files: &[
            ("DurableRetry.p", include_str!("guide/examples/durable-retry/DurableRetry.p")),
            ("DurableRetry.pproj", include_str!("guide/examples/durable-retry/DurableRetry.pproj")),
            ("README.md", include_str!("guide/examples/durable-retry/README.md")),
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
        about: "Publish only what was validated for the current version.",
        files: &[
            ("README.md", include_str!("guide/examples/versioned-publication/README.md")),
            ("Revalidated.p", include_str!("guide/examples/versioned-publication/Revalidated.p")),
            ("StaleRetry.p", include_str!("guide/examples/versioned-publication/StaleRetry.p")),
            ("test_completion.py", include_str!("guide/examples/versioned-publication/test_completion.py")),
        ],
    },
];

const HELP: &str = "\
Usage: snappea guide [TOPIC]
       snappea guide examples
       snappea guide example NAME [--out DIR]

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
one; `--out DIR` writes its files to DIR/NAME so you can run them.

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
        _ => usage_error(host, "expected a topic, `examples`, or `example NAME [--out DIR]`", Some("guide")),
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
    let mut s = String::from("# Examples\n\nComplete, checked models. Print one with `snappea guide example NAME`;\n");
    s.push_str("write its files with `snappea guide example NAME --out DIR`.\n\n");
    for e in EXAMPLES {
        s.push_str(&format!("- `{}`: {}\n", e.name, e.about));
    }
    s
}

/// Every file of an example, README first, each under a heading with its name.
fn show(e: &Example) -> String {
    let mut s = format!(
        "# Example {}\n\n{}\nWrite these files with `snappea guide example {} --out DIR`.\n",
        e.name, e.about, e.name
    );
    let readme = e.files.iter().filter(|(f, _)| *f == "README.md");
    for (file, text) in readme.chain(e.files.iter().filter(|(f, _)| *f != "README.md")) {
        s.push_str(&format!("\n## {}\n\n{}", file, text));
        if !text.ends_with('\n') {
            s.push('\n');
        }
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
        "Wrote {} files to {}. Next: cd there and read README.md; it names the commands to run, \
         including the deliberately broken cases.",
        e.files.len(),
        target.display()
    ));
    OK
}
