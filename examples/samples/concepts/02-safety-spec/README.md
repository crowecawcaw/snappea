---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - How do I write a safety spec that checks results independently of the design?
  - Where should I announce events, and when can a spec observe sent events directly?
  - Why does a spec fed by reply-receipt events report false bugs?
---

# Safety specs: two writers race on a register

A **safety spec** says "nothing bad ever happens": a read never returns a stale value,
a seat is never sold twice. In P it is a `spec` monitor that observes events, keeps its
own copy of what the system *should* look like, and asserts every result against that
copy. Here two writers race to write a register, then a reader reads it; either write
can land last, and the spec must know which one did without reading the register.

**Contract:**

- A read returns the value of the last write that committed before it
  (`LastWriteWins`).
- The read is answered (`LastWriteWins` stays hot until it is).

**Atomic primitives:** the register's `Write` handler is one atomic write (a row update
or a put), and its `Read` handler is one atomic read.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Register.p`](PSrc/Register.p) | The design: one register; write and read are each one atomic step |
| [`PSpec/LastWriteWins.p`](PSpec/LastWriteWins.p) | `LastWriteWins` (correct) and `LastAckWins` (wrong on purpose) |
| [`PTst/Tests.p`](PTst/Tests.p) | Two `Writer`s, the driver that reads after both finish, two test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoWritersThenRead` | Writers of 1 and 2 race; the driver reads after both are acknowledged | Either write can land last; the read must match whichever did | Passes |
| `tcBugSpecUsesAckOrder` | Same driver, checked by `LastAckWins` | Shows that reply-receipt order is not commit order: a correct register fails a badly instrumented spec | Expected bug: `but the last acknowledged write was` |

## The key lines

The register **announces at the commit point**: in the handler that changes the value,
so the spec sees writes in the order they took effect.

```p
on Write do (w: (client: machine, value: int)) {
  value = w.value;
  announce WriteCommitted, (value = w.value,);
  send w.client, WriteAck, (value = w.value,);
}
```

The spec rebuilds the expected value from those announcements, and compares each read
reply against it. `format` puts the actual and expected values in the failure message,
so a counterexample explains itself:

```p
spec LastWriteWins observes WriteCommitted, ReadResponse {
  var expected: int;
  start hot state Unread {
    on WriteCommitted do (c: (value: int)) { expected = c.value; }
    on ReadResponse do (r: (value: int)) {
      assert r.value == expected,
        format("read returned {0}, but the last committed write was {1}", r.value, expected);
      goto ReadAnswered;
    }
  }
  ...
}
```

`ReadResponse` is a normal `send`, observed directly. A spec sees a sent event at the
moment of the `send`, inside the sender's handler, so observing it is as good as an
announcement when the send happens in the same atomic step as the commit. Use
`announce` when no message marks the commit, or the message is sent later.

The `hot` start state is not the point of this sample. It makes "the read is answered"
checkable, so a register that drops an acknowledgement fails too
(see [03-liveness](../03-liveness/README.md)).

## Common mistake: announcing where the reply arrives

It is tempting to instrument the client: "when my write is acknowledged, tell the
spec". `LastAckWins` does exactly that, through `WriteAckReceived` announced by each
`Writer`. The register is correct, yet `tcBugSpecUsesAckOrder` fails:

```text
Assertion Failed: PSpec/LastWriteWins.p:44:7 read returned 2, but the last acknowledged write was 1
```

1. The register commits 1, then 2, and sends both acknowledgements.
2. Writer 2 handles its acknowledgement first, then writer 1 handles its own.
3. The spec now believes 1 was the last write; the read correctly returns 2.

Acknowledgements to *different* machines can be handled in any order, so the spec saw
a history that never happened. Announce inside the handler that performs the atomic
step. In implementation tests the same rule holds: record the event where the store
commits (or with the store's version number), not where the client gets its reply.

## Try this

Make the register keep the first write: replace `value = w.value;` with
`if (value == 0) { value = w.value; }`. `tcTwoWritersThenRead` then fails with a
message such as:

```text
Assertion Failed: PSpec/LastWriteWins.p:19:7 read returned 2, but the last committed write was 1
```

## Run

```sh
cd examples/samples/concepts/02-safety-spec
snappea check --deny mutation
```

`tcBugSpecUsesAckOrder` counts as passing when it fails with the declared message.

## Bounds and omissions

Two writers, one write each, one read after both finish. Reads concurrent with writes
are not modeled; with them, the spec would accept any value committed before the read
was served, which is exactly what `expected` holds at that moment.

## Testing the implementation

Emit `WriteCommitted` from the store right after the write transaction commits (or
attach the store's version to the reply and order by it), and `ReadResponse` with the
value a read returned. Check those events against `LastWriteWins`. To force the race,
hold the first writer's acknowledgement until the second write has committed.
