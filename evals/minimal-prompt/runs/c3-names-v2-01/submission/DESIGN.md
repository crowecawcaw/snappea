# Unique usernames: design

Files: `Names.p` (model + spec), `solution.py` (implementation), `test_solution.py` (tests).
Check the model with `snappea check`; run tests with `python -m pytest`.

## Contract
- Names are 1-16 chars of `a-z0-9-`, case-insensitive, stored lower case. Invalid -> `False`/`None`, no change.
- `register`: succeeds iff the user has no name and nobody holds it; same name again -> `True`.
- `rename`: only for a user holding a name; succeeds iff nobody else holds the target (same name -> `True`); old name is freed.
- Same-user calls never overlap. At quiescence `owner(n)==u` iff `name_of(u)==n`.

## Primitives
`runtime.Store`: atomic `load(key)` and `compare_swap(key, version, value)` per key. No cross-key atomicity.
Two record kinds: `name:<n>` -> owner or None; `user:<u>` -> name or None.

## Algorithm
The name record is the arbiter of uniqueness.
- register: read user record; claim name record by CAS (None -> user, retry on conflict, fail if held); write user record.
- rename: read user record; claim new name; write user record; release old name record (CAS to None).
Order matters: claim first (so uniqueness never depends on a second key), release last (the old name is never
free while the user record still points to it, and never held by nobody while the user still claims it).
Non-quiescent states may show a user holding two names transiently; the quiescent invariant holds.

## Scenarios (all in the model and as Python tests)
1. Two users read the same free name, both CAS: one loses and must re-read and fail (`tcSameName`, test_two_users_read...).
2. Competitor takes target between our load and CAS, for register and for rename (rename leaves the old name intact).
3. Swap: u1 a->b while u2 b->a: both fail (`tcSwap`).
4. Handoff: another user registers the old name before / after the release (`tcHandoff`, two tests).
5. Rename to the same name returns True (`tcRenameSame`); register same name again; register when already named.
6. Invalid names: lengths 0/1/16/17, bad chars, upper case; nothing changes.
7. Spurious version bumps on name/user records must be retried, not ignored.
8. State lives in the store: second `Registry` on the same store sees and respects it.

## Model bounds
Users 2, names 2, ints for ids. `tcGenerated`: user A runs 2 generated ops, user B 1, concurrently; `tcGeneratedSeq`
same, one user at a time (exact results asserted). Fixed concurrent scenarios above. Searches are complete.
The spec checks exact results when no calls overlap; with overlap it checks that the final store state equals what
each user was told (`name_of`) and that `owner` is its inverse. The `attempts` asserts are test-only progress bounds.

## What the model found
No design bug in this algorithm. The order (claim -> set user -> release) was chosen from the scenarios: releasing
before setting the user record, or setting the user record before claiming, breaks scenarios 2/3 (confirmed by
mutations in the Python tests).

## Mutation results
Python: breaks tried and caught: ignored CAS result (claim, set user, release), no claim check, skipped release,
claim/set order swapped, wrong length bounds, no lower-casing, key collision (no `name:` prefix), missing
same-name shortcuts. Equivalent (not caught): `x or None` on `owner`.
P `snappea check`: 11 of 101 mutants undetected, all loop-bookkeeping of the test-only `attempts` bounds,
the `i <= 4` ghost key in Store init, and `version - 1` (needs ABA across 3+ writes of one key; not modeled).

## Not covered
Crashes between steps (a crash after the claim leaves a name held with no user pointing to it; the task has no
crash model), more than 2 users/names, unbounded contention, store failures, non-string users.
