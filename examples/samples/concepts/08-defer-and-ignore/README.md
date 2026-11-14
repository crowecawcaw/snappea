---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - What happens to an event that arrives in a state that does not handle it?
  - When should a state defer an event, and when should it ignore it?
  - Why is "cannot handle" a design bug and not a modeling annoyance?
---

# Defer and ignore: requests that arrive before the server is ready

A server must load its configuration from storage before it can serve, but clients
and a health probe start at the same time, so their events can arrive while it is
still initializing. Each state must say what happens to every event it can receive:
handle it now, keep it for later (`defer`), or drop it (`ignore`). An event nobody
planned for fails the run, and that failure is the useful part.

| The state says | The event |
| --- | --- |
| `on E do ...` / `on E goto ...` | is handled now |
| `defer E;` | stays in the queue, in order, until a state that handles it; later events can go first |
| `ignore E;` | is taken off the queue and dropped |
| nothing | fails the run: `in state 'S' received event 'E' that it cannot handle` |

**Contract:**

- The server finishes initializing (`ServedAfterInit` is hot while it starts).
- It answers requests and health checks only after loading its configuration, and
  every answer uses that configuration (`ServedAfterInit`).
- Every request is answered, and so is a health check sent once the server is known to
  be serving (`ServedAfterInit` is hot while one is owed).

**Atomic primitives:** storage's `Load` handler is one read; each server handler is one
step of a single-threaded event loop.

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Server.p`](PSrc/Server.p) | `Storage`, `Server`, and the broken `NoDeferServer` |
| [`PSpec/ServedAfterInit.p`](PSpec/ServedAfterInit.p) | The contract: hot while starting and while anything is owed |
| [`PTst/Tests.p`](PTst/Tests.p) | Two clients and a probe racing the server's startup |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcStartupRace` | 2 clients and a probe start with the server; client 1 checks health after its response | Requests and checks before, during and after initialization | Passes |
| `tcBugNoDefer` | Same, with `NoDeferServer` (no `defer Request`) | A request arriving before the configuration | Expected bug: `in state 'Initializing' received event 'Request' that it cannot handle` |

## The key lines

```p
start state Initializing {
  // A client is waiting for this answer: keep the request until Serving.
  defer Request;
  // "Not healthy yet" is the answer: the probe times out and asks again.
  ignore HealthCheck;
  entry (storage: Storage) { send storage, Load, (server = this,); }
  on Loaded do (l: (config: int)) {
    config = l.config;
    announce Ready;
    goto Serving;
  }
}

state Serving {
  on Request do (r: (client: machine, id: int)) { send r.client, Response, (id = r.id, config = config); }
  on HealthCheck do (h: (probe: machine)) { send h.probe, Healthy; }
}
```

- **`defer`** when the event is work someone is waiting for. The deferred requests
  are served, in order, as soon as `Serving` is entered.
- **`ignore`** when the event only means something now, and dropping it is the
  correct answer. A real health probe times out and asks again; answering it late
  would report a state that may no longer hold. (The spec here accepts either,
  because a late "healthy" is still true; the choice is about the real probe.)

Every `ignore` carries a comment saying why the event is safe to drop there. The
drivers ignore the `Healthy` replies they do not use (`ignore Healthy;` in `Client`
and `Probe`); the spec checks those replies, so the drivers have nothing to do with
them.

The negative control fails with a message that names the machine, state and event,
points at the state, and suggests handling, deferring or ignoring it:

```text
error: NoDeferServer(3) in state 'Initializing' received event 'Request' that it cannot handle
```

## Common mistake: `ignore` to silence the error

The quickest way to make "cannot handle" go away is `ignore Request;`. The run no
longer crashes, and the request is gone. Only a progress spec notices:

```text
ServedAfterInit detected liveness bug in hot state 'Owing' at the end of program execution.
```

Before writing `ignore`, ask who is waiting for that event. If anyone is, it needs
`defer` or a handler. [03-liveness](../03-liveness/README.md) shows the same bug with a
busy server.

## Try this

Delete `ignore HealthCheck;` from `Server`'s `Initializing` state. The probe's early
check now has nowhere to go:

```text
error: Server(3) in state 'Initializing' received event 'HealthCheck' that it cannot handle
```

It is not found on the first schedule: in most orders the probe's check arrives after
the server is ready. This is why the checker tries every order instead of the likely
one.

## Run

```sh
cd examples/samples/concepts/08-defer-and-ignore
snappea check --deny mutation
```

`tcBugNoDefer` counts as passing when it fails with the declared message.

## Bounds and omissions

One server, one storage read that always succeeds, two requests, two health checks.
Storage failures and retries of the load are not modeled.

## Testing the implementation

Emit `Ready` where the server finishes loading, `Requested` from the clients' request
wrapper, `Response` and `Healthy` where the server replies, and `HealthCheckDue` from
a probe that checks only after a successful request. To force the race, delay the
storage read in a test and send requests and a health check before releasing it.
