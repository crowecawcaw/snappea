---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I use mutation testing to find what my spec does not check?
  - How do I read undetected mutants and turn each into an assertion?
  - When is it right to suppress a mutant, and how?
  - What do the driver limits in the check output mean?
---

# Mutation-driven specs: from "never negative" to a real withdrawal contract

A spec that passes with full coverage can still check almost nothing. Mutation testing
makes small changes to the design and reruns the tests: a change no test notices marks
behaviour the spec does not constrain. Here two clients withdraw at once from the
account in [04-nondeterminism](../04-nondeterminism/README.md) (same events, same spec
name); a weak spec misses 13 of 14 mutants, and the real contract adds one check per
kind of gap.

**Contract:**

- A withdrawal succeeds exactly when the balance covers it
  (`WithdrawalsMatchBalance`).
- Every reply reports the balance the earlier successful withdrawals imply
  (`WithdrawalsMatchBalance`).
- Every withdrawal is answered (`WithdrawalsMatchBalance` stays hot until then).

**Atomic primitives:** the account's `Withdraw` handler is one check-and-debit
transaction, and replies in the same step.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Account.p`](PSrc/Account.p) | The design, with a zero-withdrawal fast path |
| [`PSpec/Events.p`](PSpec/Events.p) | The driver's announcements, shared by both specs |
| [`PSpec/Withdrawals.p`](PSpec/Withdrawals.p) | The contract after mutation testing |
| [`weak/Withdrawals.p`](weak/Withdrawals.p) | The first, weak spec; not part of the project |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients with chosen amounts, one test case |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoWithdrawals` | Two clients at once, amounts from {0, 4, 6, 10} on a balance of 10 | Exactly the balance, a debit that changes the other client's decision, and the fast path | Passes; every mutant detected or suppressed |

## The concept

`snappea check` runs mutation testing after the tests pass. Each **undetected mutant**
is a change to the design that no test noticed. For each one, decide:

1. **A missing check**: add an assertion or a progress (hot-state) obligation. Usual.
2. **A missing scenario**: the test never reaches the state where the change matters.
   Add a test or widen the driver's choices.
3. **An equivalent change**: no execution can tell it apart. Suppress it with a reason.

Lints catch some weak specs before any exploration; mutation testing catches the rest.

## The weak spec

[`weak/Withdrawals.p`](weak/Withdrawals.p) checks one thing:

```p
spec WithdrawalsMatchBalance observes WithdrawResponse {
  start state Watching {
    on WithdrawResponse do (r: (id: int, ok: bool, balance: int)) {
      assert r.balance >= 0, format("withdrawal {0} left balance {1}", r.id, r.balance);
    }
  }
}
```

Lint already objects, before exploring:

```text
$ snappea check PSrc/Account.p PSpec/Events.p weak/Withdrawals.p PTst/Tests.p
error[unused-payload-field]: monitor payload field 'r.id' is never read outside diagnostic output; ...
error[unused-payload-field]: monitor payload field 'r.ok' is never read outside diagnostic output; ...
error[no-progress-spec]: test 'tcTwoWithdrawals' waits for the model's responses, but none of its specs has a hot state, so a model that stops responding passes
```

With `--warn lint` it explores, passes with full coverage, and mutation testing reports:

```text
Mutation testing: 1 of 14 mutants detected, 2 suppressed (0.6s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Account.p:13  Account.Serving entry: removed `balance = initial;`
  PSrc/Account.p:15  Account.Serving, on Withdraw: removed the whole body (4 statements) (and 11 more undetected inside it; -v lists them)
```

A *coarse* mutant ("removed the whole body") means nothing notices the handler doing
nothing at all; fix those first. `-v` lists the mutants inside it, and the JSON result
has them under `mutation.undetected[].undetected_inside`.

## What each check kills

The strong spec, [`PSpec/Withdrawals.p`](PSpec/Withdrawals.p), rebuilds the expected
balance from the opening balance and the requested amounts, never from the account's
fields. Remove one check at a time and mutation testing shows what it was for:

