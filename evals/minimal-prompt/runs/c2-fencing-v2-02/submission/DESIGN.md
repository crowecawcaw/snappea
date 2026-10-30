# Lock service with fencing tokens: design

## Contract
1. `acquire` grants iff no unexpired lease at `now` (a lease granted at `g` is expired for `now >= g+ttl`); it returns a fresh token, strictly greater than every earlier token of the store.
2. If the caller already holds the unexpired lease, `acquire` returns that lease's token (no new token, no state change); otherwise `None`.
3. `release(client, token, now)` succeeds iff the lease is held by `client` with `token` and unexpired at `now`; any other call has no effect.
4. `Resource.write(token, v)` is rejected, with no effect, if the token was never issued (`token < 1` or `> last issued`) or is lower than a token already used for an accepted write to that resource; equal tokens are accepted.
5. Rejected writes do not raise the resource's mark. Resources of different names are independent.
6. Tokens never go down, even after release or expiry.

## Atomic primitives
Only `Store.load(key)` and `Store.compare_swap(key, version, value)` on one key. No cross-key transaction. So:
- `"lock"` holds `{last, holder, token, exp}` in one record: lease and token counter change in one CAS, so a fresh token cannot be handed out twice. `last` is never reset by release/expiry.
- `"resource:<name>"` holds `{high, value}` in one record: the fence check and the value update are one CAS.
- `Resource.write` reads `"lock"` and the resource record separately. This is safe because `last` only grows: a token seen as issued stays issued. A token seen as unissued is rejected, which linearizes at that read.
- All state is in the store; no per-object state (second objects on one store behave the same).

## Scenarios (risks)
1. Two clients acquire having read the same version: exactly one wins.
2. Competitor re-acquires between my load (saw expired) and my CAS: I must redecide and get `None`.
3. Expiry boundary: `now == exp-1` live, `now == exp` expired; holder re-acquiring at `exp` gets a new token.
4. Release races: wrong client only, wrong token only, expired only, already released, lease changed between load and CAS.
5. Release followed by acquire: token must still increase (counter kept in the record).
6. Write with token never issued (0, negative, above last, nothing issued) must not raise the mark.
7. Write with lower token after higher accepted; equal token; competing higher write between my load and CAS.
8. Two writes read the same resource version.
9. Two resources and two service objects on one store.
10. Out-of-order `now` between clients.

## Model (`Lock.p`)
Store machine = lock record + resource record with load/CAS handlers; `Client.run` runs the real algorithm with send/receive at every step. `FenceContract` (spec) computes expected results from the contract and commit history, independently of the algorithm: read-only outcomes are checked against the oracle state at the load, writes at commit; every client reply must equal that. Tests: `tcGenLease` (acquire at 3 times, then generated acquire/release vs a generated op of client 2; ttl 2, tokens 1..2), `tcGenFence` (two clients acquire at generated times, then write generated tokens 0..3), `tcExpiry`, `tcRelease` (fixed boundary scenarios). Bounds: 2 clients, <=3 ops total per generated test (3 ops each in fixed ones), times 0..4, tokens 0..3, ttl 2, one resource. Searches completed for all four.

## What the model found
No design bug in the final design. While modelling, the key decision was forced by the primitives: the token counter must live in the same record as the lease (a separate counter key would allow duplicate or non-monotone tokens), and write must read `last` before the resource record so "issued" is monotone.

## Implementation tests (`test_solution.py`, `Observed.p`)
Forced interleavings via store wrappers (same-version barrier, competitor between load and CAS), boundary tests, retries through second objects, and an observation test that records real call arguments/results and checks them with `check_events` against `ObservedContract` (sequential contract spec, ttl 2). 24 hand-made breaks (ignored CAS, `<`/`<=` flips, dropped checks, counter reset, shared key, state kept in the object) each fail a test.

## Not covered
Read is trivial and unmodelled. Real thread timing beyond forced interleavings; more than 2 clients, ttl other than 2 in the model; store failures/crash; `now` values far apart in the model (0..4 only); the observation spec checks sequential histories, not concurrent commit order.

## Check status
`snappea check` exits 0, all 5 model tests (tcGenLease, tcGenFence, tcExpiry, tcRelease, tcRace) complete their search. Mutation: 91 of 117 model mutants detected. The 26 survivors are reported with an incomplete per-mutant search (1000 schedules): mostly done/ok flag and loop-control mutations in `Client.run` that cause non-termination or only alter bookkeeping, plus store CAS-version mutations that the bounded schedule budget did not reach. Not closed; the Python break tests (step 5) cover the same code in the real implementation.
