# Shared bounded queue: design

## Contract
- `put` stores the item and returns True iff fewer than `capacity` items are queued; else False, no effect.
- `take` removes and returns the oldest item, or None when empty.
- A repeated `put`/`take` with the same request_id returns its first result (including False / None) and has no further effect, even after other requests changed the queue, and through any `Queue` object on the same store.
- A put and a take sharing a request_id are different requests.
- `snapshot` returns contents oldest first. Items and memos change together or not at all.

## Atomic primitives
Only `Store.load()` (version + copy) and `Store.compare_swap(version, record)` (whole record, only at the current version). Everything between them can interleave.

## Design
One record `{items, puts: {id: bool}, takes: {id: item|None}}`. Each call: load; if its id is memoized return that; else compute the new items and memo; CAS; on failure reload and recompute. Memos live in the record so one CAS commits effect and memo together (no per-object state). Memos grow without bound (known limitation; pruning needs a client-side protocol the task does not define).

## Scenarios
1. Two puts read the same version with one slot left: exactly one succeeds.
2. A rival fills the queue between a put's load and CAS: put re-decides and is rejected.
3. A rival empties the queue between a take's load and CAS: take returns None, no stale item.
4. A rival takes one of two items between load and CAS: take returns the next one.
5. A rival frees a slot between load and CAS: put succeeds on the retry.
6. Same request id sent by two clients at once: applied once, both get the same result.
7. Accepted put/take retried after others changed the queue: no second effect.
8. Rejected put retried after space freed stays False; empty take retried after a put stays None.
9. Same id for put and take; capacity boundaries (n-1, n, n+1) for capacities 1, 2, 3, 5; item 0.

## Model (Queue.p, `snappea check` exits 0, all searches complete)
Store machine (load, CAS, replay-read), Client machines run load/decide/CAS/retry, `QueueContract` spec computes expected results independently from requested commands, checks result, full contents, commit version, and reply. Bounds: 2 clients (one fresh call each, then one retry each), capacity 1-2, prefill 0..capacity, put/take choice, request ids 0/1, items 1/2 (duplicates must have equal args), plus six fixed scenario tests. One accepted mutant (`announce eConflicted`, a livelock guard only) is annotated.

## Found by the model
Early drafts of the spec let the oracle trust the store's executed command, so a client running the wrong command was invisible; the spec now checks commands against announced requests. Commit versions and exact call counts are asserted so no extra or missing operation passes. The design itself (memo in the same record as the items) held in all schedules.

## Tests (`python -m pytest`)
Boundaries, forced interleavings (barrier so calls read the same version; competitor between load and CAS), retries via a second object, threads conservation test, and observation tests that check real call histories with `snappea.check_events` against `QueueContract`. Hand-made breaks (ignore CAS result, drop either memo check, skip memoizing rejection/empty, `<=`, off-by-one capacity, LIFO, shared memo maps, per-object memo, stale snapshot reuse, fixed capacity, truncated snapshot) each fail a test.

## Not covered
More than 2 concurrent clients in the model; memo growth; mismatched arguments on a repeated request id (undefined by the task); crash/recovery; observation tests are serial or controlled-interleaving histories, not free thread schedules.
