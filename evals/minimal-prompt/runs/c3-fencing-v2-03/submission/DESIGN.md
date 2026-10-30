# Lock service with fencing tokens

## Contract
- `acquire(c, now)`: if no unexpired lease (`holder` set and `now < expiry`), grant a fresh token
  greater than every token ever issued, lease `[now, now+ttl)`. If `c` holds an unexpired lease, return its
  token and do not extend it. Otherwise `None`.
- `release(c, token, now)`: True (and the lease ends) iff `c` holds the lease with `token` and `now < expiry`;
  otherwise False with no effect.
- `Resource.write(token, v)`: False with no effect if `token` was never issued (`token < 1` or `> last issued`)
  or `token < ` the highest token used by an accepted write to that resource. Otherwise store, True.
  An equal token is accepted.
- Callers' `now` values may be out of order; each call judges expiry by its own `now`.

## Primitives
Only per-key `load` and `compare_swap` (`runtime.Store`). No cross-key atomicity. State kept in the store:
- key `lock`: `{holder, token, expiry, last}`. `last` (highest token ever issued) is never lowered, so release and
  expiry keep it and tokens strictly increase. One record means issuing a token and taking the lease are one CAS.
- key `res:<name>`: `(max_used, value)`; the check and the update are one CAS.
Everything else interleaves: `write` reads the lock record and the resource record separately.
This is safe because `last` only grows: a token seen as issued stays issued.

## Scenarios (each is in the model or tests)
1. Two acquirers read the same lock version; one CAS must lose and re-decide (None).
2. A competitor acquires between load and CAS; the loser must re-read.
3. A release loses its CAS because the lease expired and another client took the lock; it must return False.
4. Two releases of the same token; only one succeeds.
5. Expiry boundary: `now == expiry` is expired for acquire and release; `expiry-1` is not.
6. Re-acquire by the holder returns the same token and does not extend the lease.
7. Release then re-acquire (by the same or another client) gives a larger token.
8. Release with wrong client only / wrong token only / expired only.
9. Write with token 0, negative, never issued, issued-but-lower, equal, higher; a write whose CAS loses to a
   higher-token write must be rejected on retry, and to a lower-token write must still succeed.
10. Two `Resource`/`LockService` objects on one store; two resources sharing the lock but not max tokens;
   another store never issued the token.
11. Out-of-order `now`.

## Model (`Fencing.p`, `Fencing.pproj`)
- `Store` machine: lock record and resource record with versioned load and CAS.
- `LockClient`, `Writer`: load, decide, CAS, retry (every `send`/`receive` is a scheduling point).
- Spec `Contract` is independent of the algorithm. It keeps a history of committed lock/resource versions and
  checks every decision (including None / False / rejected results, which are linearized at their read version)
  against the state at that version; commits must be from the latest version, produce exactly the state the contract
  implies, and the client's returned value must equal its linearized result.
- Bounds: ttl 2, times 0..3, token 0..3. Tests: `tcTwoLocks` (2 lock clients), `tcOneLockTwoWriters`,
  `tcOneLongLockOneWriter` (acquire, re-acquire, release, acquire plus 2 writes). Each search completes (all schedules).
  The four-client case is not explored (state explosion), so writers contending with two lock clients are covered
  only by the Python tests.
- `snappea check` result: all three tests complete, 100% model coverage; mutation: 137 of 166 mutants detected.
  The 29 survivors are model-harness mutants, not algorithm gaps: retry counters (`tries`) whose only job is the
  contention-bound assertion, driver choices (`held == 0` fuzz token, release times), `id = cfg.id`, and snapshot
  assignments (`l = x`, `r = x`, `s = x`) whose loss is invisible because the spec trusts the snapshot version the
  client reports, and zero defaults agree with the initial state. The CAS version guards and the Store state updates
  are all killed. Because of the survivors `snappea check` exits 1; I did not suppress them.
  The same weakness is closed on the implementation side by the Python tests, which compare real results with a
  sequential oracle and with public reads.
- Found: with the first model draft the spec failed, only due to my own scenario/spec mismatches (event name
  clash `eDone`; "too little work" check). No algorithm bug was found. The design decision that mattered
  (keep `last` in the same record as the lease, never lowered on release) was made from step 1 risks.

## Tests (`test_solution.py`, `python -m pytest`)
Boundary tests, forced interleavings via a wrapping store (barrier so both read the same version; hook that runs a
competitor right after a load), cross-object tests, a thread stress test, and observation tests: real calls
recorded as events and checked with `snappea.check_events` against `obs/Observe.p` (a sequential oracle with ttl 3,
random out-of-order times). `test_oracle_rejects_wrong_results` shows the oracle can fail.
Hand-made breaks (expiry `<=`, release ignoring token/client/expiry, ignoring CAS results, tokens from wrong field,
fence checks removed or `<=`, resource key shared, ttl off by one, release lowering `last`) each fail a test.

## Not covered
Processes, crashes, store failures, token overflow, concurrent observation order (observation is sequential),
non-int tokens, clients with ttl differing across services.
