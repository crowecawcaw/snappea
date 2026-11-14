---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model a process that crashes and restarts in P?
  - How do I separate durable state from volatile state in a model?
  - Where can a crash happen in a P model, and where can it not?
---

# Crash and restart: what survives

A ticket server hands out unique, increasing tickets. Its process can crash at any
moment and is restarted by a process manager. The question every recovery design must
answer is **which state survives**: whatever lives only in the process is gone, so the
restarted server must rebuild from durable storage. This sample models that and shows
the checker catching a server that keeps its counter in memory.

## The concept

Model durability by **where state lives**, not by flags:

- **Volatile** state is a field of the crashing machine (`Server.last`, `Server.client`).
- **Durable** state is a field of a separate machine that does not crash (`Storage.highWater`).
- **A crash** ends the machine (`raise halt`). Events still queued for it, and replies
  sent to it later, are dropped, just like in-flight requests to a dead process.
- **A restart** is a *new* machine that starts with fresh fields and reads what it needs
  from storage.

A crash can only land **between handlers**: everything inside one handler runs as one
step. So split the design into a handler per real step wherever a crash between them
matters (here: write to storage, then reply after the ack).

Use this whenever a design claims to survive restarts: counters, idempotency keys,
leases, outboxes, anything "recovered on startup".

## Key lines

The server persists before it replies ([`PSrc/Server.p`](PSrc/Server.p)), in two handlers,
so a crash can land between the write and the reply:

```p
state Serving {
  on Next do (c: machine) {
    client = c;
    last = last + 1;
    send storage, Write, (server = this, value = last);
    goto Persisting;
  }
  on Crash goto Crashed;
}
state Persisting {
  on WriteAck goto Serving with { send client, Ticket, last; ... }
  on Crash goto Crashed;
}
state Crashed { entry { send owner, Exited; raise halt; } }
```

On restart it rebuilds from storage before serving (`Recovering` defers requests until
then). The driver ([`PTst/Tests.p`](PTst/Tests.p)) is the process manager: it may send one
`Crash` at a moment the scheduler picks, then starts a new `Server` on the same
`Storage` and tells the clients its address. A client still waiting resends.

## Test cases

| Test | Drives | Expected |
| --- | --- | --- |
| `tcCrashAnywhere` | Two clients, at most one crash at any point, restart on the same storage | Passes; all schedules explored (about 11,000) |
| `tcBugMemoryOnly` | The same, with a server that keeps its counter only in memory | **Fails**: `ticket 1 issued twice` |

```sh
snappea check tcCrashAnywhere --timeout 20s
snappea check mutation tcCrashAnywhere
snappea check tcBugMemoryOnly        # exits 0 when the declared bug matches
```

## What the checker shows

Output excerpts illustrate the report; counts, timings and source lines vary with
the checker version and run budget. Read the current result before claiming exhaustiveness.

```text
ok    tcCrashAnywhere (all 10994 schedules, 2.98s)
      Coverage: 2/2 states, 45/45 statements, 4/4 branches, 14/14 event handlers, ...
      Driver limits (held in all 10994 schedules):
        no CrashOnce sends Crash after Restarted
        creates 2 Client, 1 to 2 Server and 1 Storage, never more
        CrashOnce sends Crash at most once
```

The coverage line shows every `on Crash` handler ran: the crash landed while
recovering, while idle and while a write was in flight. The driver limits state the
bound: one crash. Auto mode explores the bounded model within the chosen budget.
Read `search_complete` (or `all N schedules`) before claiming exhaustiveness; increasing
`--timeout` gives the checker more time, but does not guarantee completion.

The memory-only server fails in the first schedule: it issues ticket 1, crashes,
restarts from zero and issues ticket 1 again.

```text
expected bug  tcBugMemoryOnly (matching declared failure)
      Assertion Failed: PSpec/Contract.p:22:7 ticket 1 issued twice
```

A crash in `Persisting` shows the other half of the lesson: the write committed, the
client never got the ticket, and the restarted server skips it. Tickets may have gaps;
the contract ([`PSpec/Contract.p`](PSpec/Contract.p)) only forbids duplicates and going
backwards, and requires every client to end up holding a ticket issued to it.

## Common mistakes

- **Resetting fields in an `Crash` handler instead of ending the machine.** Then the
  model decides what a crash forgets, and it is easy to forget too little. A new
  machine forgets everything that is not in storage.
- **Expecting a crash inside a handler.** Moving the reply before the write (`send
  client, Ticket` in `Serving`, then the write) still passes here: both sends happen in
  the same step, and the model's storage never loses a write that was sent. If your real
  code can crash between two writes, put them in separate handlers.
- **Always crashing.** With an unconditional crash, a client that never sends its first
  request still gets its ticket, because the restart makes it resend. Mutation testing
  reported exactly that; the driver now crashes with `if ($)`, so no-crash runs are
  checked too.

## Try this

In `Server.Recovering`, delete `last = highWater;`. The lint fails first:

```text
error[unchecked-state]: machine Storage writes field 'highWater' 1 time, but no assertion,
condition, spec or sent event depends on it, so a wrong value there passes every test case
```

For this deliberately broken exercise only, `--warn lint` lets the checker continue
past that diagnostic and find the bug: `Assertion Failed: PSpec/Contract.p:22:7
ticket 1 issued twice`.

## Bounds and omissions

Two clients, one ticket each, at most one crash, storage never fails or loses a write,
the restarted server is started only after the old one exited (no two live servers).
Not modeled: torn or lost writes, storage crashes, a zombie process still serving
after its replacement starts (that needs fencing), clients that crash.

## Testing the implementation

Emit `Issued` after the reply is sent and `GotTicket` in the client. To force the
crash points, kill the process (or throw from a fault-injection hook) after the
storage write commits but before the reply, then restart it and check that the next
ticket is higher than every ticket handed out.
