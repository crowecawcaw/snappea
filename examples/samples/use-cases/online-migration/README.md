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

- `MigrationSafety`: a write that returned is in both stores (no lost writes), and new
  only ever holds versions old committed for that key (nothing resurrected or
  invented). New never moves a key back to an older version.
- `MigrationSafety`: at cutover every key reads the same from new as from old. This is
  the verify step: the model shows it never finds a difference.
- `MigrationSafety`: a read returns at least the newest write that returned before the
  read started, and a reader never sees a key go back to an older version, across
  cutover too.
- `MigrationSafety`: every read is served by exactly one store read: old before
  cutover, new after.
- `MigrationCompletes` (hot state): every write returns, every read is answered, reads
  are cut over.

**Atomic primitives**

- Old store: write a row (assigns the next version, `lsn`; deletes are soft, so a
  delete has a version), read a row for a client or for the backfill scan.
- New store: put a row only if its version is newer than the stored one, keeping
  tombstones (`UPDATE ... WHERE lsn < :lsn`, or insert if absent); read a row.
- Router: grant a write slot, route a read, and flip reads to new once no read or
  write is in flight (new ones wait meanwhile).

The backfill reads a row and then writes it: two steps, with writers in between.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `MigrationSafety` (no lost or resurrected writes, fresh and monotonic reads) and `MigrationCompletes` (progress) |
| [`PSrc/Stores.p`](PSrc/Stores.p) | The old store and the fixed new store |
| [`PSrc/Migration.p`](PSrc/Migration.p) | `Router`, `Writer`, `Reader`, `Migrator` |
| [`PSrc/BrokenStores.p`](PSrc/BrokenStores.p) | Negative controls: blind copy, insert-if-absent with hard deletes, a router that does not drain |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBackfillRacesWriter` | Key 1 = 10, one writer puts 20 during the backfill | The classic stale copy. All schedules | Passes |
| `tcDeleteDuringBackfill` | One writer deletes key 1 during the backfill | Resurrection. All schedules | Passes |
| `tcTwoWritersOneKey` | Two writers put 20 and 30 on key 1 | Dual-writes reach new in the other order than old committed them. All schedules | Passes |
| `tcReadsAcrossCutover` | One writer, one reader reading key 1 twice | Reads stay fresh and monotonic across the flip. All schedules | Passes |
| `tcWriteAfterCutover` | A write and two reads start after cutover | Lifecycle boundary: writes still go old then new; reads come from new. All schedules | Passes |
| `tcGeneratedMigration` | Two keys, two writers each putting or deleting either key, a reader of key 1, 2 or 3 (never written) | Keys do not leak into each other; reading a missing key. Sampled | Passes |
| `tcBugBlindBackfill` | Unconditional copy, as in `tcBackfillRacesWriter` | The stale copy lands | Expected bug: `new store moved key 1 back from version 2 to 1` |
| `tcBugResurrectedDelete` | Insert-if-absent, hard deletes, as in `tcDeleteDuringBackfill` | The delete (version 2) is undone | Expected bug: `new store moved key 1 back from version 2 to 1` |
| `tcBugCutoverMidWrite` | A router that flips at once, one writer, no reader | A half-done write at the flip | Expected bug: `cutover broken: new store lost a change: key 1 reads 10 from new but 20 from old` |
| `tcBugCutoverMidRead` | A router that flips at once, one reader, no writer | A read routed to old is served after the flip | Expected bug: `cutover broken: old store served reader 1's read of key 1 after cutover` |

Each negative control fails with exactly one message, whatever schedule the search
finds first: the stale copy is caught when it lands in new, before a write returns or
the cutover compares the stores. The router without the drain has two flaws, so each
gets its own control with only the traffic that exposes it.

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

## Run

```sh
snappea check tcBackfillRacesWriter tcDeleteDuringBackfill tcTwoWritersOneKey tcReadsAcrossCutover tcWriteAfterCutover tcGeneratedMigration --deny mutation
snappea check 'tcBug*'    # passes only when each declared failure is found
```

## Bounds and omissions

One or two keys, one or two writers with one write each, one reader with two reads,
one backfill pass. Stores never fail and messages are not lost. Not modeled: a crash
of the migrator mid-backfill (it would restart the scan; the conditional put makes
that safe), writes that bypass the router, decommissioning old, schema changes, and
the time the drain pauses traffic.

Each writer has one operation in flight and the stores reply exactly once over
reliable channels, so an `OldWritten` reply belongs to the current operation; the
writer does not read its request id (the checker notes this as an advisory). Adding
retries, duplicate replies, or concurrent writes requires correlating replies before
advancing.

One suppression: the new store's `>` may become `>=`. Versions are unique per write, so
an equal version is the same row.

## Testing the implementation

Emit `OldCommitted` and `NewApplied` from each store after its write commits (with
the row's version), `WriteAcked` when a write call returns, `ReadStarted` and
`ReadSeen` around each read, `ReadServed` from whichever store serves a client read,
and `Cutover` when the routing flag flips. To force the stale copy, pause the backfill
after it reads a row, run a write to completion, then let the copy proceed. For the resurrection,
do the same with a delete. For the cutover, hold a write between its old and new
writes and request the cutover: it must wait.

