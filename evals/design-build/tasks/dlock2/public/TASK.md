# Distributed read/write lock service

Build the lock service for our storage cluster: write `LockServer` and `LockClient` in
`solution.py` against the interface in `sim.py`.

## The design we have

This is how the current service (modeled on MinIO's dsync) works:

- There are `N` lock servers (tests use 3 to 5) and no master. Any client can lock any
  resource name, in write (exclusive) or read (shared) mode.
- To lock, a client broadcasts the request to all `N` servers. Each server grants or
  refuses from its own in-memory lock table. If `N/2 + 1` servers grant, the client
  holds the lock; otherwise it releases what it got and retries a little later.
- Read locks may need fewer grants than write locks, so reads keep working when more
  servers are down.
- To unlock, the client tells every server to drop its entry.
- Stale locks (from a client that died, or an unlock that never arrived) are removed
  by a maintenance pass that each server runs every so often: for each entry older than
  a threshold, the server asks the other servers whether they still have it, and drops
  it if they do not.
- Applications may hold a lock for a long time.

We want the same behavior, but we have had reports of two writers holding the same
lock and of locks that never come free. Keep, change or replace any part of the design.

## Deployment

The service runs on ordinary machines and an ordinary network. Processes crash.
Lock servers restart without anything on disk. Clients that crash do not come back.
Messages get lost, duplicated, reordered and delayed. Each machine's clock runs at a
rate within `RHO` (10%) of real time and they were never synchronized. The application
on a client can be told it lost a lock (`self.lost`) and stops using the resource at
once.

## Requirements

1. **Safety.** Never two holders of a resource at once unless both hold it for reading.
   A client holds a resource from `self.acquired(resource)` until the application calls
   `unlock(resource)`, the client calls `self.lost(resource)`, or the client crashes.
2. **Liveness.** Suppose that from real time `T` on, no node crashes or restarts, a
   majority of servers is up, no message sent before `T` arrives after `T`, and every
   message sent from `T` on is delivered exactly once within `MAX_DELAY` (5) ticks. Then:
   - a request for a resource that no live client holds or requests is acquired by
     `T + LIVENESS` (150 ticks), whatever state earlier failures left behind;
   - a lock acquired at or after `T + LIVENESS` is held until the application unlocks
     it (never `lost`), however long that takes;
   - read requests do not wait for each other;
   - when several clients compete, each request is eventually acquired.
3. Nodes share nothing except messages: no module-level or class-level state, no files.

## Interface

Read `sim.py`. `LockServer(Node)` overrides `on_start`, `on_message` and `on_timer`.
`LockClient(ClientBase)` also implements `lock(resource, mode)` and `unlock(resource)`,
and calls `self.acquired(resource)` and `self.lost(resource)`. Use `self.random()` for
any randomness so runs are reproducible.

`Sim` runs scenarios: it plays the application, injects crashes, restarts and network
faults, sets clocks, and records safety violations. Our hidden tests use the same
`Sim` with adversarial scenarios.

## Deliverables

- `solution.py` with `LockServer` and `LockClient`.
- `DESIGN.md`: the protocol, why it is safe and live, and the cases you considered.
- Your tests, runnable with `python -m pytest`.
