# Consumer group rebalance with generation fencing

A coordinator assigns partitions to the group's consumers. On every membership change
(join, leave, session expiry) it starts a new generation, reassigns, and sends each
member its partitions. Consumers fetch the committed offset of each partition they
own, process records one at a time, and commit the next offset after each record,
tagged with their generation. Processing comes before the commit, so a record can be
processed twice (at-least-once), never skipped.

## Contract

- Committed offsets never go back, and only cover records someone processed.
- A commit or offset fetch is accepted only from the partition's owner in the current
  generation; one from that owner is never rejected. A served fetch returns the
  committed offset. A consumer is told a commit succeeded only if it was written.
- Each generation lists each member once and gives every partition to exactly one
  member (none when the group is empty). Within a generation a partition is held by
  at most one consumer: the one it was assigned to.
- A consumer whose commit or fetch was rejected stops working on that assignment.
- Every fetch and commit gets a reply, every record of every partition is eventually
  committed while the group has a live member, and a live consumer that was fenced
  out of the group eventually rejoins.

## Atomic steps

Each coordinator handler (join, leave, expire, fetch, commit) is one step on the
group's state, as when one broker owns the group. Consumers and the coordinator talk
only through messages, which arrive in order per sender.

## What can happen

Consumers join at any time. A consumer retries a join that timed out, so the
coordinator can see the same join twice. A consumer can shut down cleanly at any
moment (it leaves the group). A member's session can expire at any moment: because
the member crashed, or while it is still running, in which case it carries on as a
zombie until one of its requests is rejected. Any member can be the one that leaves
or expires, including the only one. The coordinator does not crash, and messages are
not lost.

Not in scope: heartbeats (the session timer is part of the environment), cooperative
rebalancing, coordinator failover. A consumer expelled while idle sends no request, so
it is not required to rejoin.
