---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - What is the smallest P model that checks something useful?
  - What are machines, states, events, send, announce and spec monitors?
  - How do I declare a test case and read the checker's output?
---

# Your first model: a client pings a counter

A P model is a set of **machines** that send each other **events**. The checker runs
the machines in every order the network allows and a **spec monitor** watches the
events and fails the run when the contract breaks. This sample is the smallest model
that does all of that: two clients each ping a shared counter service twice, and the
counter replies with how many pings it has counted.

**Contract:** every planned ping gets exactly one pong, and the k-th pong the counter
sends reports a count of k: no ping is lost or counted twice.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/PingCounter.p`](PSrc/PingCounter.p) | The design: the `Counter` service and the `Client` |
| [`PSpec/PongsCountPings.p`](PSpec/PongsCountPings.p) | The contract as a spec monitor |
| [`PTst/Tests.p`](PTst/Tests.p) | The test driver `Main` and the test case |
| [`FirstModel.pproj`](FirstModel.pproj) | The project file: which folders to load |

## The key lines

A machine is a state machine with its own variables. `on E do` handles an event;
`send` puts an event in another machine's queue and returns at once:

```p
machine Counter {
  var count: int;
  start state Serving {
    on ePing do (p: (client: Client)) {
      count = count + 1;
      send p.client, ePong, (count = count,);
    }
  }
}
```

`entry` runs each time a machine enters a state, including re-entering the same one.
The client sends a ping on each entry to `Pinging` and goes back there until it has
sent enough:

```p
state Pinging {
  entry {
    pinged = pinged + 1;
    send counter, ePing, (client = this,);
  }
  on ePong do {
    if (pinged < pings) { goto Pinging; }
  }
}
```

A spec monitor sees every event in its `observes` list, whether sent with `send` or
recorded with `announce`. A sent event is seen at the moment of the `send`, so the spec
sees pongs in the order the counter sent them. `announce` is for facts no message
carries: here the driver announces how many pings it planned, so the spec knows how
many pongs to demand.

```p
// PTst/Tests.p: the driver
announce ePingsPlanned, 4;
new Client((counter = counter, pings = 2));
new Client((counter = counter, pings = 2));

// PSpec/PongsCountPings.p: the spec
hot state Waiting {
  on ePong do (p: (count: int)) {
    ponged = ponged + 1;
    assert p.count == ponged, format("pong {0} reported count {1}", ponged, p.count);
    if (ponged == planned) { goto AllAnswered; }
  }
}
```

`hot` means "the run must not end here". A run that stops with a pong still owed fails,
so a counter that never replies is caught, not just a counter that replies wrongly.

A test case names the driver (`main`), the specs to check and the machines it may
create:

```p
test tcTwoClients [main=Main]: assert PongsCountPings in { Main, Counter, Client };
```

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClients` | Two clients, two pings each, interleaved | Every pong carries the right count, and none is missing | passes, all 186 schedules |

## Run it

```sh
cd examples/samples/concepts/01-first-model
snappea check --deny mutation
```

```text
ok    tcTwoClients (all 186 schedules, 0.08s)
      Coverage: 3/3 states, 19/19 statements, 4/4 branches, 4/4 event handlers, ...
      Driver limits (held in all 186 schedules):
        creates 1 Counter and 2 Client, never more
        Client is always created with pings = 2
Mutation testing: 17 of 17 mutants detected (0.0s)
```

- **all 186 schedules**: the search was exhaustive. Each schedule is one order in
  which the machines took turns (whose ping the counter handles first, when each
  client handles its pong); the checker tried every one.
- **Coverage**: every statement, branch and handler ran in some schedule.
- **Driver limits**: what this test never tries. Here, two clients and two pings each.
- **Mutation testing**: the checker changed the design 17 ways (removed the increment,
  removed the reply, flipped `<` to `<=`, ...) and the spec caught every one.

## Common mistake: a spec with no hot state

Drop `hot` from `state Waiting` and run `snappea check` again. The test still passes
with full coverage, but mutation testing reports:

```text
Mutation testing: 7 of 17 mutants detected (0.1s)
Not detected by any test case (every test case still passes with the change):
  PSrc/PingCounter.p:11  Counter.Serving, on ePing: removed the whole body (2 statements) ...
  PSrc/PingCounter.p:33  Client.Pinging entry: removed the whole body (2 statements) ...
```

A counter that never replies breaks no assertion: no pong arrives, so no `assert`
runs. Safety specs check what happens; only a hot state checks that something happens.
[03-liveness](../03-liveness/README.md) covers hot states in depth.

## Try this

In `Counter`, reply before counting (swap the two lines in the `ePing` handler). The
check fails on the first schedule:

```text
FAIL  tcTwoClients (bug found after 1 schedule, 0.00s)
      Assertion Failed: PSpec/PongsCountPings.p:23:7 pong 1 reported count 0
      Replay: snappea replay PCheckerOutput/tcTwoClients/bug.schedule
```

## Bounds

Two clients, two pings each, no lost or duplicated messages. P delivers events between
one sender and one receiver in the order they were sent; events from different senders
interleave in every order. [05-multiple-clients](../05-multiple-clients/README.md)
covers per-client state and how many clients a test needs.

## In your implementation

Emit `ePong` with its count from the counter service each time it replies (where the
count is assigned, not where the client receives it), and the planned count when the
test starts. `snappea observe` or `pysnappea` checks the
recorded events against the same `PongsCountPings` spec.
