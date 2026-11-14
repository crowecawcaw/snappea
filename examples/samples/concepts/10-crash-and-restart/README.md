---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I model a process that crashes and restarts in P?
  - How do I separate durable state from volatile state in a model?
  - Where can a crash happen in a P model, and where can it not?
  - How do I show that "persist before replying" matters?
---

# Crash and restart: a ticket server that must not reissue tickets

A ticket server hands out unique, increasing tickets. Its process can crash at any
moment and is restarted by a process manager. Every recovery design must answer
**which state survives**: whatever lives only in the process is gone, so the restarted
server must rebuild from durable storage, and it must have stored a ticket before any
client can hold it.

**Contract:**

- Tickets are unique and increasing, across crashes too (`TicketsUnique`).
- A client accepts only a ticket issued to it, and only one (`TicketsUnique`).
- Every client that asks eventually holds a ticket (`TicketsUnique` stays hot until
  then).

**Atomic primitives:** storage's `Write` handler is one durable write, committed as soon
as storage handles it, even if the server that asked has crashed; its `Read` is one
read. Each server handler is one step of the server process: a crash lands between
handlers, never inside one.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Server.p`](PSrc/Server.p) | The design: persist the counter, then reply |
| [`PSrc/Storage.p`](PSrc/Storage.p) | Durable storage: a separate machine that never crashes |
| [`PSrc/Client.p`](PSrc/Client.p) | A client that resends after a restart if still waiting |
| [`PSrc/MemoryOnlyServer.p`](PSrc/MemoryOnlyServer.p) | Negative control: keeps the counter only in memory |
| [`PSrc/ReplyFirstServer.p`](PSrc/ReplyFirstServer.p) | Negative control: replies first, persists on its next turn |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `TicketsUnique` |
| [`PTst/Tests.p`](PTst/Tests.p) | The process manager `CrashOnce`, module bindings, three test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcCrashAnywhere` | Two clients, at most one crash at any point, restart on the same storage | A crash while recovering, while idle, or with a write in flight must not reissue a ticket | Passes |
| `tcBugMemoryOnly` | The same, with `MemoryOnlyServer` bound in for `Server` | Nothing survives the crash: the restart starts again from ticket 1 | Expected bug: `ticket 1 issued twice` |
| `tcBugReplyFirst` | The same, with `ReplyFirstServer` bound in for `Server` | A crash between the reply and the write loses a ticket a client already holds | Expected bug: `ticket 1 issued twice` |

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
matters.

Use this whenever a design claims to survive restarts: counters, idempotency keys,
leases, outboxes, anything "recovered on startup".

## The key lines

The server persists before it replies ([`PSrc/Server.p`](PSrc/Server.p)), in two
handlers, so a crash can land between the write and the reply:

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
then). The driver ([`PTst/Tests.p`](PTst/Tests.p)) is the process manager: it may send
one `Crash` at a moment the scheduler picks, then starts a new `Server` on the same
`Storage` and tells the clients its address. A client still waiting resends.

A crash in `Persisting` shows the other half of the lesson: the write committed, the
client never got the ticket, and the restarted server skips it. Tickets may have gaps;
the contract only forbids duplicates and going backwards.

## Breaking "persist before replying"

The rule only means something if the model can break it. `ReplyFirstServer` is a
write-behind server: it replies at once and sends itself `Flush` to persist on its next
turn. That puts the crash window in its own handler:

```p
on Next do (c: machine) {
  last = last + 1;
  send c, Ticket, last;      // BUG: the reply goes out before the counter is durable
  ...
  send this, Flush;
  goto Flushing;
}
```

A crash before `Flush` (or before storage handles the write) restarts the server at the
old high-water mark, and it hands ticket 1 out again. Both negative controls reuse the
same driver through module bindings:

```p
module ReplyFirst = { ReplyFirstServer -> Server };

// snappea: expect-bug "ticket 1 issued twice"
test tcBugReplyFirst [main=CrashOnce]:
  assert TicketsUnique in (union { CrashOnce, Storage, Client }, ReplyFirst);
```

## Common mistakes

- **Resetting fields in a `Crash` handler instead of ending the machine.** Then the
  model decides what a crash forgets, and it is easy to forget too little. A new
  machine forgets everything that is not in storage.
- **Expecting a crash inside a handler.** If the reply and the write were sent in the
  same handler, in either order, no crash could land between them. If your real code
  can crash between two steps, put them in separate handlers.
- **Always crashing.** With an unconditional crash, the restart makes every client
  resend, and mutation testing reports 4 undetected mutants, including a client that
  never sends its first request and storage that never records a write. The driver
  crashes with `if ($)`, so runs without a crash are checked too.

## Try this

In `Server.Recovering`, delete `last = highWater;`. The restarted server forgets the
counter, like the memory-only one:

```text
Assertion Failed: PSpec/Contract.p:22:7 ticket 1 issued twice
```

## Run

```sh
cd examples/samples/concepts/10-crash-and-restart
snappea check --deny mutation
```

`tcCrashAnywhere` explores every schedule in a few seconds. The `tcBug*` tests count as
passing when they fail with the declared message.

## Bounds and omissions

Two clients, one ticket each, at most one crash, storage never fails or loses a write,
the restarted server is started only after the old one exited (no two live servers).
Not modeled: torn or lost writes, storage crashes, a zombie process still serving
after its replacement starts (that needs fencing), clients that crash.

## Testing the implementation

Emit `Issued` where the server sends a ticket and `GotTicket` in the client. To force
the crash points, kill the process (or throw from a fault-injection hook) after the
storage write commits but before the reply, and, for a write-behind design, after the
reply but before the flush. Then restart it and check that the next ticket is higher
than every ticket handed out.
