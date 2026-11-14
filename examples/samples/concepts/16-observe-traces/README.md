---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I check my application's event logs against the spec I model-checked?
  - How do partition keys, --check-hot and --strict work in snappea observe?
  - How do I write a spec that works for both the model and real logs?
---

# Observe: one order-lifecycle spec for the model and for real logs

Model checking explores a model's schedules. It says nothing about whether the real
service behaves like the model. `snappea observe` closes that gap: it runs the **same
spec monitor** over events your application logged. Here an order can be paid,
shipped or cancelled, and a cancel races the warehouse's shipping; one spec checks the
model and four JSON-lines logs, one valid and three broken.

**Contract:**

- An order is paid only after it was placed and a payment was requested, for its full
  amount, and never if a cancel was requested before any payment (`OrderLifecycle`).
- A paid order ends shipped or refunded in full; an unpaid order can only be cancelled,
  with nothing to refund (`OrderLifecycle`).
- A placed order ends paid or cancelled, and a paid order ends shipped or refunded
  (`OrderLifecycle` is hot in `Placed` and `Paid`).

**Atomic primitives:** each order-service handler decides one request (place, pay,
cancel, ship) for the order, like one database transaction on the order row.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/OrderLifecycle.p`](PSpec/OrderLifecycle.p) | The spec, used by `snappea check` and `snappea observe` |
| [`PSrc/OrderService.p`](PSrc/OrderService.p) | The modeled service and warehouse; cancel races shipping |
| [`PTst/Tests.p`](PTst/Tests.p) | A customer that cancels before paying, or pays and may cancel |
| [`logs/valid.jsonl`](logs/valid.jsonl) | Three orders: shipped; paid then refunded; cancelled before payment |
| [`logs/paid-after-cancel.jsonl`](logs/paid-after-cancel.jsonl) | Cancel requested before payment, but the order was paid and shipped |
| [`logs/never-shipped.jsonl`](logs/never-shipped.jsonl) | An order paid and never shipped or refunded |
| [`logs/misspelled.jsonl`](logs/misspelled.jsonl) | `OrderShiped` instead of `OrderShipped` |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcCancelRacesShipping` | One customer: cancel then pay (a second browser tab), or pay and maybe cancel while the warehouse ships | The cancel must win before payment; after payment exactly one of ship or refund happens | Passes |

The observe commands below are checked by hand, not by `check_samples.py`:

| Command | Expected |
| --- | --- |
| `snappea observe --spec OrderLifecycle --check-hot --strict logs/valid.jsonl` | exit 0 |
| `snappea observe --spec OrderLifecycle --check-hot --strict logs/paid-after-cancel.jsonl` | exit 1: `order 21 paid although it was cancelled before payment was requested` |
| `snappea observe --spec OrderLifecycle logs/never-shipped.jsonl` | exit 0: without `--check-hot` the open order is not a failure |
| `snappea observe --spec OrderLifecycle --check-hot logs/never-shipped.jsonl` | exit 1: `OrderLifecycle detected liveness bug in hot state 'Paid'` |
| `snappea observe --spec OrderLifecycle --check-hot --strict logs/misspelled.jsonl` | exit 2: `Unknown event 'OrderShiped'` |
| `snappea observe --spec OrderLifecycle --strict --key 99 logs/valid.jsonl` | exit 2: nothing checked |

## The concept

- **One spec, two uses.** `OrderLifecycle` observes `OrderPlaced`, `OrderPaid`,
  `OrderShipped`, `OrderCancelled` and the client's `PayRequested` and
  `CancelRequested`. The model announces them; the application logs them with the
  same names and payloads.
- **Keys give each entity its own monitor.** Every log line has a `key`; observe creates
  a fresh spec instance per key. Write the spec about *one* order, and log the order id
  as the key. (The model drives one order, so one instance is right there too.)
- **Hot states at the end of a log** are checked only with `--check-hot`: an order left
  `Paid` (never shipped or refunded) then fails.
