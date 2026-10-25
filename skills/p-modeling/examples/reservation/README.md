# Reserving the last unit

- **Contract:** committed reservations never exceed initial stock; remaining stock agrees with reservation records.
- **Correct:** `ReservationStore.reserve_if_unchanged` validates version/stock and updates stock + reservation under one lock. A database needs equivalent conditional update or transaction guarantees.
- **Broken:** both clients read the last unit, check stale snapshots, then independently write zero stock and a reservation. Locking only the final writes still oversells and loses a decrement.
- **Model:** `atomic.p` represents one conditional storage primitive; `stale-write.p` separates reads from writes so clients can interfere.
- **Oracle:** `capacity.p` subtracts committed reservation events independently; negative capacity fails. Hot `Open` requires a reservation in these bounded cases.
- **Implementation tests:** call real `storage.py` methods under `check_spec`, check emitted source locations, and independently compare actual records with initial/remaining stock. The broken implementation fails both checks.

## Run

From this directory:

```sh
python -m pip install pysnappea
python -m unittest -v
snappea check Atomic.pproj --timeout 5s
snappea check lint Atomic.pproj
snappea check lint StaleWrite.pproj
```

- Distribution: `pysnappea`; import: `snappea`.
- Expected: Python tests pass, lint clean, atomic project complete coverage and 12/12 mutants detected with the validated checker.
- Negative control:

```sh
snappea check StaleWrite.pproj --timeout 5s
```

- Expected: exit 1, `reservations exceed available capacity`. Coverage is partial and mutation checking skipped after failure.

## Boundaries

- Python `Lock` protects threads using this object, not other processes. Real storage must supply equivalent atomicity.
- Omits crashes, retries, idempotency, timeouts, persistence, and multiple processes.
- P explores finite event-level abstractions, not database semantics or every implementation path.
- Events are emitted after successful changes. Explicit recorder validation can fail tests; runtime emission must not alter application control flow.
- `strict=True` requires relevant events; `check_hot=True` checks outstanding progress. Neither proves complete instrumentation.
