# Bounded concurrent calls and delayed retries

- **Contract:** balance starts at 2. Credits succeed; debits succeed iff affordable.
  Every result, including rejection, is memoized by ID. Retrying identical arguments
  returns the original result without changing balance.
- **Algorithm:** client loads one version/balance/memo snapshot, computes a candidate,
  then CASes the entire record. A loser reloads and recomputes. Balance and memo
  become visible together. A memo-hit read has no write.
- **Oracle:** owns its own balance and memo, independent of storage versions and
  candidate construction. Checks actual acceptance, actual resulting balance,
  original retry arguments/results and actual client replies. Result announcements
  occur at Store transitions; response receipt order is not treated as commit order.
- **Completion:** hot state remains until both clients finish both invocations;
  `eDone` checks two IDs, four operations and two returned results per ID. A silent
  driver or dropped work cannot satisfy the scenario. A test-only assertion bounds
  an invocation to two load/CAS rounds: exactly one other fresh write can interfere.
  This checks progress for this workload, not a production retry policy.

## Driver choices

- Two independent clients share one Store. Loads and later CAS messages can
  interleave, including two reads of one version and a failed CAS.
- `Generated` samples each request definition once: debit/credit, amount 1/3,
  ID 0/1. Values lie below/above initial balance 2; they expose both accepted and
  rejected debits. The checker explores inputs **and** enabled schedules.
- Both first calls finish before either retry begins. A retry is therefore checked
  against a state containing the other completed operation, not just immediately
  against its own effect. Retry arguments are saved, never independently sampled.
- `DelayedAcceptedRetry`: two debit-1 calls leave 0. A replay must not debit again.
- `DelayedRejectedRetry`: debit-3 can reject before credit-3 makes it affordable.
  Its old rejection must remain false; the credit replay must not add again.
- Immediate resulting-state checks expose rejection/retry side effects before a
  later operation can overwrite them. Client reply checks also catch wrong response
  payloads even when the service's internal audit announcement was correct.

## Run

```sh
snappea check Balance.pproj --timeout 5s
python check_examples.py --binary /path/to/snappea
```

The script uses temporary copies, checks completed positive searches, and changes
one exact operation at a time for negative controls:

| Fault | Required distinguishing observation |
| --- | --- |
| Ignore CAS version | Second stale candidate loses an acknowledged effect |
| Apply a replay again | Old result is unchanged but current balance changes |
| Mutate on rejection | False return is correct but resulting balance is wrong |
| Corrupt the reply | Commit announcement is correct but client receives wrong result |

Controls must fail a named property assertion; compile, lint, coverage and timeout
errors do not count as detection. These hand-selected faults validate important
properties but are not a complete mutation score.

## Validate the oracle independently

`valid-concurrent.jsonl` records debit-3 rejection at 2, credit-3 success to 5,
responses delivered in reverse order, then both unchanged retries and completion.
`invalid-retry.jsonl` instead claims the rejected debit replay reduced 5 back to 2.
The script checks both; these manually calculated traces test the oracle only.
They are not implementation observations. For actual Python capture, use the
[reservation example](../reservation/README.md).

## Assumptions and omissions

- Exactly two clients, one request definition each, one delayed retry each, amounts
  1/3, initial balance 2; four completed calls. Not arbitrary histories or clients.
- `eLoad` represents one consistent snapshot; adjacent Store assignments represent
  one atomic compare-and-swap over balance AND memo. Real storage must supply both.
- Tiny numeric bounds avoid overflow; production overflow behavior is unmodeled.
- No crashes, lost replies, durable restart, memo expiration, maliciously changed
  retry arguments or external writers. See [durable retry](../durable-retry/README.md)
  for a separate recovery model.
- Hot-state checking catches unfinished modeled work; it does not establish
  production fairness or progress under permanent contention/failure.
- This is a driver/oracle example, not a Python implementation-refinement proof.
