# `snappea::emit`

Emit application events once, then check them against P specs in tests (with
`snappea::test`) and in production (with `snappea observe`). With only the `emit`
feature, `snappea` compiles no checker and depends only on `serde` / `serde_json`:

```toml
[dependencies]
snappea = { version = "0.2", default-features = false, features = ["emit"] }
```

```rust
use snappea::emit::{emit, json};

// After the reservation commits:
emit("eReserved", "coffee", json!({"order": "o1", "units": 2}));
```

`emit(event, key, payload)` takes the P event name, the partition key (anything
`Display`; each key gets its own spec instance) and a `Serialize` payload: `json!`, a
`#[derive(Serialize)]` struct, a scalar, or `()` for none. `emit_at(time, ...)` sets the
time (a `SystemTime`, or `i64` epoch nanoseconds for a logical clock). The call site is
recorded as `src` and shown in failure reports.

Where `emit` sends events:

| Process | Events go to |
| --- | --- |
| A test with a `snappea::test` recording | that recording |
| Started by a test (`SNAPPEA_EVENTS_DIR` set) | its own `<name>-<pid>-<random>.jsonl` in that folder |
| After `snappea::emit::set_sink(...)` | the sink: JSON lines `snappea observe` reads |
| Otherwise | nowhere: a few atomic loads, no lock |

```rust,no_run
use snappea::emit::Emitter;
use std::time::Duration;

snappea::emit::set_sink(Emitter::file("/var/log/app/events/inventory.jsonl")?);
// or Emitter::stdout(), Emitter::writer(w), Emitter::callback(|event| ...)
// ... at shutdown:
snappea::emit::close(); // writes what is queued; snappea::emit::flush(timeout) only waits
# Ok::<(), std::io::Error>(())
```

```sh
snappea observe -pf specs/inventory.p --spec InventoryCorrect -l '/var/log/app/events/*.jsonl'
```

`set_sink` replaces the earlier sink (`Emitter::off()` turns it off) and does nothing
in a process a test started. [Testing and production](https://github.com/crowecawcaw/snappea/blob/main/docs/production.md) covers
deploying this.

An `Emitter` is also a value: code that takes one (instead of calling the global
`emit`) gets `recording.emitter()` in tests and a production `Emitter` otherwise;
clones share a destination. `Emitter::callback(f)` calls `f` with each `Event` on the
emitting thread, under the emitter's lock, so `f` must be quick; the other production
sinks write from a background thread.

Emitting never panics, returns an error, or blocks on I/O. Production writers queue up
to 16,384 lines and drop events when the queue is full or a write fails;
`snappea::emit::dropped()` (or `Emitter::dropped()`) counts them. When off, `emit` costs a few atomic loads, but the
payload is still built: guard costly payloads with `is_on()`. The events folder writer
appends each line synchronously, so a child's events are on disk when it exits.

Lines follow the [events folder format](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md#events-folder-format):
`timestamp` is ISO-8601 with nanoseconds, from a wall clock that never goes back within a
process, plus `src`, `seq` and `pid`.