- **`--strict`** fails when no relevant event was checked at all, which catches disabled
  instrumentation or a key filter that matches nothing.
- **Unknown event names** (a typo in the logging code) fail by default, with a
  suggestion.

Model checking finds design bugs in schedules you could not test; observation checks
real executions, one at a time, with whatever happened in them. Use both.

## The key lines

A log line ([`logs/valid.jsonl`](logs/valid.jsonl)):

```json
{"key": "7", "timestamp": "2026-11-12T10:00:02.000Z", "event": "OrderPaid", "payload": {"order": 7, "amount": 30}}
```

The spec learns its order from the first event, because a client may log its request
before the service logs the order:

```p
fun MatchOrder(o: int) {
  if (!known) { order = o; known = true; }
  assert o == order, format("event for order {0} reached the monitor of order {1}", o, order);
}
```

Helper functions get their own names: the spec's `NotePayRequest` and
`NoteCancelRequest` handle the `PayRequested` and `CancelRequested` events, and the
driver's `SendPay` and `SendCancel` send `Pay` and `Cancel`. A function never shares a
name with an event.

## Reading the output

The valid log covers the whole spec, so the logs exercised every rule:

```text
$ snappea observe --spec OrderLifecycle --check-hot --strict logs/valid.jsonl
ok    OrderLifecycle (12 events, 3 keys)
      Coverage: 4/4 states, 25/25 statements, 4/4 branches, 9/9 event handlers, 4/4 condition outcomes
```

A violation names the key and the assertion:

```text
$ snappea observe --spec OrderLifecycle --check-hot --strict logs/paid-after-cancel.jsonl
FAIL  OrderLifecycle (1 error; 5 events, 1 key)
      key 21: Assertion failure: PSpec/OrderLifecycle.p:57:7 order 21 paid although it was cancelled before payment was requested
```

The open order, with `--check-hot`:

```text
FAIL  OrderLifecycle (1 error; 3 events, 1 key)
      key 30: PImplementation.OrderLifecycle detected liveness bug in hot state 'Paid' at the end of program execution.
```

The typo:

```text
      Unknown event 'OrderShiped': not declared in the P sources (1 occurrence, first at logs/misspelled.jsonl:4); did you mean OrderShipped?
```

The coverage list under each result ("Not covered by the events") says which rules the
log never exercised: a log of only happy paths leaves the cancel handlers uncovered.

## Common mistakes

- **A spec about all entities at once** (a map from order id to state) works in the
  model but duplicates what keys already do. Write it per entity.
- **Forgetting `--check-hot`.** A log where an order is paid and never shipped passes
  without it. Use it on complete logs (a finished test run), not on a log that is
  still being written.
- **Checking logs without `--strict` in CI.** If the logging is switched off, every
  check passes on an empty log.
- **Logging only outcomes.** Without `CancelRequested` the spec could not tell a cancel
  that lost a fair race from one the service ignored. Log requests as they are sent.

## Try this

In `logs/valid.jsonl`, change order 8's refund from `"refunded": 12` to
`"refunded": 10`. Observation fails with exit 1:

```text
key 8: Assertion failure: PSpec/OrderLifecycle.p:80:7 order 8 refunded 10 of 12
```

## Run

```sh
cd examples/samples/concepts/16-observe-traces
snappea check --deny mutation
snappea observe --spec OrderLifecycle --check-hot --strict logs/valid.jsonl
```

## Bounds and omissions

The model has one order and one customer. Observation checks each key on its own:
rules that relate different orders (a shared stock, a daily limit) need a key that
covers all of them, such as `""`. Timestamps order each key's events; across machines,
give each key one writer or a logical clock (see docs/observe.md).

## Testing the implementation

Emit the six events from the service and client with the order id as the key (the
language helpers in docs/language-support.md write this format), run the integration
tests, then run `snappea observe --all-specs --check-hot --strict` on the events folder.
To force the race, hold the warehouse's ship call until the customer's cancel has been
sent.
