---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a cancel request racing with fulfilment on the same order row?
  - How do I make sure every card authorization is captured or voided?
  - How do I model a payment capture that fails and is retried?
---

# Cancelling an order while the warehouse ships it

An e-commerce order goes Placed → Authorized (card hold) → Picked → Shipped, and the
warehouse then captures the payment. The customer can cancel at any time; a cancel
succeeds only before shipping and voids the card hold. Checkout, the warehouse and
the cancel request all write the same order row.

**Contract**

- A shipped order is never cancelled or voided; a cancelled order is never shipped
  or captured. A status change is refused only when the order is already over.
- Every authorization is eventually captured or voided: no dangling holds.
- Every order ends shipped (and the customer is told) or cancelled.
- Every cancel is answered, and the answer matches the order: "cancelled" only if it
  is cancelled, "too late" only if it shipped. Cancelling twice gets the same answer
  both times, and the hold is voided once.

**Atomic primitives**

- Order row: one conditional status transition,
  `UPDATE orders SET status = :next WHERE id = :order AND status IN (:allowed)`,
  which returns whether it changed the row and the status it found.
- Payment gateway: authorize, capture, void. Capture is idempotent per order id.

Each actor does one transition per message, so a cancel can land between any two
steps: before checkout records the hold, between "picked" and "shipped", and after.

**Design decisions the model checks**

- The cancel handler voids the hold only if it changed the row and the order had a
  hold (`prior != Placed`). If the cancel landed before checkout recorded the hold,
  checkout's `Placed → Authorized` transition fails and checkout voids instead.
- The warehouse ships with `WHERE status IN (Picked)`. If that fails, the order was
  cancelled after picking: it restocks and stops.
- A failed capture is retried with the same idempotency key until it succeeds. The
  gateway can fail before committing or after committing with the reply lost; the
  warehouse cannot tell which, and does not need to.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `OrderSafety` (lifecycle and money), `OrdersSettle` (progress and answers, hot state) |
| [`PSrc/OrderStore.p`](PSrc/OrderStore.p) | The order table and its conditional transition |
| [`PSrc/Payments.p`](PSrc/Payments.p) | The gateway, with injected capture failures (test environment, marked as a driver) |
| [`PSrc/Order.p`](PSrc/Order.p) | `Checkout`, `CancelRequest` (the cancel API handler) and `Warehouse` |
| [`PSrc/BlindWarehouse.p`](PSrc/BlindWarehouse.p) | Negative control: ships with an unconditional write |
| [`PTst/Tests.p`](PTst/Tests.p) | The customer driver, the `CancelledWhilePicked` scenario, the test cases |

## Test cases

| Test | Drives | Why it matters | Result |
| --- | --- | --- | --- |
| `tcCancelRacesWarehouse` | One order, one cancel sent as soon as the order exists | The core race, every interleaving: cancel before the hold is recorded, before picking, between picking and shipping, after shipping | passes, exhaustive |
| `tcDuplicateCancel` | One order, two cancels | Duplicate cancel: same answer twice, one void | passes, exhaustive (about 10 s) |
| `tcCaptureRetry` | One order, the gateway fails one capture | Capture failing before or after commit; the retry must take the money once | passes, exhaustive |
| `tcCancelAfterShip` | Cancel sent only after the shipping notice | Lifecycle boundary: refused with "too late", nothing voided | passes, exhaustive |
| `tcGeneratedOrders` | Two orders, zero or one cancel each, one capture failure | Orders do not interfere through the shared store, gateway and warehouse | passes, sampled |
| `tcBugBlindShip` | `BlindWarehouse` in the core race | **Expected to fail**: `order 1 shipped after it was cancelled` | fails |

The `CancelledWhilePicked` scenario confirms the checks reach the edge case the design
exists for: a `Picked → Cancelled` transition.

## Run

```sh
snappea check tcCancelRacesWarehouse tcDuplicateCancel tcCaptureRetry tcCancelAfterShip tcGeneratedOrders --timeout 20s
snappea check mutation tcCancelRacesWarehouse tcDuplicateCancel tcCaptureRetry tcCancelAfterShip tcGeneratedOrders
snappea check tcBugBlindShip     # exits 1: order 1 shipped after it was cancelled
```

## Bounds and omissions

One or two orders, at most two cancels per order, at most one capture failure.
Authorization never fails, voids never fail, and messages are not lost. Every order
gets an authorization, even one cancelled before checkout reached the gateway (the
progress spec relies on this). Not modeled: authorization expiry, partial shipments,
refunds after capture, a crash of the warehouse between shipping and capture (the
retry would have to be durable).

One suppression: the store's `eTransitionRefused` announcement is observation only;
removing it removes a check, not behavior.

## Testing the implementation

Emit `eOrderCreated`, `eStatusChanged` and `eTransitionRefused` from the order
repository after each `UPDATE` commits (with the row's prior status), and
`eAuthorized`, `eCaptured`, `eVoided` from the payment client when the gateway
confirms. Record the cancel API's response as `eCancelAnswered` in the test client.
To force the race, block the warehouse worker after its pick transition commits, run
the cancel request to completion, then release the worker: it must not ship. Make the
gateway stub fail the first capture after committing it, and check one capture.
