---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What is write skew, and why does snapshot isolation allow it?
  - How do I model a database's isolation level in P instead of assuming serializability?
  - How do I fix write skew by materializing the conflict?
---

# Write skew under snapshot isolation

A hospital schedules doctors on call. The rule: at least one doctor is on call at all
times. A doctor can go off call if someone else is still on call. The application checks
the rule inside a transaction, on a database running snapshot isolation (PostgreSQL's
`REPEATABLE READ`, Oracle's `SERIALIZABLE`, most MVCC databases' default "strong"
level).

**Difficulty: hard.** The check runs inside a transaction, so it looks safe; the
database prevents lost updates; the bug needs knowledge of what snapshot isolation does
*not* promise.

**Contract.** After every commit at least one doctor is on call. A doctor goes off call
only if they asked; "granted" means their row committed as off call; "refused" happens
only when they are the last doctor on call. Every request gets an answer.

**The database (`SnapshotStore`), modeled explicitly:** a transaction reads a snapshot
(the committed state at its start), buffers its writes, and commits them all at once.
First committer wins: a commit aborts if another transaction committed a write to a row
it writes since its snapshot. Rows it only read are not checked. Atomic primitives:
take a snapshot and read from it; commit with the conflict check.

## The flaw

Each transaction reads both doctors' rows but writes only its own, so two concurrent
off-call transactions have disjoint write sets and snapshot isolation commits both,
although each one's decision depended on the row the other changed.

```sql
BEGIN;  -- snapshot isolation
SELECT count(*) FROM doctors WHERE on_call;          -- 2
UPDATE doctors SET on_call = false WHERE id = :me;   -- writes only my row
COMMIT;
```

## The counterexample

From `tcBugWriteSkew` (doctors 1 and 2 on call, both ask at once):

1. Doctor 1's transaction takes a snapshot: 2 doctors on call.
2. Doctor 2's transaction takes a snapshot: 2 doctors on call.
3. Doctor 1 commits `doctor 1 = off`. No conflict; doctor 1 is told they are off call.
4. Doctor 2 commits `doctor 2 = off`. Its write set (row 2) does not overlap the commit
   since its snapshot (row 1), so it commits too.
5. Nobody is on call:

```text
Assertion Failed: nobody is on call: the last on-call doctors went off call together
```

## Why ordinary tests miss it

Tests run requests one at a time, and every sequential order is correct. Even a
concurrency test needs both transactions to take their snapshots before either commits,
a window of a few milliseconds. Unit tests against an in-memory store or SQLite (which
serializes writers) cannot show it at all. And reviewers trust "it's in a transaction".
The checker found it in the first schedule, because the model states what snapshot
isolation actually does.

## The fix

Materialize the conflict. Every transaction that changes who is on call also writes a
shared roster row (`UPDATE roster SET changed_by = :me`). Two concurrent off-call
transactions now write the same row, so first-committer-wins aborts one. Its client
retries with a fresh snapshot, sees one doctor on call, and refuses. The database is
unchanged; only the application's transaction is.

Other fixes: lock the rows you read (`SELECT ... FOR UPDATE`), or run at true
`SERIALIZABLE` (PostgreSQL's SSI validates read sets). All make the transactions
conflict on what they read, not only on what they write.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/OnCall.p`](PSpec/OnCall.p) | `AlwaysSomeoneOnCall`: the rule after every commit, justified answers, every request answered (hot state) |
| [`PSrc/SnapshotStore.p`](PSrc/SnapshotStore.p) | The database: snapshot reads, buffered writes, first committer wins. Shared |
| [`PSrc/Doctor.p`](PSrc/Doctor.p) | Flawed: check the count, write only my row |
| [`PSrc/RosterDoctor.p`](PSrc/RosterDoctor.p) | Fixed: also write the roster row, retry on conflict |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, `PhoneUpdate` (an unrelated transaction), test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcFixedBothOffCall` | Two on call, both ask at once | The write-skew race: one granted, one aborted, retried and refused | passes |
| `tcFixedTwoOfThree` | Three on call, two ask at once | Both must be granted after the conflict and retry; a fix that refuses on contention fails | passes |
| `tcFixedOneAfterAnother` | Doctor 1 goes off call, then doctor 2 asks | Lifecycle: the last doctor on call is refused | passes |
| `tcFixedUnrelatedCommit` | A doctor goes off call while another transaction writes row 100 | Disjoint writes do not conflict: the store's conflict check is per row | passes |
| `tcBugWriteSkew` | Flawed client, two on call, both ask | **Expected bug**: `nobody is on call` | Expected bug found |

## Run

```sh
snappea check WriteSkew.pproj 'tcFixed*' --deny mutation
snappea check WriteSkew.pproj tcBugWriteSkew   # succeeds only when the declared bug is found
```

The negative controls use native `snappea: expect-bug` declarations. A matching
failure passes the test; no failure or a different failure fails it. Search coverage
depends on the budget: claim exhaustion only when the current run reports completion.

## Bounds and omissions

Two or three doctors, at most two requests, one unrelated transaction. Doctors only go
off call (never back on). The database never fails; commits are not lost. Not modeled:
`SELECT ... FOR UPDATE` and SSI (named above as alternatives), deadlocks between
locking transactions, read-only anomalies.

One suppression: aborting on any concurrent commit (instead of per row) is stricter
than snapshot isolation and still correct, since the client retries; the contract
cannot tell them apart.

## Testing the implementation

Emit `Committed` with the rows each transaction wrote (from the commit path, or a
change feed) and `OffCallAnswered` from the API. To force the race, run two off-call
requests whose transactions both pause after the `SELECT` (a test hook, or a lock both
wait on), then let both commit. Against the flawed code both succeed and the roster is
empty; with the roster row one fails with a serialization error and is retried.
