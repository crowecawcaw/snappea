---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model a request timeout in P?
  - How do I model a timer that fires after it was cancelled?
  - Why does P treat time as an ordering choice, and what does that catch?
  - How do I check a timeout race without guessing which message arrived first?
---

# Timers: a timeout racing the reply

A client calls a server with a timeout. The server may never answer, so the timeout is
what guarantees the call ends. But the reply and the timeout race: a reply can arrive
after the client gave up, and a timer can fire after the client cancelled it. The
client must tag both with the request id and drop whatever belongs to an earlier
request.

**Contract:**

- Each request reaches the server and ends exactly once (`OutcomesJustified`, hot
  until then).
- A success is justified by the server's reply to that request and carries the
  server's result for it (`OutcomesJustified`).
- A failure is justified by that request's own timeout (`OutcomesJustified`).
- When the server always answers and the timeout is longer than any reply, every call
  succeeds (`tcServerAnswers` with `OutcomesJustified`).

**Atomic primitives:** the server's `Request` handler is one request served (or lost);
the timer firing is one OS timer callback; each client handler is one step of the
client's event loop.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Client.p`](PSrc/Client.p) | The `Client` and the `Server` it calls (a driver that may drop replies) |
| [`PSrc/Timer.p`](PSrc/Timer.p) | A one-shot timer machine with cancel |
| [`PSrc/StaleReplyClient.p`](PSrc/StaleReplyClient.p) | Negative control: a client without the reply's id check |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `OutcomesJustified` |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the `LongTimer` stand-in, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTimeoutRace` | One client, two requests in a row, a server that may drop each reply, timers that fire before or after it | Late replies and timers that fire after their cancel must be dropped | Passes |
| `tcServerAnswers` | The same client, a server that always answers, timeouts longer than any reply (`LongTimer`) | Rules out a client that ignores replies and waits for its timer | Passes |
| `tcBugStaleReply` | `StaleReplyClient` against the dropping server | A late reply to request 1 is taken as the answer to request 2 | Expected bug: `request 2 returned 2, but the server computes 4 for it` |

## The concept

This sample uses an untimed model. A timer is a separate machine that sends `Timeout`
on a turn the scheduler picks, so the checker tries the timeout **before and after**
every reply. That is the right abstraction for most designs: you do not know which
comes first, and the client must be correct either way. (For deadlines measured in
milliseconds, see [17-time-budgets](../17-time-budgets/README.md).)

Use a timer machine whenever code waits with a timeout, retries after a delay, expires
a lease or hold, or sends heartbeats.

## The key lines

The timer ([`PSrc/Timer.p`](PSrc/Timer.p)) fires on its own later turn, so a cancel that
is already queued wins and a later one loses:

```p
start state Armed {
  entry (t: (holder: machine, tag: int)) { ...; send this, Fire; }
  on Fire goto Fired;        // announces TimerFired, sends Timeout to the holder
  on Cancel goto Cancelled;
}
```

The client ([`PSrc/Client.p`](PSrc/Client.p)) tags the request and its timer with the
request id and drops anything for an earlier request:

```p
on Reply do (r: (id: int, result: int)) {
  if (r.id != current) { return; }   // late reply to a request that timed out
  send timer, Cancel;                // best effort
  Decide(true, r.result);
}
on Timeout do (tag: int) {
  if (tag != current) { return; }    // a cancelled timer that fired anyway
  Decide(false, 0);
}
```

The spec ([`PSpec/Contract.p`](PSpec/Contract.p)) does not guess which message reached
the client first. It checks that each outcome is **justified by the history**: a
success needs a `Replied` for that request with the right result, a failure needs a
`TimerFired` for it. The server and the timer announce just **before** their `send`,
so the justification always comes first, in the model and in a real log:

```p
if (o.ok) {
  assert o.result == requested[o.id] * 2,
    format("request {0} returned {1}, but the server computes {2} for it", o.id, o.result, requested[o.id] * 2);
  assert o.id in replied, format("request {0} succeeded, but the server never replied to it", o.id);
} else {
  assert o.id in timedOut, format("request {0} timed out, but its timer never fired", o.id);
}
```

Those rules allow a client that ignores every reply and waits for its timer. That is
what `tcServerAnswers` rules out: with a server that always answers and a `LongTimer`
bound in place of `Timer` (a timeout longer than any reply, so in this model it never
fires), a client that drops replies never decides, and the hot state fails.

The two requests use different values (1 and 2), so a reply credited to the wrong
request is a wrong result, not an invisible one. Full branch coverage proves the
checker reached both stale cases (`r.id != current` and `tag != current` were each true
in some schedule).

## Common mistakes

- **Inferring inbox order from announcements.** A spec that says "the reply was
  announced before the timeout, so the client must succeed" works only in the model.
  In a real log, network latency separates "sent" from "received", and the check
  raises false alarms. Check that each outcome is justified instead.
- **Expecting the cancel to be checked.** The design is correct whether or not the cancel
  wins, so removing it changes nothing a spec can see. Mutation testing reports that; the
  sample suppresses it as an equivalent mutant (`a timeout that fires anyway is ignored
  by its tag`). If your design *relies* on cancel winning, it has a race.
- **Treating the server as code under test.** The server only stands in for the
  environment and its faults, so it is marked `// snappea: driver`: its dropped reply
  is test input and is not mutated.

The negative control `StaleReplyClient` ignores `r.id` on purpose, so the payload lint
reports exactly its flaw. The suppression there names that: `negative control: its
flaw is that it ignores r.id`.

## Try this

Delete `if (tag != current) { return; }` in `Client`'s `Timeout` handler. A timer for
request 1 that fired after its cancel now ends request 2:

```text
Assertion Failed: PSpec/Contract.p:48:9 request 2 timed out, but its timer never fired
```

## Run

```sh
cd examples/samples/concepts/09-timers
snappea check --deny mutation
```

`tcBugStaleReply` counts as passing when it fails with the declared message.

## Bounds and omissions

One client, two sequential requests, one timer per request, requests always reach the
server, replies may be dropped. Not modeled: retries (see
[17-time-budgets](../17-time-budgets/README.md) and
[11-unreliable-network](../11-unreliable-network/README.md)), durations (any timeout
may beat any reply), several clients sharing a server.

## Testing the implementation

Emit `Requested` when the test issues a call, `Received` and `Replied` from the server
(`Replied` just before the reply is sent), `TimerFired` from the timer callback before
it notifies the client, and `Outcome` from the client. The same `OutcomesJustified`
spec then checks a real log. To force the race in a test, use a fake clock: fire the
timeout, then deliver the held reply; and complete a call, then fire its cancelled timer.
