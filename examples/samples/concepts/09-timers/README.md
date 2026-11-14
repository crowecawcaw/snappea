---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model a request timeout in P?
  - How do I model a timer that fires after it was cancelled?
  - Why does P treat time as an ordering choice, and what does that catch?
---

# Timers: a timeout racing the reply

A client calls a server with a timeout. The server may never answer, so the timeout is
what guarantees the call ends. But the reply and the timeout race: a reply can arrive
after the client gave up, and a timer can fire after the client cancelled it. This
sample shows how to model that and check the client handles both.

## The concept

This sample uses an untimed model. A timer is a separate machine that sends
`Timeout` on a turn the
scheduler picks, so the checker tries the timeout **before and after** every reply.
That is the right abstraction for most designs: you do not know which comes first, and
the client must be correct either way. (For deadlines measured in milliseconds, see
`17-time-budgets`.)

Use a timer machine in a design model whenever code waits with a timeout, retries after
a delay, expires a lease or hold, or sends heartbeats.

## Key lines

The timer ([`PSrc/Timer.p`](PSrc/Timer.p)) fires on its own later turn, so a cancel that
is already queued wins and a later one loses:

```p
start state Armed {
  entry (t: (holder: machine, tag: int)) { ...; send this, Fire; }
  on Fire goto Fired;        // sends Timeout to the holder
  on Cancel goto Cancelled;
}
```

The client ([`PSrc/Client.p`](PSrc/Client.p)) tags the request and its timer with the
request id and drops anything for an earlier request:

```p
on Reply do (r: (id: int, result: int)) {
  if (r.id != current) { return; }   // late reply to a request that timed out
  send timer, Cancel;               // best effort
  Decide(true, r.result);
}
on Timeout do (tag: int) {
  if (tag != current) { return; }    // a cancelled timer that fired anyway
  Decide(false, 0);
}
```

The spec ([`PSpec/Contract.p`](PSpec/Contract.p)) checks that each request reaches the
server and ends exactly once, that a success carries this request's result, and that
the outcome matches whichever message reached the client's inbox first. The server and
the timer announce right **after** their `send`, which is the moment the message is in
the inbox.

## Test cases

| Test | Drives | Expected |
| --- | --- | --- |
| `tcTimeoutRace` | One client, two requests in a row, a server that may drop each reply | Passes; all schedules explored |
| `tcBugStaleReply` | A client without the reply's id check | **Fails**: `request 2 returned 2, but the server computes 4 for it` |

```sh
snappea check tcTimeoutRace --deny mutation
snappea check tcBugStaleReply        # exits 0 when the declared bug matches
```

## What the checker shows

Output excerpts illustrate the report; counts, timings and source lines vary with
the checker version and run budget. Read the current result before claiming exhaustiveness.

```text
ok    tcTimeoutRace (all 1111 schedules, 0.27s)
      Coverage: 2/2 states, 55/55 statements, 14/14 branches, 12/12 event handlers, ...
      Driver limits (held in all 1111 schedules):
        creates 1 Server, 1 Client and 2 Timer, never more
        Server sends Reply at most twice
...
Mutation testing: 39 of 39 mutants detected, 1 suppressed (0.3s)
```

Full branch coverage matters here: it proves the checker reached both stale cases
(`r.id != current` and `tag != current` were each true in some schedule). The broken
client fails in a schedule where request 1 times out, request 2 is sent, and then the
late reply to request 1 arrives:

```text
expected bug  tcBugStaleReply (matching declared failure)
      Assertion Failed: PSpec/Contract.p:42:9 request 2 returned 2, but the server computes 4 for it
```

The two requests use different values (1 and 2) so that a reply credited to the wrong
request is a wrong result, not an invisible one.

## Common mistakes

- **Announcing before the `send`.** The scheduler can switch machines at a `send`, so an
  `announce` placed before it can be followed by other machines' sends that reach the
  inbox first. Move `announce Served` above the `send` in `Server` and `tcTimeoutRace`
  fails with `request 2 reported ok = False, but the other message arrived first`,
  a false alarm from the model, not the client.
- **Expecting the cancel to be checked.** The design is correct whether or not the cancel
  wins, so removing it changes nothing a spec can see. Mutation testing reports that; the
  sample suppresses it with a reason (`allow mutation:remove-statement -- cancel is best
  effort; ...`). If your design *relies* on cancel winning, it has a race.
- **Treating the server as code under test.** The server only stands in for the
  environment and its faults, so it is marked `// snappea: driver`: its dropped reply
  is test input and is not mutated.

## Try this

Delete `if (tag != current) { return; }` in `Client`'s `Timeout` handler. A timer for
request 1 that fired after its cancel now ends request 2:

```text
FAIL  tcTimeoutRace
      Assertion Failed: PSpec/Contract.p:47:7 request 2 reported ok = False, but the other message arrived first
```

## Bounds and omissions

One client, two sequential requests, one timer per request, requests always reach the
server, replies may be dropped. Not modeled: retries (see `11-unreliable-network`),
durations (any timeout may beat any reply), several clients sharing a server.

## Testing the implementation

Emit `Received` and `Served` from the server, `TimerFired` from the timer callback and
`Outcome` from the client. To force the race in a test, use a fake clock: fire the
timeout, then deliver the held reply; and complete a call, then fire its cancelled timer.
