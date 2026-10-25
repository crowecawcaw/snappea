# Version-bound publication

- **Design:** two publishers read an active version/record, derive the next generation, validate against that snapshot, and CAS the active pointer.
- **Correct:** CAS loser rereads, recomputes, and revalidates. After another publisher commits generation 1, prepare generation 2.
- **Broken:** change only the expected CAS version while retaining generation 1 and version-0 evidence.
- **Oracle:** `EvidenceMatchesCurrentVersion` tracks its own version/pointer; checks evidence version, base record, next-generation relation, and success response.

## Run

From this directory, with `snappea` on `PATH`:

```sh
snappea check Revalidated.p
snappea check StaleRetry.p
```

- `Revalidated.p`: exit 0, clean lint, complete bounded exploration and required coverage; includes default mutation feedback.
- `StaleRetry.p`: exit 1, evidence for version 0 published at current version 1; mutation analysis skipped after failure.
- Source build: `cargo build --release -p snappea`; add repository `target/release` to `PATH`.

## Boundaries

- `eRead` returns version/pointer. `eTryPublish` atomically checks version, advances it, and replaces the pointer before sending a response.
- Preparation/validation precede publication; immutable contents are abstracted as integer generations. Validation accepts exactly the next generation.
- Implementation needs equivalent transaction, lock, or conditional-write guarantees.
- Omits crashes, durability, recovery, external readers, overflow, real validation/cryptography, and unbounded configurations.
- Passing search covers modeled interleavings only. Surviving mutations require interpretation; neither search nor mutations prove implementation correctness or fairness.
