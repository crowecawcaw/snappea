# Using Snap Pea from Rust

The checker is part of the [`snappea` crate](../crates/snappea/README.md) (library name `snappea`;
`snappea = "0.1"` with default features): `snappea::syntax`, `snappea::sema`, `snappea::checker`,
`snappea::project` and `snappea::driver`. To check application events from Rust tests, use `snappea::test` instead; see
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

`check` runs on the calling thread. To use several threads, create a
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
runs the example above.

Exhaustive search hashes program states to skip repeated ones, and it cannot see inside
foreign values or functions. By default a program with foreign functions is only sampled,
and a finished exhaustive search over it says it proves nothing. Return `true` from
`ForeignFunctions::searchable` when the functions keep no state between calls and
`ForeignValue::hash_value` tells unequal values apart; exhaustive search then treats
the program like any other.
