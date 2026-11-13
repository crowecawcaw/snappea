# Scenario monitors

A `scenario` monitor records that an event pattern occurred: it measures coverage
and checks nothing. Make the pattern distinguish the situation you want to reach. Use it when a passing test means little unless some
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


## Inspect what actually matched

`snappea check -v` shows one satisfying execution for each covered scenario, naming
its test case. JSON includes `satisfyingExecution` in each covered scenario entry,
also saved in that test's `scenario_coverage.json`. Older artifacts without evidence
still load. Normal text keeps the compact coverage summary.

The evidence lists the events observed by that scenario, including ignored events,
through its first acceptance. Each line includes the sending/announcing actor,
event and payload, scenario state before handling, and scenario handler location
(or state location when there is no handler). These are observation events, not a
record of message delivery to the destination machine. Other actors' steps and
unobserved events are omitted; this is not a full replay trace.

The checker keeps the shortest observed accepting history by event count, combining
workers and selected tests. It does not search for a globally shortest history.
Evidence retains at most 128 events and 1024 characters per event; `truncated` marks
omissions, while `observedEvents` and `acceptingState` describe the acceptance even
when the retained prefix does not reach it. Recording evidence does not change
scheduling, state hashing, or scenario acceptance.

Check whether the history really demonstrates the named race. For example,
`eRollbackRequested` followed by `eConfigApplied` does not establish that rollback
overtook a delayed push: an ordinary ordered execution could match too. Announce an
event such as `eStalePushRejected` at the branch that distinguishes the race, then
require that event in the scenario. Carry operation or version IDs and compare them
so events from unrelated operations cannot satisfy the intended sequence.

Use an existing design event when it establishes the same fact. Keep correctness
assertions in specs; a satisfied scenario alone does not prove correctness.
