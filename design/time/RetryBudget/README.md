# RetryBudget: the time extension without new syntax

Sample 5 of `../examples/` rewritten with no new syntax. The extension is:

| Piece | Where |
|---|---|
| `Now()`, `NowBest()`, `Elapse(lo, hi)`, `Mark()`, `Elapsed(m)`, `ElapsedBest(m)` | foreign functions in `PSrc/Time.p` |
| One-shot `Timer` | plain P in `PSrc/Timer.p` |
| Delivery latencies | `PTst/latency.toml` |
| Time-ordered scheduling | `snappea check --timed` |

Runs with the current build:

```
$ snappea check tcRetry --timeout 10s                        # no latency file
FAIL  tcRetry (bug found after ...)
      Assertion Failed: PSpec/Budget.p:8:7 resolved (ok = False) after up to 1050000us; deadline is 1000000us

$ snappea check tcRetry --timeout 10s --latency PTst/latency.toml          # same bug
$ snappea check tcRetry --timeout 10s --latency PTst/latency.toml --timed  # same bug
```

Time is tracked as soon as the model declares the time functions: the timer
durations and backoffs are model time, so the all-drops path (3 x 300ms timeout
+ 50ms + 100ms backoff = 1050ms) exceeds the 1s budget even with zero
latencies. The latency file adds network time (1..2ms per hop here; 60..90ms in
`cross-region`).

Fix: `timeoutAfter = ms(250)` makes the give-up path 900ms and the check passes
in all three modes. The slowest successful path is the third attempt answered
at the last moment: 750ms + 2 + 36 + 2 = 790ms.

The bug's schedule file, `PCheckerOutput/tcRetry/bug.schedule`, records the time
mode and the resolved profile (`--snappea-time:` and `--snappea-latency:` lines), so
`snappea replay PCheckerOutput/tcRetry/bug.schedule` needs no other options.

## What `--timed` changes here

Untimed, a Timer's `eTimeout` (stamp +300ms) can be handled before an `eResp`
stamped around +40ms, so the checker also explores retries after successful replies.
They do not change the worst-case bound here, but a spec counting attempts per
request would report impossible retry counts. With `--timed` those orders are
pruned.

## Trade-offs vs. the syntax in PROPOSAL.md

* Times are `int` microseconds; `Now()` is the worst case and `NowBest()` the
  best case. No uncertain comparisons: `if (Now() > deadline)` is
  pessimistic (fails fast whenever the deadline might have passed).
* Per-send latency overrides become Timer machines or profile entries.
* Budgets use the stopwatch `Mark()` / `Elapsed(m)`, not `Now() - t0`: the
  checker keeps the correlation between the two readings (affine forms, see
  PROPOSAL.md), so shared history cancels. Subtracting two worst cases could
  understate a budget after a fan-in; subtracting two intervals overstates it.
* The `Timer` is a separate machine, so the scheduler already chooses when its
  timeout lands relative to replies. (The native `send ... latency d` needs
  delivery to be its own scheduler step to get the same; see PROPOSAL.md.)
