# Testing and production

Instrument once: the same `emit` calls that tests check also write a production log
that `snappea observe` checks against the same P spec. This guide uses the inventory
examples ([spec](../examples/model/PSpec/inventory.p)); the
[Go example](../examples/go/README.md#5-production-logs-with-snappea-observe) runs every
step below in CI.

## 1. Emit once

Emit after a change commits, keyed by the entity the spec tracks (here the SKU):

```go
snappeaemit.Emit("eReserved", sku, map[string]any{"order": order, "units": units})
```

Where the event goes depends on the process, not the call:

| Process | Events go to |
| --- | --- |
| A test with a recording | that recording, checked when the test ends |
| Started by a test (`SNAPPEA_EVENTS_DIR` set) | the test's events folder |
| Production, after `set_sink` | a JSON-lines log (or stdout, or your callback) |
| Anything else | nowhere, at the cost of one atomic load |

## 2. Check in tests

The language helpers check every recording against the spec. Tests catch
violations before deploy; production checking catches the ones tests didn't
exercise. The helpers default to `unknown_events = error`, so a misspelled event
name fails a test instead of being dropped silently. See
[language-support.md](language-support.md) and each language's README.

## 3. Write events in production

Set a sink once at start-up and close it at shutdown:

| Language | Start-up | Shutdown |
| --- | --- | --- |
| Python | `snappea.set_sink(path)` | automatic at exit (`sink.close()` to do it earlier) |
| Go | `s, err := snappeaemit.File(path); snappeaemit.SetSink(s)` | `defer snappeaemit.Close()` |
| Java | `SnappeaEmit.setSink(Sink.file(Path.of(path)))` | a shutdown hook flushes; `SnappeaEmit.close()` |
| Rust | `snappea::emit::set_sink(Emitter::file(path)?)` | `snappea::emit::close()` |

`stdout` (for container log collectors), `writer` and `callback` sinks are also
available. Rules that matter in production:

- **One file per process.** Name it by service, host and pid, e.g.
  `/var/log/app/events/inventory-$HOST-$PID.jsonl`. Lines from one process never
  split or interleave; lines from several processes in one file can.
- **Emitting never fails the request.** Sinks write from a background thread through
  a bounded queue. A full queue, a failed write or an unencodable payload drops the
  event and counts it in `dropped`. Export that count as a metric: a dropped event
  can cause a false violation later (section 6).
- **Rotate by renaming**, as logrotate does by default. Avoid `copytruncate`, which
  can lose or duplicate lines.

## 4. Check the logs

Batch, over closed (rotated) files or a copy of them:

```sh
snappea observe specs/inventory.p '/var/log/app/events/*.jsonl' \
  --spec InventoryCorrect --json-report report.json
```

Quote the glob: `snappea` expands it, including `**`. Exit status 0 is clean, 1 a spec
violation, 2 a tool or input error; read details from the JSON result (`--json` or
`--json-report`, [format](observe.md#options)). A file still being written may end in a
partial line, which is a `parser` error (exit 2), so check rotated files, or check
live:

```sh
tail -n +1 -F /var/log/app/events/inventory.jsonl |
  snappea observe specs/inventory.p --spec InventoryCorrect --stdin
```

Live mode checks events as they arrive and stops at the first violation. It does
not sort: each key's timestamps must not decrease within the stream. Several live
streams need a global sequence number (`--sequence`, e.g. from a database outbox);
see [Live checks](observe.md#live-checks-for-external-tests).

## 5. Partition keys

Each key gets its own monitor, which starts at the key's first event and sees only
that key's events. So:

- **Key by the entity the spec reasons about.** The inventory spec tracks one
  product's stock, so the key is the SKU. Events that must be checked together
  need the same key.
- **The input must hold each key's whole history.** If retention dropped a
  product's `eStocked`, its first `eReserved` reads as "reservation before stock
  setup". Check windows that start where keys start, or keep the logs for the
  keys' lifetime. `--key A --key B` checks only some keys.
- **Memory grows with keys**, since each key's monitor is kept until the run ends.

## 6. Clock skew

Batch checking sorts each key's events by timestamp; equal timestamps keep input
order. Timestamps never decrease within one process, but hosts' clocks differ, so
events for one key written by two hosts a few milliseconds apart can be checked in
the wrong order. Pick one:

- **One writer per key**: route each key to one process (sharding, a single
  owner). The example does this, one product per host.
- **Logical time**: pass a per-key version or a database sequence as the time
  (`emit_at` / `at=`), so order comes from the data, not the clock.
- **A global sequence** for live checks: `--sequence FIELD`.

## 7. Unknown event names

Tests and `snappea observe` default to `unknown_events = error`. Production logs
often carry events the spec doesn't model: pass `--unknown-events ignore` for those,
or `--unknown-events warn` after a deploy that changes instrumentation, so a typo
shows up as a warning with the likely intended name.

## 8. When production reports a violation

A violation means the events for one key broke the spec. Either the system did
something the spec forbids, or the events don't reflect what it did. Don't block
or roll back requests from the observer; it runs out of band.

1. Read the report: `key`, `message` (the P assertion and its location), and the
   key's last events, each with `src`, the `emit` call that wrote it.
2. Rule out an observation problem: dropped events (`dropped` > 0), a key written
   by several hosts (section 6), a history cut off by retention (section 5), an
   emit before the commit, or a misspelled name.
3. If the events are right, it is a real bug, found with the evidence attached.
   Treat it as an incident in proportion to the spec (an oversell is a data
   problem). Get the key's events with `--key`, turn them into a regression test
   that replays them through the helper (`add_log`, or `emit_at` per line), fix the
   code, and keep the test.
4. If the spec is wrong (the behaviour is allowed), fix the spec, and re-run the
   tests and the model check.

Exit status 2 is not a violation: a parser error, a truncated line, an unknown
event under `--unknown-events error`, or `--strict` finding no events. Alert on it
separately; it usually means the instrumentation or the log pipeline broke.

## Existing structured logs

A service that already logs JSON can be checked without new emitters.
`--parser-config` maps fields by dotted path. For lines like

```json
{"time": "2026-10-19T10:00:01Z", "msg": "eReserved", "sku": "coffee", "order": "o1", "units": 2, "user": "bob"}
```

```sh
snappea observe specs/inventory.p app.log --spec InventoryCorrect \
  --parser-config '{"event": "msg", "key": "sku", "timestamp": "time", "payload": ""}'
```

`"payload": ""` uses the whole line as the payload; fields the P type does not
declare are ignored. Use a path such as `"attrs"` for a nested payload. Limits:

- The event name must be a string field holding the P event name exactly. It cannot
  be derived from a message text or several fields. Lines whose name the spec
  doesn't declare are ignored, as in section 7.
- Every line must be a JSON object with the event field. A plain-text line or one
  without the field is a parser error (exit 2): filter first (`jq -c 'select(.msg)'`)
  or send P events to their own log.
- Payload field names and types must match the P event exactly: `"units": "2"` is a
  parser error where P expects an `int`. There is no renaming per field.
- Timestamps are ISO-8601 or epoch milliseconds. Epoch seconds read as
  milliseconds.

Thin adapters for existing loggers (a `slog` handler, a logback appender, a
`tracing` layer, a Python `logging` handler) that write this format are a possible
follow-up; none ship yet. Until then, use the emitters or `--parser-config`.
