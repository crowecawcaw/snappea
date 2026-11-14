// The ticketing contract, stated over events the store and the payment service announce
// at their commit points. Both specs rebuild what must be true from the history of
// events alone; neither reads the store's fields or trusts its decisions.

event Held: (seat: int, hold: int);
event HoldRejected: (seat: int, hold: int);
event Expired: (seat: int, hold: int);
event HoldReleased: (seat: int, hold: int);
event Sold: (seat: int, hold: int);
event ConfirmRejected: (seat: int, hold: int);
event Charged: (hold: int);
event Refunded: (hold: int);

// Safety: a seat is held by at most one hold at a time, is sold at most once, and only
// to the hold that currently has it. A rejection must be justified by the seat's state.
spec SeatsSoldOnce observes Held, HoldRejected, Expired, HoldReleased, Sold, ConfirmRejected {
  // seat -> hold that currently holds it (absent: free or sold)
  var heldBy: map[int, int];
  // seat -> hold it was sold to
  var soldTo: map[int, int];
  var expired: set[int];

  start state Watching {
    on Held do (p: (seat: int, hold: int)) {
      assert !(p.seat in heldBy), format("seat {0} held twice", p.seat);
      assert !(p.seat in soldTo), format("seat {0} held after it was sold", p.seat);
      heldBy[p.seat] = p.hold;
    }
    // snappea-lint: allow unused-payload-field -- this spec checks seat availability; CheckoutsEnd correlates the rejected hold
    on HoldRejected do (p: (seat: int, hold: int)) {
      assert p.seat in heldBy || p.seat in soldTo,
        format("hold {0} rejected although seat {1} was free", p.hold, p.seat);
    }
    on Expired do (p: (seat: int, hold: int)) {
      assert p.seat in heldBy && heldBy[p.seat] == p.hold,
        format("hold {0} expired but did not hold seat {1}", p.hold, p.seat);
      heldBy -= (p.seat);
      expired += (p.hold);
    }
    on HoldReleased do (p: (seat: int, hold: int)) {
      assert p.seat in heldBy && heldBy[p.seat] == p.hold,
        format("hold {0} released but did not hold seat {1}", p.hold, p.seat);
      heldBy -= (p.seat);
    }
    // snappea-lint: allow unused-payload-field -- money is tracked per hold; SeatsSoldOnce checks seats
    on Sold do (p: (seat: int, hold: int)) {
      assert !(p.seat in soldTo), format("seat {0} sold twice", p.seat);
      assert p.seat in heldBy && heldBy[p.seat] == p.hold,
        format("seat {0} sold to hold {1}, which does not hold it", p.seat, p.hold);
      heldBy -= (p.seat);
      soldTo[p.seat] = p.hold;
    }
    // snappea-lint: allow unused-payload-field -- this spec checks expiry; CheckoutsEnd checks the requested seat
    on ConfirmRejected do (p: (seat: int, hold: int)) {
      // The only legitimate reason to refuse a confirmation is that the hold expired.
      assert p.hold in expired,
        format("confirmation of live hold {0} on seat {1} was rejected", p.hold, p.seat);
    }
  }
}

