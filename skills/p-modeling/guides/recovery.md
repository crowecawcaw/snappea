# Recovery and retry

## Preserve the right state

- Separate durable effects/outcome records from volatile worker memory, pending replies, and local flags.
- Restart clears volatile state; retain committed durable state unless modeling storage loss explicitly.
- Preserve request identity across retries. Generating a new ID changes the idempotency contract.
- Model effect + outcome memo atomically only if a real primitive co-commits them. Otherwise allow a crash between writes.

## Histories to challenge

- Commit effect → lose response → restart → retry the same ID. Missing response does not imply missing commit.
- Complete A → B advances the same state → retry A. If promised, return A's original result without reapplying A over B.
- Track original per-request responses separately from current state; they can legitimately differ.
- Distinguish replay/no-op log records from new state-changing effects.
- See [durable retry](../examples/durable-retry/README.md) for correct and split-commit/reapplied-effect variants.

## Stale evidence

- Keep the old version, concurrent update, and stale retry explicit.
- Version/lease checks protect writes only when enforced at the atomic update.
- After CAS conflict, reread, recompute, and revalidate version-dependent work; changing only the expected version may publish stale evidence.
- See [versioned publication](../examples/versioned-publication/README.md).

## Bounds and progress

- Start with one request/effect/restart; add actors and IDs when they change the claim.
- State omitted crash points, expiration, conflicting payloads under one ID, network behavior, and storage failures.
- Completion needs assumptions such as eventual recovery and scheduling. Safety may not.
- A bounded pass does not establish arbitrary-crash or implementation correctness.
