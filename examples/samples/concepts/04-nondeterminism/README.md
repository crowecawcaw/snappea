---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I let the checker choose input values or failures?
  - When should a choice go in the driver, and when in the design model?
  - What does "all N schedules" mean, and what do driver limits tell me?
---

# Nondeterminism: every withdrawal amount and every database timeout

A test with fixed inputs checks one story. A **choice** lets the checker try every
value, and every combination with every message order, in one test. Here an account
holds 2; the driver withdraws twice, each time an amount the checker chooses, and the
account's database may time out on any request. The boundary (exactly the balance) and
a timeout before or after a success all come out of two choices.

| Expression | Means |
| --- | --- |
| `choose(n)` | an `int` from `0` to `n - 1` |
| `choose(collection)` | any element of a `seq`, `set` or map keys |
| `$` | a `bool` |
| `$$` | a `bool` that upstream P treats as *fair* for liveness (not false forever in a long run). For safety checking it behaves like `$`. |

**Contract:**

- A withdrawal succeeds exactly when the balance covers it, and every reply reports the
  balance the earlier successful withdrawals imply (`WithdrawalsMatchBalance`).
- "Unavailable" changes nothing (`WithdrawalsMatchBalance`).
- Every request gets an answer (`WithdrawalsMatchBalance` stays hot until then).

**Atomic primitives:** the account's `Withdraw` handler is one database transaction:
check the balance, debit it, and reply, or fail as a whole with a timeout.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Account.p`](PSrc/Account.p) | The design, with an environment choice (`$`) for database timeouts |
| [`PSpec/Withdrawals.p`](PSpec/Withdrawals.p) | `WithdrawalsMatchBalance`: expected balance, rebuilt from the requests |
| [`PTst/Tests.p`](PTst/Tests.p) | The driver, with an input choice (`choose(4)`), and the test case |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoWithdrawals` | Two withdrawals of 0-3 from a balance of 2, one after the other; each may time out | Boundary (exactly the balance), refusal, a timeout before or after a success | Passes |

## The key lines

**Inputs go in the driver.** What a user or caller might send is the test's business.
`choose(4)` covers below (0, 1), exactly at (2) and above (3) the balance:

```p
entry {
  var amount: int;
  amount = choose(4);
  made = made + 1;
  announce WithdrawRequested, (id = made, amount = amount);
  send account, Withdraw, (client = this, id = made, amount = amount);
}
```

**Environment behavior goes in the model.** What the design does not control (a
timeout, a lost message, a crash) is a choice inside the machine that stands for it:

```p
if ($) {
  send w.client, Unavailable, (id = w.id,);
} else if (w.amount <= balance) {
  ...
```

Put the choice where the real decision is made. A driver that "decides" the database
times out would need to know about the database, and the account could no longer be
tested with a different driver.

The checker does not mutate choices: `if ($)` is never forced to `true` or `false`,
since both are already explored.

## Reading the output

```text
ok    tcTwoWithdrawals (all 63 schedules, 2 workers, 0.01s)
      Coverage: 3/3 states, 39/39 statements, 10/10 branches, 7/7 event handlers, 10/10 condition outcomes, 2/4 transition pairs
      Driver limits (held in all 63 schedules):
        creates 1 Account, never more
        TwoWithdrawals sends Withdraw exactly twice
        Withdraw is only sent with id = 1 or 2
        Account is always created with 2
Mutation testing: 13 of 13 mutants detected (0.0s)
```

- **all N schedules** means the search was **exhaustive**: every combination of
  choices and message orders, within this driver's bounds, was explored, and none
  broke the contract. The count is smaller than 4 x 2 x 4 x 2 because the checker
  skips a continuation once it has explored that same state. A search that runs out
  of time says `N schedules` without `all`; then it is a sample, not a proof.
- **Driver limits** list what held in every schedule: things this test never tries.
  No limit is listed on `amount`, because the driver tries every value it can send.

## Common mistake: a fixed input

Replace `amount = choose(4);` with `amount = 1;`. The driver limits gain
`Withdraw is only sent with id = 1 or 2, amount = 1`, and the check fails on coverage,
because no request is ever refused:

```text
Not covered by the test case:
  PSpec/Withdrawals.p:36  WithdrawalsMatchBalance.Open, on WithdrawResponse: if (amount <= expected) never false
  PSrc/Account.p:22       Account.Serving, on Withdraw: if (w.amount <= balance) never false
```

Pick choice ranges from the contract's boundaries: below, exactly at and above each
limit. Small ranges are enough; `choose(1000)` multiplies the search by 1000 and adds
no new case.

## Try this

Change the account's test to `w.amount < balance` (an off-by-one at the boundary).
The checker finds the amount-equals-balance case:

```text
Assertion Failed: PSpec/Withdrawals.p:37:9 withdrawal 1 of 2 refused although the balance was 2
```

## Run

```sh
cd examples/samples/concepts/04-nondeterminism
snappea check --deny mutation
```

## Bounds and omissions

One account, two sequential withdrawals of 0-3 from a balance of 2, any request may
time out. Concurrent withdrawals are in [05-multiple-clients](../05-multiple-clients/README.md)
(a different resource) and [15-mutation-driven-specs](../15-mutation-driven-specs/README.md)
(this account). A timeout here means the database did nothing; a timeout after the
write committed is a different, harder case (the client cannot tell them apart), not
modeled here.

## Testing the implementation

Emit `WithdrawRequested` when the client sends a withdrawal, and `WithdrawResponse` or
`Unavailable` from the account where it replies. To reach the timeout case, inject a
database timeout with a fault-injection wrapper; to reach the boundary, withdraw
exactly the balance.
