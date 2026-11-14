---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I prove a test actually reaches the race I care about?
  - How do I write a scenario monitor with hot and cold states?
  - How do I read scenario coverage and its gaps?
---

# Scenarios: prove the test reached the race

A passing check says no explored schedule broke a spec. It does not say the schedules
contained the interesting race. A **scenario** is a monitor that asserts nothing: it
records whether some schedule went through a sequence of events you name. This sample
uses two scenarios on a compare-and-swap counter, one covered and one that two clients
can never reach.

## The concept

A scenario looks like a spec, with `scenario` instead of `spec`:

- **hot** states are steps still to happen; the start state is hot;
- a **cold** state is the goal: a schedule that enters it *covers* the scenario;
- an unhandled observed event leaves the scenario in its current state. Ordinary
  design machines and specs still require an explicit event policy.

Scenarios observe sent events as well as announced ones, so they can watch the
design's own messages (`CasResponse` here) without extra announcements.

Write one for each race your design discussion names ("both read the same version",
"the timer fires after the reply", "the crash lands mid-write"), and check it is covered.
The spec ([`PSpec/Contract.p`](PSpec/Contract.p)) checks correctness; the scenario
only proves the checker went there.

## Key lines

[`PTst/Tests.p`](PTst/Tests.p):

```p
scenario ConflictThenRetry observes CasResponse, Written {
  start hot state WaitConflict {
    on CasResponse do (ok: bool) { if (!ok) { goto WaitRetry; } }
  }
  hot state WaitRetry {
    on Written goto Retried; // with two clients, this is the loser's retry
  }
  cold state Retried { }
}
```

`RefusedTwice` waits for two refusals in one run, which needs three clients.
The stronger interpretation of `ConflictThenRetry` as the losing client's retry depends
on the two-client bound. With three clients, its witness proves a refusal followed by
a commit, without identifying which client committed.

## Test cases

| Test | Drives | Covers |
| --- | --- | --- |
| `tcTwoClients` | Two clients, one increment each | `ConflictThenRetry`; all schedules (about 200) |
| `tcThreeClients` | Three clients, one increment each | Both scenarios; all schedules (about 7,500) |

Both pass. There is no `tcBug*` test: the contrast here is a scenario gap.

```sh
snappea check --deny mutation
snappea check tcTwoClients           # shows RefusedTwice as a gap
```

## What the checker shows

Output excerpts illustrate the report; counts, timings and source lines vary with
the checker version and run budget. Read the current result before claiming exhaustiveness.

With only the two-client test selected:

```text
Unified scenario coverage across 1 test case:
  1/2 scenarios covered in >=1 test case; 1 coverage gap.
  [covered] ConflictThenRetry: covered in 1/1 test cases (tcTwoClients); 36 total triggers, 5 satisfying timelines (summed per test case)
  [  GAP  ] RefusedTwice: never covered in any of 1 test case; best progress anywhere 2/3 states
```

How to read it:

- **covered in 1/1 test cases** names the tests that reached the cold state.
- **total triggers** counts schedules that covered it; **satisfying timelines** counts
  distinct event orders among them. One timeline means the race happens in exactly one
  shape; more means the checker found several ways in.
- **best progress 2/3 states** says the furthest any schedule got: `WaitFirst`, then
  `WaitSecond`, never `Seen`. One refusal happened, the second never did. With two
  clients only one can lose a round, so the fix is a test with more clients, not a
  longer timeout.

Selecting both tests covers both scenarios. The lines `ConflictThenRetry: not included
in selected test cases` under "Outside the selected tests" are about *code* coverage
totals: scenarios are not part of them, but they still run in every test.

## Common mistakes

- **A gap does not fail the check.** `snappea check tcTwoClients` exits 0 with the gap
  above. Read the scenario summary, or look for `"satisfied": false` in the JSON.
- **Mistaking a prefix for the race.** Reaching `WaitRetry` is partial progress; only
  reaching `Retried` establishes that the later write occurred.
- **A scenario with no cold state, or a cold start state**, can never be meaningful;
  the lints `scenario-never-accepts` and `scenario-cold-start` reject both.
- **Asserting in a scenario.** Put checks in a spec. A covered scenario says the race
  happened, not that the design handled it.

## Try this

Run only `tcTwoClients`, then run both tests. Compare the missing
`RefusedTwice` goal with its best partial-progress trace and the satisfying witness
from `tcThreeClients`. Each witness shows the event sequence that moved the monitor;
use it to check that the scenario means the race you intended.

## Bounds and omissions

Two or three clients, one increment each, a store that never fails. Not modeled: lost
messages, crashes, clients giving up after a retry limit.

## Testing the implementation

Emit `Written` from the store after each committed compare-and-swap and `ClientDone`
when a client finishes. To reproduce `ConflictThenRetry`, pause one client between its
read and its compare-and-swap until another client has written.
