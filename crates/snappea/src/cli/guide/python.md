# Check actual implementation behavior

## Capture and check

- Install `pysnappea`; import `snappea`.
- Emit after meaningful committed changes, with relevant IDs, values, and versions.
- Partition by independent history; operations sharing an invariant must reach the same monitor instance.
- Save the observation test with implementation tests so later changes rerun it.

```python
from snappea import check_spec
from storage import ReservationStore


def test_competing_reservations():
    with check_spec(
        "Capacity",
        spec_files=["Reservation.p"],
        strict=True,
        check_hot=True,
        unknown_events="error",
    ):
        store = ReservationStore(1)  # emits eOpened
        first = store.read()
        second = store.read()
        assert store.reserve_if_unchanged("order-a", 1, first)
        assert not store.reserve_if_unchanged("order-b", 1, second)
```

- `check_spec` captures application `emit` calls; it does not intercept arbitrary methods. `check_events(events, "Capacity", spec_files=[...])` checks an existing recording.
- Without application emission, wrap real calls and record actual inputs, results, and public reads. Do not invent internal commits. Hand-written traces test only the monitor.
- Preserve distinctions: mapping all noninitial heads to one value can hide wrong results.
- See reservation (`snappea guide example reservation`) for saved tests, a rejected defective implementation, and an independent Python invariant.

## Evidence limits

- `strict=True` fails when a selected spec receives no relevant events; it cannot prove every operation emitted.
- `unknown_events="error"` rejects undeclared/misspelled names. CLI observation defaults to ignoring unknown events; declared events ignored by a spec are unchecked by it.
- `check_hot=True` checks unfinished monitor obligations at history end; observation does not explore application schedules.
- Emission before commit does not prove commit. Post-commit emission can be lost on crash.
- Concurrent post-commit emitters can reorder records. Order-sensitive checks need a version/sequence captured at the atomic operation or a controlled test boundary.
- Join child processes before checking their recordings; isolate each test/history. Event `src` identifies its emitting call site in failures.
- Runtime emitters must remain best-effort: no raised errors or blocked requests. Operational evidence should expose drops; explicit tests should validate recordings and fail on violations.

## Maintain correspondence

- Compare real reads, writes, atomicity, version checks, retries, and persistence with the model. Fix mismatches or justify equivalent primitives.
- Use observed monitor coverage to select missing application tests; model coverage measures different evidence.
- Keep direct boundary, concurrency, and crash tests. Test encoded record limits and valid logical operations exceeding one record, including after history grows.
- A passing observation validates the delivered history against the monitor, not missing events, unobserved schedules, or assumptions.
- Options: [Python helpers](https://github.com/crowecawcaw/snappea/blob/main/python/src/snappea/__init__.py), [strict observation](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md#strict-observation).
