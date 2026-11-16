---
diataxis: reference
personas: [application-developer, coding-agent, p-user]
questions:
  - How do I prove that a test reached a specific race?
  - How does a scenario monitor run, and how do I read its summary?
  - What do I do when no test satisfies a scenario?
---

# Scenario monitors

A `scenario` monitor records that an event pattern occurred; it checks nothing. Use
one when a pass means little unless a schedule hit a specific race.

```p
// a compare-and-swap was refused, and a write committed after it
scenario ConflictThenRetry observes eCasResp, eWritten {
  start hot state WaitConflict {
    on eCasResp do (ok: bool) { if (!ok) { goto WaitRetry; } }
  }
  hot state WaitRetry {
    on eWritten goto Retried;
  }
  cold state Retried { }
}
```

- Every scenario watches every test case; do not add it to `assert ... in`.
- It observes `send` and `announce`, like a spec. Prefer an event the model sends.
- A schedule satisfies it on entering a `cold` state other than the start state.
  `hot` never fails a test here.
- `snappea check` ignores observed events a state does not handle; `snappea-compat`
  fails as upstream P does, so add `ignore E;` there.
- An event only a scenario observes is still `unchecked-event`; observe it in a spec.
- Make the pattern distinguish the race: announce an event at the branch only the
  race takes, and compare operation IDs so unrelated operations cannot match.

## Reading the summary

```text
Unified scenario coverage across 2 test cases:
  1/2 scenarios covered in >=1 test case; 1 coverage gap.
  [covered] ConflictThenRetry: covered in 2/2 test cases (tcThreeClients, tcTwoClients); 140 total triggers, 15 satisfying timelines (summed per test case)
  [  GAP  ] RefusedTwice: never covered in any of 2 test cases; best progress anywhere 2/3 states
```

- **total triggers**: schedules that satisfied it, summed over test cases.
- **satisfying timelines**: distinct per-machine event orders among them; few means
  few distinct ways to reach the race.
- **best progress**: the most scenario states one schedule entered.
- `snappea check -v` (JSON `satisfyingExecution`) shows one satisfying history per
  scenario. Check that it really shows the named race.

## A scenario no test satisfies

It is a coverage gap and fails the check (`summary.scenario_gaps`). Extend a test or
raise its bounds; run all test cases before trusting a gap. Accept an intended gap
with a reason:

```p
// snappea-lint: allow coverage -- needs three clients; kept to document the race
scenario RefusedTwice observes eCasResp { ... }
```

Details: [checking models](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md#coverage).

## Check what matched

- `snappea check -v` prints one satisfying execution per covered scenario (JSON:
  `satisfyingExecution`). Confirm it shows the race, not an ordinary ordering that
  matches too: announce an event at the branch that distinguishes the race (for
  example `eStalePushRejected`), require it, and compare operation or version IDs.
- Scenarios are required by default; `// snappea-lint: allow coverage -- reason` on
  the declaration makes one optional. Expected-bug tests satisfy none.
- `not_reached`: every selected search completed without reaching it. `unresolved`:
  some search was incomplete. Neither is a counterexample.
- Machine-wide coverage exemptions are listed with counts; prefer handler or branch
  scopes.
