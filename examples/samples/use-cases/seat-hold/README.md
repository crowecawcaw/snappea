---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a hold that expires while the customer is paying?
  - What does a progress spec catch that a safety spec misses?
  - How do I keep a known-bad design as a negative control?
---

# Seat holds that expire during payment

A ticketing service lets a customer hold a seat, pay, then confirm. Holds expire so
abandoned checkouts free their seats. Payment is slow, so the expiry timer can fire
between payment and confirmation, and another customer can hold the seat meanwhile.

**Contract**

- A seat is held by at most one hold at a time and sold at most once, only to the
  hold that currently has it. Rejections must be justified by the seat's state.
- Every charge ends in exactly one of a sale or a refund.
- Every checkout ends with the outcome the store's decisions justify, and every hold
  is eventually sold or expired, even when the customer walks away.

**Atomic primitives** (one transaction on the seat's row each): hold (free → held),
confirm (held by this hold → sold), expire (held by this hold → free). Payment and
the timer are separate actors, so everything between them interleaves.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `SeatsSoldOnce` (safety), `ChargesResolved` and `CheckoutsEnd` (progress, hot states) |
| [`PSrc/SeatStore.p`](PSrc/SeatStore.p) | The store and the one-shot `ExpiryTimer` per hold |
| [`PSrc/Customer.p`](PSrc/Customer.p) | The checkout algorithm and the payment service |
| [`PSrc/UnfencedStore.p`](PSrc/UnfencedStore.p) | Negative control: confirm ignores whether the hold expired |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, a scenario, and the test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcTwoCustomersOneSeat` | Two customers, one seat, nobody walks away | The core race: expiry between payment and confirm, then a second hold |
| `tcGeneratedCheckouts` | Three customers, seats chosen from two, each may abandon | Contended and uncontended seats together; abandoned holds must expire |
| `tcHoldAfterSale` | Second customer starts only after the first bought | Lifecycle boundary: a store that forgets sold seats re-sells here |
| `tcBugConfirmIgnoresExpiry` | The unfenced store | **Expected to fail**: `seat 7 sold to hold 1, which does not hold it` |

The `ExpiredWhilePaying` scenario confirms the checks reach the interesting history:
a charge, the hold expiring, the confirmation rejected, the refund.

## What building it taught

The first version had only `SeatsSoldOnce`. It passed with full coverage, and
mutation testing still reported 22 changes no test noticed: removing the payment
reply, the hold reply or the expiry timer left customers stuck, and a stuck customer
violates no safety rule. Each fix is a pattern worth copying:

- **A progress spec with hot states** (`CheckoutsEnd`) makes "every checkout ends"
  checkable, so a dropped reply fails.
- **The driver announces intent** (`eCheckoutsPlanned`, `eCheckoutStarted` with the
  seat it wants). Drivers are not mutated, so the spec still knows what should
  happen when the customer code is broken. Without this, removing `seat = input.seat`
  went unnoticed: every customer silently contended for seat 0.
- **Abandoning customers** are the reason expiry exists. Without them, removing the
  timer changed nothing observable.
- **A lifecycle-boundary test** (`tcHoldAfterSale`) catches a store that forgets sold
  seats, which concurrent tests reach too rarely for mutation budgets.

## Run

```sh
snappea check tcTwoCustomersOneSeat tcGeneratedCheckouts tcHoldAfterSale --deny mutation
snappea check tcBugConfirmIgnoresExpiry      # exits 1 with the oversell above
```

`tcTwoCustomersOneSeat` and `tcHoldAfterSale` explore every schedule;
`tcGeneratedCheckouts` is sampled within its time budget.

## Bounds and omissions

Two or three customers, two seats, one hold per checkout, one expiry per hold.
Payment never fails and messages are not lost. Not modeled: payment failures,
retried confirmations, holds of several seats at once, clock skew between the timer
and the database.

## Testing the implementation

Emit `eHeld`, `eHoldRejected`, `eExpired`, `eSold` and `eConfirmRejected` from the
store after each transaction commits, and `eCharged`/`eRefunded` from the payment
client. To reach the race in a test, block the payment call until the expiry job has
run and a second customer holds the seat, then let the first confirmation proceed.
