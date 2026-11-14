---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I let the checker choose input values or failures?
  - When should a choice go in the driver, and when in the design model?
  - What does "all N schedules" mean, and what do driver limits tell me?
---

# Nondeterminism: let the checker choose

A test with fixed inputs checks one story. A **choice** lets the checker try every
value, and every combination with every message order, in one test:

| Expression | Means |
| --- | --- |
| `choose(n)` | an `int` from `0` to `n - 1` |
| `$` | a `bool` |
| `$$` | a `bool` that upstream P treats as *fair* for liveness (not false forever in a long run). For safety checking it behaves like `$`. |

This sample is an account with a balance of 2. The driver withdraws twice, each time an
amount the checker chooses; the account's database may time out on any request.

**Contract:** a withdrawal succeeds exactly when the balance covers it; every reply
reports the balance the earlier successful withdrawals imply; "unavailable" changes
nothing; every request gets an answer.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Account.p`](PSrc/Account.p) | The design, with an environment choice (`$`) for database timeouts |
| [`PSpec/Withdrawals.p`](PSpec/Withdrawals.p) | `WithdrawalsMatchBalance`: expected balance, rebuilt from the requests |
| [`PTst/Tests.p`](PTst/Tests.p) | The driver, with an input choice (`choose(4)`), and the test case |

## The key lines

**Inputs go in the driver.** What a user or caller might send is the test's business.
`choose(4)` covers below (0, 1), exactly at (2) and above (3) the balance:

```p
entry {
  var amount: int;
  amount = choose(4);
  announce WithdrawRequested, amount;
  send account, Withdraw, (client = this, amount = amount);
}
```

**Environment behavior goes in the model.** What the design does not control (a
timeout, a lost message, a crash) is a choice inside the machine that stands for it:

```p
if ($) {
  send w.client, Unavailable;
} else if (w.amount <= balance) {
  ...
```

Put the choice where the real decision is made. A driver that "decides" the database
times out would need to know about the database, and the account could no longer be
tested with a different driver.

The checker does not mutate choices: `if ($)` is never forced to `true` or `false`,
since both are already explored.

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoWithdrawals` | Two withdrawals of 0-3 from a balance of 2, each may time out | Boundary (exactly the balance), refusal, a timeout before or after a success | passes, all 52 schedules |

Schedule counts, timings and report excerpts below are illustrative. Use the current
run's completion status to decide whether it explored all schedules.

## Run it

```sh
cd examples/samples/concepts/04-nondeterminism
snappea check --deny mutation
```

```text
ok    tcTwoWithdrawals (all 52 schedules, 0.01s)
      Coverage: 3/3 states, 30/30 statements, 8/8 branches, 7/7 event handlers, ...
      Driver limits (held in all 52 schedules):
        creates 1 Account, never more
        TwoWithdrawals sends Withdraw exactly twice
        Account is always created with 2
Mutation testing: 13 of 13 mutants detected (0.0s)
```

- **all 52 schedules** means the search was **exhaustive**: every combination of
  choices and message orders, within this driver's bounds, was explored, and none
  broke the contract. The count is smaller than 4 x 2 x 4 x 2 because the checker
  skips a continuation once it has explored that same state. A search that runs out
  of time says `N schedules` without `all`; then it is a sample, not a proof.
- **Driver limits** list what held in every schedule: things this test never tries.
  No limit is listed on `amount`, because the driver tries every value it can send.

## Common mistake: a fixed input

Replace `amount = choose(4);` with `amount = 1;`. The run is now 6 schedules, the
driver limits gain `Withdraw is always sent with amount = 1`, and the check fails on
coverage, because no request is ever refused:

```text
Not covered by the test case:
  PSrc/Account.p:21       Account.Serving, on Withdraw: if (w.amount <= balance) never false
  PSpec/Withdrawals.p:28  WithdrawalsMatchBalance.Pending, on WithdrawResponse: if (amount <= expected) never false
Coverage requirements were not met for 4 points.
```

Pick choice ranges from the contract's boundaries: below, exactly at and above each
limit. Small ranges are enough; `choose(1000)` multiplies the search by 1000 and adds
no new case.

## Try this

Change the account's test to `w.amount < balance` (an off-by-one at the boundary).
The checker finds the amount-equals-balance case:

```text
FAIL  tcTwoWithdrawals (bug found after 1 schedule, 0.00s)
      Assertion Failed: PSpec/Withdrawals.p:29:9 withdrawal of 2 refused although the balance was 2
```

## Bounds

One account, two sequential withdrawals of 0-3 from a balance of 2, any request may
time out. Concurrent withdrawals are not modeled (see
[05-multiple-clients](../05-multiple-clients/README.md)). A timeout here means the
database did nothing; a timeout after the write committed is a different, harder case
(the client cannot tell them apart), not modeled here.

## In your implementation

Emit `WithdrawRequested` when the client sends a withdrawal and `WithdrawResponse` or
`Unavailable` when it gets the answer (one client at a time, so the order is safe).
To reach the timeout case, inject a database timeout with a fault-injection wrapper.
