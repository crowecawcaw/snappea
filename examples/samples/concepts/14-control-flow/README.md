---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do goto, raise, entry and exit handlers work in P?
  - How do I pass data into a state?
  - When do I use raise, halt and null?
---

# Control flow: a connection that retries, gets evicted, and always says goodbye

A P machine's control flow is a state machine plus ordinary statements. Here a
connection connects with up to two attempts, stays established until its app closes
it or the server evicts it, and always tells the server goodbye so the slot is freed.
The close and the eviction race, so the goodbye must happen on every way out.

**Contract:**

- No more sessions are open than the server has slots, and a session is never given
  out twice (`ConnectionsWithinSlots`).
- A connection is refused only when every slot is taken, and tries at most twice
  (`ConnectionsWithinSlots`).
- Every connection closes for a reason the history justifies: its app asked
  (`CloseRequested`), the server evicted it (`Evicted`), or it was refused twice
  (`ConnectionsWithinSlots`).
- Every connection closes and every session is freed (`ConnectionsWithinSlots` stays
  hot until then).

**Atomic primitives:** the server's `Syn` handler checks and claims a slot in one step
(or evicts and refuses); its `Fin` handler frees one. Each connection handler is one
step of the client's event loop.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Connection.p`](PSrc/Connection.p) | The connection's lifecycle: every construct in the table below |
| [`PSrc/Server.p`](PSrc/Server.p) | The server's slot table, with a `while` scan and early return |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ConnectionsWithinSlots` |
| [`PTst/Tests.p`](PTst/Tests.p) | The `App` driver, three test drivers, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoConnectionsOneSlot` | Two connections, one slot, both close when connected | Eviction, refusal, retry, give-up; close racing reset | Passes |
| `tcTwoConnectionsTwoSlots` | Two connections, two slots | Exactly at capacity: nobody is refused, each frees its own session | Passes |
| `tcEvictIdleHolder` | The first connection stays open; the second arrives once it is connected | Only eviction can free the slot | Passes |

## The concept

| Construct | What it does | In this sample |
| --- | --- | --- |
| `goto S;` | Leaves this state (runs its `exit`), enters `S` (runs its `entry`) | `Connecting` re-enters itself for the next attempt |
| `goto S, v;` | The same, passing `v` to `S`'s `entry (x: T)` | `goto Established, r.session;` |
| `on E goto S;` | Handles `E` by moving to `S`; `E`'s payload goes to `S`'s entry | `on Reset goto Closed;` (reason `ServerEviction`) |
| `on E goto S with { ... }` | Runs the block, then moves | `on Open goto Connecting with (owner: machine) { app = owner; }` |
| `entry` / `exit` | Run on every way in / out of a state | `Established`'s `exit` sends `Fin` on close *and* on reset |
| `raise E, v;` | Handles `E` in this machine now, ahead of queued events | `raise GiveUp, CapacityRefused;` |
| `raise halt;` | Stops the machine; later events to it are dropped | `Closed` |
| `return` in a handler or function | Ends it early; functions can return a value | `FreeSlot(): int` |
| `while`, `foreach`, `if` | As usual, with `break` and `continue`; there is no `for` | `Server`'s slot scan |
| `null` | The empty machine reference | a free slot is `null` |

A `goto` or `raise` ends the handler: statements after it do not run.

## The key lines

[`PSrc/Connection.p`](PSrc/Connection.p):

```p
on SynAck do (r: (ok: bool, session: int)) {
  if (r.ok) { goto Established, r.session; }
  if (attempts < 2) { goto Connecting; }     // entry runs again: next attempt
  raise GiveUp, CapacityRefused;
}
on GiveUp goto Closed;                       // Closed's entry gets CapacityRefused
...
state Established {
  entry (s: int) { session = s; send app, Connected, this; }
  on Close goto Closed;
  on Reset goto Closed;
  exit { send server, Fin, session; }        // every way out says goodbye
}
```

[`PSrc/Server.p`](PSrc/Server.p) scans its slots with a `while` and returns early. When
every slot is taken it evicts the holder of slot 0, a simple deterministic policy:

```p
fun FreeSlot(): int {
  var i: int;
  while (i < sizeof(slots)) {
    if (slots[i] == null) { return i; }
    i = i + 1;
  }
  return -1;
}
```

The spec can tell the close reasons apart because each one has its own evidence: the
app announces `CloseRequested` before it sends `Close`, and the server announces
`Evicted` where it sends `Reset`. A connection that reports `ClientClose` without a
request, or `ServerEviction` without an eviction, fails.

Each test exists because mutation testing showed a gap without it:

- With only one slot, every session id is 0, so removing `session = s;` survived
  ("never changes a value"): `tcTwoConnectionsTwoSlots` adds session 1.
- When every app closes on its own, the server never *needs* to evict, so removing
  the `Reset` survived: `tcEvictIdleHolder` has a holder that never closes.

## Common mistake: cleanup on one path only

Put the goodbye in the close handler instead of `exit`:

```p
on Close goto Closed with { send server, Fin, session; }
on Reset goto Closed;
```

An evicted connection now never frees its slot, and the tests with eviction fail:

```text
ConnectionsWithinSlots detected liveness bug in hot state 'Running' at the end of program execution.
```

Use `exit` for anything that must happen however a state is left.

## Try this

In `Server`, send the wrong reason with the reset: `send slots[0], Reset, ClientClose;`.
The evicted connection now claims its app closed it:

```text
Assertion Failed: PSpec/Contract.p:72:9 Connection(3) closed by its client, but its app never asked
```

## Run

```sh
cd examples/samples/concepts/14-control-flow
snappea check --deny mutation
```

## Bounds and omissions

One server, up to two slots, two connections, two attempts each, no lost messages.
Not modeled: keep-alives, half-closed connections, a server that crashes.

## Testing the implementation

Emit `Accepted`, `Rejected`, `Evicted` and `Freed` from the server's slot table,
`CloseRequested` where the app closes, and `ConnectionClosed` from the client's close
path with its reason. Force eviction by holding one connection open and connecting a
second; force the race by closing the first at the moment it is reset.
