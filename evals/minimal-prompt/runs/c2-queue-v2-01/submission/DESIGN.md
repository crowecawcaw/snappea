# Shared bounded queue design

## Contract
- One logical queue per Store, shared by every Queue object; FIFO, at most `capacity` items.
- `put` returns False when full; `take` returns the oldest item or None when empty.
- A retry (same kind + request_id) returns its first result and has no further effect,
  even if the state changed since, and even if the first result was False/None.
- A put and a take with the same request_id are different requests.

## Primitives
Only `Store.load()` (version, record) and `Store.compare_swap(version, record)` are atomic.
Everything else interleaves. Record = `{items, memos}`; memo keys `put:<id>` / `take:<id>`.
Items and memo are committed in one CAS. Memos are kept forever (no expiry).

## Scenarios
1. Two clients load the same version; one CAS fails and must re-decide.
2. Competitor fills the queue (or empties it) between this call's load and CAS.
3. Retry of an accepted put after its item was taken (no second item).
4. Retry of a rejected put after space freed (still False).
5. Retry of an empty take after a put (still None); retry of a take after others changed the queue.
6. Same id as put and as take.
7. Retry through a second Queue object on the same store.
8. Same request racing with itself.
9. Capacity boundaries 1, 2, 3, 5 (at and one past the limit).

## Model (Queue.p)
Store machine (load/try/replay), Client machines running load -> decide -> CAS -> retry,
`QueueContract` spec computing expected results from request history only.
Bounds: capacity 2, 2 clients; tcGenerated = one op per client (kind, id sampled) plus a
retry each; tcFullRetry and tcEmptyRetry = two ops per client plus a delayed retry.
All searches complete; `snappea check` exits 0 with full coverage and no undetected mutants.
Four mutants are accepted with lint comments (equivalent versions, retry-start timing,
the progress bound itself).

## Found / learned
The model has no bug in this design. Design points it forced: the memo must be in the
same CAS as the effect; a memo hit must also return the memo's result for rejected/empty.
Writing the tests exposed that a rejected put is memoized permanently, so callers wanting
to retry-until-space must use a fresh request id (the stress test does).

## Implementation tests (test_queue.py)
Forced interleavings via a wrapper store, retries after state change, second object,
boundaries, thread stress, an observation test checking real calls against QueueContract
with `snappea.check_events`, and a negative test for the observation. Hand-made breaks
(ignore CAS, drop memo check/store, boundary flips, shared key, LIFO, no removal,
memo in object) all fail tests.

## Not covered
Crashes, lost replies, memo growth/expiry, conflicting reuse of an id with other
arguments (returns the original result), capacity != 2 in the P model (Python tests
cover other capacities), more than 2 clients, concurrent observation ordering.