// Money: every charge ends in exactly one of a sale or a refund. While a charge is
// unresolved the spec is hot, so an execution that ends with a customer charged for
// no seat (and no refund) fails.
spec ChargesResolved observes Charged, Sold, Refunded {
  var open: set[int];
  var resolved: set[int];

  start state Settled {
    on Charged do (p: (hold: int)) { Charge(p.hold); goto Owing; }
    on Sold do { assert false, "seat sold with no open charge"; }
    on Refunded do { assert false, "refund with no open charge"; }
  }

  hot state Owing {
    on Charged do (p: (hold: int)) { Charge(p.hold); }
    // snappea-lint: allow unused-payload-field -- money is tracked per hold; SeatsSoldOnce checks seats
    on Sold do (p: (seat: int, hold: int)) {
      Resolve(p.hold);
      if (sizeof(open) == 0) { goto Settled; }
    }
    on Refunded do (p: (hold: int)) {
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
// justifies, and every hold is eventually sold, expired or cancelled (no seat stays held forever,
// even when its customer walks away). The driver announces CheckoutStarted with what
// the customer wants, so a customer that never acts, or asks for the wrong seat, fails.
enum CheckoutOutcome { HoldUnavailable, Bought, LostHold, Abandoned, Cancelled }
// abandons: walks away after holding (only expiry frees the seat).
// cancels: cancels the hold right after getting it.
type Checkout = (hold: int, seat: int, abandons: bool, cancels: bool);
// The driver announces how many checkouts it will run before starting any, so a driver
// stuck waiting for a checkout that never ends fails too.
event CheckoutsPlanned: int;
event CheckoutStarted: Checkout;
event CheckoutEnded: (hold: int, outcome: CheckoutOutcome);

spec CheckoutsEnd observes CheckoutsPlanned, CheckoutStarted, Held, HoldRejected, Sold, Expired,
    HoldReleased, ConfirmRejected, CheckoutEnded {
  // hold -> what the customer asked for; removed when the checkout ends
  var started: map[int, Checkout];
  var liveHolds: set[int];
  // holds whose customer cancels them
  var cancelling: set[int];
  // hold -> the outcome its customer must report
  var due: map[int, CheckoutOutcome];
  var unfinished: int;

  start state Idle {
    on CheckoutsPlanned do (n: int) { Planned(n); }
    on CheckoutStarted do { assert false, "checkout started without a plan"; }
    on Held do { assert false, "hold for a checkout that never started"; }
    on HoldRejected do { assert false, "rejection for a checkout that never started"; }
    on Sold do { assert false, "sale for a checkout that never started"; }
    on Expired do { assert false, "expiry with no live hold"; }
    on HoldReleased do { assert false, "release with no live hold"; }
    on ConfirmRejected do { assert false, "confirmation for a checkout that never started"; }
    on CheckoutEnded do { assert false, "checkout ended before it started"; }
  }

  hot state Busy {
    on CheckoutsPlanned do { assert false, "checkouts planned twice"; }
    on CheckoutStarted do (p: Checkout) { Started(p); }
    on Held do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      liveHolds += (p.hold);
      if (started[p.hold].abandons) { due[p.hold] = Abandoned; }
      if (started[p.hold].cancels) { due[p.hold] = Cancelled; }
    }
    on HoldRejected do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      due[p.hold] = HoldUnavailable;
    }
    // snappea-lint: allow unused-payload-field -- money is tracked per hold; SeatsSoldOnce checks seats
    on Sold do (p: (seat: int, hold: int)) {
      assert p.hold in liveHolds, format("seat {0} sold to hold {1}, which is not live", p.seat, p.hold);
      liveHolds -= (p.hold);
      due[p.hold] = Bought;
      Settle();
    }
    // snappea-lint: allow unused-payload-field -- SeatsSoldOnce checks seat ownership; this monitor tracks hold completion
    on Expired do (p: (seat: int, hold: int)) {
      assert p.hold in liveHolds, format("seat {0} freed for hold {1}, which is not live", p.seat, p.hold);
      liveHolds -= (p.hold);
      Settle();
    }
    // snappea-lint: allow unused-payload-field -- SeatsSoldOnce checks seat ownership; this monitor tracks hold completion
    on HoldReleased do (p: (seat: int, hold: int)) {
      assert p.hold in liveHolds, format("seat {0} released for hold {1}, which is not live", p.seat, p.hold);
      assert p.hold in cancelling, format("hold {0} released, but its customer never cancelled", p.hold);
      liveHolds -= (p.hold);
      Settle();
    }
    on ConfirmRejected do (p: (seat: int, hold: int)) {
      Requested(p.seat, p.hold);
      assert !started[p.hold].abandons && !started[p.hold].cancels,
        format("abandoned or cancelled checkout {0} tried to confirm", p.hold);
      due[p.hold] = LostHold;
    }
    on CheckoutEnded do (p: (hold: int, outcome: CheckoutOutcome)) {
      assert p.hold in started, format("checkout {0} ended twice or never started", p.hold);
      assert p.hold in due, format("checkout {0} ended before the store decided", p.hold);
      assert p.outcome == due[p.hold],
        format("checkout {0} reported {1}, the store decided {2}", p.hold, p.outcome, due[p.hold]);
      assert p.outcome != Cancelled || !(p.hold in liveHolds),
        format("checkout {0} cancelled, but its hold still has the seat", p.hold);
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

  fun Started(c: Checkout) {
    assert !(c.hold in started) && !(c.hold in due), format("checkout {0} started twice", c.hold);
    assert !(c.abandons && c.cancels), format("checkout {0} both abandons and cancels", c.hold);
    started[c.hold] = c;
    if (c.cancels) { cancelling += (c.hold); }
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
