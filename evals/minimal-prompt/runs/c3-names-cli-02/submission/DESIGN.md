# Unique usernames

Primitives: per-key load and compare-and-swap; no cross-key transaction.
Records: ("name", n) -> holder or None; ("user", u) -> name or None.
Contract: a name has at most one holder; at rest owner(n)==u iff name_of(u)==n.

Algorithm: the name record is the claim (CAS from free). register = claim, then set user
record. rename = claim new, set user record, then free old. A name is never doubly held,
and between steps only owner() runs ahead of name_of() (not at rest).
A user's record and a held name are only written by that user, whose calls never overlap,
so those writes need one CAS.

Scenarios: two users register the same name from the same version; a competitor takes the
name between load and CAS; swap-rename x<->y; reuse of a freed name; rename without a name.
Model: Names.p (2 users, 2 names, 3 ops each). tcTwoUsers checks the invariant,
tcSequential checks exact results. Both pass; the mutation run hit its time limit with
only timeout-type or load-shape survivors. Not covered: crashes between steps.
