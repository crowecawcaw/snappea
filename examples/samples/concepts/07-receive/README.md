---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - When should a client use a blocking receive instead of one state per request?
  - What happens to other events while a machine waits in receive?
  - How do I write the same RPC-style client both ways?
---

# Blocking `receive` versus state handlers: a get/CAS counter client

Real client code is usually written as calls: `v = get(); ok = cas(v.version, v.value + 1)`.
P can model it as a function that blocks in **`receive`** until the reply arrives, or
as **states**, one per outstanding request. Here two clients increment a shared
versioned store with get, then compare-and-swap, and retry when another client wrote
in between; the same client is written both ways and checked against the same spec.

**Contract:**

- Every increment the application asks for is applied exactly once, so the committed
  values go 1, 2, 3 (`IncrementsApplied`).
- Every requested increment commits (`IncrementsApplied` stays hot until then).

**Atomic primitives:** the store's `Get` handler reads the value and its version
together; its `Cas` handler writes only if the version is unchanged. Between the two,
another client can run.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Store.p`](PSrc/Store.p) | The store: `Get` and `Cas`, each one atomic step |
| [`PSrc/ReceiveClient.p`](PSrc/ReceiveClient.p) | The client with `receive` |
| [`PSrc/StateClient.p`](PSrc/StateClient.p) | The same client as states |
| [`PSpec/IncrementsApplied.p`](PSpec/IncrementsApplied.p) | The contract (safety and progress) |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients per test; one gets two increments back to back |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcReceiveClients` | 2 `ReceiveClient`s, 3 increments | CAS conflicts force retries; an increment arrives mid-call | Passes |
| `tcStateClients` | 2 `StateClient`s, 3 increments | The same race, state-based client | Passes |

## The key lines

With `receive`, the retry loop is plain code:

```p
fun ApplyIncrement() {
  var current: (version: int, value: int);
  var done: bool;
  while (!done) {
    current = FetchCurrent();
    done = TryCas(current.version, current.value + 1);
  }
}

fun FetchCurrent(): (version: int, value: int) {
  var result: (version: int, value: int);
  send store, Get, (client = this,);
  receive {
    case Got: (r: (version: int, value: int)) { result = r; }
  }
  return result;
}
```

As states, the same loop is spread over `Reading` and `Writing`, and the retry is a
`goto`:

```p
state Reading {
  defer Increment;
  entry { send store, Get, (client = this,); }
  on Got do (r: (version: int, value: int)) { current = r; goto Writing; }
}

state Writing {
  defer Increment;
  entry { send store, Cas, (client = this, version = current.version, value = current.value + 1); }
  on CasResult do (r: (ok: bool)) {
    if (r.ok) { goto Idle; } else { goto Reading; }
  }
}
```

Both are checked the same way: each `send` and `receive` is a point where other
machines can run.

## What happens to other events while waiting

Here the first client gets a second `Increment` while it is still working on the
first.

- **In `receive`**, only the listed cases are taken. Any other event stays in the queue
  while the machine waits, as if deferred. When the `receive` returns, the machine
  carries on, and the state it is then in must handle, defer or ignore each queued
  event; an event that state does not mention is an unhandled-event error.
  `ReceiveClient` waits inside `Idle`'s `Increment` handler and is still in `Idle`
  afterwards, so the second increment simply runs next.
- **With states**, each waiting state decides for itself. `Reading` and `Writing` must
  each say `defer Increment;`.

So the trade-off: `receive` is easier to read and keeps the algorithm in one place,
but which events are safe to receive meanwhile depends on the state the machine is in
when the call returns, which is easy to miss. Use `receive` for clients that make one
call at a time and get few other events. Use states when the machine must react to
other events while it waits (a cancel, a timeout, a notification), so each reaction is
written down where it applies.

## The suppressed mutant

`Store.p` suppresses one mutant: `version + 1` → `version - 1`. Versions are only
compared for equality, so counting down is equally correct; no execution can tell the
two apart. That is an equivalent mutant, and the suppression comment says why.

## Common mistake: one handler for the whole call

Writing the increment as one store handler (`on Increment do { value = value + 1; }`)
removes the get/CAS gap, and with it the race the model is for. Keep the client's
`send` and `receive` (or states) between reading and writing, so another client can
run there.

## Try this

Remove `defer Increment;` from `StateClient`'s `Writing` state. The first client gets
its second increment while waiting for the CAS reply:

```text
error: StateClient(3) in state 'Writing' received event 'Increment' that it cannot handle
```

## Run

```sh
cd examples/samples/concepts/07-receive
snappea check --deny mutation
```

## Bounds and omissions

Two clients, three increments, one store. Messages are not lost; a client never gives
up retrying, which ends because only three writes ever happen. Each client serves its
increments one at a time, which is the real design for an RPC-style client.

## Testing the implementation

Emit `Committed` from the store where the CAS succeeds, with the new value, and
`IncrementsPlanned` when the test starts. The interesting real-code test forces a
conflict: let a second client write between the first client's get and its CAS.
[12-scenarios](../12-scenarios/README.md) uses the same store and shows how to prove a
test reached that conflict.
