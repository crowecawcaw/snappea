---
diataxis: how-to
personas: ["contributor"]
questions:
  - "How do I build, validate and benchmark Snap Pea?"
---

# Development

The commands to build, test and lint are in [AGENTS.md](../AGENTS.md).

## Crates

The published workspace crates share one release version:

- `snappea-syntax`: lexer, parser, AST, and source locations, with no I/O.
- `snappea-lint`: extensible [model-quality rules](linting.md), depending only on syntax.
- `snappea`: the checker, language helpers, and CLI. Its `snappea::syntax` module
  re-exports `snappea-syntax` to preserve the existing API.

The `snappea` library is split into modules:

| Module | Purpose |
|---|---|
| `snappea::syntax` | Lexer, parser and AST. |
| `snappea::sema` | Name resolution and type checking; produces a typed program. |
| `snappea::checker` | Interpreter, schedulers, PEx search, monitors, traces, foreign functions and log checking. |
| `snappea::host` | The environment commands run in: console, file system, clock, threads. `MemHost` is in-memory (tests, wasm); `snappea::cli` has the native one. |
| `snappea::project` | Finding and reading P sources: `.pproj` files, `--pfiles`, the current folder; type-error reporting. |
| `snappea::driver` | The commands on a host, shared by `snappea` and `snappea-compat`: `check` (checking test cases, replay, output folders, traces, coverage), `observe` (checking logs: `compile`, `prepare`, `Observer::run`, `json_report`) and `scenario` (scenario coverage reports). `check::Style` picks the output: `Upstream` (upstream's folder layout, history and file names) or `Snappea` (`<out>/<test>/bug.*`, replaced by each run). |
| `snappea::compat` | Upstream's command line for `snappea-compat` (`p check` options and help, `p merge-scenario-coverage`, PObserve's options and report folder), on `snappea::driver`. Nothing else depends on it. |
| `snappea::cli` | The `snappea` command line and the native host (hidden from the docs; used by the binaries and `snappea-wasm`). |
| `snappea::test` | In-process checks for Rust tests. |
| `snappea::emit` | The event emitter for application code. |

The binaries are `src/bin/snappea.rs` and `src/bin/snappea-compat.rs`. Features select what is
compiled: `emit` (only `snappea::emit`, for production code), `test` (`snappea::test`, `emit` and
the checker) and `cli` (the binaries and the checker); the default is `cli` and `test`.
Tests are in `crates/snappea/tests`, named after the module they cover.

`crates/snappea-wasm` (not published) is the WebAssembly module and JavaScript wrapper.

The checker modules use no file system, process or threading APIs, so they build for
`wasm32-unknown-unknown` unchanged. `snappea::driver` reaches all of these through a `Host`;
the threads for `snappea check --workers` come from `Host::threads`, which only the native
host provides, so the WebAssembly module (which runs the `snappea` command line) uses one
worker.

## WebAssembly

```sh
rustup target add wasm32-unknown-unknown
cargo build --profile wasm --target wasm32-unknown-unknown -p snappea-wasm
node crates/snappea-wasm/js/smoke.mjs target/wasm32-unknown-unknown/wasm/snappea_wasm.wasm
```

`crates/snappea-wasm/js/snappea.mjs` exposes `run(args, files)` over an in-memory file system.
The module is about 1 MB (320 KB gzipped). The playground in `web/` is built on it; see
[web/README.md](../web/README.md).

## Validation

Upstream's test suites are vendored in `tests/upstream`.

* Type checker: all 424 upstream regression tests compile or fail to compile as
  upstream expects. Checked in `cargo test`, or
  `cargo run --release -p snappea --example typecheck_corpus -- tests/upstream/RegressionTests`.
* Checker: all 249 tests of the suites upstream's harness runs find a bug exactly when
  upstream expects one, with upstream's settings (1000 schedules, 1000 steps) across
  seeds. `cargo run --release -p snappea --example corpus -- <suite dir>`.
  A faster version runs in `cargo test`.
* Tutorials: mirrors upstream's tutorial CI and reports the same three failing tests.
  `cargo run --release -p snappea --example tutorials -- tests/upstream/Tutorial`
