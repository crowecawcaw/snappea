---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I check that every request is eventually answered?
  - What do hot and cold states mean in a spec monitor?
  - What does a hot state not guarantee (fairness, cycles, time)?
---

# Liveness: every request is eventually answered

A safety spec checks that every answer is right. It cannot notice an answer that never
comes: no event, no assertion. **Liveness** ("something good eventually happens") needs
a spec with a **hot state**: a state the run must not end in. Enter it when an
obligation starts (a request is made), leave it when the obligation is met (the last
pending request is answered).

Add one for every promise of the form "every request gets an answer" or "every job
completes". This sample has a server that hands one request at a time to a worker;
requests that arrive while it is busy must wait, not vanish.

**Contract:** every request a client makes is eventually answered, exactly once.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Server.p`](PSrc/Server.p) | The design: `Server` queues requests while its `Worker` is busy |
| [`PSrc/DroppingServer.p`](PSrc/DroppingServer.p) | Negative control: drops requests while busy |
| [`PSpec/RequestsAnswered.p`](PSpec/RequestsAnswered.p) | The progress spec, with a cold and a hot state |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients against each server, two test cases |

## The key lines

The spec is cold while nothing is owed and hot while any request is pending:

```p
spec RequestsAnswered observes eRequested, eResponse {
  var pending: set[int];
  start cold state AllAnswered {
    on eRequested do (r: (id: int)) { Owe(r.id); goto Waiting; }
    ...
  }
  hot state Waiting {
    on eRequested do (r: (id: int)) { Owe(r.id); }
    on eResponse do (p: (id: int)) {
      assert p.id in pending, format("response to request {0}, which nobody is waiting for", p.id);
      pending -= (p.id);
      if (sizeof(pending) == 0) { goto AllAnswered; }
    }
  }
}
```

The clients are test drivers and announce what they ask for (`eRequested`). The spec
learns of the obligation from the driver, not from the server, so a server that loses
a request cannot also hide it.

The correct server keeps new requests in its queue while busy; the broken one drops
them. Both look harmless, and neither ever sends a wrong reply:

```p
state Busy {
  defer eRequest;      // Server: handle it after returning to Idle
  // ignore eRequest;  // DroppingServer: gone for good
  on eJobDone do (r: tRequest) { send r.client, eResponse, (id = r.id,); goto Idle; }
}
```

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoClients` | Two clients request at once; one arrives while the server is busy | The second request must wait and still be answered | passes, all 769 schedules |
| `tcBugDropsWhileBusy` | Same clients, `DroppingServer` | Shows a liveness bug no safety assertion sees | **fails**: `liveness bug in hot state 'Waiting'` |

## Run it

```sh
cd examples/samples/concepts/03-liveness
snappea check tcTwoClients --deny mutation
snappea check tcBugDropsWhileBusy       # exits 1
```

```text
ok    tcTwoClients (all 769 schedules, 0.25s)
Mutation testing: 11 of 11 mutants detected (0.0s)

FAIL  tcBugDropsWhileBusy (bug found after 1 schedule, 0.00s)
      PImplementation.RequestsAnswered detected liveness bug in hot state 'Waiting' at the end of program execution.
      Replay: snappea replay PCheckerOutput/tcBugDropsWhileBusy/bug.schedule
```

The report names the spec and the hot state, not the request. Replay the schedule
(`snappea replay ...`) to see which request was dropped: client 2's request reached
the server while it was `Busy`.

## What a hot state guarantees, and what it does not

- **It is checked when the run ends.** A run ends when no machine can do anything:
  every queue is empty or holds only deferred events. If a spec is in a hot state then,
  the check fails. That is what caught `DroppingServer`.
- **"Eventually" has no deadline.** A hot state says the answer comes, not how soon.
  Time and timeouts need a timer machine or `--timed`.
- **The scheduler is assumed fair.** The checker does not count it as a bug that one
  machine could be starved forever while others keep running; real liveness can fail
  that way under an unfair scheduler or lossy network that the model does not have.
  Messages are never lost in P unless you model the loss.
- **Runs that never end are only sampled.** If the model can loop forever (a client
  retrying a request the server keeps refusing), there is no end of run to check. Only
  the fair random part of the search looks for a monitor that stays hot for very long;
  an exhaustive search that revisits states proves nothing about such cycles. Bound
  retries in the model, or add a retry budget, so every run ends.

## Common mistake: a safety spec alone

Without `hot`, `RequestsAnswered` still asserts that every response matches a pending
request, and `DroppingServer` passes. The linter catches the missing progress spec
when a driver waits for replies; remove `hot` from `Waiting` and `snappea check` stops
before exploring:

```text
error[no-progress-spec]: test 'tcTwoClients' waits for the model's responses, but none of its specs has a hot state, so a model that stops responding passes
```

## Try this

In `Server`, replace `defer eRequest;` with `ignore eRequest;`. `tcTwoClients` then
fails the same way as the negative control:

```text
FAIL  tcTwoClients (bug found after 1 schedule, 0.00s)
      PImplementation.RequestsAnswered detected liveness bug in hot state 'Waiting' at the end of program execution.
```

(Deleting the line instead gives a different error: an event the state does not
handle. See [08-defer-and-ignore](../08-defer-and-ignore/README.md).)

## Bounds

Two clients, one request each, one worker, no message loss and no timeouts.

## In your implementation

Emit `eRequested` when a client sends a request and `eResponse` when the server
replies. In an observation test, check the recorded events against `RequestsAnswered`
at the end of the test: a request still pending then is the same failure.
