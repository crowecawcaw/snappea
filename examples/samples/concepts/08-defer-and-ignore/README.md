---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - What happens to an event that arrives in a state that does not handle it?
  - When should a state defer an event, and when should it ignore it?
  - Why is "cannot be handled" a design bug and not a modeling annoyance?
---

# Event queues: `defer`, `ignore` and unhandled events

Every machine has a queue. When it is ready for the next event, it takes the first one
its current state can deal with:

| The state says | The event |
| --- | --- |
| `on E do ...` / `on E goto ...` | is handled now |
| `defer E;` | stays in the queue, in order, until a state that handles it; later events can go first |
| `ignore E;` | is taken off the queue and dropped |
| nothing | fails the run: `received event 'E' that cannot be handled` |

The failure is the useful part: it means an event can arrive at a moment the design
did not plan for. Decide, per state, whether that event must wait or does not matter.

This sample is a server that must load its configuration from storage before it can
serve. Clients and a health probe start at the same time, so their events can arrive
while it is still initializing.

**Contract:** the server finishes initializing; it answers requests and health checks
only after loading its configuration; every answer uses that configuration; every
request is answered, and so is a health check sent once the server is known to be
serving.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Server.p`](PSrc/Server.p) | `Storage`, `Server`, and the broken `NoDeferServer` |
| [`PSpec/ServedAfterInit.p`](PSpec/ServedAfterInit.p) | The contract: hot while starting and while anything is owed |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients and a probe racing the server's startup |

## The key lines

```p
start state Initializing {
  defer eRequest;        // a request must not be lost: serve it once ready
  ignore eHealthCheck;   // "not ready yet" is the answer: no reply
  entry (storage: Storage) { send storage, eLoad, (server = this,); }
  on eLoaded do (l: (config: int)) {
    config = l.config;
    announce eReady;
    goto Serving;
  }
}

state Serving {
  on eRequest do (r: (client: machine, id: int)) { send r.client, eResponse, (id = r.id, config = config); }
  on eHealthCheck do (h: (probe: machine)) { send h.probe, eHealthy; }
}
```

- **`defer`** when the event is work someone is waiting for. The deferred requests
  are served, in order, as soon as `Serving` is entered.
- **`ignore`** when the event only means something now, and dropping it is the
  correct answer. A real health probe times out and asks again; answering it late
  would report a state that may no longer hold. (The spec here accepts either,
  because a late "healthy" is still true; the choice is about the real probe.)

The drivers ignore replies they do not use (`ignore eResponse;`, `ignore eHealthy;`)
for the same reason: each machine must say what it does with every event it can get.

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcStartupRace` | 2 clients and a probe start with the server; client 1 checks health after its response | Requests and checks before, during and after initialization | passes, all 3,661 schedules |
| `tcBugNoDefer` | Same, with `NoDeferServer` (no `defer eRequest`) | A request arriving before the configuration | **fails**: `NoDeferServer(3) received event 'PImplementation.eRequest' that cannot be handled.` |

## Run it

```sh
cd examples/samples/concepts/08-defer-and-ignore
snappea check tcStartupRace --deny mutation
snappea check tcBugNoDefer             # exits 1
```

```text
ok    tcStartupRace (all 3661 schedules, 0.98s)
      Coverage: 3/3 states, 39/39 statements, 4/4 branches, 15/15 event handlers, ...
Mutation testing: 8 of 8 mutants detected (0.1s)

FAIL  tcBugNoDefer (bug found after 1 schedule, 0.00s)
      NoDeferServer(3) received event 'PImplementation.eRequest' that cannot be handled.
      Replay: snappea replay PCheckerOutput/tcBugNoDefer/bug.schedule
```

The message names the machine and the event, not the state. Replay the schedule to see
it: `NoDeferServer` was still in `Initializing`.

## Common mistake: `ignore` to silence the error

The quickest way to make "cannot be handled" go away is `ignore eRequest;`. The run no
longer crashes, and the request is gone. Only a progress spec notices:

```text
FAIL  tcStartupRace (bug found after 42 schedules, 0.04s)
      PImplementation.ServedAfterInit detected liveness bug in hot state 'Owing' at the end of program execution.
```

Before writing `ignore`, ask who is waiting for that event. If anyone is, it needs
`defer` or a handler. [03-liveness](../03-liveness/README.md) shows the same bug with a
busy server.

## Try this

Delete `ignore eHealthCheck;` from `Server`'s `Initializing` state. The probe's early
check now has nowhere to go:

```text
FAIL  tcStartupRace (bug found after 49 schedules, 0.04s)
      Server(3) received event 'PImplementation.eHealthCheck' that cannot be handled.
```

It took 49 schedules: in most orders the probe's check arrives after the server is
ready. This is why the checker tries every order instead of the likely one.

## Bounds

One server, one storage read that always succeeds, two requests, two health checks.
Storage failures and retries of the load are not modeled.

## In your implementation

Emit `eReady` where the server finishes loading, `eRequested` and `eResponse` from the
clients' request wrapper, and `eHealthCheckDue`/`eHealthy` from a probe that checks
only after a successful request. To force the race, delay the storage read in a test
and send requests and a health check before releasing it.
