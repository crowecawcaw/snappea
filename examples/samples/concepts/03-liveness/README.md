---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I check that every request is eventually answered?
  - What do hot and cold states mean in a spec monitor?
  - What does a hot state not guarantee (fairness, cycles, time)?
---

# Liveness: a busy server must not lose requests

A safety spec checks that every answer is right. It cannot notice an answer that never
comes: no event, no assertion. **Liveness** ("something good eventually happens") needs
a spec with a **hot state**, a state the run must not end in. Here a server hands one
request at a time to a worker; a request that arrives while it is busy must wait, not
vanish, and no wrong reply ever shows the difference.

**Contract:**

- Every request a client makes is eventually answered, exactly once
  (`RequestsAnswered`).

**Atomic primitives:** each server handler is one step of a single-threaded dispatcher:
take a request off the queue and hand it to the worker, or take the worker's result and
reply.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Server.p`](PSrc/Server.p) | The design: `Server` queues requests while its `Worker` is busy |
| [`PSrc/DroppingServer.p`](PSrc/DroppingServer.p) | Negative control: drops requests while busy |
| [`PSpec/RequestsAnswered.p`](PSpec/RequestsAnswered.p) | The progress spec, with a cold and a hot state |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients against each server, two test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClients` | Two clients request at once; one request arrives while the server is busy | The second request must wait and still be answered | Passes |
| `tcBugDropsWhileBusy` | Same clients, `DroppingServer` | A liveness bug no safety assertion sees | Expected bug: `RequestsAnswered detected liveness bug in hot state 'Waiting'` |

## The key lines

The spec is cold while nothing is owed and hot while any request is pending. Enter the
hot state when an obligation starts, leave it when the last one is met:

```p
spec RequestsAnswered observes Requested, Response {
  var pending: set[int];
  start cold state AllAnswered {
    on Requested do (r: (id: int)) { Owe(r.id); goto Waiting; }
    ...
  }
  hot state Waiting {
    on Requested do (r: (id: int)) { Owe(r.id); }
    on Response do (p: (id: int)) {
      assert p.id in pending, format("response to request {0}, which nobody is waiting for", p.id);
      pending -= (p.id);
      if (sizeof(pending) == 0) { goto AllAnswered; }
    }
  }
}
```

The clients are test drivers and announce what they ask for (`Requested`). The spec
learns of the obligation from the driver, not from the server, so a server that loses
a request cannot also hide it.

The correct server keeps new requests in its queue while busy; the broken one drops
them. Both look harmless, and neither ever sends a wrong reply:

```p
state Busy {
  defer Request;      // Server: handle it after returning to Idle
  // ignore Request;  // DroppingServer: gone for good
  on JobDone do (r: Task) { send r.client, Response, (id = r.id,); goto Idle; }
}
```

The failure names the spec, the hot state and the spec's fields, so you can see which
request was dropped:

```text
RequestsAnswered detected liveness bug in hot state 'Waiting' at the end of program execution.
RequestsAnswered's fields: pending = {2}
```

Replay the schedule (`snappea replay PCheckerOutput/.../bug.schedule`) to see client 2's
request reach the server while it was `Busy`.

## What a hot state guarantees, and what it does not

- **It is checked when the run ends.** A run ends when no machine can do anything:
  every queue is empty or holds only deferred events. If a spec is in a hot state then,
  the check fails. That is what caught `DroppingServer`.
- **"Eventually" has no deadline.** A hot state says the answer comes, not how soon.
  Time and timeouts need a timer machine ([09-timers](../09-timers/README.md)) or time
  budgets ([17-time-budgets](../17-time-budgets/README.md)).
- **The scheduler is assumed fair.** The checker does not count it as a bug that one
  machine could be starved forever while others keep running; real liveness can fail
  that way under an unfair scheduler or lossy network that the model does not have.
  Messages are never lost in P unless you model the loss.
- **Runs that never end are only sampled.** If the model can loop forever (a client
  retrying a request the server keeps refusing), there is no end of run to check. Bound
  retries in the model, or add a retry budget, so every run ends.

## Common mistake: a safety spec alone

Without `hot`, `RequestsAnswered` still asserts that every response matches a pending
request, and `DroppingServer` would pass. The linter catches the missing progress spec
when a driver waits for replies; remove `hot` from `Waiting` and `snappea check` stops
before exploring:

```text
error[no-progress-spec]: test 'tcTwoClients' waits for the model's responses, but none of its specs has a hot state, so a model that stops responding passes
```

## Try this

In `Server`, replace `defer Request;` with `ignore Request;`. `tcTwoClients` then
fails the same way as the negative control:

```text
RequestsAnswered detected liveness bug in hot state 'Waiting' at the end of program execution.
```

Deleting the line instead gives a different error, an event the state does not handle:
`Server(2) in state 'Busy' received event 'Request' that it cannot handle`. See
[08-defer-and-ignore](../08-defer-and-ignore/README.md).

## Run

```sh
cd examples/samples/concepts/03-liveness
snappea check --deny mutation
```

`tcBugDropsWhileBusy` counts as passing when it fails with the declared message.

## Bounds and omissions

Two clients, one request each, one worker, no message loss and no timeouts.

## Testing the implementation

Emit `Requested` when a client sends a request and `Response` when the server replies.
In an observation test, check the recorded events against `RequestsAnswered` at the
end of the test (`snappea observe --check-hot`): a request still pending then is the
same failure. To force the race, hold the worker on the first job until the second
request has arrived.
