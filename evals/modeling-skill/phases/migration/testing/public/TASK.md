# Test an online schema migration service

Read `migration.py`, `events.p`, and `contract.p`. Save a Python unittest module named `test_migration.py` that tests public behavior and validates captured calls with the P monitor. Do not modify the supplied files.

The service starts with legacy rows `a -> 10` and `b -> 20`, both at revision 0. New-schema values use `encoded = 10 * value + 1`. Keys are strings and values are integers. `prepare()` returns `(token, watermark)` for an immutable snapshot of the full authoritative table and supersedes any previously staged snapshot. `apply(token)` merges the current snapshot into the shadow table, replacing only older shadow revisions; the current token may be applied repeatedly, while superseded or unknown tokens return false. `write(key, value)` and `delete(key)` each allocate a fresh positive revision greater than every previous mutation and atomically update both authoritative and shadow rows. Deletes write revisioned tombstones. `ready()` is true exactly when the shadow table matches all authoritative rows, including tombstones. `cutover()` succeeds only when ready. Reads return `(present, value)` and must agree before and after a successful cutover. `inspect_new(key)` reports `(revision, deleted, encoded)`; an unknown shadow row is `(-1, False, -1)`, while a tombstone has its revision, `True`, and `-1`.

The service has no event instrumentation. Capture actual public calls and their results in your test wrapper. Use the supplied schema:

- `ePrepare(token, watermark)`
- `eApply(token, applied)`
- `eWrite(item, value, revision)` and `eDelete(item, revision)`
- `eCutover(accepted)`
- `eRead(item, present, value)`
- `eNewRow(item, revision, deleted, encoded)` and `eReady(complete)`

Run the saved tests with `python -m unittest -v`.
