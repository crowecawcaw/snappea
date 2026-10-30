# Lock service with fencing tokens

## Contract
- Tokens strictly increase over the store's life; a grant never happens while another
  lease is unexpired (expired iff now >= start + ttl); re-acquire by the holder returns its token.
- release succeeds only for the holder with the matching token while unexpired.
- Resource.write rejects never-issued tokens and tokens below the highest accepted one.

## Primitives
Per-key atomic load and CAS only. Records: `("lock",)` {counter, holder, token, expires};
`("res", name)` {max, value}. The counter is in the lock record so a grant and its token
are one CAS. A write reads the counter (monotone, so a stale read is safe), then CASes the resource.

## Scenarios
Two clients loading the same lock version; competitor acquiring between load and CAS;
expiry boundary; out-of-order `now`; release racing with takeover; stale release token;
two writers of a resource at the same version; stale token after a newer accepted write;
never-issued tokens (0, negative, future); separate resources; two service objects on one store.

## Model (model/Fencing.p)
Store machine with load/CAS for lock and resource; 2 clients, each acquiring twice
with `now = choose(4)` and writing with the token. Spec `Contract` checks increasing tokens,
no grant under an unexpired lease, and accepted writes non-decreasing and issued.
`snappea check --no-mutation --timeout 60s` (needs ~30s) passes with complete search and full coverage. Mutation testing
of the full model hit its time limit with many survivors (the spec is weak on store-primitive
mutants such as CAS version checks, since the clients' own retry hides some); not resolved.
Release is not modeled (covered by tests only).

## Tests
`python -m pytest`: forced interleavings, boundaries, replay of P spec via `check_events`,
and manual deliberate breaks (boundary flips, ignored CAS result, dropped checks) each fail a test.
