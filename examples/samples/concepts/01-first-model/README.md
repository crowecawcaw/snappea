---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - What is the smallest P model that checks something useful?
  - What are machines, states, events, send, announce and spec monitors?
  - How do I declare a test case and read the checker's output?
---

# Your first model: two clients ping a shared counter

A P model is a set of **machines** that send each other **events**. The checker runs
the machines in every order the network allows, and a **spec monitor** watches the
events and fails the run when the contract breaks. Here two clients each ping a shared
counter twice, and the counter replies with how many pings it has counted; the pings
interleave in every order.

**Contract:**

- Every planned ping gets exactly one pong, and the k-th pong the counter sends reports
  a count of k: no ping is lost or counted twice (`PongsCountPings`).

**Atomic primitives:** the counter's `Ping` handler counts and replies in one step, like
a request handler that increments a counter under a lock.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/PingCounter.p`](PSrc/PingCounter.p) | The design: the `Counter` service and the `Client` |
| [`PSpec/PongsCountPings.p`](PSpec/PongsCountPings.p) | The contract as a spec monitor |
| [`PTst/Tests.p`](PTst/Tests.p) | The test driver `Main` and the test case |
| [`FirstModel.pproj`](FirstModel.pproj) | The project file: which folders to load |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClients` | Two clients, two pings each, interleaved | Every pong carries the right count, and none is missing, whichever client the counter serves first | Passes |

## The key lines

A machine is a state machine with its own variables. `on E do` handles an event;
`send` puts an event in another machine's queue and returns at once:

```p
machine Counter {
  var count: int;
  start state Serving {
    on Ping do (p: (client: Client)) {
      count = count + 1;
      send p.client, Pong, (count = count,);
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
    send counter, Ping, (client = this,);
  }
  on Pong do {
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
announce PingsPlanned, 4;
new Client((counter = counter, pings = 2));
new Client((counter = counter, pings = 2));

// PSpec/PongsCountPings.p: the spec
hot state Waiting {
  on Pong do (p: (count: int)) {
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

## Reading the output

```text
ok    tcTwoClients (all 256 schedules, 2 workers, 0.02s)
      Coverage: 3/3 states, 19/19 statements, 4/4 branches, 4/4 event handlers, 4/4 condition outcomes, 1/2 transition pairs
      Driver limits (held in all 256 schedules):
        creates 1 Counter and 2 Client, never more
        Client is always created with pings = 2
      hint: if the real environment can do more, extend the driver to try it
Mutation testing: 17 of 17 mutants detected (0.1s)
```

- **all N schedules**: the search was exhaustive. Each schedule is one order in which
  the machines took turns (whose ping the counter handles first, when each client
  handles its pong); the checker tried every one. The count depends on the checker
  version and the number of workers; "all" is what matters.
- **Coverage**: every statement, branch and handler ran in some schedule.
- **Driver limits**: what this test never tries. Here, two clients and two pings each.
- **Mutation testing**: the checker changed the design 17 ways (removed the increment,
  removed the reply, flipped `<` to `<=`, ...) and the spec caught every one.

## Common mistake: a spec with no hot state

Drop `hot` from `state Waiting`. The linter stops the check before it explores:

```text
error[no-progress-spec]: machine Client waits for 'Pong' after sending a request, but no spec of test 'tcTwoClients' has a hot state, so a model that never replies passes
```

With `--warn lint` the test passes with full coverage, but mutation testing detects
only 7 of 17 mutants: a counter that never replies, or a client that never pings, goes
unnoticed: no
pong arrives, so no `assert` runs. Safety specs check what happens; only a hot state
checks that something happens. [03-liveness](../03-liveness/README.md) covers hot
states in depth.

## Try this

In `Counter`, reply before counting (swap the two lines in the `Ping` handler). The
check fails at once:

```text
Assertion Failed: PSpec/PongsCountPings.p:23:7 pong 1 reported count 0
```

## Run

```sh
cd examples/samples/concepts/01-first-model
snappea check --deny mutation
```

## Bounds and omissions

Two clients, two pings each, no lost or duplicated messages. P delivers events between
one sender and one receiver in the order they were sent; events from different senders
interleave in every order. [05-multiple-clients](../05-multiple-clients/README.md)
covers per-client state and how many clients a test needs.

## Testing the implementation

Emit `Pong` with its count from the counter service each time it replies (where the
count is assigned, not where the client receives it), and `PingsPlanned` when the test
starts. `snappea observe` or `pysnappea` checks the recorded events against the same
`PongsCountPings` spec.
