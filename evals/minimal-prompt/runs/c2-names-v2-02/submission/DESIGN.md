# Unique usernames: design

Files: `solution.py` (implementation), `Names.p` (P model + spec `UniqueNames`),
`test_registry.py` (pytest), `runtime.py` (given store).

## Contract

- A user holds at most one name; a name belongs to at most one user.
- Names are 1-16 chars of `a-z0-9-`, case-insensitive (ASCII only), stored lower case.
  Invalid names: `False` / `None`, no change.
- `register`: succeeds iff user has no name and the name is free; repeating the
  user's own name is `True`; otherwise `False`.
- `rename`: needs a held name; succeeds iff the new name is free or already the user's;
  the old name becomes free. `False` changes nothing.
- When no call is running, `owner(n) == u` iff `name_of(u) == n`.
- Calls for one user never overlap; calls for different users and Registry objects may.

## Primitives

Per-key `load` and `compare_swap` only; no cross-key transaction. So a name change
touching `name:n` and `user:u` is several steps and other users interleave.

Records (all in the Store, nothing in the object):

- `("name", n)` -> owning user or `None` (freed names are written `None`, never deleted,
  so versions only grow).
- `("user", u)` -> the user's name or `None`. Only `u`'s own calls write it.

Algorithm:

- register: read `user:u`; if named, return `name == it`. Claim `name:n` by CAS from a
  free read (retry on lost race, reject if a read shows another holder). Then write `user:u`.
- rename: read `user:u` (old). Claim `name:new`; write `user:u = new`; then free `name:old`.
  Order matters: a name is claimed before the user points at it and freed after the user
  stops pointing at it, so no name is ever held by two users and `name_of` never points
  at a name another user owns. Between steps `owner(old)` still says `u` while
  `name_of(u)` says new: a transient, allowed only while the call is in progress.

## Scenarios (each has a test)

1. Two users register the same name, both reading it free (forced same version): one wins.
2. A competitor takes the name between this call's read and CAS: CAS fails, re-read, reject.
3. Same for a rename to a taken name: user keeps old name, old name still owned.
4. Name freed by a rename, then registered by someone else; a retry after the change succeeds.
5. Name freed and retaken between read and CAS (ABA): version check rejects.
6. Two users rename to each other's names, same-version reads: both `False`, nothing moves.
7. Boundaries: lengths 0, 1, 16, 17; `-`; uppercase; spaces, `_`, non-ASCII, Kelvin sign
   (lowers to `k`), trailing newline, `None`, int.
8. Each precondition false alone: has other name; name held; no name for rename; invalid name
   on rename (old name kept).
9. Second `Registry` on the same store sees and conflicts with the first (state in Store).
10. Thread stress with many users/names: invariant and uniqueness at the end.

## Model (`Names.p`)

Store machine with versioned keys, `eLoad` / `eCas`; two `User` machines, each doing
two operations (step 1 is register or rename, step 2 is register or rename, names 0/1 chosen
by `choose(2)`), then a barrier and public reads. Spec `UniqueNames` is independent of the
algorithm: it follows commits announced by the store at the CAS, checks name never claimed
while held, rejections only when a probe saw another holder, successes hold the name,
rejected register/rename have a cause, every user completed its ops, and after quiescence
`owner`/`name_of` agree and match the spec. Bounds: 2 users, 2 names, 2 ops each, one store;
search is complete (about 100k schedules, ~12 s). Claim retry is bounded by an assertion
(`tries <= 3`) so a livelock is a failure.

## What the model found

- Modeling errors only (announce ordering at the client instead of the store commit point,
  reads before quiescence). The algorithm passed once the oracle was correct; the
  claim-before-point, free-after-repoint order was chosen up front because the reverse
  (free old first, or point user before claim) breaks uniqueness or loses the old name on a
  failed rename; both reverse orders are tested as deliberate breaks and fail tests.

## Not covered

- Crashes between steps (a crash after claim leaves a name held by a user with no
  name; there is no recovery). Out of scope: calls complete.
- More than 2 users / names in the model; Python stress covers more but not exhaustively.
- Overlapping calls for the same user (excluded by the contract).
- Python observation test checks sequential/forced histories only.
- Some `snappea check` mutants survive within the mutation time limit: equivalent ones
  (defaults such as `step = 0`, `res = false`, `cur = 0`, version `+1` vs `-1`), and a few
  whose search was incomplete in the time limit. Treat with that caveat.
- An equivalent variant (fail a rename/register when the single claim CAS loses, instead
  of retrying) is also linearizable; tests do not require retrying.