* Traces: readable and JSON traces match upstream's golden files
  (`tests/upstream/CorrectLogs`), checked in `crates/snappea/tests/compat_cli.rs`.
* `snappea-compat check`: console output, exit codes, traces, schedules, coverage and the
  output folder match upstream's on every regression test, tutorial test case and a set of
  command lines, recorded once from upstream (`tests/upstream/goldens`) and checked in
  `crates/snappea/tests/compat_check_upstream.rs`. See
  [How compatibility is checked](compatibility.md#how-compatibility-is-checked).
* `snappea-compat observe`: `scripts/compare-pobserve.py` runs upstream PObserve and
  `snappea-compat observe` on all 96 logs of upstream's regression suite, with and without
  `--assumeInputsAreSorted`. All 192 runs give the same exit code; 186 give identical
  output, and the other 6 differ only in the null-dereference message. The small logs
  and upstream's outputs are vendored in `tests/upstream/PObserve`: CI checks 96 runs,
  94 with full normalized console/file comparisons and two with exit codes only, in
  `crates/snappea/tests/compat_observe_upstream.rs`. The 192-run figures describe the
  separate manual comparison, not CI. See the [upstream parity page](https://snappea.dev/docs/parity.html).

## Benchmarks

`scripts/bench.py` runs `snappea-compat check` on six tutorial tests with fixed schedule
counts and seed, on one worker, so that the schedules match upstream's and history;
`run --cmd target/release/snappea --workers N` measures N workers. It records check time,
schedules per second, startup time (a one-schedule check) and warm schedules per second
(after startup) and, with `--instructions` (via Valgrind), instructions executed.

```sh
cargo build --release -p snappea
python3 scripts/bench.py run --out bench.json
python3 scripts/bench.py report --current bench.json --history old.jsonl
```

* Every commit on main: `.github/workflows/bench.yml` appends the result to
  `main.jsonl` on the `benchmarks` branch and reports the change in the run summary.
  [Charts](https://snappea.dev/bench/). Wall-clock times on shared
  runners vary by about ±10%; instruction counts are stable to about 0.2% and are the
  better signal. GitHub also hands out runners with different CPUs, up to 40% apart in
  speed, so charts have one line per CPU model, and the run summary compares times
  only with the previous run on the same CPU (instructions with the previous run on
  any CPU).
* Against upstream P: run the "Benchmark vs upstream P" workflow by hand. It installs P
  from NuGet, runs both tools on the same runner and appends the comparison to
  `upstream.jsonl` on the `benchmarks` branch.

To see what each native strategy costs per schedule (the CI benchmark above runs
`snappea-compat`'s random strategy only), run the same tutorial workloads through
random, feedback with and without checkpoints, auto, PCT and DFS:

```sh
cargo run --release -p snappea --example strategy_bench -- tests/upstream/Tutorial
```

Search effectiveness is measured separately from throughput:

```sh
python3 scripts/bench-search.py --out search.json
```

This compares random, feedback, DFS, randomized DFS and auto on a fixed multi-bug
suite across 20 seeds, counting schedules to each bug and to all known bugs. Runs
that miss bugs within 10,000 schedules remain explicitly incomplete. The
`Search effectiveness` workflow records every main commit in `search.jsonl` on
the `benchmarks` branch; the latest result is on the [benchmarks page](https://snappea.dev/bench/#search). See [methodology and recorded experiments](../benchmarks/search/README.md)
for the workloads, raw results and limits of the comparisons.

For a quick pass across every runnable example/tutorial/regression model test
case, use `python3 scripts/bench-corpus.py --out corpus.json`. It writes a per-case
`corpus.md` table and raw first-discovery schedule counts for random, feedback,
DFS and auto. Unknown totals and unsuccessful/incomplete runs remain explicit;
see the [corpus methodology](../benchmarks/search/README.md). CI preserves each run
as `corpus/<commit>/<run-id>-<attempt>.json.gz` on the `benchmarks` branch. The older
`corpus.jsonl` history is retained but no longer appended to.
