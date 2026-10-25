# Model an online schema migration

Design a service that changes a key/value table from legacy integer values to an encoded shadow table while requests continue. The risky boundary is a backfill that captures a snapshot, pauses, and applies after live writes or deletes have changed the authoritative data.

## Contract and operations

- The logical table starts with `a -> 10` and `b -> 20`, each at revision 0. Keys are strings and values are integers.
- The new schema stores `10 * value + 1`. A tombstone is a deleted row with its own revision; it is not equivalent to an old live row or an unknown row.
- `prepare()` atomically copies the complete authoritative table, including tombstones, into a staged snapshot and returns `(token, watermark)`. Tokens are positive and strictly increasing; the watermark is the greatest logical revision in the captured snapshot, or 0 if no change has occurred. A new prepare supersedes any unapplied snapshot.
- `apply(token)` merges the current staged snapshot into the shadow table. Each row may replace a shadow row only when its snapshot revision is newer. One call to `apply` is atomic with respect to other public calls; writes and deletes may interleave between prepare and apply. Applying the current token again is safe and returns true. Superseded or unknown tokens return false.
- `write(key, value)` and `delete(key)` each allocate a fresh positive logical revision greater than every previously issued revision. Each operation atomically changes the authoritative table and its shadow row, including a shadow tombstone on delete. The operations are live while backfill is pending and after cutover.
- `ready()` is true exactly when every authoritative row, including tombstones, equals its shadow row. `cutover()` returns true and switches logical reads to the shadow table only when ready; otherwise it returns false and leaves the read path unchanged. An accepted cutover stays active. Later live writes and deletes remain visible.
- `read(key)` returns `(present, value)` with the logical integer value from the active read path, decoding the new representation after cutover; absent and deleted keys both return `(False, 0)`. `inspect_new(key)` returns `(revision, deleted, encoded)`, using `(-1, False, -1)` for a key never present in the shadow table and `(revision, True, -1)` for a tombstone. Revisions and encoded values are observable through this inspection call.

## Deliver

- `model.p`: executable design with separate live request and backfill actors. Keep snapshot preparation and application as distinct operations so live writes and deletes can interleave.
- `monitor.p`: independent `spec MigrationContract`, using `events.p` and checking the external history contract.
- `tests.p`: check useful success and exercise the contract's concurrent migration lifecycle.
- `DESIGN.md`: explain the real atomic boundaries, how the model represents a concurrent live actor and backfill actor, bounds, omissions, and check results.

The event fields are public method inputs/results in linearization order. Do not rely on hidden model state for the monitor. Run `snappea check events.p monitor.p model.p tests.p` from this directory.
