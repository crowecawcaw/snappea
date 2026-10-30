# Unique usernames

Contract: a name belongs to at most one user, a user holds at most one name; at rest
owner(n)==u iff name_of(u)==n. Primitive: per-key load/CAS only.
Design: records ("n",name)->owner and ("u",user)->name. Register/rename claim the name by CAS
(free->user), write the user's own record (only that user writes it), and rename frees the old
name last. Scenarios: same-name race, swap race, competitor between load and CAS, name reuse
after rename. Model: Names.p (tcRace: 2 clients, interleaved; tcSeq: exact result spec).
`snappea check` passes; 30 model mutants survive (mostly store/audit internals, sentinel
boundaries) - not chased. Not modelled: crashes between steps (a crash after claiming leaves a
name held without a user record; not repaired).
