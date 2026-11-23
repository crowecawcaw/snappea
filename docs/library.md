---
diataxis: reference
personas: ["application-developer", "contributor"]
questions:
  - "How do I embed Snap Pea as a Rust library?"
  - "How do I check a project's files from Rust, as `snappea check` does?"
---

# Using Snap Pea from Rust

The checker is part of the [`snappea` crate](../crates/snappea/README.md) (library name `snappea`;
`snappea = "0.3"` with default features): `snappea::syntax`, `snappea::sema`, `snappea::checker`,
`snappea::host`, `snappea::project` and `snappea::driver`. To check application events from Rust tests, use `snappea::test` instead; see
the [Rust examples](../examples/rust/README.md).

```rust
let mut sm = snappea::syntax::SourceMap::new();
sm.add("model.p", source);
let analysis = snappea::sema::compile(&sm, &snappea::sema::Options::default())?;
let tc = analysis.program.test_cases().into_iter().find(|t| t.name == "tcMain").unwrap();
let checker = snappea::checker::engine::Checker::new(&analysis.program);
let opts = snappea::checker::engine::CheckerOptions { iterations: 1000, seed: 42, ..Default::default() };
let result = checker.check(&tc, &opts, &mut Vec::<String>::new())?;
println!("{} bug(s)", result.report.num_bugs);
```

The `driver` and `project` functions work on a `snappea::host::Host`: the console, files,
clock and threads. `snappea::host::NativeHost` is the real machine (what the `snappea`
binary uses); `snappea::host::MemHost` keeps files in memory, for tests. To check a
project's test case as `snappea check` does, with its sources resolved and type errors
reported:

```rust
use snappea::driver::check::{self, CheckConfig};
use snappea::host::NativeHost;

let mut host = NativeHost::new(); // paths are relative to the working directory
let mut cfg = CheckConfig::snappea(); // as `snappea check`; CheckConfig::upstream() as `p check`
cfg.input.pfiles = vec!["model.p".into()]; // or cfg.input.pproj = Some("Model.pproj".into())
check::resolve_sources(&mut host, &mut cfg)?; // errors are printed to stderr
let loaded = check::load(&mut host, &cfg)?;
cfg.test_case_name = "tcMain".into();
check::set_output_directory(&mut host, &mut cfg)?;
let outcome = check::testing_process(&mut host, &loaded, &cfg)?;
println!("{} bug(s) in {} schedules", outcome.bugs, outcome.schedules);
```

`Checker::check` runs on the calling thread. To use several threads, create a
`snappea::checker::engine::WorkerPool`, run `Checker::explore` for each worker on its own
thread (each with its own `Checker` of the shared program) and merge the results with
`Checker::finish`; `run_workers` in `crates/snappea/src/driver/check.rs` does this.

To follow a long check, set `CheckConfig::progress` and implement `Host::progress`: the
driver (`snappea::driver::check::testing_process`) sends a
`snappea::driver::progress::Progress` a few times a second, the same snapshot
`snappea check --progress json` prints. `Checker::check` on its own calls
`Output::schedule_finished` after every schedule instead.

Logs are checked with `snappea::checker::observe::Spec::new(&program, "MySpec")?.observe(...)`,
which takes the log entries, a `Parser` and `ObserveOptions` and returns a `Report`.

Foreign types and functions are supplied through
`snappea::checker::foreign::ForeignFunctions`. `crates/snappea/tests/checker_foreign.rs` is an
example; it ports the PriorityQueue tutorial's C# code. `crates/snappea/tests/checker_library.rs`
runs the examples above.

Exhaustive search hashes program states to skip repeated ones, and it cannot see inside
foreign values or functions. By default a program with foreign functions is only sampled,
and a finished exhaustive search over it says it proves nothing. Return `true` from
`ForeignFunctions::searchable` when the functions keep no state between calls and
`ForeignValue::hash_value` tells unequal values apart; exhaustive search then treats
the program like any other.
