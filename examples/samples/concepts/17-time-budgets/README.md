---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I check that a retry policy answers within the caller's deadline?
  - How do Elapse, Mark and Elapsed work in a Snap Pea model?
  - Do I need a latency profile or --timed to check a time budget?
---

# Time budgets: three attempts must fit inside a one-second deadline

Plain P has no durations: a timeout is only an ordering choice (see
[09-timers](../09-timers/README.md)). Snap Pea adds time without new syntax: declare a
few foreign functions and each machine gets a clock. Here a client calls a flaky server
with three attempts, a 250 ms per-attempt timeout and 50 then 100 ms of backoff, and
its caller needs an answer within one second. The first policy, with a 300 ms
timeout, does not fit when every reply is lost.

**Contract:**

- The caller gets an answer, success or error, within 1 s of the call
  (`AnswersWithinDeadline`).
- At most 3 attempts, and a retry only after the previous attempt timed out
  (`AnswersWithinDeadline`).
- A success only for an attempt the server answered; an error only after the third
  attempt timed out (`AnswersWithinDeadline`).
- The client answers, and every attempt reaches the server (`AnswersWithinDeadline`
  stays hot until then).

**Atomic primitives:** the server's `Request` handler is one request served (8-36 ms of
work) or lost; a timer firing is one OS timer callback; each client handler is one step
of the client's event loop.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Time.p`](PSrc/Time.p) | The time functions; declaring them turns time on |
| [`PSrc/Timer.p`](PSrc/Timer.p) | A one-shot timer: waits `after`, then sends (a driver) |
| [`PSrc/RetryClient.p`](PSrc/RetryClient.p) | The retry `Client` and the flaky `Server` (a driver) |
| [`PSpec/Budget.p`](PSpec/Budget.p) | `AnswersWithinDeadline` |
| [`PTst/Tests.p`](PTst/Tests.p) | The two policies, the `LongTimer` stand-in, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcRetryWithinDeadline` | 250 ms timeout, a server that may lose each reply: worst case 3 x 250 + 50 + 100 = 900 ms | The retry chain fits the deadline even when every reply is lost; late replies are dropped | Passes |
| `tcServerAnswers` | The same client, a server that always answers, timeouts longer than any reply (`LongTimer`) | Rules out a client that ignores replies and gives up within the deadline | Passes |
| `tcBugRetryOverDeadline` | 300 ms timeout: 3 x 300 + 150 = 1050 ms when every reply is lost | The first policy misses the deadline | Expected bug: `after up to 1050000 us; the deadline is 1000000 us` |

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

**Timed models are searched like any other.** The default run explores every schedule
of all three tests in about a second; read `search_complete` (or `all N schedules`) in
the report as usual.

## The key lines

The caller starts the stopwatch as it calls ([`PTst/Tests.p`](PTst/Tests.p)):

```p
announce Called, Mark();
new Client((server = new Server(dropsReplies), policy = (timeout = timeout, backoff = Backoff())));
```

The spec ([`PSpec/Budget.p`](PSpec/Budget.p)) checks the budget when the client answers,
and checks that the answer is justified by what happened:

```p
assert Elapsed(call) <= ms(1000),
  format("answered (ok = {0}) after up to {1} us; the deadline is 1000000 us", ok, Elapsed(call));
if (ok) {
  assert attempts in replied, format("attempt {0} succeeded, but the server never replied to it", attempts);
} else {
  assert attempts == 3, format("gave up after {0} attempts", attempts);
  assert attempts in timedOut, format("gave up before attempt {0} timed out", attempts);
}
```

The server and the timer announce `Replied` and `TimerFired` just before they send, so
the justification always comes before the client acts on it, in the model and in a real
log. The spec does not guess which message reached the client first.

With `--timed`, the stale-reply branch (`a != attempt`) is no longer covered: a reply
8-36 ms after the request can never lose to a 250 ms timeout. That is the point of
`--timed`, and the reason this sample's default run is untimed: it keeps the "reply
after timeout" race in the checked behaviour.

## Common mistakes

- **Subtracting two `Now()` readings for a budget.** Use `Mark()` and `Elapsed(m)`:
  the checker keeps the shared history between the two readings, so the bound is exact.
- **Measuring on the wrong clock.** `Elapsed` in a spec uses the clock of the machine
  that announced the event. Announce the answer from the client, not from a later
  machine, or the budget includes time the caller never waited.
- **Putting the stopwatch in the code under test.** The caller (the driver) marks the
  start, so a client that forgets to start a timer cannot hide its own lateness.
- **Checking only the deadline.** A client that ignores every reply and gives up after
  three timeouts meets the deadline. `tcServerAnswers` is what makes that fail.

## Try this

Delete `if (a != attempt) { return; }` from `Client.Trying`'s `Reply` handler. A late
reply to attempt 1 or 2 now answers attempt 3:

```text
Assertion Failed: PSpec/Budget.p:50:9 attempt 3 succeeded, but the server never replied to it
```

## Run

```sh
cd examples/samples/concepts/17-time-budgets
snappea check --deny mutation
snappea check --timed     # exits 1: the stale-reply branch is unreachable in time order
```

`tcBugRetryOverDeadline` counts as passing when it fails with the declared message, with
or without `--timed`.

## Bounds and omissions

One client, one server that may lose replies, three attempts, fixed backoffs. Network
delay is zero unless you pass `--latency`; the server's work is 8-36 ms. Not modeled:
jitter on backoff, a deadline propagated to the server, clock skew.

## Testing the implementation

Emit `Attempt` and `Outcome` with timestamps from the client, `Received` and `Replied`
from the server, and `TimerFired` from the timer callback, and check them with
`snappea observe`, whose specs read log timestamps through the same `Mark` and
`Elapsed` functions (see docs/observe.md, "Time and latency budgets"). To hit the worst
case in a test, drop every reply with a fake server and a fake clock.
