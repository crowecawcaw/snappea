# Reserving the last unit

Two clients each want the only available unit. A read tells each client what was available at that moment; it does not hold that capacity for the client. The contract here is that committed reservations never exceed the initial stock and that the stored remaining stock agrees with the committed reservation records.

`ReservationStore.reserve_if_unchanged` uses a lock to validate the snapshot version and stock and update both stock and the reservation record in one critical section. A database implementation needs the equivalent real primitive, such as a conditional `UPDATE ... WHERE version = ? AND available >= ?` whose row count is checked, or a transaction with suitable locking. Merely putting a client’s read, check, and later write into one P handler would hide the race by assumption.

The deliberately broken method still locks each final update, but it checks an old snapshot before taking that lock and never verifies that the snapshot is current. The deterministic test schedule makes both clients read first, then commit. Each writes `available = 0` and records one reservation, so the result contains two reservations for one unit. This is both overselling and a lost update: the stock field reflects only one decrement while two reservation rows exist.

`capacity.p` is the independent `Capacity` monitor used by both the P tests and the Python tests. It subtracts every actual `eReserved` event from the capacity and fails if the result goes negative. It never accepts a `valid` flag from the application. The hot `Open` state also requires an available unit to be reserved in these bounded model cases. In Python, `strict=True` requires the monitor to have relevant events and `check_hot=True` checks that progress expectation. `emit` is called after a successful state change; the test’s `check_spec` evaluates the recorded events when its context exits. The application does not use monitor results to decide whether to commit.

The Python test calls the real `storage.py` methods while the `snappea` recorder captures their `emit` calls. It checks the same P monitor against those implementation-origin events, and checks the source location of the actual reservation event. Separately, `assert_inventory_invariant` derives the total from the actual reservation records and compares it with both initial and remaining stock. The intentionally broken implementation fails both checks.

The P model has an explicit correspondence boundary. `atomic.p` makes the conditional storage operation one `Store` event handler; that is valid for this property only when the implementation really has one atomic compare-and-update boundary. `stale-write.p` splits the read response from the later write, allowing both clients to carry the same old snapshot into separate commits. The P model explores small executions of those abstractions; it does not prove a database’s transaction semantics or all behavior of a deployed service.

## Run it

Install `pysnappea` (which provides the `snappea` Python import and checker), or use your existing installation. From this example directory:

```sh
python -m pip install pysnappea
python -m unittest -v
snappea check Atomic.pproj --timeout 5s
snappea check lint Atomic.pproj
snappea check lint StaleWrite.pproj
```

The Python tests pass. The atomic P project passes with complete model coverage and all 12 generated mutations detected. The lint commands report no findings. Each command takes well under a second here.

The broken model is an expected failure. Run it separately and expect exit status `1` with `reservations exceed available capacity` in the report:

```sh
snappea check StaleWrite.pproj --timeout 5s
```

It stops at the first counterexample, so its coverage is partial and mutation checking is skipped. That is expected for this negative control; the passing atomic project is the one used for full coverage and mutation feedback.

## Assumptions and limits

- The Python store stands in for a single storage authority. Its `Lock` makes the conditional validation and update atomic among threads using this object. A real service must rely on the database’s documented transaction or compare-and-swap guarantee.
- This schedule isolates concurrent stale reads. It omits crashes, retries, idempotency, timeouts, multiple processes, and persistence; the lock does not represent a cross-process guarantee.
- The P model checks the event-level capacity contract for the modeled clients and finite values. It does not establish correctness for every input, workload, storage engine, or implementation path. Events cannot reveal behavior that the implementation does not emit.
- The production `emit` API is observational instrumentation. These tests use an explicit recorder that can fail the test after calls complete; they do not route checker failures into application control flow.
