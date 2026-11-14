---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How does P find a deadlock, and what does the report look like?
  - Why do transfers that lock accounts in argument order deadlock?
  - How does a global lock order fix it, including cycles of three or more?
---

# Lock-order deadlock: transfers that lock accounts in argument order

**Difficulty: Easy.**

A bank moves money between accounts with transactions on one database. A transfer
locks its two account rows (`SELECT ... FOR UPDATE`) one statement at a time, then
commits, which applies the debit and the credit and releases both locks. A lock
request on a row another transaction holds waits.

**The flaw in one sentence:** each transfer locks `from` and then `to`, so two
transfers in opposite directions (A to B, B to A) can each lock their first account
and then wait forever for the other's.

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `TransfersIsolated` (safety): a row lock has one holder at a time; a balance
  changes only under the lock of the transfer changing it; each transfer debits its
  source and credits its destination once; every balance matches.
- `TransfersComplete` (progress, hot state `TransferPending`): every requested
  transfer returns after both sides were applied.

**Atomic primitives** (one handler each in `Database`): lock one row (grant or queue
the request), and commit (apply both sides, release both locks, grant each lock to
its next waiter). Between a transfer's two lock statements, other transactions run.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/Database.p`](PSrc/Database.p) | The database: row locks with waiters, and commit |
| [`PSrc/ArgumentOrder.p`](PSrc/ArgumentOrder.p) | The flawed worker: locks `src`, then `dst` |
| [`PSrc/GlobalOrder.p`](PSrc/GlobalOrder.p) | The fix: locks the lower account id first |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugOppositeTransfers` | 1 to 2 and 2 to 1, argument order | The classic two-lock deadlock | Expected bug: `TransfersComplete detected liveness bug in hot state 'TransferPending'` (both transfers pending) |
| `tcBugCycleOfThree` | 1 to 2, 2 to 3, 3 to 1, argument order | A deadlock with no opposite pair | Expected bug: same message (all three transfers pending) |
| `tcFixedOppositeTransfers` | The opposite pair, global order | The fix for the classic case | Passes |
| `tcFixedCycleOfThree` | The three-way cycle, global order | A fix that only special-cases reversed pairs fails here | Passes |

## The counterexample

From `tcBugOppositeTransfers`, with accounts 1 and 2:

1. Transfer 1 (1 to 2) requests the lock on account 1. Granted.
2. Transfer 2 (2 to 1) requests the lock on account 2. Granted.
3. Transfer 1 requests account 2. Transfer 2 holds it, so transfer 1 waits.
4. Transfer 2 requests account 1. Transfer 1 holds it, so transfer 2 waits.
5. No machine has anything to do. The execution ends with both transfers pending,
   and `TransfersComplete` is still in `TransferPending`: a liveness bug.

`snappea check` reports:

```text
TransfersComplete detected liveness bug in hot state 'TransferPending' at the end of program execution.
TransfersComplete's fields: pending = {1: 0, 2: 0}, unfinished = 2
```

`tcBugCycleOfThree` shows the same with three transfers 1 to 2, 2 to 3, 3 to 1: no
two of them are opposite, yet each holds one account and waits for the next. Its
fields line is `pending = {1: 0, 2: 0, 3: 0}, unfinished = 3`.

**How a hang is reported.** The workers here wait in state handlers (`on Locked`),
not in a blocking `receive`. A machine waiting in a state handler is simply idle:
when nothing can run, the execution ends quietly, every safety assertion held, and
the deadlock is visible only because `TransfersComplete` is still in its hot state.
Without a hot-state spec the same deadlock passes, so every "this eventually
finishes" promise needs one. A machine blocked in `receive` with nothing else able
to run is different: the checker reports `Deadlock detected` for it directly (see
[lost-wakeup](../lost-wakeup/)).

Any hang leaves `TransfersComplete` in `TransferPending`, so the expected bug pins the
fields line as well: every transfer outstanding, none with a side applied. A lost
reply after the commit, for example, would show `pending = {1: 2, ...}` instead.

## Why ordinary tests miss it

A test that runs one transfer, or several in the same direction, never deadlocks.
Opposite transfers deadlock only if both take their first lock before either takes
its second, a window of one database round trip. In production it shows up as rare
hung requests or "deadlock detected" errors under load (most databases detect the
cycle and abort one transaction, which turns the hang into a sporadic failure that
gets retried and ignored).

## The fix

Lock in one global order: the lower account id first, whatever the direction of
the transfer. A worker waiting for a lock always holds only lower-numbered locks
than the one it waits for, so a cycle of waiting workers would need a lock to be
lower than itself. No cycle, no deadlock. The same rule covers cycles of any length,
which is what `tcFixedCycleOfThree` checks.

## Run

```sh
snappea check LockOrderDeadlock.pproj            # everything: the fix, the negative controls, coverage, mutation testing
snappea check LockOrderDeadlock.pproj 'tcBug*'   # only the negative controls: each passes only when its declared bug is found
```

Each negative control carries a native `// snappea: expect-bug` declaration: a matching
failure passes the test; no failure or a different failure fails it. Describe a
search as exhaustive only when its report says `search_complete`.

One mutant is suppressed: `t.dst < t.src` → `t.dst <= t.src` in `OrderedWorker`,
because a transfer's two accounts always differ (the spec asserts it).

## Bounds and omissions

Two or three accounts, two or three transfers, one database. Not modeled: deadlock
detection and abort by the database (with it, the flaw becomes failed transfers
instead of hung ones; retrying them can livelock), lock timeouts, insufficient-funds
checks, crashes.

## Testing the implementation

Emit `LockGranted`, `Applied` and `LockReleased` from a thin wrapper around the
database calls, and `TransferDone` when a transfer returns. To force the race, run
two opposite transfers with a barrier after each one's first lock: the flawed code
hangs (or gets a deadlock error), the fixed code completes both.
