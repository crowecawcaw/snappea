# Distributed read/write lock service

Design and implement a distributed lock service for a storage cluster, then write
`LockServer` and `LockClient` in `solution.py` against the interface in `sim.py`.

## What we want

The cluster has `N` lock servers (`self.servers`; tests use 3 to 5) and any number of
clients. Application code on a client locks a named resource for writing (exclusive) or
reading (shared), uses it, then unlocks it. There is no central coordinator: a client
broadcasts its lock request to every lock server, each server grants or refuses from
its own lock table, and the client holds the lock once a quorum of servers has granted
it. Read locks need a smaller quorum than write locks, so readers stay available when
more servers are down. Unlock tells the servers to drop the entry.

Locks must not get stuck. If a client crashes while holding a lock, or a release never
arrives, the stale entries must eventually go away on their own so that other clients
can lock the resource again. The original design did this with a periodic maintenance
pass on each server that checks long-lived entries; you may keep or replace that.

## Environment

- Servers keep no durable state. A server can crash at any time and may later restart
  with an empty lock table (`on_start` runs on a fresh object). A minority of servers
  may be down for a long time.
- Clients can crash at any time, including while holding a lock, and never come back.
- The network can lose, duplicate and reorder messages. Most messages arrive within
  `MAX_DELAY` (5) ticks, but a message can be delayed arbitrarily long.
- All nodes see the same clock (`self.now()`); there is no clock drift.
- The application unlocks at most `HOLD_MAX` (20) ticks after `acquired`, unless the
  client crashes first. It never asks for a resource its client holds or is already
  requesting.

## Requirements

1. **Safety.** Never two holders of a resource at once unless both hold it for reading.
   A client holds a resource from its call to `self.acquired(resource)` until the
   application calls `unlock(resource)` or the client crashes. This must hold under any
   combination of the faults above.
2. **Liveness.** Suppose that from some tick `T` on, no node crashes or restarts, a
   majority of servers is up, no message sent before `T` arrives after `T`, and every
   message sent from `T` on is delivered exactly once within `MAX_DELAY` ticks. Then:
   - a request for a resource that no live client holds or requests is acquired by
     `T + LIVENESS` (150 ticks), even if crashed clients, lost releases or restarted
     servers left stale state behind before `T`;
   - read requests do not wait for each other;
   - when several clients compete, each request is eventually acquired.
3. Nodes share nothing except messages: no module-level or class-level state, no files.

## Interface

Read `sim.py`. `LockServer(Node)` overrides `on_start`, `on_message` and `on_timer`.
`LockClient(ClientBase)` also implements `lock(resource, mode)` and `unlock(resource)`,
and calls `self.acquired(resource)` once the lock is held. Use `self.random()` for any
randomness so runs are reproducible.

`Sim` runs scenarios: it plays the application, injects crashes, restarts and network
faults (`healthy`, `lossy`, or your own policy), and records safety violations. Our
hidden tests use the same `Sim` with adversarial scenarios.

## Deliverables

- `solution.py` with `LockServer` and `LockClient`.
- `DESIGN.md`: the protocol, why it is safe and live, and the cases you considered.
- Your tests, runnable with `python -m pytest`.
