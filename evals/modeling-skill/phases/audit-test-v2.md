# Frozen testing-phase v2 audit

Scope: source-first review of frozen `phase-test-v2-01` and `phase-test-v2-02` submissions and transcripts only. No grading files were read. The v2 model runs had no frozen `submission/` directories when checked, so they are not included here.

## Improvements

Both tasks explicitly say token allocation may vary while remaining strictly increasing (`TASK.md:5`). Both event monitors honor that contract: they retain `last_token`, require each successful token to be greater, and update the recorded maximum (`lease_contract.p:21-31`). Their tests no longer assert tokens equal 1, 2, 3; they check positive/nonnull results and compare later tokens with earlier ones. This avoids tying a contract test to the supplied service’s consecutive allocation policy.

Both wrappers observe actual public API arguments and returned values, and append actual public `read()` and `peek()` results. They feed those events to `check_events` with strict validation and unknown-event errors, without reading service internals or supplying a hand-authored trace. The source provenance is strong and the monitor maintains its own expected value/lease state. Both transcripts show the initial missing `events.p` setup was caught and fixed before freeze (v2-01 calls 8-9; v2-02 calls 7-8).

The tests improved rejection-side-effect evidence. In v2-01, wrong-owner and wrong-token writes are followed by `read() == 0` before any successful write; the expired write is likewise followed by a read before a later acquisition/write (`test_lease.py:95-101,129-137,139-144`). In v2-02, the initial rejected writes are followed by a read before the valid write, and expired writes are followed by a read (`test_lease.py:95-104,129-143`). Those observations would detect a rejected write that returns false but changes the value, for the specific tested calls. Both also follow rejected releases during an active lease with `peek()` of that same lease, which would detect the call silently removing the current active lease.

## Remaining gaps

Neither suite tests an old owner’s delayed release after a successor has acquired the resource. In both, the expired owner’s rejected release occurs before the new acquisition (`v2-01/test_lease.py:129-137`; `v2-02/test_lease.py:129-137`). At that point the lease is already publicly inactive, so `read()` does not reveal lease metadata changes and there is no successor for `peek()` to protect. A targeted history should expire A’s token, let B acquire, call `release(A, old_token)`, assert false, then check `peek()` still reports B and B’s token still permits a write. This distinguishes rejection of an expired lease from preserving a successor lease against delayed stale work.

V2-02 still has one changed-value blind spot after B takes over. It calls a rejected write using `bob` and Alice’s old token, then a rejected release and `peek()`; it later performs a valid Bob write of `0` before reading (`test_lease.py:108-116`). Since `peek()` does not expose the integer value, an implementation could return false while changing the value during the stale write, then have the valid write overwrite that evidence. Add a `read()` immediately after the rejected stale write and assert the pre-call value. V2-01 does not make this stale-write-after-successor call; its rejected writes are checked before later successful writes.

The tests otherwise cover strong sequential lifecycle histories: same-owner and competing acquisition rejection, wrong owner/token, successful mutation and release, exact expiry, post-expiry acquisition, zero advance, and value persistence. The supplied service is deterministic and single-threaded, so the tests are not concurrency evidence; that remains a limit of this testing phase rather than a provenance flaw.

## Assessment

V2 materially fixed the token-allocation false positives and added direct state observations after important rejected writes. V2-01 catches value-changing rejected writes in all its exercised rejection paths. V2-02 catches the main wrong-owner/wrong-token and expired-write paths, but can mask a stale-write side effect after its later successful write. Neither submission checks that an expired former owner’s delayed release preserves a successor’s active lease. The transcripts show both agents read the revised implementation guidance, and the frozen artifacts now reflect its general recommendation to observe state after rejection before later success, with the remaining scenarios above left uncovered.
