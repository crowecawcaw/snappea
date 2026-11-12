// The outbox contract, over commits (announced by the database), what the broker
// received (announced by the broker, part of the environment), outbox marks, and
// what the consumer applied.

// The driver: how many orders it will place and how many relay crashes it injects;
// then, per order, whether its transaction is meant to commit.
event eOrdersPlanned: (orders: int, crashes: int);
event eOrderPlaced: (id: int, commits: bool);
event eOrderCommitted: int;
event eOrderAborted: int;
event eBrokerReceived: int;
event eMarkedSent: int;
event eApplied: int;

// Safety: an order commits or aborts as its transaction does; nothing is published or
// applied for an order whose transaction did not commit; an outbox row is marked sent
// only after the broker has the message; the consumer applies each order at most once,
// however many copies the broker delivers. An injected relay crash happens between a
// publish and its mark.
spec AppliedOnce observes eOrderPlaced, eOrderCommitted, eOrderAborted, eBrokerReceived, eMarkedSent,
    eApplied, eRelayCrashed {
  // order id -> meant to commit?
  var placed: map[int, bool];
  var committed: set[int];
  var aborted: set[int];
  var published: set[int];
  var marked: set[int];
  var applied: set[int];

  start state Watching {
    on eOrderPlaced do (p: (id: int, commits: bool)) {
      assert !(p.id in placed), format("order {0} placed twice", p.id);
      placed[p.id] = p.commits;
    }
    on eOrderCommitted do (id: int) {
      Decided(id);
      assert placed[id], format("order {0} committed, but its transaction was meant to fail", id);
      committed += (id);
    }
    on eOrderAborted do (id: int) {
      Decided(id);
      assert !placed[id], format("order {0} aborted, but its transaction was meant to commit", id);
      aborted += (id);
    }
    on eBrokerReceived do (id: int) {
      assert id in committed, format("order {0} published, but its transaction did not commit", id);
      published += (id);
    }
    on eMarkedSent do (id: int) {
      assert id in published, format("outbox row {0} marked sent before the broker had it", id);
      marked += (id);
    }
    on eApplied do (id: int) {
      assert id in published, format("order {0} applied without being published", id);
      assert !(id in applied), format("order {0} applied twice", id);
      applied += (id);
    }
    on eRelayCrashed do {
      // Every marked row was published (checked above), so this says some row is
      // published and not yet marked.
      assert sizeof(published) > sizeof(marked), "relay crashed, but not between a publish and its mark";
    }
  }

  fun Decided(id: int) {
    assert id in placed, format("order {0} decided but never placed", id);
    assert !(id in committed) && !(id in aborted), format("order {0} decided twice", id);
  }
}

// Progress: every planned order is decided; every committed order is eventually
// applied by the consumer; every planned relay crash happens. Hot until then: a lost
// event, a relay that sleeps through a commit, or a crashed relay nobody replaced fails.
spec CommittedOrdersDelivered observes eOrdersPlanned, eOrderCommitted, eOrderAborted, eApplied,
    eRelayCrashed {
  var undecided: int;
  var crashesDue: int;
  // committed, not yet applied
  var undelivered: set[int];

  start state Idle {
    on eOrdersPlanned do (p: (orders: int, crashes: int)) {
      assert p.orders > 0, "planned no orders";
      undecided = p.orders;
      crashesDue = p.crashes;
      goto Undelivered;
    }
    on eRelayCrashed do { assert false, "relay crashed before any order was planned"; }
    on eOrderCommitted do { assert false, "order committed before any were planned"; }
    on eOrderAborted do { assert false, "order aborted before any were planned"; }
    on eApplied do { assert false, "order applied before any were planned"; }
  }

  hot state Undelivered {
    on eOrdersPlanned do { assert false, "orders planned twice"; }
    on eOrderCommitted do (id: int) {
      undecided = undecided - 1;
      undelivered += (id);
    }
    // snappea-lint: allow unused-payload-field -- AppliedOnce checks which orders abort
    on eOrderAborted do (id: int) {
      undecided = undecided - 1;
      Settle();
    }
    on eApplied do (id: int) {
      // AppliedOnce rejects an unknown or repeated id; here only progress matters.
      undelivered -= (id);
      Settle();
    }
    on eRelayCrashed do {
      assert crashesDue > 0, "relay crashed, but no crash was injected";
      crashesDue = crashesDue - 1;
      Settle();
    }
  }

  state Delivered {
    on eOrdersPlanned do { assert false, "orders planned twice"; }
    on eOrderCommitted do { assert false, "order committed after all were delivered"; }
    on eOrderAborted do { assert false, "order aborted after all were delivered"; }
    on eRelayCrashed do { assert false, "relay crashed after everything was delivered"; }
    // A late duplicate is AppliedOnce's business.
    ignore eApplied;
  }

  fun Settle() {
    if (undecided == 0 && crashesDue == 0 && sizeof(undelivered) == 0) { goto Delivered; }
  }
}

// Progress, for the outbox design: every committed order's outbox row is eventually
// marked sent, so the outbox drains and rows are not republished forever.
spec OutboxDrains observes eOrderCommitted, eMarkedSent {
  var unmarked: set[int];

  start state Drained {
    on eOrderCommitted do (id: int) { unmarked += (id); goto Pending; }
    // A second relay may mark a row again; AppliedOnce checks it was published.
    ignore eMarkedSent;
  }

  hot state Pending {
    on eOrderCommitted do (id: int) { unmarked += (id); }
    on eMarkedSent do (id: int) {
      unmarked -= (id);
      if (sizeof(unmarked) == 0) { goto Drained; }
    }
  }
}
