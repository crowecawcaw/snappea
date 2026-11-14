---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I check that a retry policy answers within the caller's deadline?
  - How do Elapse, Mark and Elapsed work in a Snap Pea model?
  - Do I need a latency profile or --timed to check a time budget?
---

# Time budgets: a retry policy inside a deadline

Plain P has no durations: a timeout is only an ordering choice (see `09-timers`). Snap
Pea adds time without new syntax: declare a few foreign functions and each machine gets
a clock. This sample checks that a client with three attempts, a per-attempt timeout
and backoff always answers its caller within one second, and shows the first policy,
which does not.

## The concept

Declare the functions you use ([`PSrc/Time.p`](PSrc/Time.p)); that turns time on.

| Function | Meaning |
| --- | --- |
| `Elapse(lo, hi)` | This machine does work taking `lo..hi` microseconds |
| `Mark()` | Start a stopwatch at this machine's clock; returns a handle |
| `Elapsed(m)` | Worst case of "now minus mark `m`", on the clock of whoever is running (in a spec: the machine whose event it observes) |

A machine created by `new` starts at its creator's clock, a `send` carries the sender's
clock to the receiver, and receiving an event moves the receiver's clock forward to it.
So a timer is still an ordinary machine: it calls `Elapse(after, after)` and sends
([`PSrc/Timer.p`](PSrc/Timer.p)). Every path's worst-case duration is computed exactly.

Use it when a requirement has a number in it: a deadline, an SLA, a timeout that must
exceed a retry chain, a lease that must outlive a renewal.

**No profile needed.** The timeouts and backoffs are model time, so the default
`snappea check` (no `--latency` file, no `--timed`) already checks the budget. A latency
profile (`--latency file.toml`) adds network delay per event; `--timed` delivers
messages in time order, pruning orders that cannot happen. Both are optional; see
[design/time/RetryBudget](../../../../design/time/RetryBudget/README.md).

## Key lines

The caller starts the stopwatch as it calls ([`PTst/Tests.p`](PTst/Tests.p)):

```p
announce Called, Mark();
new Client((server = new Server(), policy = (timeout = timeout, backoff = Backoff())));
```

The spec ([`PSpec/Budget.p`](PSpec/Budget.p)) checks the budget when the client answers:

```p
assert Elapsed(call) <= ms(1000),
  format("answered (ok = {0}) after up to {1} us; the deadline is 1000000 us", ok, Elapsed(call));
```

It also checks that the client retries or gives up only when that attempt's timeout
reached it before the reply, that it makes at most three attempts, and (hot state) that
it answers and every attempt reaches the server.

## Test cases

| Test | Policy | Expected |
| --- | --- | --- |
| `tcRetryWithinDeadline` | 250 ms timeout: worst case 3 x 250 + 50 + 100 = 900 ms | Passes (sampled; see below) |
| `tcBugRetryOverDeadline` | 300 ms timeout: 3 x 300 + 150 = 1050 ms when every reply is lost | **Fails**: `answered (ok = False) after up to 1050000 us; the deadline is 1000000 us` |

```sh
snappea check tcRetryWithinDeadline --timeout 20s
snappea check mutation tcRetryWithinDeadline
snappea check tcBugRetryOverDeadline     # exits 0 when the declared bug matches, with or without --timed
```

## What the checker shows

Output excerpts illustrate the report; counts, timings and source lines vary with
the checker version and run budget. Read the current result before claiming exhaustiveness.

```text
ok    tcRetryWithinDeadline (61744 schedules, time limit reached, 20.0s)
      Coverage: 3/3 states, 53/53 statements, 14/14 branches, 10/10 event handlers, 16/16 condition outcomes, ...
Mutation testing: 30 of 30 mutants detected (0.2s)

expected bug  tcBugRetryOverDeadline (matching declared failure)
      Assertion Failed: PSpec/Budget.p:51:7 answered (ok = False) after up to 1050000 us; the deadline is 1000000 us
```

A timed model is **never searched exhaustively**: the checker samples schedules until
the time limit, so the result says "time limit reached", and `check_samples.py` notes
the search as incomplete. The budget does not depend on the schedule much here (the
worst path is "every reply lost"), so sampling finds the bug in the first schedule.

With `--timed`, coverage drops to 12/14 branches: a reply 8-36 ms after the request
can no longer lose to a 250 ms timeout, so the stale-reply branch never runs. That is
the point of `--timed`, and the reason this sample's default run is untimed: it keeps
the "reply after timeout" race in the checked behaviour.

## Common mistakes

- **Subtracting two `Now()` readings for a budget.** Use `Mark()` and `Elapsed(m)`:
  the checker keeps the shared history between the two readings, so the bound is exact.
- **Measuring on the wrong clock.** `Elapsed` in a spec uses the clock of the machine
  that announced the event. Announce the answer from the client, not from a later
  machine, or the budget includes time the caller never waited.
- **Putting the stopwatch in the code under test.** The caller (the driver) marks the
  start, so a client that forgets to start a timer cannot hide its own lateness.
- **Expecting exhaustive search.** Raise `--timeout` for confidence; read coverage and
  mutation results, not "all N schedules".

## Try this

Delete `if (a != attempt) { return; }` from `Client.Trying`'s `Response` handler. A late
reply to attempt 1 or 2 now answers attempt 3:

```text
FAIL  tcRetryWithinDeadline (bug found after 1 schedule, 0.01s)
      Assertion Failed: PSpec/Budget.p:53:7 attempt 3 answered ok = True, but the other message arrived first
```

## Bounds and omissions

One client, one server that loses replies, three attempts, fixed backoffs. Network
delay is zero unless you pass `--latency`; the server's work is 8-36 ms. Not modeled:
jitter on backoff, a deadline propagated to the server, clock skew.

## Testing the implementation

Emit `Attempt` and `Outcome` with timestamps from the client and check them with
`snappea observe`, whose specs read log timestamps through the same `Mark`/`Elapsed`
functions (see docs/observe.md, "Time and latency budgets"). To hit the worst case in a
test, drop every reply with a fake server and a fake clock.
