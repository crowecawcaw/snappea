---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model message loss, duplication and reordering in P?
  - How do I keep a lossy-network model small enough to check exhaustively?
  - How do I check retransmission and deduplication together?
---

# Unreliable network: loss, duplicates and reordering

P delivers every `send` exactly once, and two messages from one machine to another in
the order they were sent. Real networks do neither. This sample routes messages through
a `Network` machine that loses, duplicates and reorders a bounded number of packets, and
checks that a retransmitting sender and a deduplicating receiver still deliver each
message to the application exactly once.

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

## Key lines

The network ([`PSrc/Network.p`](PSrc/Network.p)) picks a packet and a fault:

```p
p = choose(bag);
if (dropsLeft > 0 && $) {
  dropsLeft = dropsLeft - 1; lost = true; bag -= (p);
} else if (duplicatesLeft > 0 && $) {
  duplicatesLeft = duplicatesLeft - 1; send p.dest, p.ev, p.msg;   // stays in the bag
} else {
  bag -= (p); send p.dest, p.ev, p.msg;
}
```

When the bag empties after a loss, it sends the sender `eTimeout`. That encodes an
assumption: the retransmission timeout is longer than any delivery, so it fires only
when something was really lost (see `09-timers` for timeouts that race replies).

The receiver ([`PSrc/Endpoints.p`](PSrc/Endpoints.p)) delivers each id once but
acknowledges every copy, because the first acknowledgement may have been lost:

```p
on eData do (d: tMsg) {
  if (!(d.id in delivered)) { delivered += (d.id); announce eDelivered, d.id; }
  send net, eNetSend, (dest = d.from, ev = eAck, msg = (from = this, id = d.id));
}
```

The spec ([`PSpec/Contract.p`](PSpec/Contract.p)) requires each planned message to be
delivered once, acknowledged only after delivery, and (hot state) every message to be
delivered and acknowledged by the end.

## Test cases

| Test | Drives | Expected |
| --- | --- | --- |
| `tcLostPackets` | Two messages, up to two lost packets (data or acks) | Passes; all schedules (about 100) |
| `tcDuplicatedPacket` | Two messages, one duplicated packet; the copy can arrive after the next message | Passes; all schedules (about 550) |
| `tcBugNoDedup` | The duplicate test with a receiver that does not deduplicate | **Fails**: `message 1 delivered twice` |

```sh
snappea check tcLostPackets tcDuplicatedPacket --deny mutation
snappea check tcBugNoDedup        # exits 1
```

## What the checker shows

```text
ok    tcLostPackets (all 97 schedules, 0.03s)
      note: every schedule reaches nearly the same states, so this test checks little more than one run would
      Driver limits (held in all 97 schedules):
        Network sends eTimeout at most twice
        ...
ok    tcDuplicatedPacket (all 585 schedules, 0.23s)
Coverage of all test cases: 3/3 states, 64/64 statements, 18/18 branches, 9/9 event handlers, ...
Mutation testing: 30 of 30 mutants detected (0.2s)
```

The note on `tcLostPackets` is expected: the sender has one packet in flight at a time,
so losses change *which* packet is retransmitted but leave little to interleave. The
driver limits state the fault bound (`eTimeout at most twice`): two losses, no more.

## Common mistake: one test with every fault

The first version ran one test with one loss *and* one duplicate. It did not finish in
90 seconds (29,000 schedules over only 837 distinct states), while each fault alone
finishes in under a second. Faults multiply the interleavings. Give each fault class
its own test with a small budget, and add a combined test only for a combination the
design treats specially.

## Try this

Acknowledge only the first copy (move the `send net, eNetSend, (... ev = eAck ...)` into
the `if`). When the acknowledgement is lost, the retransmission is ignored and the
sender waits forever:

```text
FAIL  tcLostPackets (bug found after 2 schedules, 0.01s)
      PImplementation.ExactlyOnce detected liveness bug in hot state 'Delivering' at the end of program execution.
```

## Bounds and omissions

One sender, one receiver, two messages sent stop-and-wait, at most two losses or one
duplicate per test. Timeouts never fire while packets are in flight. Not modeled:
corrupted packets, network partitions longer than the retry budget, several senders
sharing a receiver's deduplication table, and bounding that table's size.

## Testing the implementation

Emit `eDelivered` where the receiver hands a message to the application and `eAcked`
where the sender accepts an acknowledgement. Put a fault-injecting proxy (or a fake
transport) between them that drops the first acknowledgement and replays a stale copy
of message 1 after message 2.
