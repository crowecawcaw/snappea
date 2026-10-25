# Version-bound configuration publication

This example models two publishers preparing immutable configuration records from one active version. Each publisher reads a version and the active record, derives the next configuration generation, and validates that candidate against the snapshot. Its evidence names the version it checked. It then asks the store to publish the candidate with a compare-and-swap (CAS) on the version.

If both publishers read version 0 and record 0, both prepare record 1, but only one CAS can succeed. The loser performs a fresh read. The passing model derives record 2 and revalidates it against the new snapshot before retrying. The broken model updates only the CAS expectation and reuses record 1 and its old evidence; the independent monitor rejects that publication.

## Run it

Use a current Snappea binary on `PATH`. From this example directory:

```sh
snappea check Revalidated.p
```

Expected: exit status 0. Lint is clean and the check completes all schedules with required statement, branch, and handler coverage. The command also runs Snappea's default mutation feedback.

Run the deliberately broken variant separately:

```sh
snappea check StaleRetry.p
```

Expected: exit status 1. The trace reports that a record validated for version 0 was published while the monitor's current version was 1. The failure demonstrates why updating only the CAS expectation is unsafe. The checker stops the failing test before mutation analysis.

If building from source, run `cargo build --release -p snappea` from the repository root and add its `target/release` directory to `PATH`.

## What the model means

`eRead` is the store's read primitive and returns the active version and record pointer. The record is a configuration generation: the candidate is the current generation plus one, and the modeled validation rule accepts exactly that next generation. `eTryPublish` is a version-conditional write: on a matching version, the store advances the version and changes the active record pointer in one uninterrupted local update before the response is sent; otherwise it reports a conflict without changing the pointer. That successful transition is the modeled atomic point. Candidate derivation and validation happen before it. The records stand for immutable prepared configuration snapshots; their contents are abstracted away.

`EvidenceMatchesCurrentVersion` keeps its own version and pointer state. It checks each publication's evidence version, base record, and next-generation rule against that independent state. It also checks that the store's success response names the version and record just published. The monitor is not part of the store's CAS decision.

## Limits

The modeled validation rule is intentionally small and deterministic; the model does not implement real configuration rules or certificate cryptography. Record contents and preparation work are abstracted as immutable snapshots whose generation is an integer. It assumes the store's version check, version advance, and pointer replacement really are atomic. A database transaction, lock, or conditional-write primitive must justify that assumption in an implementation.

The model omits crashes, durability, recovery, external readers, version overflow, and unbounded configurations. Its finite schedule search is evidence about these modeled interleavings only. A clean pass does not prove the implementation correct, establish fairness, or cover behavior omitted here. The default command's mutation report is also a heuristic; its residual undetected mutations need interpretation rather than being treated as a proof result.
