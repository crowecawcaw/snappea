---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - Why does an idempotency key not stop a double charge from a double click?
  - What does P's counterexample for a concurrent duplicate look like?
  - How do I make an idempotency check safe for requests that are still in flight?
---

# In-flight duplicate: idempotency key recorded after the side effect

**Difficulty: medium.**

A payment API takes an idempotency key with each charge request, so a client can
retry safely. Each request is handled by its own API worker. The workers share an
idempotency table; the payment gateway charges on every call.

**The flaw in one sentence:** the worker looks the key up, charges the card, and only
then records the key, so a duplicate that arrives while the first charge is in flight
(a double click, or a client retry after a slow response) also finds nothing and
charges again.

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `ChargedOnce` (safety): one key causes at most one gateway charge, for the
  requested amount; charge ids are unique.
- `EveryRequestAnswered` (progress, hot state): every request returns, and returns
  the one charge made for its key, so a duplicate sees the original's result.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/Gateway.p`](PSrc/Gateway.p) | The payment gateway (charges on every call) and shared types |
| [`PSrc/LookupThenRecord.p`](PSrc/LookupThenRecord.p) | The flawed design: `KeyTable` (lookup, record) and `LookupThenRecordWorker` |
| [`PSrc/ClaimFirst.p`](PSrc/ClaimFirst.p) | The fix: `ClaimTable` (claim, complete) and `ClaimingWorker` |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## The counterexample

`snappea check tcBugDoubleClick` finds the declared bug:

```text
Assertion Failed: double charge: key 7 charged twice (charges 1 and 2)
```

From `PCheckerOutput/tcBugDoubleClick/bug.txt`:

1. The client sends request 1 and request 2, both with key 7 and amount 10; two
   workers pick them up.
2. Worker 1 looks up key 7. Worker 2 looks up key 7.
3. The table answers worker 1: not found. Worker 1 calls the gateway.
4. The gateway charges key 7 (charge 1).
5. The table answers worker 2: not found. Nothing has recorded key 7 yet: worker 1
   records it only after the gateway replies.
6. Worker 2 calls the gateway, which charges key 7 again (charge 2). `ChargedOnce`
   fails.

## Why ordinary tests miss it

The usual idempotency test sends a request, waits for the response, and retries:
the retry finds the key and returns the stored result. That passes here too
(`tcFixedRetryAfterDone` is that test, and the flawed design passes it). The bug
needs the duplicate to arrive inside the window between lookup and record, which is
as long as the gateway call. That window is wide in production (hundreds of
milliseconds) and nearly zero against a mocked gateway in a unit test.

## The fix

Claim the key before the side effect. `claim` is one atomic insert-if-absent:

- key absent: insert it as *in progress*, and this worker charges;
- key in progress: wait until it completes, then return its result;
- key done: return the stored result.

After charging, `complete` stores the result and releases the waiting workers with
it. The charge can only start after a successful insert, and only one insert per key
succeeds, so there is one charge per key. Instead of waiting, the store may answer
"in progress" (HTTP 409) and let the client retry later; the safety argument is the
same.

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugDoubleClick` | Two concurrent requests, same key, flawed design | The race itself | **Expected bug**: `double charge: key 7 charged twice (charges 1 and 2)` |
| `tcFixedDoubleClick` | Same, fixed design | The duplicate waits and returns charge 1 | Passes |
| `tcFixedTripleClick` | Three concurrent requests, same key | Two waiters on one in-progress key must both be released | Passes |
| `tcFixedMixedKeys` | Two duplicates and one other key, concurrent | Another key must not wait or share a result | Passes |
| `tcFixedRetryAfterDone` | A retry after the first request returned, with a new key | Lifecycle boundary: answer from the stored result | Passes |

## Run

```sh
snappea check tcFixedDoubleClick tcFixedTripleClick tcFixedMixedKeys tcFixedRetryAfterDone --deny mutation --timeout 20s
snappea check tcBugDoubleClick        # succeeds only when the declared bug is found
```

One mutant is suppressed: `nextCharge + 1` → `nextCharge - 1` in `Gateway`, because
charge ids are only compared for equality and counting down keeps them unique.

The negative controls use native `snappea: expect-bug` declarations. A matching
failure passes the test; no failure or a different failure fails it. Search coverage
depends on the budget: claim exhaustion only when the current run reports completion.

## Bounds and omissions

Up to three requests, two keys, one table, one gateway. Messages are not lost and
nothing crashes. Not modeled: a worker that crashes after claiming (the key stays in
progress forever; a real claim needs a lease or a timeout after which the charge is
reconciled with the gateway), gateway failures, key expiry, a key reused with a
different amount (the spec rejects it as a driver error).

## Testing the implementation

Emit `Charged` from the gateway client after the gateway confirms, and `Replied`
when the API returns. To force the race, block the gateway stub on a latch, send the
duplicate request, then release the latch: the flawed code calls the stub twice;
the fixed code calls it once and both responses carry the same charge id.
