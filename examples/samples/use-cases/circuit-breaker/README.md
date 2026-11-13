---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a circuit breaker with concurrent callers?
  - How do I keep a late result from counting toward the wrong period?
  - How do I check that half-open admits exactly one trial?
---

# Circuit breaker in front of a flaky dependency

A client-side circuit breaker protects a flaky dependency. Closed: every call goes
through; K consecutive failures open it (a success resets the count). Open: calls are
rejected at once until a cooldown timer fires. Half-open: exactly one trial call goes
through; others are rejected while it is in flight. Trial success closes the breaker,
trial failure reopens it. The breaker state is an `enum tBreakerState`.

**Contract**

- While open, no call reaches the dependency. While half-open, at most one trial is
  in flight. Calls are rejected only while open or while the trial is in flight.
- The breaker opens exactly when K consecutive results of the current closed period
  failed. A result for a call sent before the last transition (a late result) counts
  for nothing.
- Half-open only after the cooldown of the current open period; the trial's result
  decides closed or open.
- Progress: every call gets exactly one answer (Rejected only if short-circuited,
  otherwise the dependency's answer), and the breaker does not stay open.

**Atomic primitive**: the breaker handles one event at a time (one lock in code).
Calls, dependency answers and timer firings interleave freely. Each call is tagged
with the breaker's *period* (its transition count) when it was sent, and each
cooldown carries the period that started it; both make stale events harmless.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `BreakerContract` (safety; its `Opened` state is hot) and `CallsAnswered` (progress) |
| [`PSrc/Breaker.p`](PSrc/Breaker.p) | The breaker |
| [`PSrc/AdmitAllBreaker.p`](PSrc/AdmitAllBreaker.p) | Negative control: half-open admits every caller |
| [`PTst/Tests.p`](PTst/Tests.p) | Scripted dependency, timer service (optionally duplicating), callers, tests |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcConcurrentCallers` | Two callers, three calls, two failures then success, K=2 | Calls in flight when the breaker opens; rejections; the trial | pass, exhaustive |
| `tcHalfOpenRace` | One failure opens (K=1); then two callers race | Two callers in half-open: exactly one trial | pass, exhaustive |
| `tcLateFailure` | Three failures, one answer slow; then a successful trial and one failure | The slow failure lands after the breaker reclosed and must not count | pass, sampled |
| `tcSuccessResetsCount` | One caller: F, T, F, F, trial T, F, F | A success resets the count; the count restarts after reclosing | pass, exhaustive |
| `tcDuplicateCooldown` | Every cooldown delivered twice, arbitrarily late; first trial fails | A stale cooldown during the trial or the next open period must not open a window | pass, exhaustive |
| `tcBugHalfOpenAdmitsAll` | `AdmitAllBreaker` in the half-open race | **Expected to fail**: `call 30 sent while a half-open trial was in flight` | fail |

The `LateResultAfterReclose` scenario confirms `tcLateFailure` reaches a result sent
before the breaker opened arriving after it closed again.

## What building it taught

- **Announce the result before the transition, then send.** The breaker announces
  `eResultSeen` and any transition before it replies to the caller. A send is a
  scheduling point; announcing after it lets other events slip between "result seen"
  and "state changed", and the spec could no longer require the transition next.
- **Removing `failures = 0` on a transition survived** until `tcSuccessResetsCount`
  reached a second closed period. A breaker that carries the count over never opens
  again; only a test that reopens it shows that.
- **Duplicate timers must be delayed independently** (`LateDuplicate`). Two back-to-back
  sends never put the duplicate into the next open period, so the period check on the
  cooldown went untested.
- **Keep maps small** in the breaker and specs (forget finished calls). Growing maps
  make every state distinct and blew the search up several times over.
- The dependency, timer and callers are drivers in `PTst/`: only the breaker is the
  design, so only it is mutated.

## Run

```sh
snappea check tcConcurrentCallers tcHalfOpenRace tcLateFailure tcSuccessResetsCount tcDuplicateCooldown --timeout 30s --deny mutation
snappea check tcBugHalfOpenAdmitsAll      # exits 1: call 30 sent while a half-open trial was in flight
```

The `--timeout` matters: without it the test cases share 10s, and the tests marked
exhaustive above may stop before covering every schedule. Each completes in under 10s
(`tcConcurrentCallers`, the largest, explores about 19,000 schedules), but the search
order depends on the seed: with some `--seed` values `tcHalfOpenRace` (2,500
schedules) is only sampled within 30s. An exhaustive run reports `all N schedules`.

## Bounds and omissions

One to four callers, at most eight calls, K of 1 or 2, one dependency whose outcomes
are scripted by arrival order. In `tcLateFailure` only call 40's answer can be
overtaken, which keeps the search small. Not modeled: call timeouts (a hung dependency
call keeps the trial in flight forever), real time, sliding-window failure rates,
half-open with more than one trial. "At most one in flight while half-open" counts the
trial only: calls sent while closed can still be in flight after the breaker opened.

Suppressed mutants: removing `callers -= (r.call)` and `sentIn -= (r.call)` in
`Breaker.p`. They only free memory; call ids are never reused, so keeping the entries
is unobservable.

## Testing the implementation

Emit `eDispatched`, `eShortCircuited`, `eResultSeen` and `eBreakerChanged` from the
breaker under its lock, `eDepAnswered` from a dependency stub, `eCooldownElapsed`
from the timer. To force the races: block a failing dependency call until the breaker
has opened and reclosed, then release it; fire the cooldown callback twice; start two
callers on a barrier right after the cooldown fires.
