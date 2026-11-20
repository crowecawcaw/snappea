# Online migration: dual-write, backfill, cutover

A table moves from an old store to a new store while the application keeps reading
and writing it. Writers dual-write for the whole migration: old first, which assigns
the row's version (`lsn`), then new with that version. A migrator backfills every key
by reading it from old and copying it to new, then asks the router to cut reads over
to new. Reads and writes go through the router.

The old store (`OldStore`) and the application's reader (`Reader`) exist already and
are given in `PSrc/`. The design is the new store, the router, the writer and the
migrator.

## Contract

- A write that returned is in both stores. New only ever holds rows that old committed
  for that key, and never moves a key back to an older version.
- At cutover every key reads the same from new as from old.
- A read returns at least the newest write that returned before the read started, and
  a reader never sees a key go back to an older version, across cutover too.
- Every read is served by exactly one store read: by old before cutover, by new after.
- Every write returns, every read is answered, and reads are cut over, once.

## Who does what

- **Writer**, created with its router, both stores and a list of writes (`Op`: a put
  or a delete of one key, with a request id). It performs them in order, one at a
  time: ask the router for a write slot (`BeginWrite`, answered by `WriteGranted`),
  write to old (`OldWrite`, answered by `OldWritten` with the version), write the same
  row with that version to new (`NewWrite`, answered by `NewWritten`), announce
  `WriteAcked` for the request, which is the moment the application's call returns,
  and give the slot back (`EndWrite`).
- **Reader** (given). For each read it announces `ReadStarted`, sends `Route` to the
  router, gets `RowRead` from a store, announces `ReadSeen` and sends `ReadDone` to
  the router.
- **Router**, created with both stores. It forwards each `Route` as `ReadRow` to one
  store. On `CutoverReq` from the migrator it flips reads to new, announces `Cutover`
  at the flip and answers `CutoverDone`.
- **New store**. It answers `NewWrite` and `Copy` with `NewWritten`. It serves
  `ReadRow` the way the old store does: announce `ReadServed` (for `NewTable`), then
  send `RowRead` with the row, or with `Absent()` for a key it has no row for. It
  announces `NewApplied` with the key and the row whenever it changes what it holds
  for a key, and only then. The row is the one old committed: value, deleted flag and
  version.
- **Migrator**, created with its owner, the router, both stores and the keys to scan.
  For each key: `ScanRow` to old, answered by `RowRead`; `Copy` to new, answered by
  `NewWritten`. Then `CutoverReq` to the router, and on `CutoverDone` it sends
  `MigrationDone` to its owner.

## Atomic steps

One handler each.

- Old store: write a row (assigns the next version; a delete is a write with
  `deleted = true` and has a version too), read a row for a client, read a row for
  the backfill.
- New store: one statement on one row, which may depend on what the store holds for
  that key (as an `INSERT`, a `DELETE` or an `UPDATE ... WHERE` would); or a read of
  one row.
- Router: grant a write slot, take one back, route a read, note a read as done, flip.

Everything else is several steps. A dual-write is two store writes; the backfill of a
key is a read from old and then a write to new.

## What can happen

- The table has one or more keys when the migration starts. A key that was never
  written can be read; it reads as `Absent()`.
- Any number of writers and readers run, and each starts at any time: before the
  backfill, during it, while the migrator waits for the cutover, or after it.
- Writers put and delete. Two writers can write the same key at the same time. A
  writer can have several writes, one after the other.
- A reader reads one key several times, one read at a time.
- Messages are delayed arbitrarily and reordered between senders, but not lost or
  duplicated. Anything can happen between two steps of one machine.
- Stores, router, writers, readers and migrator do not crash. There is one backfill
  pass and one cutover.

Not in scope: a migrator crash, writes that bypass the router, decommissioning the old
store, schema changes, and how long a cutover pauses traffic.
