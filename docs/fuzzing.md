---
diataxis: how-to
personas: [contributor, coding-agent]
questions:
  - How do I fuzz snappea locally?
  - What does each fuzz target cover, and what counts as a failure?
  - How do I reproduce and keep fixed a crash CI found?
  - What do the property tests check?
  - How does fuzzing run in CI, and where is its corpus kept?
---

# Fuzzing and property tests

Fuzzing feeds snappea generated inputs to find crashes, hangs and runaway memory. Property
tests check that generated inputs give the right result. Both live in
`crates/snappea-fuzz`. The stable parts (property tests and saved crash inputs) run in
`cargo test --workspace`. The cargo-fuzz targets in `fuzz/` need nightly.

## Targets

Each target calls one harness in `crates/snappea-fuzz/src/lib.rs`. None may panic, hang
(10 s per input) or use more than 2 GB, whatever the input.

| Target | Input | Runs | Also checks |
|---|---|---|---|
| `parse` | P source | lexer, parser, error rendering | error spans lie inside the input, on character boundaries |
| `compile` | P source | type checker, lint, bytecode compiler, mutant generation | every diagnostic's span and snippet |
| `check` | P source | `compile`, then 4 schedules of at most 200 steps per test case with the random, PCT and DFS strategies | the same seed gives the same bugs and schedule; a bug's `.schedule` replays to the same bug |
| `observe` | a mode byte, then a log | `snappea observe` on a fixed spec (`crates/snappea-fuzz/src/observe_spec.p`) | the exit code is 0, 1 or 2 |

The `observe` mode byte turns on options: bit 0 `--p-trace`, bit 1 `--check-hot`, bit 2
`--strict`, bit 3 `--unknown-events warn`, bit 4 `--sorted`.

The fuzz profile keeps overflow checks and debug assertions on, so integer overflow
is a crash.

## Run locally

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
scripts/fuzz-corpus.sh          # seed fuzz/corpus/ from the repository's P files and logs
cargo +nightly fuzz run check fuzz/corpus/check -- \
  -dict=fuzz/p.dict -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=16384
```

`fuzz/p.dict` holds P keywords and symbols, and JSON log field names. Without it, most
inputs fail in the lexer. The corpus and crash files (`fuzz/corpus/`, `fuzz/artifacts/`)
are not committed.

## Fix a crash

1. Reproduce it: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-...`.
   For a crash CI found, download the `fuzz-<target>-crash` artifact from the run.
2. Shrink it: `cargo +nightly fuzz tmin <target> <file>`, then shrink it further by
   hand if you can. A P file of a few lines makes the best regression test.
3. Save it as `fuzz/regressions/<target>/<what-it-found>` (for example
   `compile/duplicate-module.p`).
4. Fix the crash. `cargo test -p snappea-fuzz` replays every saved input on stable.

## Property tests

`crates/snappea-fuzz/tests/properties.rs` uses proptest:

- Timestamps that `snappea::emit` writes read back as the same instant, and so does
  every `Instant.toString()` for the years 0000 to 9999.
- On generated sequences of events, `snappea::test` and `snappea observe` agree with a
  reference model of the spec in `observe_spec.p`. The payloads include any strings,
  64-bit integers, floats and nested `any` values.

When a property fails, proptest prints the smallest failing input and saves it to
`crates/snappea-fuzz/tests/properties.proptest-regressions`. Commit that file with the fix.
Set `PROPTEST_CASES=10000` for a longer local run.

## In CI

`.github/workflows/fuzz.yml` runs every target:

- **Pull requests** (those changing `crates/`, `fuzz/` or the Cargo files): 30 seconds
  per target, starting from the nightly corpus.
- **Nightly** (07:23 UTC): 5 minutes per target, skipped when main has not changed since
  the last nightly run that passed. The run minimizes each corpus (`cargo fuzz cmin`) and
  saves it to the Actions cache as `fuzz-corpus-<target>-<run id>`. The next nightly run
  and every pull request start from it. The run saves the corpus even when it finds a
  crash.
- **By hand**: Actions → Fuzz → Run workflow. It always runs, for the number of seconds
  per target you choose.

A crash fails the job and uploads its input. If the cache was evicted (GitHub drops
entries unused for 7 days), the next run starts again from the seed inputs.
