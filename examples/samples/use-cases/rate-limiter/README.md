---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a rate limiter shared by several gateways?
  - Why must a lost compare-and-swap reload and decide again?
  - How do I model a request racing with a window rollover?
---

# Distributed fixed-window rate limiter

Two API gateway nodes share one counter record in a store (one Redis key, one
DynamoDB item). The record says which window it counts and how many requests that
window admitted. Each gateway reads the clock, loads the record, decides, and writes
the new count back with compare-and-swap.

**Contract**

- `WindowLimit`: no window admits more than `Limit()` (2) requests, across all
  gateways.
- `WindowLimit`: a request is rejected only when its window is full, so a new window's
  requests are admitted again (rollover resets the count).
- `WindowLimit`: a request is counted in, or rejected for, a window that was current
  while it was in flight. Once a later window has admitted requests, no earlier window
  admits more.
- `RequestsAnswered` (hot state): every request gets exactly one answer, and the answer
  matches the decision.

**Atomic primitives**: `get` returns `(window, count, version)`; `cas` writes
`(window, count)` only if the version is unchanged. The clock read, the get and the
CAS are separate steps, so other gateways and the clock tick can run between them.

**Algorithm** (`Gateway`): read the clock once (`arrival`). Then loop: load; the
effective window is the later of `arrival` and the record's window (a record already
counting a later window means our clock reading is stale); if the record counts an
earlier window, start the count at 0; reject if full, else CAS `count + 1`. A lost
CAS reloads and decides again, and may now reject.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `WindowLimit` (safety) and `RequestsAnswered` (progress, hot state) |
| [`PSrc/CounterStore.p`](PSrc/CounterStore.p) | The store: get and CAS; announces each admission at its CAS |
| [`PSrc/Gateway.p`](PSrc/Gateway.p) | The gateway algorithm and `Limit()` |
| [`PSrc/BlindGateway.p`](PSrc/BlindGateway.p) | Negative control: load, decide, then plain SET |
| [`PTst/Tests.p`](PTst/Tests.p) | The clock (environment), drivers, a scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcLastSlotRace` | One request fills the window to `Limit()-1`, then one request at each gateway | Both read the same count; the loser of the CAS must reload and reject. All schedules | Passes |
| `tcRolloverRace` | Window 0 full, then one request per gateway and a clock tick, in any order | Stale clock reads, rollover by one gateway while the other is mid-loop. All schedules | Passes |
| `tcAfterRollover` | Window 0 full, tick, then one request per gateway | Lifecycle boundary: a new window must admit, not reuse the old full count. All schedules | Passes |
| `tcBugLoadThenStore` | `tcLastSlotRace` with `BlindGateway` and a GET/SET store | Both gateways load the same count and both admit | Expected bug: `window 0 admitted 3 requests, limit 2` |

The `LostCasThenReject` scenario confirms the checks reach a CAS that lost and then
rejected the same request on reload. Contract events alone cannot show that (an
admission followed by a rejection also happens without any race), so the specs also
observe the store's `CasResult` replies, and `WindowLimit` checks that a failed one names
a request still in flight.

## Run

```sh
snappea check tcLastSlotRace tcRolloverRace tcAfterRollover --deny mutation
snappea check tcBugLoadThenStore      # passes only when the declared failure is found
```

## Bounds and omissions

Two gateways, `Limit()` = 2, windows 0 and 1, at most four requests. Each gateway
handles one request at a time: it waits for the clock and the store in `receive`, so
every reply belongs to the current request. A real gateway serves many requests at
once; two gateways are enough for the race. The design
retries after a lost CAS without a test-only attempt counter. The progress spec
requires every planned request to receive a decision, including after contention.

Not modeled: sliding windows, several counter keys (per user or per route), clock
skew beyond "a reading can be stale by one window", store failures and timeouts
(a gateway that cannot reach the store must fail open or closed: decide which and
model it), and lost replies.

Two suppressions, both equivalent mutants: the gateway's `rec.window > window` may
become `>=` (equal windows assign the same value), and the store's version may count
down (the CAS needs only a version never handed out before).

## Testing the implementation

Emit `Admitted(req, window, count)` after each successful CAS, `CasResult(req, ok)`
for each CAS response, `Rejected(req, window)` where the gateway rejects, `Ticked`
when the window changes and `Requested`/`Answered` from the test client, then check them against
`WindowLimit` and `RequestsAnswered`. To force the race, wrap the store client so two
gateways both complete `get` before either calls `cas`; for the rollover race, make
the clock return the old window to one gateway after the other has written the new
window.
