---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I prove a test actually reaches the race I care about?
  - How do I write a scenario monitor with hot and cold states?
  - How do I read scenario coverage and its gaps?
---

# Scenarios: prove the CAS counter test reached the conflict

A passing check says no explored schedule broke a spec. It does not say the schedules
contained the interesting race. A **scenario** is a monitor that asserts nothing: it
records whether some schedule went through a sequence of events you name. Here clients
increment the compare-and-swap counter from [07-receive](../07-receive/README.md) (same
store, same event names); one scenario proves a CAS was refused and then retried, and a
second needs a refusal twice in one run, which two clients can never produce.

**Contract:**

- The committed values go 1, 2, 3, ...: no increment is lost or applied twice
  (`IncrementsApplied`).
- Every planned increment commits and every client finishes (`IncrementsApplied`
  stays hot until then).
- Reached in testing: a refused CAS followed by a commit (`ConflictThenRetry`), and two
  refusals in one run (`RefusedTwice`).

**Atomic primitives:** the store's `Get` handler reads the value and its version
together; its `Cas` handler writes only if the version is unchanged.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Counter.p`](PSrc/Counter.p) | The store (as in 07-receive) and a state-machine client |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `IncrementsApplied` |
| [`PTst/Tests.p`](PTst/Tests.p) | Two drivers, the two scenarios, two test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClients` | Two clients, one increment each | The basic conflict: covers `ConflictThenRetry` | Passes (with `tcThreeClients`; alone it reports `RefusedTwice` as a gap) |
| `tcThreeClients` | Three clients, one increment each | One client can be refused twice: covers both scenarios | Passes |

There is no `tcBug*` test: the contrast here is a scenario gap.

## The concept

A scenario looks like a spec, with `scenario` instead of `spec`:

- **hot** states are steps still to happen; the start state is hot;
- a **cold** state is the goal: a schedule that enters it *covers* the scenario;
- an observed event the current state does not handle leaves the scenario where it
  is. Ordinary design machines and specs still require an explicit event policy.

Scenarios observe sent events as well as announced ones, so they can watch the
design's own messages (`CasResult` here) without extra announcements.

Write one for each race your design discussion names ("both read the same version",
"the timer fires after the reply", "the crash lands mid-write"), and check it is
covered. The spec checks correctness; the scenario only proves the checker went there.

## The key lines

[`PTst/Tests.p`](PTst/Tests.p):

```p
scenario ConflictThenRetry observes CasResult, Committed {
  start hot state WaitConflict {
    on CasResult do (r: (ok: bool)) { if (!r.ok) { goto WaitRetry; } }
  }
  hot state WaitRetry {
    on Committed goto Retried; // with two clients, this is the loser's retry
  }
  cold state Retried { }
}
```

`RefusedTwice` waits for two refusals in one run, which needs three clients.
Reading `ConflictThenRetry` as "the losing client's retry" depends on the two-client
bound. With three clients, its witness proves a refusal followed by a commit, without
identifying which client committed.

## Reading scenario coverage

Selecting only the two-client test:

```text
Not covered by the test case:
  PTst/Tests.p:47  RefusedTwice: scenario never satisfied in 1 test case
...
Unified scenario coverage across 1 test case:
  1/2 scenarios covered in >=1 test case; 1 coverage gap.
  [covered] ConflictThenRetry: covered in 1/1 test cases (tcTwoClients); 81 total triggers, 6 satisfying timelines (summed per test case)
  [  GAP  ] RefusedTwice: never covered in any of 1 test case; best progress anywhere 2/3 states
```

- **A gap fails the check.** A scenario never satisfied is a coverage gap like an
  unreached branch, so `snappea check tcTwoClients` exits 1. Selecting both tests
  covers both scenarios and passes.
- **covered in 1/1 test cases** names the tests that reached the cold state.
- **total triggers** counts schedules that covered it; **satisfying timelines** counts
  distinct event orders among them. One timeline means the race happens in exactly one
  shape; more means the checker found several ways in.
- **best progress 2/3 states** says the furthest any schedule got: `WaitFirst`, then
  `WaitSecond`, never `Seen`. One refusal happened, the second never did. With two
  clients only one can lose a round, so the fix is a test with more clients, not a
  longer timeout.

## Common mistakes

- **Mistaking a prefix for the race.** Reaching `WaitRetry` is partial progress; only
  reaching `Retried` establishes that the later write occurred.
- **A scenario with no cold state, or a cold start state**, can never be meaningful;
  the lints `scenario-never-accepts` and `scenario-cold-start` reject both.
- **Asserting in a scenario.** Put checks in a spec. A covered scenario says the race
  happened, not that the design handled it.

## Try this

Run only `tcTwoClients`, then run both tests. Compare the missing `RefusedTwice` goal
with its best partial progress and the satisfying witness from `tcThreeClients`. Each
witness shows the event sequence that moved the monitor; use it to check that the
scenario means the race you intended.

## Run

```sh
cd examples/samples/concepts/12-scenarios
snappea check --deny mutation        # both tests: both scenarios covered
snappea check tcTwoClients           # exits 1: RefusedTwice is a gap
```

## Bounds and omissions

Two or three clients, one increment each, a store that never fails. Not modeled: lost
messages, crashes, clients giving up after a retry limit.

## Testing the implementation

Emit `Committed` from the store after each committed compare-and-swap and `ClientDone`
when a client finishes. To reproduce `ConflictThenRetry`, pause one client between its
get and its compare-and-swap until another client has written; for `RefusedTwice`,
let two other clients commit in turn while the third keeps retrying.
