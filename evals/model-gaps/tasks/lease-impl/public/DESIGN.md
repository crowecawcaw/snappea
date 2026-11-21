# Lease lock with fencing tokens

A lock service grants leases. Each grant carries a fencing token, one higher than the
last. A client that holds a lease writes to a storage service with its token, and
storage accepts a write only if its token is at least the highest it has seen. A
client can stall after its grant (a GC pause) for longer than the lease; meanwhile the
lease expires, another client acquires and writes, and then the stalled client wakes
up and writes. Fencing makes that late write harmless.

## Contract

The specs in `model/PSpec/Contract.p` state it exactly. In words:

- **Leases** (`FencedWrites`): at most one lease is live; tokens strictly increase;
  "busy" is answered only while a lease is live; a release or an expiry ends only the
  live lease (a late release must not free the successor's lease).
- **Fencing** (`FencedWrites`): storage accepts writes only with a token granted to
  that client, in token order. Once a later holder has written, no write from an
  earlier holder is accepted. A write is rejected only when a later token has
  already written.
- **Progress** (`ClientsFinish`): every client ends, with the outcome the services'
  decisions justify: `Wrote` (all its writes accepted), `Fenced` (a write rejected;
  it stops writing), `GaveUp` (it asked for the lock `max_attempts` times and was told
  "busy" each time). A client is granted at most once and makes at most its planned
  number of writes. A client that got the lock does not report its outcome while its
  lease is still live, and every lease ends, by release or by expiry.

What fencing does not promise: storage only knows tokens it has seen. If a paused
holder wakes up before its successor's first write, its write is accepted. The
contract allows that.

## Events

The specs see the system only through these events. Each one marks a **decision**,
and is emitted once, by the node that decides, at the moment it decides:

| Event | Emitted by | When |
| --- | --- | --- |
| `Granted {id, token}` | lock service | it grants client `id` a lease |
| `Busy id` | lock service | it refuses client `id`'s attempt because a lease is live |
| `Released token` | lock service | the holder's release ends the live lease |
| `Expired token` | lock service | the live lease ends because its time ran out |
| `WriteAccepted {id, token}` | storage | it applies a write |
| `WriteRejected {id, token}` | storage | it refuses a write |
| `ClientDone {id, outcome}` | client | it ends (`self.done(outcome)`) |

The simulator emits `ClientsPlanned` and `ClientStarted` itself, and `ClientDone` with
`Crashed` for a client it kills.

## Steps

- **Lock service.** Acquire: if free, grant token `last + 1`; otherwise busy. Release:
  frees the lock only for the live lease's own token. Expire: a lease ends `TTL` ticks
  after its grant unless released first. Each is one atomic step.
- **Storage.** Compare the write's token with the highest seen and accept or reject:
  one atomic step.
- **Client.** Ask for the lock; if busy, wait `BACKOFF` ticks and ask again, up to
  `max_attempts` attempts, then give up. Once granted, make its writes one after the
  other with its token, stopping at the first rejection. Then release (also when
  fenced: the lease may still be its own) and, once the lock service has answered the
  release, report its outcome.

## The environment the implementation runs in

The model checks the protocol with reliable messages and services that never fail.
The implementation runs where these also happen, and the contract must hold all the
same:

- **The network** can lose a message, deliver it twice, deliver it late (longer than
  a lease), and deliver messages in a different order than they were sent. So a
  client repeats a request every `RETRY` ticks until it is answered. Even on a
  healthy network a reply can take longer than `RETRY`.
- **Clients** can pause for any time at their grant, and can die at their grant. A
  client that died never comes back; its lease must still end.
- **The lock service and storage** can crash and restart, at any tick, any number of
  times. A crash happens between handlers, never inside one. A restarted service is
  a new object: it has lost everything in memory and every timer it had set. It
  keeps only its `disk` dict. A service that is down gets no messages; they are lost.
- Clocks are exact and shared; a lease lasts `TTL` ticks of the lock service's clock.

Faults are temporary: every crashed service restarts and the network recovers. After
that, every client must end and every lease must end, within a few hundred ticks.

## Bounds

A handful of clients, each started once, granted at most once, with at most a few
writes and attempts. One lock service, one storage. No lease renewal.
