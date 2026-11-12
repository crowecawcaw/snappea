---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What does a lost update look like, and how does P find it?
  - Why do ordinary unit tests miss a read-modify-write race?
  - How does compare-and-swap with retry fix it?
---

# Lost update: read-modify-write of a shared balance

**Difficulty: easy.**

A balance service stores one number. A deposit reads the balance, adds the amount in
the application, and writes the sum back. Each call to the store is atomic; the
read-then-write sequence is not.

**The flaw in one sentence:** two deposits that read the same balance both write
"old balance + my amount", so the second write erases the first deposit.

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `NoLostUpdate` (safety): each committed deposit is applied exactly once, and the
  balance it stores equals the sum of every deposit committed so far.
- `DepositsReturn` (progress, hot state): every deposit the driver starts returns,
  after it committed, with the balance its own write stored.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/ReadModifyWrite.p`](PSrc/ReadModifyWrite.p) | The flawed design: `BlindStore` (get, put) and `ReadModifyWriteClient` |
| [`PSrc/CompareAndSwap.p`](PSrc/CompareAndSwap.p) | The fix: `VersionedStore` (read, compare-and-swap) and `CasClient`, which retries |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers and test cases |

## The counterexample

`snappea check tcBugLostUpdate` fails with:

```text
Assertion Failed: PSpec/Contract.p:30:7 lost update: deposit 2 stored balance 3, but the deposits committed so far sum to 8
```

From `PCheckerOutput/tcBugLostUpdate/bug.txt`:

1. The driver starts deposit 1 (+5) and deposit 2 (+3). The balance is 0.
2. Client 1 sends `get`; the store answers 0.
3. Client 2 sends `get`; the store answers 0.
4. Client 1 computes 0 + 5 and sends `put 5`. The store writes 5.
5. Client 2 computes 0 + 3 and sends `put 3`. The store writes 3: deposit 1 is gone.
   `NoLostUpdate` expected 8.

## Why ordinary tests miss it

A unit test makes one deposit and checks the balance: correct. A test that makes two
deposits one after the other: correct. Only an interleaving where both reads happen
before either write loses money, and with fast local calls a concurrent stress test
rarely lands both reads in that window. P explores every order of the four messages,
and finds it on the first schedule.

## The fix

The store keeps a version next to the balance. `read` returns both; `cas(expected
version, new balance)` writes only if the version is unchanged, as one conditional
`UPDATE ... WHERE version = ?`. A client whose `cas` fails re-reads and recomputes.
A stale read can no longer be written back, so every deposit is added on top of the
latest balance. (A store that offers atomic `increment(amount)` fixes it too, with
no retry; compare-and-swap is shown because it generalizes to any update.)

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugLostUpdate` | Two read-modify-write deposits, +5 and +3 | The race itself | **Fails**: `lost update: deposit 2 stored balance 3, but the deposits committed so far sum to 8` |
| `tcFixedTwoDeposits` | Two CAS deposits | Same race against the fix: the loser retries | Passes, all 200 schedules |
| `tcFixedThreeDeposits` | Three CAS deposits | A client can lose the CAS twice in a row, so the retry must loop | Passes, all 8959 schedules (needs `--timeout`; see below) |

## Run

```sh
snappea check tcFixedTwoDeposits tcFixedThreeDeposits --deny mutation --timeout 20s
snappea check tcBugLostUpdate          # exits 1 with the lost update above
```

Without `--timeout`, `tcFixedThreeDeposits` stops early once coverage stops growing;
with it, the search completes in about 3 seconds.

One mutant is suppressed: `version + 1` → `version - 1` in `VersionedStore`, because
versions are only compared for equality and counting down keeps them unique.

## Bounds and omissions

Two or three deposits, one balance, starting at 0. Messages are not lost and the
store does not crash. Not modeled: withdrawals and overdraft rules, retried requests
(see `flawed/in-flight-duplicate`), a retry limit.

## Testing the implementation

Emit `eBalanceWritten(id, balance)` after the store's write commits, and
`eDepositReturned` when the deposit call returns. To force the race, wrap the store
so the second deposit's read runs after the first deposit's read and before its
write; with the flawed code the final balance is short by one deposit, with the fix
the first `cas` fails and retries.
