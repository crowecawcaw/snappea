# `snappea::test`

Check the events your Rust application emits against P specs, from ordinary `#[test]`s.
Checking runs in process (no `snappea` binary). The runnable
[inventory examples](https://github.com/crowecawcaw/snappea/tree/main/examples/rust) show a fixture, explicit checks, a negative
control and proptest sequences. Application code emits through
[`snappea::emit`](crate::emit), which also writes production logs for `snappea observe`.

```toml
[dependencies]
snappea = { version = "0.2", default-features = false, features = ["emit"] }

[dev-dependencies]
snappea = { version = "0.2", default-features = false, features = ["test"] }
```

## Configure once

```toml
# Cargo.toml of the crate under test. Paths are relative to this file.
[package.metadata.snappea]
spec_files = ["specs"]           # files, globs, or folders of .p files
# specs = ["InventoryCorrect"]   # default: every spec in spec_files
# exclude = []                   # left out of the default selection
# strict = true                  # default: true when specs are named
# check_hot = false
# unknown_events = "error"       # or "warn", "ignore"
# coverage = "target/coverage/monitor.json"
# mode = "auto"                  # "off" disables Model::check
```

`Model::load()` reads it. `Model::from_files(paths)` and `Model::compile(&[(name, text)])`
(for `include_str!`) set up explicitly. Builders override any setting:
`.specs([..])`, `.exclude([..])`, `.strict(..)`, `.check_hot(..)`, `.unknown_events(..)`,
`.coverage(path)`. Compile once and share: a `Model` is cheap to clone.

| Variable | Meaning |
| --- | --- |
| `SNAPPEA_SPEC_FILES` | Spec files (`PATH`-separated), overriding `spec_files` |
| `SNAPPEA_COVERAGE` | Suite coverage file, overriding `coverage` (`.coverage()` wins) |
| `SNAPPEA_MODE` | `off` makes `Model::check` run the body without checking |
| `SNAPPEA_ARTIFACTS` | Folder for the events of failed checks |
| `SNAPPEA_EVENTS_DIR` | Set by a recording for its child processes |

## Check a test

```rust
use snappea::test::{json, Model};

# let model = Model::compile(&[("inventory.p", r#"
#   event eStocked: (units: int);
#   spec InventoryCorrect observes eStocked {
#     start state Ready { on eStocked do (p: (units: int)) { assert p.units >= 0, "negative stock"; } }
#   }
# "#)])?;
// let model = Model::load()?;  (in a LazyLock)
model.check(|recording| {
    // Emit from setup too, so the spec sees seeded state.
    recording.emit("eStocked", "coffee", json!({"units": 3}));
});
# Ok::<(), snappea::test::SetupError>(())
```

`model.check(|recording| ...)` records while the closure runs, then verifies and panics
with the failure output. Put setup and cleanup inside the closure and join worker
threads there. A panic in the closure propagates unchanged, with no second check.
Cargo has no global test hook, so each test opts in by calling the fixture; never share
a recording between tests.

The explicit form returns the result instead of panicking:

```rust,ignore
let recording = model.recording();
let mut app = App::new(recording.emitter());
app.reserve("coffee", "o1", 2);
recording.verify()?; // Result<CheckReport, CheckError>
```

## Emit

`emit(event, key, payload)` takes the P event name, the partition key (anything
`Display`; each key gets its own spec instances) and a `Serialize` payload: `json!`, a
`#[derive(Serialize)]` struct, a scalar, or `()`. Named tuples are objects; enums go by
name. `emit_at(time, event, key, payload)` sets the time (`SystemTime`, or `i64` epoch
nanoseconds for a logical clock); each key's events are checked in time order.

- `recording.emit(...)` in the test body, for setup and events the test itself knows.
  A payload that cannot be serialized fails `verify()`.
- `recording.emitter()`: a cloneable `snappea::emit::Emitter` to inject into application
  code or move into threads. It panics if used after `verify()` or after the recording is dropped.
- `snappea::emit::emit(...)` (the global emitter) in application code: a recording binds
  the thread that creates it, so emits on that thread go to it. Bind the scoped threads
  the test spawns with `let _guard = recording.bind();`. For `thread::spawn` threads,
  thread pools or async workers, move a `recording.binder()` (cloneable, `'static`) in
  and call `let _guard = binder.bind();` there; after `verify()` that guard does
  nothing. An
  emit on an unbound thread goes to the only open recording; with several open (Cargo
  runs tests in parallel) it fails them all with a `Config` error, so bind or inject
  instead.

The call site is recorded (`#[track_caller]`) and shown in failure output. Emit after
the operation commits. Events the P files declare but no selected spec observes are
ignored; names no P file declares fail the check (`unknown_events = "error"`).

## Failures

`verify()` returns `CheckError::Violation(SpecViolation)` or `CheckError::Setup(SetupError)`.
Assert on fields, not text:

```rust,ignore
let Err(CheckError::Violation(v)) = recording.verify() else { panic!() };
assert_eq!((v.spec.as_str(), v.key.as_str()), ("InventoryCorrect", "coffee"));
// v.message, v.location (P assert file:line), v.events (last 20, with src),
// v.liveness (hot state under check_hot), v.others (more violations)
```

`SetupError.kind` is `Config` (options, files, compile errors, unknown spec, an
ambiguous emit on an unbound thread), `Parser` (a payload that does not match its P type, or an unreadable
line), `Validation` (strict: a spec checked no event), `UnknownEvent` or `OutOfOrder`.

Displayed (and as the `Model::check` panic):

```text
P spec InventoryCorrect was broken: reservation o2 exceeds available stock

  Events (key "coffee"):
      1  eStocked   units=3            tests/support/mod.rs:31
      2  eReserved  order=o1 units=2   tests/support/mod.rs:32
      3  eReserved  order=o2 units=2   tests/explicit.rs:44  <- broke the spec here

  Spec: specs/inventory.p:21
```

A negative control proves the spec catches a bug; it panics unless exactly that spec
is broken with a message matching the regular expression:

```rust,ignore
recording.expect_violation("InventoryCorrect", "exceeds available stock");
```

## Other processes and logs

```rust,ignore
let recording = model.recording();
let status = Command::new(server_bin).envs(recording.env()).status()?; // sets SNAPPEA_EVENTS_DIR
recording.add_events_dir("/shared/events");     // a folder the test did not create
recording.add_log("logs/events.jsonl");          // JSON lines in the events format
recording.add_log_with("logs/app.log", |line| parse(line)); // -> Option<Value> event line
recording.verify()?;
```

A child writes to `recording.events_dir()` with `snappea::emit::emit` (which reads
`SNAPPEA_EVENTS_DIR`), or any writer of the
[events folder format](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md#events-folder-format). Stop or join
children before `verify()`. A failed check keeps the folder (plus `recording.jsonl`) and
names it.

## Coverage

Set `coverage` (or `SNAPPEA_COVERAGE`) and every check merges its spec coverage (the
handlers, statements and branches of the specs the events ran) into that file, under a file lock, so parallel tests and test binaries share it. It
prints the summary when the coverage changes (`cargo test -- --nocapture` shows it). The JSON is
`snappea coverage merge-observe`'s format. Delete a stale file before a run; CI starts
clean. `CheckReport::coverage()` returns one check's coverage.

## Property tests

Create the recording inside the property body, which runs once per case and shrink
attempt, and report with `prop_assert!` so proptest shrinks the input:

```rust,ignore
proptest! {
    #[test]
    fn ops(commands in commands()) {
        let recording = model().recording();
        // ... run commands against a fresh application ...
        let checked = recording.verify();
        prop_assert!(checked.is_ok(), "{}", checked.unwrap_err());
    }
}
```
