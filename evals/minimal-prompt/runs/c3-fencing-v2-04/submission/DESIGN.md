# Lock service with fencing tokens

Files: `solution.py` (implementation), `Lock.p` + `Lock.pproj` (P model and `LockContract`
spec), `test_solution.py` (pytest). Run `snappea check` and `python -m pytest`.

## Contract
- `acquire(c, now)`: if no unexpired lease, grant a new lease expiring at `now + ttl` with token
  `last_issued + 1`; if `c` holds the unexpired lease, return its token; otherwise `None`.
  Unexpired means `now < expires`; at `now == expires` it is expired. Any `now` order is allowed.
- `release(c, t, now)`: True only if `c` holds the lease with token `t` and `now < expires`
  (then the lease ends); otherwise False with no effect.
- Tokens strictly increase for the life of the store, across release and expiry.
- `Resource.write(t, v)`: False with no effect if `t` was never issued (`t < 1` or `t > last`)
  or `t` < highest token of an accepted write to this resource; otherwise stores and True
  (an equal token is accepted).

## Primitives
Only per-key `load` and `compare_swap`. No cross-key atomicity. Two keys:
- `"lock"`: `{last, holder, token, expires}` (`last` = highest issued token; it always equals
  `token`, so `last` is kept only for clarity).
- `"res:<name>"`: `{max, value}`.
Every state change is one CAS on one key, retried after reload on failure. Read-only outcomes
(refusal, own-token return, failed release, rejected write) are linearized at the load.
Write checks issuance on the lock key and the fence on the resource key as two separate steps.
This is safe because `last` only grows (an issued token stays issued), and `max` only grows
(a token below `max` stays rejected).

## Scenarios (each is a P test and/or pytest)
1. Two acquirers read the same version (one CAS fails and must re-decide): `GenAcq`, barrier test.
2. Competitor acquires between load and CAS; same client's competitor must not create a second
   token: competitor tests.
3. Release racing with expiry + takeover, with double release, and with release + re-acquire.
4. Boundaries: `now == expires` for acquire and release; ttl off-by-one; out-of-order times.
5. Stale holder fenced after takeover; unissued token (0, negative, `last+1`); equal token.
6. Lost update on the resource: write(1) loads max 0, write(3) lands, write(1)'s CAS must fail,
   then write(2) must still be rejected: `RaceWrite`, `test_stale_write_cas_must_retry_not_lower_max`.
7. Two objects (and two names) sharing a store; resource key not colliding with the lock key.

## Model (`Lock.p`)
`Store` holds both records with load and CAS. `Client` loads, decides from the snapshot only,
and sends a candidate plus the result; the store commits it only at the expected version.
`LockContract` recomputes every result from the request history (it does not read the store)
and checks that each result belongs to the invoked operation. Tests: `tcFixed`, `tcGenLock`,
`tcGenAcq`, `tcGenWrite`, `tcStale`, `tcExpiry`, `tcSerialRelease`, `tcSerialTakeover`,
`tcRaceWrite`. Bounds: ttl 2, 2-3 clients (client 3 only issues tokens first), at most 4-7
operations in total, times 0..4, tokens 0..4, at most 4 CAS attempts per call. Searches are
complete for those bounds. `snappea check` passes with coverage and mutation checks; 0 mutants
undetected, a few equivalent ones suppressed with a reason in `Lock.p`.

## What the model found
My first model let the store recompute the decision at CAS time; that hid every stale-snapshot
bug (surviving mutants: version never bumped, version check ignored). I moved the decision into
the client and added `RaceWrite` (tokens issued first, then the racing writes): without the
version bump it fails. Mutation checks also showed the oracle did not tie results to the invoked
operation, so `eInvoked` was added.

## Implementation checks
`test_solution.py` forces interleavings with a store wrapper (a competing call between load and
CAS, a barrier so two calls read the same version), replays through second objects, tests
boundaries, runs thread stress, and checks a real recorded history against `LockContract` with
`snappea.check_events` (plus a wrong history that must be rejected).
Hand-made breaks that all fail a test: ignoring each CAS result; `<` to `<=` for acquire,
release, and the fence; dropping the expiry, client, or token check on release; skipping the
issued check or off-by-one on it; resetting `last` on release; not returning the own token;
ttl off by one; shared resource key / key collision with the lock; no retry on a stale write
CAS; a stale cached lock read; `max` kept in the object instead of the store.
One break survives and is equivalent: computing the next token from `token` instead of `last`
(they are always equal).

## Not covered
Crashes between steps, store failures, a clock that is not caller-supplied, non-int tokens or
non-string clients (clients only need equality), more than 3 clients, tokens beyond the model's
range, and fairness or liveness under unbounded contention (the retry loop has no cap).
Observation checks only sequential histories; thread stress uses plain assertions.
