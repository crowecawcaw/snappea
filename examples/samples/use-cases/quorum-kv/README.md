---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a quorum-replicated key-value store in P?
  - How do I check read-after-write without trusting the store's versions?
  - Why does R + W > N matter, and how do I show it fails otherwise?
---

# Quorum-replicated key-value store

One key on N = 3 replicas. A write goes to every replica and returns after W = 2 acks;
a read asks every replica and returns the highest-version value among the first R = 2
answers. A replica stores a write only if its version is higher than the stored one.

**Versions** come from the writer's read phase: read a quorum, take the highest counter,
write counter + 1, with the writer id as tie-break (version = counter * 10 + id). Any
completed write is on W replicas and every read quorum overlaps it, so a write that
starts after another completed gets a higher version, with no clock synchronization.
Client-supplied timestamps would need synchronized clocks, and a slow clock would make
a later write lose.

**Contract** (`ReadAfterWrite`, a regular register, stated in real-time order only; the
spec never sees versions)

- A read returns the initial value or a value whose write started before it returned.
- A read that starts after write B completed never returns the initial value, nor the
  value of a write A that completed before B started.
- Progress (`OperationsReturn`): every planned read and write returns, also with one
  replica down for good.

**Atomic primitives**: replica read, and replica conditional write (store if newer, then
ack). Everything else is client logic that interleaves.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ReadAfterWrite` (safety), `OperationsReturn` (progress) |
| [`PSrc/Replica.p`](PSrc/Replica.p) | The replica |
| [`PSrc/Client.p`](PSrc/Client.p) | `QuorumRead`, `QuorumWrite`, `Writer` (read phase + write) and `Reader` |
| [`PTst/Network.p`](PTst/Network.p) | `Deliver`: a courier per replica write, so a write can arrive late |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers (`Clients`, `Sequence`) and the test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcWriteThenRead` | A write, then a read | Read quorum overlaps the acked replicas in exactly one, while the third write is in flight |
| `tcWriteWriteRead` | Writer 1, then writer 2, then a read | The second writer's read phase must see the first version and go above it |
| `tcOneReplicaDown` | Read, write, read with the third replica dead | Quorums make progress without it, and still see the write |
| `tcWriteAndReads` | One writer and one reader concurrently | Reads before, during and after the write |
| `tcTwoWriters` | Two writers and a reader concurrently | Same counter from both read phases; the writer id breaks the tie; late writes |
| `tcBugReadOne` | Write then read with R = 1 (R + W = N) | **Expected bug**: `reader 1 returned the initial value after write 1 completed` |

## Run

```sh
snappea check tcWriteThenRead tcWriteWriteRead tcOneReplicaDown tcWriteAndReads tcTwoWriters --deny mutation
snappea check tcBugReadOne      # passes when it finds the stale read above
```

`tcWriteThenRead` (22k schedules) and `tcOneReplicaDown` explore every schedule; the
concurrent tests and `tcWriteWriteRead` are sampled.

## Bounds and omissions

One key, three replicas, at most two writes and two reads, each writer and reader
machine doing one operation (with more, answers must carry an operation id). Not
modeled: read repair, hinted handoff, replica crash and recovery with state loss,
sloppy quorums, deletes. Regular, not atomic (linearizable): two reads concurrent with
a write may see new then old. Making it atomic needs read repair (write back what you
read before returning).

## What building it taught

- **P delivers in send order per receiver.** A message is enqueued when sent, so a
  write to replica 3 sent before a read can never be handled after it. The first
  version could not show the R = 1 bug at all. `Deliver` sends each replica write
  through its own courier machine, which models a write delayed past later requests.
- Sending to exactly a chosen quorum, instead of to all N, hid the "late write"
  case and the need for the version check on the replica.
- Waiting for all three acks instead of W went unnoticed until `tcOneReplicaDown`.

## Testing the implementation

Emit `WriteStarted`/`WriteDone` and `ReadStarted`/`ReadDone` from the client
library around each call, with distinct values per write. To force the overlap case,
use a test transport that delays the write to one replica until after the next read
has been answered by that replica and one other.

## Reuse the delivery building block

The project imports `Network.pproj` from Snappea's guide sources and uses
`BBDelivery` for independently scheduled delivery. It does not add loss or duplicate
messages by itself. When copying this sample outside the repository, run
`snappea guide example building-blocks --out vendor`, commit the exported files,
and point the project's `IncludeProject` at `vendor/building-blocks/Network.pproj`.

Negative controls declare `// snappea: expect-bug "message"`. A matching bug
passes the test; a different failure or no discovered bug fails it. Check their
failure identity across seeds with `snappea check 'tcBug*' --no-mutation
--expect-bug-seeds 1,2,3 --workers 1 --timeout 10s`. Passing negative controls do
not fill coverage gaps or detect mutants in the correct design.

Search is complete only when this run reports `all N schedules` (or JSON
`search_complete: true`). A timeout, seed, or historical schedule count does not
guarantee completeness. An incomplete passing run is sampled evidence.
