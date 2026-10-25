# P patterns worth keeping nearby

The Python distribution is `pysnappea`; application and test code import `snappea`. The skill’s reservation example includes a complete implementation and a test that checks events emitted by real method calls:

```python
from snappea import check_spec
from storage import ReservationStore

with check_spec(
    "Capacity",
    spec_files=["capacity.p"],
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

This form fails if the event history violates the monitor. The example also checks an intentionally broken stale write and uses a separate Python invariant over the actual committed records. `strict=True` rejects a selected spec that saw no relevant events; it does not prove that every operation emitted an event. `unknown_events="error"` catches misspelled or undeclared event names. Runtime observation can only check events it receives. See [reservation](../examples/reservation/README.md).

## Monitor state from events

The [Capacity spec](../examples/reservation/capacity.p) tracks available units from an opening event and subsequent reservations:

```p
event eOpened: (available: int);
event eReserved: (order: string, units: int);

// The monitor is an independent contract: each committed reservation consumes
// capacity. It does not trust an application-supplied `valid` result.
spec Capacity observes eOpened, eReserved {
  var available: int;
  var orders: set[string];

  start state BeforeOpen {
    on eOpened do (p: (available: int)) {
      assert p.available >= 0, "opened with negative stock";
      available = p.available;
      goto Open;
    }
    on eReserved do { assert false, "reservation before store opened"; }
  }

  hot state Open {
    on eOpened do { assert false, "store opened twice"; }
    on eReserved do (p: (order: string, units: int)) {
      assert p.units > 0, "reservation must be positive";
      assert !(p.order in orders), "order reserved twice";
      available = available - p.units;
      assert available >= 0, "reservations exceed available capacity";
      orders += (p.order);
      goto HasReservation;
    }
  }

  state HasReservation {
    on eOpened do { assert false, "store opened twice"; }
    on eReserved do (p: (order: string, units: int)) {
      assert p.units > 0, "reservation must be positive";
      assert !(p.order in orders), "order reserved twice";
      available = available - p.units;
      assert available >= 0, "reservations exceed available capacity";
      orders += (p.order);
    }
  }
}
```

Events declare payload shape; `observes` selects which events advance the monitor; monitor variables capture contract state. Keep the oracle independent: derive capacity from committed reservation events instead of trusting an application result flag.

## Test case and commands

A P test case names the entry machine and the monitor it checks. The reservation project includes both a conditional atomic case and an intentionally broken read/check/write case:

```p
test tcAtomicConditionalReserve [main=AtomicCase]:
  assert Capacity in { Store, Client, AtomicCase };
```

From the example directory, run each complete project with:

```sh
snappea check Atomic.pproj --timeout 5s
snappea check StaleWrite.pproj --timeout 5s
```

The atomic project should pass with complete coverage; the stale-write project is a negative control and should fail with the oversell assertion. Selecting only `tcAtomicConditionalReserve` misses the monitor’s later-reservation handler, so use the full atomic project when interpreting its coverage. A passing check covers only the represented actors, bounds, and operations. The [example README](../examples/reservation/README.md) gives exact expected results and its deliberate limits.

Snappea runs a machine until a scheduling point such as `send`, `receive`, or machine creation; ordinary local assignments do not yield by themselves. A sequence of assignments can represent one storage primitive only when the implementation gives it the same atomic guarantee. Put a real read and later write across messages or phases so another client can run between them. The [durable retry example](../examples/durable-retry/README.md) shows how to keep durable and volatile state distinct across a restart; [versioned publication](../examples/versioned-publication/README.md) shows evidence tied to a version.

A `hot state` marks work that must not remain unfinished at a checked execution end when liveness checking is enabled. A `cold state` has no such obligation. For event histories, `snappea observe --check-hot` also checks whether a monitor ends hot. This validates only the supplied history; it does not schedule or exercise the implementation.

The CLI type-checks and lints before exploring. Its default limit is 10 seconds per test case; `--timeout 2m` changes that limit. A timeout is incomplete evidence. A clean exhaustive finite search covers the bounded model, not unbounded deployments. See [checking models](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md).
