---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a Kafka-style consumer group that rebalances while consumers commit?
  - Why do commits carry a generation, and what does fencing protect?
  - How does an expelled consumer (a zombie) find out and rejoin?
---

# Consumer group rebalance with generation fencing

A coordinator assigns partitions to the group's consumers. On every membership change
(join, leave, session expiry) it starts a new generation, reassigns, and sends each
member its partitions. Consumers fetch the committed offset of each partition they
own, process records one at a time, and commit the next offset after each record,
tagged with their generation. Processing comes before the commit, so a record can be
processed twice (at-least-once), never skipped.

**Contract**

- Committed offsets never go back, and only cover records someone processed.
- A commit or offset fetch is accepted only from the partition's owner in the current
  generation; one from that owner is never rejected. A consumer is told a commit
  succeeded only if it was accepted.
- Each generation lists each member once and gives every partition to exactly one
  member. Within a generation a partition is held by at most one consumer: the one
  it was assigned to.
- A consumer whose commit or fetch was rejected stops working on that assignment.
- Progress (hot state): every record of every partition is eventually committed, and
  a live consumer that was fenced out of the group eventually rejoins.

**Atomic primitives**: each coordinator handler (join, leave, expire, fetch, commit)
is one step on the group's state, as when one broker owns the group. Consumers and
the coordinator talk only through messages, which arrive in order per sender.

## Design decisions the model checks

- **Fence by generation and owner**, in the coordinator, at the commit. A consumer
  that missed a rebalance still has commits in flight; they are rejected.
- **Drop stale replies.** A reply tagged with an older generation is ignored: the
  newer assignment always arrives first (in-order delivery), so a consumer that is
  still a member never rejoins needlessly (`GroupSafety` fails if a generation lists
  a member twice).
- **Fence the offset fetch too.** The checker found that an expelled consumer whose
  new partition was already fully committed never commits again, so it never learned
  it was expelled and never rejoined. Fencing the fetch closes that. (A real client
  also learns through heartbeats, which are not modeled.)
- **A rejected request means "rejoin".** The consumer sends a join and waits for an
  assignment.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `GroupSafety` and `GroupProgress` (hot state) |
| [`PSrc/Coordinator.p`](PSrc/Coordinator.p) | Membership, generations, assignment, fenced fetch and commit |
| [`PSrc/Consumer.p`](PSrc/Consumer.p) | Fetch, process, commit; rejoin when fenced; clean shutdown |
| [`PSrc/UnfencedCoordinator.p`](PSrc/UnfencedCoordinator.p) | Negative control: accepts commits whatever their generation |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the `ZombieFenced` scenario, test cases |

## Test cases

All use two consumers, and two partitions unless stated. Times are for one worker
(`-j 1`); an exhaustive result needs that test case's time within `--timeout`.

| Test | Drives | Why it matters | Result |
| --- | --- | --- | --- |
| `tcJoinRebalance` | Both consumers start together, 2 records per partition | The core race: consumer 1 is processing partition 1 when it moves to consumer 2; its in-flight commits are fenced | passes, exhaustive (about 6 s) |
| `tcLeaveRebalance` | Consumer 2 shuts down at any moment, 1 record each | Leave: consumer 1 takes over from the last committed offset | passes, exhaustive (about 20 s) |
| `tcCrashedMember` | Member 2 joins, gets both partitions and crashes (the driver stands in for it); consumer 1 starts while member 2's session expires | Expiry is what frees a dead member's partitions, before or after consumer 1 joins | passes, exhaustive |
| `tcZombieExpired` | One partition, 1 record; consumer 2's session expires at any moment while it runs | If consumer 2 owned the partition, its fetch or commit is fenced, consumer 1 takes over, and consumer 2 rejoins | passes, exhaustive (about 4 s) |
| `tcBugNoGenerationCheck` | `UnfencedCoordinator`, a zombie, 2 records each | **Expected to fail**: `commit from generation N accepted in generation M` | fails |

In the negative control the stale commit is accepted after the new owner committed
further, so the next thing to break would be `partition 1 committed offset went back`;
the generation check reports it first.

The `ZombieFenced` scenario confirms a consumer was rejected and a new generation
followed.

## Run

```sh
snappea check tcJoinRebalance tcLeaveRebalance tcCrashedMember tcZombieExpired --timeout 30s
snappea check mutation tcJoinRebalance tcLeaveRebalance tcCrashedMember tcZombieExpired
snappea check tcBugNoGenerationCheck   # exits 1: commit from generation ... accepted
```

## Bounds and omissions

One or two partitions of one or two records, two consumers, one join, leave or
expiry beyond the initial joins. Not modeled: heartbeats (session expiry is a driver
action), cooperative (incremental) rebalancing, committing before a revoke, consumer
crashes after processing (the next owner reprocesses; at-least-once covers it),
and coordinator failover. A consumer that is expelled while idle (it finished its
partitions, or got none) sends no fetch or commit, so it never learns and never
rejoins; a real client learns through heartbeats. The progress spec therefore requires
a rejoin only from a consumer that was fenced, and `tcZombieExpired` reaches this case
without failing.

Suppressions: the coordinator's `eCommitRejected`, `eFetchRejected` and `eFetchServed`
announcements and the consumer's `eCommitAcked` are observation only; removing
`goto Idle` after a rejoin is equivalent (`Working` adopts the next assignment too).

## Testing the implementation

Emit `eGeneration` when the coordinator completes a rebalance, `eCommitted`,
`eCommitRejected`, `eFetchServed` and `eFetchRejected` when it decides,
`eAdopted`, `eProcessed` and `eCommitAcked` from the consumer. To force the zombie,
pause a consumer between processing a record and committing it, expire its session,
let the other consumer take the partition and commit further, then release the
paused consumer: its commit must be rejected and it must rejoin.
