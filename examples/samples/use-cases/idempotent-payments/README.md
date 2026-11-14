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

- The card processor is called at most once per key, with the amount requested.
- Every answer for a key, first, retried, concurrent or late, is the result of that
  one charge. A declined payment stays declined on retry, even after the card has
  funds again.
- Keys are independent: each key's answers come from its own charge.
- Every attempt gets an answer for its own key, including one that arrives while
  another request is still charging the same key.

**Atomic primitives** (key table, e.g. a unique-key insert plus a conditional update):

- `claim(key)`: if absent, insert "in progress" and tell the caller it owns the key;
  if done, return the saved result; if in progress, register the caller to be told
  the result. Checking and registering are one step, so the completion cannot fall
  between them and be missed.
- `complete(key, result)`: mark done with the result and notify registered callers.

The processor call is a separate request to an external service, between `claim` and
`complete`, so other requests interleave with it.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ChargedOnce` (safety) and `EveryAttemptAnswered` (progress, hot state) |
| [`PSrc/KeyStore.p`](PSrc/KeyStore.p) | The key table: claim and complete |
| [`PSrc/PaymentApi.p`](PSrc/PaymentApi.p) | One API server: claim, charge, complete, reply |
| [`PSrc/CheckThenActApi.p`](PSrc/CheckThenActApi.p) | Negative control: look up, charge, then record |
| [`PTst/Tests.p`](PTst/Tests.p) | Card processor and clients (environment), drivers, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcDoubleClick` | Key 1 sent to both servers at once | Concurrent duplicate, or a timed-out attempt retried while the first is still charging: one charge, the other waits for or reads the result | pass |
| `tcLostResponseRetry` | Key 1 answered by server A, then retried on server B | Lost response: the retry returns the saved result, no second charge | pass |
| `tcDeclinedStaysDeclined` | No funds: key 1 declined; top up; retry key 1; then new key 2 | The retry must stay declined; key 2 is approved, so the decline is per key | pass |
| `tcIndependentKeys` | Keys 1 and 2 at once on two servers, funds for one | Different keys never wait for or answer each other | pass |
| `tcBugCheckThenAct` | `tcDoubleClick` with `CheckThenActApi` | Negative control | expected bug: `key 1 charged twice` |

## Run

```sh
snappea check tcDoubleClick tcLostResponseRetry tcDeclinedStaysDeclined tcIndependentKeys --timeout 30s
snappea check tcBugCheckThenAct      # passes only when the declared failure is found
```

A time budget does not guarantee exhaustive search. Inspect `search_complete`
in the report before claiming every bounded schedule was checked.

## Bounds and omissions

Two servers, two keys, at most three attempts, one card. The card processor and
the clients are the environment (`PTst/`): they are not mutated, and what they
announce is ground truth. Not modeled: a server crash while a key is in progress
(the key would stay in progress forever: production needs a lease or timeout on
"in progress" records and a processor call that is itself idempotent on the key),
processor timeouts with unknown outcome, the same key reused with a different
amount (the spec flags it; the API should reject it), and key expiry.

## Testing the implementation

Record each processor call (`ProcessorCharged`) from a fake processor, and each
attempt and answer (`Attempted`, `Answered`) in the test client; check them against
`ChargedOnce` and `EveryAttemptAnswered`. To force the double click, block the fake
processor on its first call, send the duplicate to a second server instance, then
unblock. For the declined case, decline, top up the fake card, retry the same key.
