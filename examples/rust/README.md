# Rust: inventory reservations

Three Cargo integration-test targets check one [inventory service](src/lib.rs)
against the [P spec](specs/inventory.p) with [`snappea::test`](../../crates/snappea/src/test/README.md),
which checks in process: no `snappea` binary is needed. See the
[shared overview](../README.md#how-the-checks-work) for what the checks do and why.

## Run

From the repository root, with a stable Rust toolchain:

```sh
cargo test --manifest-path examples/rust/Cargo.toml --locked
cargo test --manifest-path examples/rust/Cargo.toml --locked --test explicit
cargo test --manifest-path examples/rust/Cargo.toml --locked --test checked_fixture
cargo test --manifest-path examples/rust/Cargo.toml --locked --test generated_sequences
```

This is a standalone Cargo workspace. The
[CI workflow](../../.github/workflows/rust-examples.yml) formats, lints and runs all
three targets.

`Inventory` takes a typed `FnMut(Event)` sink, so the application has no P
dependency. [tests/support/mod.rs](tests/support/mod.rs) maps those events to JSON
with the SKU as key, loads the spec once with `Model::load()` (from
`[package.metadata.snappea]` in [Cargo.toml](Cargo.toml)), and provides the fixture.

## 1. Explicit checks for selected tests

[tests/explicit.rs](tests/explicit.rs) creates `model().recording()`, builds the
inventory on its sink, seeds stock, runs operations, and unwraps `recorder.check()`.
It also asserts that rejected requests emit nothing, and has the negative control:
`recorder.expect_violation("InventoryCorrect", "exceeds available stock")` returns the
structured violation (`key`, `events`, ...) for further assertions. The last test
shows that code emitting through the global `snappea::emit::emit` is checked too: the
recording binds the test's thread.

**Gotcha:** join worker threads and finish cleanup before `check`, which consumes
the recording. Nothing is checked in `Drop`.

## 2. A checked fixture for ordinary tests

[tests/checked_fixture.rs](tests/checked_fixture.rs) wraps each `#[test]` body in
`with_inventory`, which calls `model.check(|recording| ...)`: it starts recording,
seeds fresh stock, runs the body, then checks and panics with the failure output.

**Gotcha:** Cargo has no global test hook, so each test opts in by calling the
fixture; put it in your project's existing fixture factory. Don't use a global
recorder: Cargo runs tests in parallel. A panicking body stays failed without a
second check during unwinding, so do monitor-relevant cleanup inside the closure.
`SNAPPEA_MODE=off` runs the bodies without checking.

The fixture's model has `.coverage("target/coverage/monitor.json")`, so each check
merges its [coverage](../README.md#coverage) into that file under a lock and prints
the summary when it changes (`cargo test -- --nocapture` shows it). `SNAPPEA_COVERAGE` does the same
without code.

## 3. Generated sequences with proptest

[tests/generated_sequences.rs](tests/generated_sequences.rs) generates vectors of
reserve/release commands, including invalid quantities, repeated order IDs and
releases without reservations, and runs them against the real service.

**Gotcha:** create the recorder and inventory *inside* the property body, which
proptest runs for every case and shrink attempt. Report the check with
`prop_assert!` rather than `unwrap` so proptest shrinks the command sequence.

CI runs 64 cases of up to 40 commands; raise `cases` to explore more. Failure seeds
are saved in `tests/generated_sequences.proptest-regressions`: commit that file
when fixing a bug so later runs replay it.

## Adapt to your project

Depend on the `snappea` crate under `[dev-dependencies]`:
`snappea = { version = "0.2", default-features = false, features = ["test"] }`. Add
`[package.metadata.snappea]` to your `Cargo.toml`, and map your existing domain events or outbox in a `record` function like the one in
[tests/support/mod.rs](tests/support/mod.rs). Or instrument with
[`snappea::emit`](../../crates/snappea/src/emit/README.md), which feeds test recordings
(`recording.emitter()`), child processes (`SNAPPEA_EVENTS_DIR`) and production logs for
`snappea observe` from the same calls. `emit_at` takes explicit times for deterministic
tests.
