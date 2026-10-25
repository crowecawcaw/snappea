# P syntax and commands

## Independent monitor

From [Capacity](../examples/reservation/capacity.p):

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

- `observes` selects events; monitor variables track independent contract state.
- Hot states mark outstanding obligations when liveness checking is enabled; cold states do not. Hot states do not imply fairness.

## Test case

```p
test tcAtomicConditionalReserve [main=AtomicCase]:
  assert Capacity in { Store, Client, AtomicCase };
```

- Names the entry machine, participating machines, and checked monitor.
- Scheduling points include `send`, `receive`, and machine creation; local assignments do not yield. Match non-yielding updates to real atomic primitives.

## Commands

From [reservation](../examples/reservation/README.md):

```sh
snappea check Atomic.pproj --timeout 5s
snappea check StaleWrite.pproj --timeout 5s
```

- Atomic project: expected pass with complete required coverage. Run the whole project; one case misses the later-reservation handler.
- Stale-write project: expected oversell assertion failure.
- Checking type-checks and lints first. Default timeout: 10 seconds per test case; override with `--timeout 2m`.
- A timeout is incomplete evidence; exhaustive search covers only finite modeled bounds.
- For Python capture/check syntax, see [implementation](../guides/implementation.md); install `pysnappea`, import `snappea`.
- Related: [durable retry](../examples/durable-retry/README.md), [versioned publication](../examples/versioned-publication/README.md), [checking docs](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md).
