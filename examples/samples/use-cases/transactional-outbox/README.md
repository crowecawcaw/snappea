---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a transactional outbox with a relay that can crash?
  - Why does the consumer need to deduplicate even with an outbox?
  - What goes wrong with a dual write (commit, then publish)?
---

# Transactional outbox

An order service writes the order and an outbox row in one database transaction. A
relay reads unsent outbox rows, publishes each to a broker, waits for the broker's
ack, then marks the row sent. The relay can crash between the publish and the mark,
and two relays can run at once, so the broker can receive an order twice. The
consumer deduplicates by message id.

**Contract**

- Nothing is published or applied for an order whose transaction did not commit.
- An outbox row is marked sent only after the broker has the message.
- The consumer applies each order at most once, however many copies arrive.
- Every committed order is eventually applied (so published), and every outbox row
  is eventually marked sent (the outbox drains).

**Atomic primitives**: placing an order (order row + outbox row in one transaction,
or nothing); fetching unsent rows, which registers the relay for a wake-up if there
are none (LISTEN/NOTIFY); marking one row sent; the consumer's apply-and-record-id
(one local transaction). Publish and mark are separate, so a crash can fall between.

The model makes "fetch, and register for a wake-up if empty" one step. In Postgres
that means running `LISTEN` before the `SELECT` on the same connection; listening
after an empty `SELECT` loses a commit that lands in between, and the relay sleeps
with an unsent row (see the `flawed/lost-wakeup` sample). A poll with a timeout
avoids the problem at the cost of latency.

**Two relays, deliberately.** The model runs two relays at once rather than
promising one: during a rolling deploy, or when a supervisor restarts a relay it
thinks is dead while the old process still runs, two relays exist, and preventing
that needs a fenced lease (see the lease-lock-fencing sample). The outbox is
at-least-once either way, so the design tolerates duplicates instead.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `AppliedOnce` (safety), `CommittedOrdersDelivered` and `OutboxDrains` (progress) |
| [`PSrc/OrderDb.p`](PSrc/OrderDb.p) | Orders and outbox rows: place, fetch, mark sent |
| [`PSrc/Relay.p`](PSrc/Relay.p) | The relay: fetch, publish, wait for ack, mark; sleeps on an empty outbox |
| [`PSrc/Consumer.p`](PSrc/Consumer.p) | Deduplicating consumer |
| [`PSrc/Broken.p`](PSrc/Broken.p) | Negative controls: dual write, and a consumer without dedup |
| [`PTst/Tests.p`](PTst/Tests.p) | Broker (environment), drivers that also restart crashed relays, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoRelays` | Order 1 committed, then two relays | Both publish it; the consumer gets a duplicate; marks race with publishes | pass, exhaustive |
| `tcRelayCrashAfterPublish` | One relay crashes after the ack, before the mark; a new one starts. Order 2 aborts | The row is republished from the table; the duplicate is dropped; the aborted order is never published | pass, exhaustive |
| `tcOrdersWhileRelayRuns` | One relay starts first; two orders commit while it sleeps or works | Commits must wake a sleeping relay; after a batch the relay must fetch again | pass, exhaustive |
| `tcBugDualWrite` | `DualWriteOrderService`: commit, maybe crash, then publish | Negative control: the event is lost for good | fails: `CommittedOrdersDelivered detected liveness bug in hot state 'Undelivered'` |
| `tcBugNoDedup` | `tcTwoRelays` with `NaiveConsumer` | Negative control | fails: `order 1 applied twice` |

## Run

```sh
snappea check tcTwoRelays tcRelayCrashAfterPublish tcOrdersWhileRelayRuns --timeout 30s
snappea check tcBugDualWrite tcBugNoDedup   # each exits 1 as above
```

With `--timeout`, each passing test explores every schedule in about two seconds or
less. Without it, `tcTwoRelays` may stop early once coverage stops growing.

## Bounds and omissions

At most two orders and two relays, one injected crash. The broker never loses an
acked message and delivers in the order it receives. Relays keep polling forever in
production; in the model a relay with nothing to do sleeps, and a test ends when
everything is idle. The crash is injected by a flag in the relay (test scaffolding);
the driver plays the supervisor that starts a replacement. Not modeled: broker
redelivery after a consumer crash (the consumer's apply and record must stay one
transaction), per-key ordering across relays (two relays can publish orders out of
order), outbox row cleanup, and the dedup table's retention window (a duplicate
arriving after its id is purged would be applied twice).

## Testing the implementation

Emit `eOrderCommitted`/`eOrderAborted` after the order transaction, `eMarkedSent`
after the mark, `eApplied` after the consumer's transaction, and `eBrokerReceived`
from a test broker; check them against the three specs. To force the crash, make the
test broker's ack kill the relay process (or throw before the mark), restart it, and
check the second publish is deduplicated. Run two relay instances against one
database to get duplicates without a crash.
