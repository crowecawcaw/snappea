---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - When should a client use a blocking receive instead of one state per request?
  - What happens to other events while a machine waits in receive?
  - How do I write the same RPC-style client both ways?
---

# Blocking `receive` versus state handlers

Real client code is usually written as calls: `v = get(); ok = cas(v.version, v.value + 1)`.
P can model it two ways:

- **`receive`**: a function sends a request and blocks until the reply arrives, like a
  synchronous RPC. The algorithm reads top to bottom, close to the real code.
- **States**: one state per outstanding request; the reply's handler decides the next
  step. More lines, but every waiting point is a state you can see, name and control.

Both are checked the same way: each `send` and `receive` is a point where other
machines can run. This sample writes one client both ways and checks both against
the same spec.

**The design:** a versioned store with two atomic primitives, `get` and
compare-and-swap. A client increments the value with get, then CAS at the version it
read, and retries if another client wrote in between.

**Contract:** every increment the application asks for is applied exactly once, so the
committed values go 1, 2, 3.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Store.p`](PSrc/Store.p) | The store: `eGet` and `eCas`, each one atomic step |
| [`PSrc/ReceiveClient.p`](PSrc/ReceiveClient.p) | The client with `receive` |
| [`PSrc/StateClient.p`](PSrc/StateClient.p) | The same client as states |
| [`PSpec/IncrementsApplied.p`](PSpec/IncrementsApplied.p) | The contract (safety and progress) |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients per test; one gets two increments back to back |

## The key lines

With `receive`, the retry loop is plain code:

```p
fun Increment() {
  var current: (version: int, value: int);
  var done: bool;
  while (!done) {
    current = Get();
    done = Cas(current.version, current.value + 1);
  }
}

fun Get(): (version: int, value: int) {
  var result: (version: int, value: int);
  send store, eGet, (client = this,);
  receive {
    case eGetResp: (r: (version: int, value: int)) { result = r; }
  }
  return result;
}
```

As states, the same loop is spread over `Reading` and `Writing`, and the retry is a
`goto`:

```p
state Reading {
  defer eIncrement;
  entry { send store, eGet, (client = this,); }
  on eGetResp do (r: (version: int, value: int)) { current = r; goto Writing; }
}

state Writing {
  defer eIncrement;
  entry { send store, eCas, (client = this, version = current.version, value = current.value + 1); }
  on eCasResp do (r: (ok: bool)) {
    if (r.ok) { goto Idle; } else { goto Reading; }
  }
}
```

## What happens to other events while waiting

Here the first client gets a second `eIncrement` while it is still working on the
first. Every event that arrives while a machine waits must be handled, deferred or
ignored, or the run fails with "cannot be handled":

- **In `receive`**, only the listed cases are taken. Any other event stays in the queue
  if the *current state* handles, defers or ignores it, and is processed after the
  `receive` returns. `ReceiveClient` waits inside `Idle`'s `eIncrement` handler, and
  `Idle` handles `eIncrement`, so the second increment simply waits its turn. An event
  the state does not mention at all is an error, even if a later state would handle it.
- **With states**, each waiting state decides for itself. `Reading` and `Writing` must
  each say `defer eIncrement;`.

So the trade-off: `receive` is easier to read and keeps the algorithm in one place, but
what happens to other events depends on the state the machine happens to be in, which
is easy to miss. Use `receive` for clients that make one call at a time and get few
other events. Use states when the machine must react to other events while it waits
(a cancel, a timeout, a notification), so each reaction is written down where it applies.

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcReceiveClients` | 2 `ReceiveClient`s, 3 increments | CAS conflicts force retries; an increment arrives mid-call | passes, all 885 schedules |
| `tcStateClients` | 2 `StateClient`s, 3 increments | The same race, state-based client | passes, all 920 schedules |

## Run it

```sh
cd examples/samples/concepts/07-receive
snappea check --deny mutation
```

```text
ok    tcReceiveClients (all 885 schedules, 0.36s)
ok    tcStateClients (all 920 schedules, 0.23s)
Coverage of all test cases: 3/3 states, 50/50 statements, 6/6 branches, 8/8 event handlers, ...
Mutation testing: 39 of 39 mutants detected, 1 suppressed (0.4s)
```

The suppressed mutant is `version + 1` → `version - 1` in the store: versions only need
to differ, so counting down is equally correct. The suppression comment in
`PSrc/Store.p` says so.

## Common mistake: one handler for the whole call

Writing the increment as one store handler (`on eIncrement do { value = value + 1; }`)
removes the get/CAS gap, and with it the race the model is for. Keep the client's
`send` and `receive` (or states) between reading and writing, so another client can
run there.

## Try this

Remove `defer eIncrement;` from `StateClient`'s `Writing` state. The first client gets
its second increment while waiting for the CAS reply:

```text
FAIL  tcStateClients (bug found after 1 schedule, 0.02s)
      StateClient(3) received event 'PImplementation.eIncrement' that cannot be handled.
```

## Bounds

Two clients, three increments, one store. Messages are not lost; a client never gives
up retrying, which ends because only three writes ever happen.

## In your implementation

Emit `eCommitted` from the store where the CAS succeeds, with the new value, and the
number of increments when the test starts. The interesting real-code test forces a
conflict: let a second client write between the first client's get and its CAS.
