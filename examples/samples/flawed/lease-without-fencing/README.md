---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - Why is a time-based lease not enough to keep an old leader from writing?
  - How do I model a process pause and lease expiry in P?
  - How do fencing tokens fix it, and what do they not fix?
---

# Lease without fencing: an old leader writes after its lease expired

**Difficulty: medium.**

Several nodes want to be the single writer to a storage service. A lock service
grants a time-limited lease, one holder at a time; each grant has a new, larger
token. A leader checks that its lease is still valid, then writes. When a lease
expires, the next waiting node gets it. The old holder is not told; it can only
check its own clock.

**The flaw in one sentence:** the leader's check and its write are separate steps,
so a leader that pauses between them (GC, page fault, slow disk, descheduled VM)
writes after its lease expired and a new leader has written, and storage accepts it.

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `NoStaleWrites` (safety): one lease holder at a time, and storage never accepts a
  write from an older lease after accepting one from a newer lease. A rejection must
  be justified by such a newer write. Leases are numbered by the spec itself, in
  grant order.
- `NodesFinish` (progress, hot state): every node gets the lease, its planned values
  are decided in order until one is rejected, and it stops, reporting fenced exactly
  when storage rejected it.

Note what the contract does **not** say: "only the current lease holder writes". No
design can promise that, because a write can be in flight while the lease expires.
What storage can promise is that an old leader never overwrites a newer one.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/LockService.p`](PSrc/LockService.p) | Lock service, one-shot lease timer, shared events |
| [`PSrc/Unfenced.p`](PSrc/Unfenced.p) | The flawed design: storage accepts every write |
| [`PSrc/Fenced.p`](PSrc/Fenced.p) | The fix: writes carry the lease token, storage rejects older tokens |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, a crashing node, a scenario, test cases |

**Modeling time.** There is no clock. The lease timer is a machine that sends one
`Expire` at a moment the scheduler picks, so the checker tries expiry at every
point: before the holder hears of its grant, between its writes, after it released.
The pause is the node sending itself `Resume` between the lease check and the
write; any number of other steps can run before it is delivered.

## The counterexample

`snappea check tcBugStaleLeaderWrite` finds the declared bug:

```text
Assertion Failed: stale write: storage accepted value 11 from node 1 under lease 1 after a write under lease 2
```

From `PCheckerOutput/tcBugStaleLeaderWrite/bug.txt` (node 1 plans values 10 and 11,
node 2 plans 20):

1. Node 1 asks for the lease and gets lease 1.
2. Lease 1's timer fires and the lock service ends lease 1. Node 1 is not told.
3. Node 1 checks its lease (from its point of view it is fresh), pauses, writes 10.
   Storage accepts it.
4. Node 2 asks for the lease and gets lease 2, writes 20, storage accepts it, and
   node 2 releases the lease.
5. Node 1 resumes and writes its second value, 11. Storage accepts it: the old
   leader has overwritten the new leader's data. `NoStaleWrites` fails.

## Why ordinary tests miss it

The window is a pause longer than the remaining lease, which tests never produce:
leases are seconds, test pauses are microseconds. In production, a stop-the-world GC
or a VM migration does it a few times a year, and the result is silent corruption,
not an error. Checking the clock more carefully does not help: the pause can come
after the last check, inside the network stack.

## The fix

Fencing tokens. Each write carries the token of the lease it was made under, and
storage stores the largest token it has accepted. A write with a smaller token comes
from a replaced leader, so storage rejects it and the node steps down. The check is
done by the one party that orders all writes, so no pause in the node can bypass it.

What the fix does not do: an expired leader's write can still be accepted if no
newer leader has written yet (that is allowed by the contract: nothing was
overwritten). Storage must support a conditional write; a plain blob store cannot be
fenced this way.

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugStaleLeaderWrite` | Two nodes, unfenced storage | The stale write | **Expected bug**: `stale write: storage accepted value 11 from node 1 under lease 1 after a write under lease 2` |
| `tcFixedTwoLeaders` | Two nodes, fenced; node 1 writes twice | Old leader fenced; a leader's second write under the same token is still accepted | Passes |
| `tcFixedLeaderCrashes` | A node takes the lease and dies; another must lead | Lifecycle boundary: expiry is the only way out | Passes |

The `OldLeaderFenced` scenario confirms a rejection happens in `tcFixedTwoLeaders`.

## Run

```sh
snappea check tcFixedTwoLeaders tcFixedLeaderCrashes --deny mutation --timeout 20s
snappea check tcBugStaleLeaderWrite    # succeeds only when the declared bug is found
```

Removing release handling is indistinguishable from waiting for expiry in this
untimed model. Accepting a stale timer is likewise indistinguishable from the
current lease's own timer firing. The corresponding mutations are suppressed
individually; the protocol still checks tokens. Use a timed model to check the
lease duration itself.

The negative controls use native `snappea: expect-bug` declarations. A matching
failure passes the test; no failure or a different failure fails it. Search coverage
depends on the budget: claim exhaustion only when the current run reports completion.

## Bounds and omissions

Two nodes, at most two writes each, one lease per node. Not modeled: lease renewal,
clock drift (irrelevant once storage fences), several storage replicas (each must
fence, or a quorum must), a crashed node coming back.

## Testing the implementation

Emit `LeaseGranted`/`LeaseEnded` from the lock client and `WriteAccepted`/
`WriteRejected` from storage. To force the race, give the lock a very short lease,
block the first leader after its lease check (a hook or a debugger breakpoint), let
the lease expire and a second leader write, then unblock: unfenced storage takes the
stale write, fenced storage rejects it.
