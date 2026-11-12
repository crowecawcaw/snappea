// The ticketing contract, stated over events the store and the payment service announce
// at their commit points. Both specs rebuild what must be true from the history of
// events alone; neither reads the store's fields or trusts its decisions.

event eHeld: (seat: int, hold: int);
event eHoldRejected: (seat: int, hold: int);
event eExpired: (seat: int, hold: int);
event eSold: (seat: int, hold: int);
event eConfirmRejected: (seat: int, hold: int);
event eCharged: (hold: int);
event eRefunded: (hold: int);

// Safety: a seat is held by at most one hold at a time, is sold at most once, and only
// to the hold that currently has it. A rejection must be justified by the seat's state.
spec SeatsSoldOnce observes eHeld, eHoldRejected, eExpired, eSold, eConfirmRejected {
  // seat -> hold that currently holds it (absent: free or sold)
  var heldBy: map[int, int];
  // seat -> hold it was sold to
  var soldTo: map[int, int];
  var expired: set[int];

  start state Watching {
    on eHeld do (p: (seat: int, hold: int)) {
      assert !(p.seat in heldBy), format("seat {0} held twice", p.seat);
      assert !(p.seat in soldTo), format("seat {0} held after it was sold", p.seat);
      heldBy[p.seat] = p.hold;
    }
    on eHoldRejected do (p: (seat: int, hold: int)) {
      assert p.seat in heldBy || p.seat in soldTo,
        format("hold {0} rejected although seat {1} was free", p.hold, p.seat);
    }
    on eExpired do (p: (seat: int, hold: int)) {
      assert p.seat in heldBy && heldBy[p.seat] == p.hold,
        format("hold {0} expired but did not hold seat {1}", p.hold, p.seat);
      heldBy -= (p.seat);
      expired += (p.hold);
    }
    on eSold do (p: (seat: int, hold: int)) {
      assert !(p.seat in soldTo), format("seat {0} sold twice", p.seat);
      assert p.seat in heldBy && heldBy[p.seat] == p.hold,
        format("seat {0} sold to hold {1}, which does not hold it", p.seat, p.hold);
      heldBy -= (p.seat);
      soldTo[p.seat] = p.hold;
    }
    on eConfirmRejected do (p: (seat: int, hold: int)) {
      // The only legitimate reason to refuse a confirmation is that the hold expired.
      assert p.hold in expired,
        format("confirmation of live hold {0} on seat {1} was rejected", p.hold, p.seat);
    }
  }
}

// Money: every charge ends in exactly one of a sale or a refund. While a charge is
// unresolved the spec is hot, so an execution that ends with a customer charged for
// no seat (and no refund) fails.
spec ChargesResolved observes eCharged, eSold, eRefunded {
  var open: set[int];
  var resolved: set[int];

  start state Settled {
    on eCharged do (p: (hold: int)) { Charge(p.hold); goto Owing; }
    on eSold do { assert false, "seat sold with no open charge"; }
    on eRefunded do { assert false, "refund with no open charge"; }
  }

  hot state Owing {
    on eCharged do (p: (hold: int)) { Charge(p.hold); }
    // snappea-lint: allow unused-payload-field -- money is tracked per hold; SeatsSoldOnce checks seats
    on eSold do (p: (seat: int, hold: int)) {
      Resolve(p.hold);
      if (sizeof(open) == 0) { goto Settled; }
    }
    on eRefunded do (p: (hold: int)) {
      Resolve(p.hold);
      if (sizeof(open) == 0) { goto Settled; }
    }
  }

  fun Charge(hold: int) {
    assert !(hold in open) && !(hold in resolved), format("hold {0} charged twice", hold);
    open += (hold);
  }

  fun Resolve(hold: int) {
    assert hold in open, format("hold {0} sold or refunded without an open charge", hold);
    open -= (hold);
    resolved += (hold);
  }
}

