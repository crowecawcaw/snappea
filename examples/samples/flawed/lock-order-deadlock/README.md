---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How does P find a deadlock, and what does the report look like?
  - Why do transfers that lock accounts in argument order deadlock?
  - How does a global lock order fix it, including cycles of three or more?
---

# Lock-order deadlock: transfers that lock accounts in argument order

**Difficulty: medium.**

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
- `TransfersComplete` (progress): while any requested transfer has not returned, the
  spec is in the hot state `TransferPending`.

## How P reports a deadlock

P has no deadlock detector as such. A deadlock is an execution where every machine
is waiting for a message nobody will send: no machine can take a step, so the
execution ends. If a spec is in a hot state at that moment, the promise it tracks
was never kept, and the checker reports a **liveness bug**:

```text
PImplementation.TransfersComplete detected liveness bug in hot state 'TransferPending' at the end of program execution.
```

Without a hot-state spec, the same deadlock passes silently: every safety assertion
held, the program just stopped. Every "this eventually finishes" promise needs one.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/Database.p`](PSrc/Database.p) | The database: row locks with waiters, and commit |
| [`PSrc/ArgumentOrder.p`](PSrc/ArgumentOrder.p) | The flawed worker: locks `src`, then `dst` |
| [`PSrc/GlobalOrder.p`](PSrc/GlobalOrder.p) | The fix: locks the lower account id first |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## The counterexample

From `PCheckerOutput/tcBugOppositeTransfers/bug.txt`, with accounts 1 and 2:

1. Transfer 1 (1 to 2) requests the lock on account 1. Granted.
2. Transfer 2 (2 to 1) requests the lock on account 2. Granted.
3. Transfer 1 requests account 2. Transfer 2 holds it, so transfer 1 waits.
4. Transfer 2 requests account 1. Transfer 1 holds it, so transfer 2 waits.
5. No machine has anything to do. The execution ends with both transfers pending,
   and `TransfersComplete` is still in `TransferPending`: liveness bug.

`tcBugCycleOfThree` shows the same with three transfers 1 to 2, 2 to 3, 3 to 1: no
two of them are opposite, yet each holds one account and waits for the next.

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

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugOppositeTransfers` | 1 to 2 and 2 to 1, argument order | The classic two-lock deadlock | **Expected bug**: `liveness bug in hot state 'TransferPending'` |
| `tcBugCycleOfThree` | 1 to 2, 2 to 3, 3 to 1, argument order | A deadlock with no opposite pair | **Expected bug**: same message |
| `tcFixedOppositeTransfers` | The opposite pair, global order | The fix for the classic case | Passes |
| `tcFixedCycleOfThree` | The three-way cycle, global order | A fix that only special-cases reversed pairs fails here | Passes |

## Run

```sh
snappea check tcFixedOppositeTransfers tcFixedCycleOfThree --deny mutation --timeout 20s
snappea check tcBugOppositeTransfers   # succeeds only when the declared bug is found
```

One mutant is suppressed: `t.dst < t.src` → `t.dst <= t.src` in `OrderedWorker`,
because a transfer's two accounts always differ (the spec asserts it).

The negative controls use native `snappea: expect-bug` declarations. A matching
failure passes the test; no failure or a different failure fails it. Search coverage
depends on the budget: claim exhaustion only when the current run reports completion.

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
