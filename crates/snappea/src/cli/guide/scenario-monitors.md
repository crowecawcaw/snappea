# Scenario monitors

A `scenario` monitor proves a test reached a situation, such as a race: it measures
coverage and checks nothing. Use it when a passing test means little unless some
schedule hit a specific interleaving. (`snappea guide scenarios` is about choosing test
scenarios; this page is about the `scenario` declaration.)

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

## How it runs

- Every scenario watches every test case; do not add it to `assert ... in`.
- It observes events as a spec does: events sent with `send` as well as `announce`d
  ones. Prefer observing an event the model really sends: it shows the race happened
  in the model's own messages.
- A schedule satisfies the scenario when it enters a `cold` state other than the start
  state. Mark the start and waiting states `hot` (or leave them unmarked) and the goal
  `cold`. `hot` here never fails a test: scenarios are exempt from liveness checking.
- In `snappea check`, an observed event a state has no handler for is ignored, so a
  scenario needs no `ignore` lines. `snappea-compat` keeps upstream P's behavior: it
  fails with `received event ... that cannot be handled`; add `ignore E;` for upstream.
- Scenarios check nothing. An event only a scenario observes is still unchecked
  (`unchecked-event`); observe it in a spec too.

## Reading the summary

```text
Unified scenario coverage across 2 test cases:
  1/2 scenarios covered in >=1 test case; 1 coverage gap.
  [covered] ConflictThenRetry: covered in 2/2 test cases (tcThreeClients, tcTwoClients); 140 total triggers, 15 satisfying timelines (summed per test case)
  [  GAP  ] RefusedTwice: never covered in any of 2 test cases; best progress anywhere 2/3 states
```

- **total triggers**: explored schedules that satisfied the scenario, summed over the
  test cases.
- **satisfying timelines**: distinct timelines among those schedules, summed over the
  test cases. A timeline abstracts a schedule by the order in which each machine
  received its events, so schedules that differ only in unrelated interleavings count
  once. Few timelines means few distinct ways of reaching the situation.
- **best progress anywhere 2/3 states**: the most distinct states of the scenario one
  schedule entered, in any test case. It shows how far the tests got before the
  scenario stalled.

## A scenario no test satisfies

A scenario that no selected test case satisfies is a coverage gap: `snappea check`
lists it under `Not covered`, counts it in `summary.coverage_gaps` and
`summary.scenario_gaps`, and fails like uncovered code. Add or extend a test case that
drives the model through it, or raise the test's bounds. Running a subset of the test
cases can leave a scenario that another test case satisfies unsatisfied; run them all
before relying on the gap. If the gap is intended, accept it on the scenario with a
reason:

```p
// snappea-lint: allow coverage -- needs three clients; kept to document the race
scenario RefusedTwice observes eCasResp { ... }
```

See [checking models](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md#coverage).