| Without | Undetected mutants |
| --- | --- |
| The **hot state** `Open` (every withdrawal is answered) | The whole `Withdraw` body removed; the fast path's reply removed |
| `assert r.ok == (amount <= expected)` (accept exactly when covered) | The fast path answers `false`; `ok = ...` removed; `w.amount <= balance` → `<` |
| `assert r.balance == expected` (the reported balance is the real one) | `if (ok)` replaced by `true`: a refused withdrawal is still debited |

Two driver choices make these detectable: an amount equal to the balance (10), the
boundary for `<=` versus `<`; and an amount only the first withdrawal can get (6 after
4), so a missing or extra debit changes a later decision.

## The suppressed mutants

The account answers a zero withdrawal without touching the balance. The general path
gives the same reply for 0, so removing the fast path, or never taking it, cannot be
observed by any execution. Those are equivalent mutants, suppressed per kind with a
reason ([`PSrc/Account.p`](PSrc/Account.p)):

```p
// snappea-lint: allow mutation:remove-statement -- equivalent: the general path gives the same reply for 0
// snappea-lint: allow mutation:condition-false -- equivalent: the general path gives the same reply for 0
if (w.amount == 0) {
```

Suppress one kind per line (`mutation:<kind>`), not `mutation` for everything: here
`w.amount == 0` → `!=` is *not* equivalent, and stays checked. The report lists the
suppressed sites. Suppressions apply only to the annotated statement; statements
inside this `if` remain subject to mutation testing.

## Driver limits

```text
ok    tcTwoWithdrawals (all 1977 schedules, 2 workers, 0.12s)
      Coverage: 3/3 states, 33/33 statements, 8/8 branches, 5/5 event handlers, 8/8 condition outcomes
      Driver limits (held in all 1977 schedules):
        each Client sends Withdraw exactly once
        creates 1 Account and 2 Client, never more
        Withdraw is only sent with id = 1 or 2
        Account is always created with 10
      hint: if the real environment can do more, extend the driver to try it
Mutation testing: 14 of 14 mutants detected, 2 suppressed (0.1s)
```

**Driver limits** are facts that held in every explored schedule. Each one is a bound
on what the test can find. Here: no client withdraws twice (so a bug that only shows on
a second withdrawal from the same client cannot be found), there are only two clients,
and the opening balance is always 10. If real clients can do more, extend the driver;
if not, write the limit into your design notes as an assumption.

## Common mistakes

- **Stopping at full coverage.** The weak spec had 100% statement and branch coverage.
- **Suppressing a missing check.** "Nobody calls it with exactly the balance" is a
  driver gap, not an equivalent mutant: add the value to the driver.
- **Trusting the design's own fields.** Asserting `r.balance >= 0` checks the account
  against itself. Rebuild the expected value from the inputs.

## Try this

Delete the `assert r.ok == (amount <= expected), ...;` statement from
`PSpec/Withdrawals.p`. The tests still pass, and mutation testing reports:

```text
Mutation testing: 11 of 14 mutants detected, 2 suppressed (0.3s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Account.p:22  Account.Serving, on Withdraw: `true` → `false`
  PSrc/Account.p:25  Account.Serving, on Withdraw: removed `ok = w.amount <= balance;`
  PSrc/Account.p:25  Account.Serving, on Withdraw: `w.amount <= balance` → `w.amount < balance`
```

`snappea check --deny mutation` (or `snappea check mutation`) then exits 1.

## Run

```sh
cd examples/samples/concepts/15-mutation-driven-specs
snappea check --deny mutation
snappea check PSrc/Account.p PSpec/Events.p weak/Withdrawals.p PTst/Tests.p --warn lint   # the weak spec
```

## Bounds and omissions

One account, two clients, one withdrawal each. Not modeled: deposits, database
timeouts (see 04-nondeterminism), concurrent transfers between accounts, retried
withdrawals.

## Testing the implementation

Emit `WithdrawRequested` where the caller asks and `WithdrawResponse` where the account
replies (after the debit transaction commits, refusals included). The same mutation
habit applies to the implementation: break the comparison, drop the debit, drop the
reply, and check a test fails each time.