// Progress: every checkout the driver starts ends, with the outcome the store's history
// justifies, and every hold is eventually sold or expired (no seat stays held forever,
// even when its customer walks away). The driver announces eCheckoutStarted with what
// the customer wants, so a customer that never acts, or asks for the wrong seat, fails.
enum tOutcome { HoldRejected, Bought, LostHold, Abandoned }
type tCheckout = (hold: int, seat: int, abandons: bool);
// The driver announces how many checkouts it will run before starting any, so a driver
// stuck waiting for a checkout that never ends fails too.
event eCheckoutsPlanned: int;
event eCheckoutStarted: tCheckout;
event eCheckoutEnded: (hold: int, outcome: tOutcome);

spec CheckoutsEnd observes eCheckoutsPlanned, eCheckoutStarted, eHeld, eHoldRejected, eSold, eExpired,
    eConfirmRejected, eCheckoutEnded {
  // hold -> what the customer asked for; removed when the checkout ends
  var started: map[int, tCheckout];
  var liveHolds: set[int];
  // hold -> the outcome its customer must report
  var due: map[int, tOutcome];
  var unfinished: int;

  start state Idle {
    on eCheckoutsPlanned do (n: int) { Planned(n); }
    on eCheckoutStarted do { assert false, "checkout started without a plan"; }
    on eHeld do { assert false, "hold for a checkout that never started"; }
    on eHoldRejected do { assert false, "rejection for a checkout that never started"; }
    on eSold do { assert false, "sale for a checkout that never started"; }
    on eExpired do { assert false, "expiry with no live hold"; }
    on eConfirmRejected do { assert false, "confirmation for a checkout that never started"; }
    on eCheckoutEnded do { assert false, "checkout ended before it started"; }
  }

  hot state Busy {
    on eCheckoutsPlanned do { assert false, "checkouts planned twice"; }
    on eCheckoutStarted do (p: tCheckout) { Started(p); }
    on eHeld do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      liveHolds += (p.hold);
      if (started[p.hold].abandons) { due[p.hold] = Abandoned; }
    }
    on eHoldRejected do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      due[p.hold] = HoldRejected;
    }
    on eSold do (p: (seat: int, hold: int)) {
      assert p.hold in liveHolds, format("seat {0} sold to hold {1}, which is not live", p.seat, p.hold);
      liveHolds -= (p.hold);
      due[p.hold] = Bought;
      Settle();
    }
    on eExpired do (p: (seat: int, hold: int)) {
      assert p.hold in liveHolds, format("seat {0} freed for hold {1}, which is not live", p.seat, p.hold);
      liveHolds -= (p.hold);
      Settle();
    }
    on eConfirmRejected do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      assert !started[p.hold].abandons, format("abandoned checkout {0} tried to confirm", p.hold);
      due[p.hold] = LostHold;
    }
    on eCheckoutEnded do (p: (hold: int, outcome: tOutcome)) {
      assert p.hold in started, format("checkout {0} ended twice or never started", p.hold);
      assert p.hold in due, format("checkout {0} ended before the store decided", p.hold);
      assert p.outcome == due[p.hold],
        format("checkout {0} reported {1}, the store decided {2}", p.hold, p.outcome, due[p.hold]);
      started -= (p.hold);
      unfinished = unfinished - 1;
      Settle();
    }
  }

  fun Planned(n: int) {
    assert n > 0, "planned no checkouts";
    unfinished = n;
    goto Busy;
  }

  fun Started(c: tCheckout) {
    assert !(c.hold in started) && !(c.hold in due), format("checkout {0} started twice", c.hold);
    started[c.hold] = c;
  }

  fun Requested(seat: int, hold: int) {
    assert hold in started, format("hold {0} on seat {1} was never requested", hold, seat);
    assert started[hold].seat == seat,
      format("hold {0} acted on seat {1}, but its customer wanted seat {2}", hold, seat, started[hold].seat);
  }

  fun Settle() {
    if (unfinished == 0 && sizeof(liveHolds) == 0) { goto Idle; }
  }
}
