---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I use mutation testing to find what my spec does not check?
  - How do I read undetected mutants and turn each into an assertion?
  - When is it right to suppress a mutant, and how?
  - What do the driver limits in the check output mean?
---

# Mutation-driven specs: from "never negative" to a real contract

A spec that passes with full coverage can still check almost nothing. Mutation testing
makes small changes to the design and reruns the tests: a change no test notices marks
behaviour the spec does not constrain. This sample starts from a weak spec for a bank
account, shows the 15 changes it misses, and adds one assertion per kind of gap.

## The concept

`snappea check` runs mutation testing after the tests pass. Each **undetected mutant**
is a line of the design you could break without any test failing. For each one, decide:

1. **A missing check**: add an assertion or a progress (hot-state) obligation. Usual.
2. **A missing scenario**: the test never reaches the state where the change matters.
   Add a test or widen the driver's choices.
3. **An equivalent change**: it cannot change behaviour. Suppress it with a reason.

Lints catch some weak specs before any exploration; mutation testing catches the rest.

## The weak spec

[`weak/Contract.p`](weak/Contract.p) (not part of the project) checks one thing:

```p
spec Contract observes eWithdrawn {
  start state Watching {
    on eWithdrawn do (w: (id: int, ok: bool, balance: int)) {
      assert w.balance >= 0, format("balance went negative: {0}", w.balance);
    }
  }
}
```

Lint already objects, before exploring:

```text
$ snappea check PSrc/Account.p PSpec/Events.p weak/Contract.p PTst/Tests.p
error[unchecked-event]: machine Account sends 'eWithdrawResp', but no spec observes it and no machine that handles it changes anything a check depends on, ...
error[unused-payload-field]: monitor payload field 'w.id' is never read; ...
error[unused-payload-field]: monitor payload field 'w.ok' is never read; ...
error[no-progress-spec]: test 'tcTwoWithdrawals' waits for the model's responses, but none of its specs has a hot state, so a model that stops responding passes
```

With `--warn lint` it explores, passes with full coverage, and mutation testing reports:

```text
ok    tcTwoWithdrawals (all 1125 schedules, 0.13s)
Mutation testing: 1 of 16 mutants detected, 3 suppressed (1.5s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Account.p:12  Account.Open entry: removed `balance = initial;`
  PSrc/Account.p:14  Account.Open, on eWithdraw: removed the whole body (4 statements) (and 10 more undetected inside it)
  PSrc/Account.p:32  Account.Answer: removed the whole body (2 statements) (and 2 more undetected inside it)
```

A *coarse* mutant ("removed the whole body") means nothing notices the handler doing
nothing at all; fix those first. The JSON result lists the mutants inside it
(`mutation.undetected[].undetected_inside`).

## What each assertion kills

The strong spec, [`PSpec/Contract.p`](PSpec/Contract.p), rebuilds the expected balance
from the opening balance and the requested amounts, never from the account's fields.

| Undetected with the weak spec | Added to the spec |
| --- | --- |
| Removed `Answer(...)`, removed the `send` of the reply, the whole handler emptied | **Hot state** `Running` until every client announced `eAnswered`: a withdrawal nobody answers fails at the end |
| `w.amount <= balance` → `<` (refuses exactly the balance); removed `ok = ...`; fast path answers `false`; removed `balance = initial` | `assert w.ok == (requested[w.id] <= expected)`: accept exactly when the balance covers it |
| Removed `balance = balance - w.amount`, `-` → `+`, `if (ok)` removed or forced false, `w.amount == 0` forced true or turned into `!=` | `assert w.balance == expected`: the reported balance is the real one |
| Removed `announce eWithdrawn` | `eAnswered` must match a committed decision (`a.id in decided`) |

Two driver choices make these detectable: an amount equal to the balance (10), the
boundary for `<=` versus `<`; and an amount only the first withdrawal can get (6 after
4), so a missing debit changes a later decision.

## The suppressed mutant

The account answers a zero withdrawal without touching the balance. The general path
gives the same answer for 0, so removing the fast path, or never taking it, cannot be
observed by any test. That is an equivalent mutant, suppressed per kind with a reason
([`PSrc/Account.p`](PSrc/Account.p)):

```p
// snappea-lint: allow mutation:remove-statement -- optimisation; the general path gives the same result for 0
// snappea-lint: allow mutation:condition-false -- optimisation; the general path gives the same result for 0
if (w.amount == 0) {
```

Suppress one kind per line (`mutation:<kind>`), not `mutation` for everything: here
`w.amount == 0` → `!=` is *not* equivalent, and stays checked. The run reports
`3 suppressed`.

## What the checker shows now

```text
ok    tcTwoWithdrawals (all 1587 schedules, 0.27s)
      Coverage: 3/3 states, 36/36 statements, 8/8 branches, 6/6 event handlers, 8/8 condition outcomes, ...
      Driver limits (held in all 1587 schedules):
        each Client sends eWithdraw exactly once
        creates 1 Account and 2 Client, never more
        eWithdraw is only sent with id = 1 or 2
        Account is always created with 10
      hint: if the real environment can do more, extend the driver to try it
Mutation testing: 16 of 16 mutants detected, 3 suppressed (0.1s)
```

**Driver limits** are facts that held in every explored schedule. Each one is a bound
on what the test can find. Here: no client withdraws twice (so a bug that only shows on
a second withdrawal from the same client cannot be found), there are only two clients,
and the opening balance is always 10. If real clients can do more, extend the driver;
if not, write the limit into your design notes as an assumption.

## Test cases

| Test | Drives | Expected |
| --- | --- | --- |
| `tcTwoWithdrawals` | Two clients, amounts from {0, 4, 6, 10} on a balance of 10 | Passes; all schedules (about 1,600); every mutant detected or suppressed |

```sh
snappea check --deny mutation
snappea check PSrc/Account.p PSpec/Events.p weak/Contract.p PTst/Tests.p --warn lint   # the weak spec
```

## Common mistakes

- **Stopping at full coverage.** The weak spec had 100% statement and branch coverage.
- **Suppressing a missing check.** "Nobody calls it with exactly the balance" is a
  driver gap, not an equivalent mutant: add the value to the driver.
- **Trusting the design's own fields.** Asserting `w.balance >= 0` checks the account
  against itself. Rebuild the expected value from the inputs.

## Try this

Delete the `assert w.ok == (requested[w.id] <= expected), ...;` statement from
`PSpec/Contract.p`. The tests still pass, and mutation testing reports:

```text
Mutation testing: 13 of 16 mutants detected, 3 suppressed (0.5s)
Not detected by any test case (every test case still passes with the change):
  PSrc/Account.p:21  Account.Open, on eWithdraw: `true` → `false`
  PSrc/Account.p:24  Account.Open, on eWithdraw: removed `ok = w.amount <= balance;`
  PSrc/Account.p:24  Account.Open, on eWithdraw: `w.amount <= balance` → `w.amount < balance`
```

`snappea check --deny mutation` (or `snappea check mutation`) then exits 1.

## Bounds and omissions

One account, two clients, one withdrawal each. Not modeled: deposits, concurrent
transfers between accounts, retried withdrawals.

## Testing the implementation

Emit `eWithdrawn` after the debit transaction commits (including refusals) and
`eAnswered` where the caller receives the result. The same mutation habit applies to
the implementation: break the comparison, drop the debit, drop the reply, and check a
test fails each time.
