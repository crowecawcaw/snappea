---
diataxis: tutorial
personas: [coding-agent, application-developer]
questions:
  - How do immutable staged writes and one root CAS survive crashes and retries?
---

# Publish immutable data through one durable root

```sh
snappea guide example blob-publication --out /tmp
cd /tmp/blob-publication
python -m pip install pysnappea pytest  # distribution pysnappea; import snappea
snappea check Publication.p --timeout 30s --deny mutation --json
python -m pytest -q
```

Check the JSON summary as well as the exit status: require zero `incomplete`, zero
`coverage_gaps`, zero `mutants_undetected`, and no bypassed quality gates. A bounded
complete search checks these actors, IDs and fault points; it is not a proof for
arbitrary concurrency or crashes. Use a longer budget for an incomplete search,
not `--warn coverage` to turn missing work into a passing claim.

The operation increments two heads together and returns its original result on
retry. Read `publication.py`: the atomic primitives are a durable immutable blob
write and compare-and-swap of **one root pointer**. Neither the whole method nor
all its storage writes form a transaction.

1. Read one root and its immutable manifest: both head references and request outcomes.
2. Derive new head values and stage their immutable blobs separately.
3. Stage an immutable manifest containing both references and the new request outcome.
4. CAS the root from the version read in step 1 to the manifest from step 3.
5. On conflict, reread, recompute both heads and rebuild the outcome map. On retry of
   an already committed ID, return its saved result without republishing an old root.

Equal head values can deduplicate to one content hash: three staging calls need not
create three new records.

A reader captures one root and follows immutable references. It cannot observe half
of a publication. A crash before the root CAS leaves unreachable staged records;
a crash after the CAS loses a response but preserves both heads and the outcome memo.
The example never deletes records; safe reclamation needs its own protocol.

`Publication.p` makes the boundaries explicit: Store handles each of the three
staging calls separately from root publication; Publisher may abandon local work
after any stage, or lose its response after successful root CAS. A restart discards
its local candidate and reads durable state, retaining the request ID. A separate no-crash case runs two clients concurrently on the root version, forcing
recomputation after a lost CAS. Four crash cases run the faulted client before another writer. A sixth case pauses
after the first snapshot, completes a competitor, then resumes through a failed CAS
and post-commit lost response. These fixed boundaries keep exhaustive checks small.
The model reloads durable outcomes after every root-CAS attempt. Python returns a
successful acknowledgment directly and reloads after conflict or restart; the extra
model read makes recovery explicit rather than establishing implementation equivalence.
`PublicationContract` independently checks staged references, unique effects,
matching heads, original retry results, and completion of both requested operations.

`test_publication.py` saves real Python regressions: all four durable crash points,
staging invisibility, a forced CAS loser, and an oversized record rejected before
publication. Each fault fixture asserts it actually fired. The Python tests use
ordinary assertions on actual returns and head reads; they do not pretend that an
inline monitor or return-order log proves the entire concurrent storage protocol.
For a shared P monitor checking captured public events, run `cas-queue` next.

## Limits

The model has two clients and two request IDs: one no-crash race, four ordered
recovery cases, and one controlled stale-snapshot/CAS-conflict case followed by a
lost response. Faulted cases have one chosen crash in the first client. They do not
explore arbitrary concurrent crash/recovery schedules. Staging boundaries are finite. It abstracts immutable content hashes as distinct record IDs;
records survive worker crashes. It omits partial/corrupt writes, storage loss,
network behavior and cryptographic collisions. The storage operations must really
provide atomic durable writes and CAS; a process-local lock alone is not durability.

Python's in-memory BlobStore simulates those primitives. It retains state when a
worker is recreated, not across interpreter or machine restart. Head values are
separate blobs, but the manifest and retained outcome map still have a finite record
limit, checked on every stage. Unbounded IDs, large metadata or large values need
chunking/indexing and a stated retention policy. Passing these small workloads does not establish scalability or correctness outside
the stated bounds.
