event eOpened: (available: int);
event eReserved: (order: string, units: int);

// The monitor is an independent contract: each committed reservation consumes
// capacity. It does not trust an application-supplied `valid` result.
// This is a committed-effect safety oracle, not a complete acceptance contract:
// Python tests separately require useful success and check rejected calls leave
// actual stock/records unchanged. Post-lock events may reorder across threads;
// subtracting positive reservations is order-independent for this invariant.
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
