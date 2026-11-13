---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model an online table migration with dual writes and a backfill?
  - Why does a backfill need a conditional put, and which condition is enough?
  - What must the cutover wait for so that reads never go back in time?
---

# Online migration: dual-write, backfill, cutover

A table moves to a new store while the application keeps reading and writing it.
Writers dual-write for the whole migration: old first (which assigns the row's
version), then new with that version. A migrator backfills every key by reading it
from old and copying it to new, then asks the router to cut reads over to new.

**Contract**

- A write that returned is in both stores (no lost writes), and new only ever holds
  versions old committed for that key (nothing resurrected or invented). New never
  moves a key back to an older version.
- At cutover every key reads the same from new as from old. This is the verify step:
  the model shows it never finds a difference.
- A read returns at least the newest write that returned before the read started,
  and a reader never sees a key go back to an older version, across cutover too.
  After cutover old serves no reads.
- The migration finishes: every write returns, every read is answered, reads are cut
  over (progress, hot state).

**Atomic primitives**

- Old store: write a row (assigns the next version, `lsn`; deletes are soft, so a
  delete has a version), read a row.
- New store: put a row only if its version is newer than the stored one, keeping
  tombstones (`UPDATE ... WHERE lsn < :lsn`, or insert if absent); read a row.
- Router: grant a write slot, route a read, and flip reads to new once no read or
  write is in flight (new ones wait meanwhile).

The backfill reads a row and then writes it: two steps, with writers in between.

## The races and the fixes

1. **Stale copy.** The backfill reads key 1 = 10, a writer writes 20 to old and new,
   then the copy of 10 reaches new. An unconditional copy overwrites 20.
   Fix: the version-compare put refuses the older copy.
2. **Resurrected delete.** The usual first fix is "insert if absent". It stops the
   stale overwrite, but a hard delete makes the key absent again, so a copy of the
   pre-delete row read before the delete brings it back. Fix: keep tombstones with
   their version and compare versions.
3. **Cutover with work in flight.** If reads flip while a write has reached old but
   not new, a reader that saw the write in old can read the older value from new. A
   read routed to old just before the flip can also be served after a newer write
   reached old. Fix: drain reads and writes, then flip, atomically.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `MigrationSafety` (no lost or resurrected writes, fresh and monotonic reads) and `MigrationCompletes` (progress) |
| [`PSrc/Stores.p`](PSrc/Stores.p) | The old store and the fixed new store |
| [`PSrc/Migration.p`](PSrc/Migration.p) | `Router`, `Writer`, `Reader`, `Migrator` |
| [`PSrc/BrokenStores.p`](PSrc/BrokenStores.p) | Negative controls: blind copy, insert-if-absent with hard deletes, a router that does not drain |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## Test cases

| Test | Drives | Why it matters | Result |
| --- | --- | --- | --- |
| `tcBackfillRacesWriter` | Key 1 = 10, one writer puts 20 during the backfill | The classic stale copy | passes, exhaustive |
| `tcDeleteDuringBackfill` | One writer deletes key 1 during the backfill | Resurrection | passes, exhaustive |
| `tcTwoWritersOneKey` | Two writers put 20 and 30 on key 1 | Dual-writes reach new in the other order than old committed them | passes, exhaustive (about 15 s) |
| `tcReadsAcrossCutover` | One writer, one reader reading key 1 twice | Reads stay fresh and monotonic across the flip | passes, exhaustive (about 27 s; sampled under a shorter `--timeout`) |
| `tcWriteAfterCutover` | A write and two reads start after cutover | Lifecycle boundary: writes still go old then new; reads come from new | passes, exhaustive |
| `tcGeneratedMigration` | Two keys, two writers each putting or deleting either key, a reader of key 1, 2 or 3 (never written) | Keys do not leak into each other; reading a missing key | passes, sampled |
| `tcBugBlindBackfill` | Unconditional copy, as in `tcBackfillRacesWriter` | **Expected to fail**: `new store moved key 1 back from version 2 to 1` | fails |
| `tcBugResurrectedDelete` | Insert-if-absent, hard deletes, as in `tcDeleteDuringBackfill` | **Expected to fail**: `new store moved key 1 back from version 2 to 1` (the delete was version 2) | fails |
| `tcBugCutoverMidWrite` | A router that flips at once, one writer, no reader | **Expected to fail**: `cutover broken: new store lost a change: key 1 reads 10 from new but 20 from old` | fails |
| `tcBugCutoverMidRead` | A router that flips at once, one reader, no writer | **Expected to fail**: `cutover broken: old store served a read after cutover` | fails |

Each negative control fails with exactly one message, whatever schedule the search
finds first: the stale copy is caught when it lands in new, before a write returns or
the cutover compares the stores. The router without the drain has two flaws, so each
gets its own control with only the traffic that exposes it.

## Run

```sh
snappea check tcBackfillRacesWriter tcDeleteDuringBackfill tcTwoWritersOneKey tcReadsAcrossCutover tcWriteAfterCutover tcGeneratedMigration --timeout 30s
snappea check mutation tcBackfillRacesWriter tcDeleteDuringBackfill tcTwoWritersOneKey tcReadsAcrossCutover tcWriteAfterCutover tcGeneratedMigration
snappea check tcBugBlindBackfill     # exits 1: new store moved key 1 back from version 2 to 1
```

## Bounds and omissions

One or two keys, one or two writers with one write each, one reader with two reads,
one backfill pass. Stores never fail and messages are not lost. Not modeled: a crash
of the migrator mid-backfill (it would restart the scan; the conditional put makes
that safe), writes that bypass the router, decommissioning old, schema changes, and
the time the drain pauses traffic.

Two suppressions: the new store's `>` may become `>=` (versions are unique per write,
so an equal version is the same row), and the old store's `eOldRead` announcement is
observation only.

## Testing the implementation

Emit `eOldCommitted` and `eNewApplied` from each store after its write commits (with
the row's version), `eWriteAcked` when a write call returns, `eReadStarted` and
`eReadSeen` around each read, `eOldRead` when old serves a read, and `eCutover` when
the routing flag flips. To force the stale copy, pause the backfill after it reads a
row, run a write to completion, then let the copy proceed. For the resurrection,
do the same with a delete. For the cutover, hold a write between its old and new
writes and request the cutover: it must wait.
