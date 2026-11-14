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
write counter + 1, with the writer id as tie-break (version = `(counter, writer)`). Any
completed write is on W replicas and every read quorum overlaps it, so a write that
starts after another completed gets a higher version, with no clock synchronization.
Client-supplied timestamps would need synchronized clocks, and a slow clock would make
a later write lose.

**Contract**

- `ReadAfterWrite` (a regular register, stated in real-time order only; the spec never
  sees versions): a read returns the initial value or a value whose write started
  before it returned. A read that starts after write B completed never returns the
  initial value, nor the value of a write A that completed before B started.
- `ReplicasKeepNewest`: after each write, a replica holds the newest version it was
  offered (higher counter, then higher writer id). Once every write has reached every
  live replica, all replicas hold the same version and value (hot state).
- `OperationsReturn` (hot state): every planned read and write returns, also with one
  replica down for good.

**Atomic primitives**: replica read, and replica conditional write (store if newer, then
ack). Everything else is client logic that interleaves.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ReadAfterWrite` and `ReplicasKeepNewest` (safety and convergence), `OperationsReturn` (progress) |
| [`PSrc/Replica.p`](PSrc/Replica.p) | The replica |
| [`PSrc/Client.p`](PSrc/Client.p) | `QuorumRead`, `QuorumWrite`, `Writer` (read phase + write) and `Reader` |
| [`PTst/Network.p`](PTst/Network.p) | `Deliver`: a courier per replica write, so a write can arrive late |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers (`Clients`, `Sequence`) and the test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcWriteThenRead` | A write, then a read | Read quorum overlaps the acked replicas in exactly one, while the third write is in flight. All schedules | Passes |
| `tcWriteWriteRead` | Writer 1, then writer 2, then a read | The second writer's read phase must see the first version and go above it. Sampled | Passes |
| `tcOneReplicaDown` | Read, write, read with the third replica dead | Quorums make progress without it, and still see the write. All schedules | Passes |
| `tcWriteAndReads` | One writer and one reader concurrently | Reads before, during and after the write. Sampled | Passes |
| `tcTwoWriters` | Two writers and a reader concurrently | Same counter from both read phases; the writer id breaks the tie so replicas converge; late writes. Sampled | Passes |
| `tcBugReadOne` | Write then read with R = 1 (R + W = N) | The read reaches only the replica the write has not reached | Expected bug: `reader 1 returned the initial value after write 1 completed` |

## What building it taught

- **P delivers in send order per receiver.** A message is enqueued when sent, so a
  write to replica 3 sent before a read can never be handled after it. The first
  version could not show the R = 1 bug at all. `Deliver` sends each replica write
  through its own courier machine, which models a write delayed past later requests.
- Sending to exactly a chosen quorum, instead of to all N, hid the "late write"
  case and the need for the version check on the replica.
- Waiting for all three acks instead of W went unnoticed until `tcOneReplicaDown`.
- A regular register does not care which of two concurrent writes wins, so
  `ReadAfterWrite` alone let the tie-break go unchecked. `ReplicasKeepNewest` checks
  the replica's rule and that replicas converge.

## Reuse the delivery building block

The project imports `Network.pproj` from Snappea's guide sources and uses
`BBDelivery` for independently scheduled delivery. It does not add loss or duplicate
messages by itself. When copying this sample outside the repository, run
`snappea guide example building-blocks --out vendor`, commit the exported files,
and point the project's `IncludeProject` at `vendor/building-blocks/Network.pproj`.

## Run

```sh
snappea check tcWriteThenRead tcWriteWriteRead tcOneReplicaDown tcWriteAndReads tcTwoWriters --deny mutation
snappea check tcBugReadOne      # passes when it finds the stale read above
```

## Bounds and omissions

One key, three replicas, at most two writes and two reads, each writer and reader
machine doing one operation. The client library waits in `receive`, so each machine
has one operation in flight and every answer it gets belongs to that operation; a
client that runs several operations must tag answers with an operation id. Not
modeled: read repair, hinted handoff, replica crash and recovery with state loss,
sloppy quorums, deletes. Regular, not atomic (linearizable): two reads concurrent with
a write may see new then old. Making it atomic needs read repair (write back what you
read before returning).

One suppression: the replica's writer-id comparison may become `>=`. A version names
one write, so an equal version carries the value already stored.

## Testing the implementation

Emit `WriteStarted`/`WriteDone` and `ReadStarted`/`ReadDone` from the client
library around each call, with distinct values per write, and `Stored` from each
replica after its conditional write. To force the overlap case, use a test transport
that delays the write to one replica until after the next read has been answered by
that replica and one other.
