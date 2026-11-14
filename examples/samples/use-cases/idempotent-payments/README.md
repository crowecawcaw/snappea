---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I make a payment API safe to retry with idempotency keys?
  - What should a duplicate request do while the first one is still charging?
  - Why is check-key, charge, then record not enough?
---

# Payment API with idempotency keys

A client charges a card through a payment API and sends an idempotency key with each
payment. Responses can be lost, so the client retries with the same key; a double
click sends the same key twice at once, possibly to two API servers.

**Contract**

- `ChargedOnce`: the card processor is called at most once per key, with the amount
  requested.
- `ChargedOnce`: every answer for a key, first, retried, concurrent or late, is the
  result of that one charge. A declined payment stays declined on retry, even after
  the card has funds again. Keys are independent: each key's answers come from its
  own charge. A "retry later" answer comes only when another attempt with the key
  was made.
- `EveryAttemptAnswered` (hot state): every attempt eventually gets a final answer
  for its own key, including one that arrives while another request is still
  charging the same key (it is told to retry, and retries).

**Atomic primitives** (key table, e.g. a unique-key insert plus a conditional update):

- `claim(key)`: if absent, insert "in progress" and tell the caller it owns the key;
  otherwise return the record: done with its result, or still in progress.
- `complete(key, result)`: mark the record done with the result.

The processor call is a separate request to an external service, between `claim` and
`complete`, so other requests interleave with it. A request that finds the key in
progress answers "retry later" (HTTP 409, as Stripe does), and the client retries the
same key. Nothing pushes the result to a waiting request.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ChargedOnce` (safety) and `EveryAttemptAnswered` (progress, hot state) |
| [`PSrc/KeyStore.p`](PSrc/KeyStore.p) | The key table: claim and complete |
| [`PSrc/PaymentApi.p`](PSrc/PaymentApi.p) | One API server: claim, charge, complete, reply; "retry later" while in progress |
| [`PSrc/CheckThenActApi.p`](PSrc/CheckThenActApi.p) | Negative control: look up, charge, then record |
| [`PTst/Tests.p`](PTst/Tests.p) | Card processor and clients that retry on 409 (environment), drivers, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcDoubleClick` | Key 1 sent to both servers at once | Concurrent duplicate, or a timed-out attempt retried while the first is still charging: one charge, the other is told to retry and then reads the result. All states for safety; progress is sampled (see Bounds) | Passes |
| `tcLostResponseRetry` | Key 1 answered by server A, then retried on server B | Lost response: the retry returns the saved result, no second charge. All schedules | Passes |
| `tcDeclinedStaysDeclined` | No funds: key 1 declined; top up; retry key 1; then new key 2 | The retry must stay declined; key 2 is approved, so the decline is per key. All schedules | Passes |
| `tcIndependentKeys` | Keys 1 and 2 at once on two servers, funds for one | Different keys never block or answer each other. All schedules | Passes |
| `tcBugCheckThenAct` | `tcDoubleClick` with `CheckThenActApi` | Two duplicates look up before either records | Expected bug: `key 1 charged twice` |

## Run

```sh
snappea check tcDoubleClick tcLostResponseRetry tcDeclinedStaysDeclined tcIndependentKeys --deny mutation
snappea check tcBugCheckThenAct      # passes only when the declared failure is found
```

## Bounds and omissions

Two servers, two keys, at most three attempts, one card. Each server handles one
request at a time: it waits for the key table and the processor in `receive`, so their
replies always belong to the current request. A real server runs many workers; the two
servers stand in for that concurrency. A client retries "retry later" without limit
and without backoff delay. Progress then relies on the charging server eventually
running, so in `tcDoubleClick` the checker explores every state for safety but
reports progress as sampled.

The card processor and the clients are the environment (`PTst/`): they are not
mutated, and what they announce is ground truth. Not modeled: a server crash while a key is in progress
(the key would stay in progress forever: production needs a lease or timeout on
"in progress" records and a processor call that is itself idempotent on the key),
processor timeouts with unknown outcome, the same key reused with a different
amount (the spec flags it; the API should reject it), and key expiry.

## Testing the implementation

Record each processor call (`ProcessorCharged`) from a fake processor, and each
attempt, 409 and answer (`Attempted`, `ChargeConflict`, `Answered`) in the test
client; check them against `ChargedOnce` and `EveryAttemptAnswered`. To force the
double click, block the fake processor on its first call, send the duplicate to a
second server instance (it must answer 409), then unblock. For the declined case, decline, top up the fake card, retry the same key.
