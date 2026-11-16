---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I capture real Python calls and check them against a P spec?
  - What does checking observed executions prove, and what not?
---

# Check real Python calls against a P spec

`pip install pysnappea`; import `snappea`. Keep specs in their own .p file.

```python
from snappea import check_spec
from storage import ReservationStore

def test_competing_reservations():
    with check_spec("Capacity", spec_files=["Reservation.p"], strict=True,
                    check_hot=True, unknown_events="error"):
        store = ReservationStore(1)  # emits eOpened
        first = store.read()
        second = store.read()
        assert store.reserve_if_unchanged("order-a", 1, first)
        assert not store.reserve_if_unchanged("order-b", 1, second)
```

- `check_spec` captures the application's `snappea.emit(...)` calls. Without emit
  calls, record actual inputs, results and public reads as
  `{"event": ..., "payload": {...}}` dicts and check them with
  `check_events(events, "Capacity", spec_files=[...], strict=True)`. Live system
  logs: `snappea observe`.
- Record only what happened; never invent internal commits or add locks to make
  events line up. Emit after the commit, with the IDs, values and versions.
- Operations sharing an invariant must reach the same monitor instance.
- Runnable: `snappea guide example reservation`, `snappea guide example cas-queue`.

## What a pass proves

- The recorded history satisfies the monitor. It explores no schedules and cannot
  see missing events.
- `strict=True` fails when a spec gets no events; `unknown_events="error"` rejects
  misspelled names; `check_hot=True` fails on obligations open at the end.
- Under concurrency, return and emit order are not commit order. Use the monitor on
  sequential or forced orders, or capture a sequence number at the atomic step.
- A spec for `snappea observe` must accept every order the real system can log.
- Emitters are best-effort: they never raise or block, and a crash can lose an emit.

[Python helpers](https://github.com/crowecawcaw/snappea/blob/main/python/src/snappea/__init__.py),
[strict observation](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md#strict-observation).
