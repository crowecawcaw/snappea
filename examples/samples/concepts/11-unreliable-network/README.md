---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model message loss, duplication and reordering in P?
  - How do I keep a lossy-network model small enough to check exhaustively?
  - How do I check retransmission and deduplication together?
---

# Unreliable network: exactly-once delivery over loss, duplicates and reordering

P delivers every `send` exactly once, and two messages from one machine to another in
the order they were sent. Real networks do neither. Here messages go through a
`Network` machine that loses, duplicates and reorders a bounded number of packets; a
sender retransmits until acknowledged, and a receiver deduplicates by message id, so
the application still gets each message exactly once.

**Contract:**

- Every delivered message was planned, and none is delivered twice (`ExactlyOnce`).
- A message is acknowledged only after it was delivered, and only once
  (`ExactlyOnce`).
- Every planned message is eventually acknowledged (`MessagesAcknowledged`).

**Atomic primitives:** each receiver handler is one "check the dedup table, deliver,
acknowledge" step (a transaction on the dedup table); each sender handler is one step of
the sender's loop. The network is the environment.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Endpoints.p`](PSrc/Endpoints.p) | `Sender` (stop and wait, retransmit) and `Receiver` (dedup, ack every copy) |
| [`PSrc/Network.p`](PSrc/Network.p) | The network: a bag of packets with loss and duplicate budgets (a driver) |
| [`PSrc/NoDedupReceiver.p`](PSrc/NoDedupReceiver.p) | Negative control: trusts the network to deliver once |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ExactlyOnce` (safety) and `MessagesAcknowledged` (progress) |
| [`PTst/Tests.p`](PTst/Tests.p) | One driver per fault class, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcLostPackets` | Two messages, up to two lost packets (data or acknowledgements) | A lost acknowledgement must be repaired by retransmission | Passes |
| `tcDuplicatedPacket` | Two messages, one duplicated packet; the copy can arrive after the next message | Deduplication and reordering together | Passes |
| `tcBugNoDedup` | The duplicate test with a receiver that does not deduplicate | A duplicate reaches the application | Expected bug: `delivered twice` |

The expected message has no id: the duplicate can be message 1 or message 2, depending
on the schedule.

## The concept

Put the faults in the environment, not in the design:

- **A network machine** holds packets in flight in a bag and delivers them one at a time
  in any order (`choose(bag)`): that is reordering.
- **Loss** removes a packet without delivering it; **duplication** delivers a copy and
  keeps the packet in flight.
- **Budgets** (`drops`, `duplicates`) bound the faults, so the search is finite. Without
  a bound, "the network loses every retransmission" is a legal schedule, and the
  checker would report a liveness failure no design can fix.
- **The network is a driver** (`// snappea: driver`): its `$` choices are test input,
  and mutation testing does not mutate it.

Use this when a design claims at-least-once, at-most-once or exactly-once delivery, or
relies on retries, acknowledgements, sequence numbers or idempotency keys.

A lighter alternative is a helper function `if ($) { send t, e, p; }` for loss only.
It cannot reorder, and its loss is unbounded, so prefer the machine once retries matter.

## The key lines

The network ([`PSrc/Network.p`](PSrc/Network.p)) picks a packet and a fault:

```p
p = choose(bag);
if (dropsLeft > 0 && $) {
  dropsLeft = dropsLeft - 1; lost = true; bag -= (p);
} else if (duplicatesLeft > 0 && $) {
  duplicatesLeft = duplicatesLeft - 1; send p.destination, p.message, p.payload;   // stays in the bag
} else {
  bag -= (p); send p.destination, p.message, p.payload;
}
```

When the bag empties after a loss, it sends the sender `Timeout`. That encodes an
assumption: the retransmission timeout is longer than any delivery, so it fires only
when something was really lost (see [09-timers](../09-timers/README.md) for timeouts
that race replies).

The receiver ([`PSrc/Endpoints.p`](PSrc/Endpoints.p)) delivers each id once but
acknowledges every copy, because the first acknowledgement may have been lost:

```p
on Data do (d: DataMessage) {
  if (!(d.id in delivered)) { delivered += (d.id); announce Delivered, d.id; }
  send net, NetSend, (destination = d.from, message = Ack, payload = d.id);
}
```

The specs ([`PSpec/Contract.p`](PSpec/Contract.p)) separate safety from progress.
`ExactlyOnce` stays active after all acknowledgements, so a late duplicate fails the
same assertion. `MessagesAcknowledged` stays hot until every planned message is
acknowledged; it relies on `ExactlyOnce` to check that those acknowledgements are valid
and distinct. Select both specs in every test.

The checker's note on `tcLostPackets` is expected: "every schedule reaches nearly the
same states". The sender has one packet in flight at a time, so losses change *which*
packet is retransmitted but leave little to interleave. The driver limits state the
fault bound (`Network sends Timeout at most twice`): two losses, no more.

## Common mistake: one test with every fault

Faults multiply the interleavings. One loss and one duplicate together explore several
times the schedules of one duplicate alone, and each extra fault multiplies again.
Give each fault class its own test with a small budget, and add a combined test only
for a combination the design treats specially.

## Try this

Acknowledge only the first copy (move the `send net, NetSend, (... message = Ack ...)`
into the `if`). When the acknowledgement is lost, the retransmission is ignored and the
sender waits forever:

```text
MessagesAcknowledged detected liveness bug in hot state 'Awaiting' at the end of program execution.
```

## Run

```sh
cd examples/samples/concepts/11-unreliable-network
snappea check --deny mutation
```

`tcBugNoDedup` counts as passing when it fails with the declared message.

## Bounds and omissions

One sender, one receiver, two messages sent stop-and-wait, at most two losses or one
duplicate per test. Timeouts never fire while packets are in flight. Not modeled:
corrupted packets, network partitions longer than the retry budget, several senders
sharing a receiver's deduplication table, and bounding that table's size.

For other models, import the [network building block](../../../../crates/snappea/src/cli/guide/examples/building-blocks/README.md)
instead of copying this one. This tutorial keeps its small transport visible because
its timeout contract is different: it fires only after the packet bag drains. The
reusable block allows independently delayed delivery and does not supply a timer.

## Testing the implementation

Emit `Delivered` where the receiver hands a message to the application and `Acked`
where the sender accepts an acknowledgement. Put a fault-injecting proxy (or a fake
transport) between them that drops the first acknowledgement and replays a stale copy
of message 1 after message 2.
