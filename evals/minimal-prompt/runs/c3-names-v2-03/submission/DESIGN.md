# Unique usernames: design

## Contract
- Names: 1-16 chars of `a-z0-9-`, case-insensitive, stored lower case. Invalid name: `False`/`None`, no change.
- `register`: succeeds if the user has no name and nobody holds the name; re-registering the held name is `True`.
- `rename`: only for a user with a name; succeeds if nobody else holds the new name, frees the old; same name is `True`.
- Quiescent invariant: `owner(n) == u` exactly when `name_of(u) == n`.

## Primitives
Per key atomic `load` and `compare_swap`; no cross-key transaction. Calls for one user never overlap.
Keys: `name:<n>` -> user, `user:<u>` -> name (`None` = free / none).

## Algorithm
- `register`: load `user:u`; decide from it; claim `name:n` with one CAS from free; then write `user:u`.
- `rename`: load `user:u`; claim the new `name:n`; write `user:u`; release old `name:o` by CAS to free.
  The name CAS is the only contested step; the user record and the held old name have a single writer, so those writes cannot lose (the model asserts it).
- While a rename is in flight the user holds both names; another user registering the old name then fails (it is linearized before the rename). Quiescent state is consistent.

## Scenarios (Names.p, 8 tests, all searched completely)
two users read the same free name (one CAS loses); same user re-registering; swap renames (A->y while B->x);
disjoint users must always succeed; rename to the held name; rename with no name; second user takes a name
freed by a completed rename; generated names for two users with register+rename and register+register.

## Model bounds and oracle
2 users, 2 ops each, names 3..4 (plus fixed scenarios with up to 6), 1000s of schedules, exhaustive.
`Contract` spec: transitions of each name go from the current holder; per-user results follow the user's own last
success; uncontended or sequential operations must succeed; final state is consistent both ways and equals the
announced transitions and last successes.

## What the model found
No bug in the final design. The first coverage/mutation runs showed gaps (no scenario for a nameless rename, rename
to the same name, name ids overlapping user ids, versions not asserted to increase); each was fixed with a scenario or assertion.
A design note: a failed register of a name whose rename-away is in flight is allowed (linearized earlier), so the spec
only demands success when no other call is in flight (sequential or disjoint names).

## Python tests (test_solution.py)
Forced interleavings (same free name read by two; competitor between load and CAS for register and rename; rival
between claim and release), second object on same store, validation boundaries, thread stress, and observation tests
using `check_events` with `Contract`. Hand-made breaks (ignore CAS result, skip load, no release, release first,
flip each precondition/boundary, state in object, wrong key, etc.) all fail a test, except lowercase-before-regex
(equivalent: `A-Z` in the regex is harmless).

## Not covered
Crashes between steps (a crash after claim leaves a name held with no user record), more than two users or ops,
real-concurrency linearizability beyond the model, observation of concurrent returns (only sequential orderings).
