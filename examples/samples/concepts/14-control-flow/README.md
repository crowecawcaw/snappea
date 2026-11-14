---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do goto, raise, entry and exit handlers work in P?
  - How do I pass data into a state?
  - When do I use raise, halt and null?
---

# Control flow: a connection lifecycle

A P machine's control flow is a state machine plus ordinary statements. This sample
shows each construct in a connection that connects with up to two attempts, stays open
until its owner closes it or the server evicts it, and always tells the server goodbye.

## The concept

| Construct | What it does | In this sample |
| --- | --- | --- |
| `goto S;` | Leaves this state (runs its `exit`), enters `S` (runs its `entry`) | `Connecting` re-enters itself for the next attempt |
| `goto S, v;` | The same, passing `v` to `S`'s `entry (x: T)` | `goto Open, r.session;` |
| `on E goto S;` | Handles `E` by moving to `S`; `E`'s payload goes to `S`'s entry | `on Reset goto Closed;` (reason `Evicted`) |
| `on E goto S with { ... }` | Runs the block, then moves | `on Open goto Connecting with (owner: machine) { app = owner; }` |
| `entry` / `exit` | Run on every way in / out of a state | `Open`'s `exit` sends `Fin` on close *and* on reset |
| `raise E, v;` | Handles `E` in this machine now, ahead of queued events | `raise GiveUp, CapacityRefused;` |
| `raise halt;` | Stops the machine; later events to it are dropped | `Closed` |
| `return` in a handler or function | Ends it early; functions can return a value | `FreeSlot(): int` |
| `while`, `foreach`, `if` | As usual, with `break` and `continue`; there is no `for` | `Server`'s slot scan |
| `null` | The empty machine reference | a free slot is `null` |

A `goto` or `raise` ends the handler: statements after it do not run.

## Key lines

[`PSrc/Connection.p`](PSrc/Connection.p):

```p
on SynAck do (r: (ok: bool, session: int)) {
  if (r.ok) { goto Open, r.session; }
  if (attempts < 2) { goto Connecting; }     // entry runs again: next attempt
  raise GiveUp, CapacityRefused;
}
on GiveUp goto Closed;                       // Closed's entry gets CapacityRefused
...
state Open {
  entry (s: int) { session = s; send app, Connected, this; }
  on Close goto Closed;
  on Reset goto Closed;
  exit { send server, Fin, session; }        // every way out says goodbye
}
```

[`PSrc/Server.p`](PSrc/Server.p) scans its slots with a `while` and returns early:

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

The spec ([`PSpec/Contract.p`](PSpec/Contract.p)) checks that no more sessions are open
than slots, that a refusal happens only when every slot is taken, that a connection
gives up after exactly two refusals and closes for a justified reason, and (hot) that
every connection closes and every session is freed.

## Test cases

| Test | Drives | Why |
| --- | --- | --- |
| `tcTwoConnectionsOneSlot` | Two connections, one slot, both close when connected | Eviction, refusal, retry, give-up; close racing reset |
| `tcTwoConnectionsTwoSlots` | Two connections, two slots | Exactly at capacity: nobody is refused, each frees its own session |
| `tcEvictIdleHolder` | The first connection stays open; the second arrives once it is connected | Only eviction can free the slot |

All pass and explore every schedule (about 2,000, 1,300 and 41).

```sh
snappea check --timeout 20s --deny mutation
```

## What the checker shows

Output excerpts illustrate the report; counts, timings and source lines vary with
the checker version and run budget. Read the current result before claiming exhaustiveness.

```text
ok    tcTwoConnectionsOneSlot (all 1966 schedules, 0.68s)
ok    tcTwoConnectionsTwoSlots (all 1291 schedules, 0.34s)
ok    tcEvictIdleHolder (all 41 schedules, 0.01s)
Coverage of all test cases: 3/3 states, 77/77 statements, 16/16 branches, 15/15 event handlers, ...
Mutation testing: 52 of 52 mutants detected (0.3s)
```

Each test exists because mutation testing showed a gap without it:

- With only one slot, every session id is 0, so `removed session = s;` survived
  ("never changes a value"): `tcTwoConnectionsTwoSlots` adds session 1.
- When every application closes on its own, the server never *needs* to evict, so
  removing the eviction survived: `tcEvictIdleHolder` has a holder that never closes.
- Removing `attempts = attempts + 1` survived until the spec asserted that nobody is
  refused a third time.

## Common mistake: cleanup on one path only

Put the goodbye in the close handler instead of `exit`:

```p
on Close goto Closed with { send server, Fin, session; }
on Reset goto Closed;
```

An evicted connection now never frees its slot:

```text
FAIL  tcTwoConnectionsOneSlot (bug found after 1 schedule, 0.02s)
      PImplementation.Lifecycle detected liveness bug in hot state 'Running' at the end of program execution.
```

Use `exit` for anything that must happen however a state is left.

## Try this

Delete the `exit` block of `Open`. Both `tcTwoConnectionsOneSlot` and
`tcEvictIdleHolder` fail with the liveness bug above: sessions are never freed.

## Bounds and omissions

One server, up to two slots, two connections, two attempts each, no lost messages.
Not modeled: keep-alives, half-closed connections, a server that crashes.

## Testing the implementation

Emit `Accepted`, `Rejected` and `Freed` from the server's slot table and `Closed`
from the client's close path. Force eviction by holding one connection open and
connecting a second; force the race by closing the first at the moment it is reset.
