---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - Why does a user not see their own write when reads go to replicas?
  - How do session tokens give read-your-writes and monotonic reads on async replicas?
  - How do I state session guarantees as a P spec?
---

# Stale reads from asynchronous replicas

A database has a primary and asynchronous read replicas. Writes go to the primary,
which commits each with the next log sequence number (LSN) and ships the log to every
replica; each replica applies it at its own pace. A load balancer spreads reads over the
replicas. This model has one register (one key).

**Difficulty: medium.** Replica lag is usually milliseconds, so the bug shows up as a
rare "my change vanished" report that no one can reproduce.

**Contract (session guarantees).** For each client session:

- **read-your-writes**: a read returns the session's own last write, or something newer;
- **monotonic reads**: a read never returns something older than an earlier read did;
- reads return only committed values, and every operation completes.

**Atomic primitives**: the primary's commit; a replica applying one log record; a
replica serving one read. Each replica's log stream (`LogShipper`) delivers records in
order with any delay.

## The flaw

A replica answers a read from whatever it has applied so far, so a read routed to a
replica the session's write (or an earlier read's value) has not reached yet returns
older data.

## The counterexamples

`tcBugReadYourWrites` (one session, one replica):

1. The session writes 11 (LSN 1); the replica applies it.
2. The session writes 12 (LSN 2); the primary commits it and ships it, but the log
   stream has not delivered it yet.
3. The session reads; the replica answers 11.

```text
read-your-writes violated: session 1 read value 11, older than its own write of value 12
```

`tcBugMonotonicReads` (session 1 writes once, session 2 reads twice, two replicas):

1. Session 1 writes 11; replica 0 applies it, replica 1's stream is still behind.
2. Session 2 reads from replica 0 and gets 11.
3. Session 2 reads again; the load balancer sends it to replica 1, which answers 0.

```text
monotonic reads violated: session 2 read value 0 after it had already read value 11
```

## Why ordinary tests miss it

Test environments usually have no replicas, or replicas a millisecond behind, and the
test reads long after replication caught up. Integration tests with a single connection
always hit the same replica, which hides monotonic-read violations. Under production
load, lag spikes and the load balancer spreads requests, and users see their edits
vanish or a page flip between old and new.

## The fix

Session tokens. The session remembers the highest LSN it has seen, from its own write
acknowledgements and from every read reply. Each read carries that token, and a replica
answers only once it has applied at least that LSN; until then it holds the read (it
could instead forward it to the primary). Reads still go to any replica, so the read
load stays spread. Read-your-writes holds because the token covers the session's own
writes; monotonic reads hold because it also covers every value the session has read.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/SessionGuarantees.p`](PSpec/SessionGuarantees.p) | `SessionGuarantees`: both guarantees per session, from the commit order; every planned operation completes (hot state) |
| [`PSrc/Primary.p`](PSrc/Primary.p) | The primary and the per-replica `LogShipper`, shared by both designs |
| [`PSrc/Replica.p`](PSrc/Replica.p) | Flawed: replica answers at once; session without a token |
| [`PSrc/SessionReplica.p`](PSrc/SessionReplica.p) | Fixed: replica holds reads until it reaches the token; session with a token |
| [`PTst/Tests.p`](PTst/Tests.p) | Plans, drivers (which also play the load balancer), test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcFixedReadYourWrites` | One session writes twice, then reads; one replica | The read must wait for the second write, not just the first | passes, all schedules |
| `tcFixedMonotonicReads` | Session 1 writes; session 2 reads replica 0, then replica 1 | The token from the first read holds the second | passes, all schedules |
| `tcFixedTwoWriters` | Two sessions write, then read; one replica | A read's token can be two records ahead: the replica keeps holding it after applying one | passes, all schedules |
| `tcBugReadYourWrites` | Flawed design, as `tcFixedReadYourWrites` | **Expected to fail**: `read-your-writes violated` | fails at once |
| `tcBugMonotonicReads` | Flawed design, as `tcFixedMonotonicReads` | **Expected to fail**: `monotonic reads violated` | fails within a few hundred schedules |

## Run

```sh
snappea check StaleReplicaRead.pproj 'tcFixed*' --deny mutation
snappea check StaleReplicaRead.pproj 'tcBug*'    # both exit 1 with the messages above
```

## Bounds and omissions

One key, one or two replicas, two sessions, at most three operations each. The driver
plays the load balancer with a fixed routing (read i goes to replica i), which includes
the unlucky routing; random routing multiplies the schedules without new histories.
Replicas never fail or fall back to an older snapshot; the primary never fails over.
Not modeled: causal consistency across sessions, consistent-prefix across keys,
forwarding to the primary instead of waiting, a timeout on the wait.

Suppressions: the session's read counter only steers routing (any routing is allowed),
and the written values only need to be distinct.

## Testing the implementation

Emit `eWritten` from the primary's commit and `eReadServed` from the replica as it
answers, with the session id. To force the race, pause the replication stream to one
replica (or add an artificial apply delay), write through the session, then read
through it with the load balancer pinned to the paused replica. Without tokens the
read misses the write; with tokens it blocks until you resume replication.
