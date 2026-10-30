# Unique usernames: design

## Contract
- Names: 1-16 chars of `a-z 0-9 -`, case-insensitive, stored lower case. Non-ASCII
  (including characters that lower-case into ASCII, e.g. KELVIN SIGN) and non-str are invalid.
  Invalid input returns False/None and changes nothing.
- A user holds at most one name; a name has at most one holder.
- `register`: succeeds iff user has no name and the name is free; same name again -> True.
- `rename`: only for a user with a name; to a free name (old name becomes free) or the own name -> True.
- When no call is in progress, `owner(n) == u` iff `name_of(u) == n`.

## Primitives
Only `Store.load(key)` and `Store.compare_swap(key, version, value)` per key are atomic; no cross-key
transaction. Calls for one user never overlap; different users may.

Two kinds of key: `("user", u)` -> name|None and `("name", n)` -> owner|None.
A single call needs two keys, so it cannot be one step: everything between steps interleaves.

## Algorithm (model: `Names.p` `Client`, code: `solution.py`)
- register: load user key; if named, answer `name == current`. Claim name (load name key; if held -> False;
  else CAS None->user, retry on conflict). Then write the user key.
- rename: load user key (no name -> False; same name -> True). Claim the new name, write the user key,
  then release the old name (CAS user->None).
- The name claim is the only contended step and is the linearization point of a success. The user key is
  written only by its user (calls for a user don't overlap), so those CAS never fail in practice.
- Claim-before-release means a user may briefly hold two names (rename) or a claimed name not yet in its
  user record (register); both are "call in progress". It never lets two users hold one name, and a
  crash-free call always ends consistent. Reads: `owner` reads the name key, `name_of` the user key.

## Scenarios (all in the model and in Python tests)
1. Two/three users register the same free name (both read the key before either CAS): exactly one wins.
2. Competitor takes the name between this call's load and CAS: call returns False, state unchanged.
3. Competitor takes a rename target between load and CAS: rename False, old name still held.
4. Swap cycle: u1<->u2 rename into each other's names while both read first: both False, nothing lost.
5. Rename frees a name just after another user saw it taken: first answer False (valid), later register ok.
6. Rename into a name another rename is also targeting.
7. Rename/register with own current name; register with a different name when already named.
8. Boundaries: length 0/1/16/17, case, chars `_ . space \n`, non-ASCII, non-str.
9. Two Registry objects over one Store (state is in the store).

## Model and bounds
`Spec.p` holds the independent `Contract` spec (never reads versions/retries): name claims only when free,
releases only by holder, each reply checked against the user's own history (exact for named users, success
needs a claim, failure needs a rival that held the name), and a quiescent snapshot checked for
bijection and agreement with replies. `Names.p` holds Store/Client/Scenario and tests. 2-3 users, 2 names,
1-3 ops each. Generated test: user0 two ops, user1 one op, op kind and name chosen freely. All 7 tests
complete exhaustively; coverage complete; mutants all detected (19 suppressed with reasons: equivalent or
retry-guards). `snappea check` exits 0.

## What the model found
No race in the final design. Design alternatives rejected while modelling: writing the user record first
(a reader could see name_of(u)=n with owner(n) elsewhere, and failed rename leaves junk), and releasing the
old name before claiming (a failed rename would lose the old name; caught by `test_rename...`/spec snapshot).
Modelling also showed the extra branches "claim finds own name" and "release finds a non-holder" are
unreachable, so they were dropped from the code.

## Tests (`python -m pytest`)
- `test_registry.py`: contract, boundaries, forced interleavings (barrier so both read the same version;
  competitor between load and CAS; swap), second Registry on one store, thread stress with end-state checks.
- `test_observe.py`: real calls recorded as P events (writes in commit order via a store wrapper) and checked
  by `Contract` with `snappea.check_events`; also a deliberately bad history that must be rejected.
- Hand-made breaks (ignore CAS result, steal held name, skip claim/release/set, drop lower/isascii, shift
  length and character-class boundaries, state in object, wrong key, ...) each fail a test.

## Not covered
Crashes between steps (a crash after claim leaves the name reserved by a user with no record, or a rename
leaves both names held); no recovery/GC. Store failures/exceptions, >3 users or >2 names in the model,
non-str users, long-run thread timing (stress is only a smoke test), concurrent calls for one user.
