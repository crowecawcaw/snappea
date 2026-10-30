# Lock service with fencing tokens

## Contract
- `acquire(c, now)`: if a lease is held and `now < expires` -> the holder's own token if `c` is
  the holder, else `None`. Otherwise grants a new lease expiring at `now + ttl` with token
  `last + 1`. Tokens strictly increase over the life of the store (the counter survives
  release and expiry and lives in the store, not the object).
- `release(c, token, now)`: True only if `c` holds the lease with `token` and `now < expires`;
  then the lease is cleared. Otherwise False, no effect.
- `Resource.write(token, v)`: False and no effect if `token` was never issued (`< 1` or
  `> last`) or `token <` the highest token used by an accepted write to this resource.
  Otherwise stores and returns True (an equal token is accepted).
- `now` values from different clients may be out of order; the rules above are applied as
  written to whatever `now` is supplied (no clock is kept).

## Atomic primitives
Only `Store.load(key)` and `Store.compare_swap(key, version, value)` per key. No cross-key
transaction. Everything else interleaves.

## Algorithm
- Key `lock`: `{last, holder, token, expires}`. acquire/release: load, decide, CAS, retry on loss.
- Key `res:<name>`: `{max, value}`. write: load `lock` (token issued?), load resource, decide,
  CAS resource, retry on loss. The issued check needs no CAS on `lock` because `last` only grows.

## Scenarios (each is a P test and a Python test)
1. Several clients acquire with the same `now`: one winner (tcContention, forced barrier test).
2. Competitor acquires between load and CAS (tcGenerated, forced hook).
3. Expiry boundary `now == expires` re-grants; `now == expires - 1` does not (tcContention, tcStale).
4. Stale holder: late release with old token and late write with old token after takeover (tcStale).
5. Release with only the client wrong / only the token wrong / exactly at expiry.
6. Writes: token 0, never-issued token, token just above `last`, equal token twice, lower after higher.
7. Two resources and two `Resource` objects on one store share/do not share fencing state (tcFencing).
8. Retry after state changed: re-acquire by old holder after takeover returns `None`, not its old token.
9. Out-of-order `now` between clients.
10. Two releases of the same lease racing: exactly one True.

## Model (`Fencing.p`, `Fencing.pproj`)
Machines: `Store` (the real primitives, `lock` + 2 resource keys), `Client` (the algorithm with
separate load / CAS steps). `Contract` is an independent spec that keeps its own version
history of the lock and resource state and recomputes the expected result of every operation;
it checks returned results, committed records, and that every operation linearized and returned.
Tests: tcGenerated (fresh store, acquire/release pairs), tcGen{AA,AR,RR,RA,AW,WA,WW,RW}
(client 1 acquires at 0, then client 1 runs one op at now=1 and client 2 runs one op,
now 1..2 for client 2, tokens 1..2), tcContention (3 clients), tcStale, tcFencing.
Bounds: ttl=2; 2-3 clients; at most 5 ops per client; 2 resources in fixed scenarios, 1 in
generated. `snappea check` exits 0: all 12 searches complete within these bounds, coverage
complete. Mutation: 117 of 129 detected; the 12 survivors were reported by the tool with an
incomplete mutant search (mutants that stall or only touch driver plumbing, e.g. op id
numbering, `ops[i]` kind dispatch in the driver, the Store seed), not by a completed search.

## What the model found
Process finding: announcing the linearization point from the client after a successful CAS
let another client read the new version first, so the spec saw a version it did not know.
The fix (a modelling fix, not an implementation bug) is to announce at the Store transition.
No algorithm bug was found: CAS-retry on a single `lock` record and an `max`-record per
resource satisfy the contract in all explored bounds.

## Implementation tests (`test_solution.py`)
Forced interleavings via a hook store (competitor between load and CAS for acquire, release
and write; barrier so two acquires read the same version), retries through second objects,
boundary tests, and an observation test that checks real calls against `Contract` with
`snappea.check_events`. Hand-made breaks that fail a test: ignored CAS result (acquire,
release, write), `<=` at expiry (acquire, release), `<`/`<=` at fencing, token reuse, counter
dropped on release, no re-entrant token, missing client or token check on release, missing
issued check, off-by-one unissued bounds, shared resource key, key collision, state in object.

## Not covered
Crashes, store failures, token overflow, non-int `now`, unbounded retry fairness (loops
retry until CAS wins), more than 3 clients in the model, `Resource.read` linearization
(only tested in Python), equal-time value races (two writes of the same token: last wins).
