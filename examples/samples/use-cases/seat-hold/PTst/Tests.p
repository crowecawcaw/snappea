// Drivers. Customers are part of the design under test (they run the checkout
// algorithm), so only these setup machines are drivers.

fun StartCustomers(owner: machine, store: machine, checkouts: seq[Checkout]) {
  var payments: Payments;
  var c: Checkout;
  payments = new Payments();
  announce CheckoutsPlanned, sizeof(checkouts);
  foreach (c in checkouts) {
    StartCheckout(owner, store, payments, c);
  }
}

fun StartCheckout(owner: machine, store: machine, payments: Payments, c: Checkout) {
  // The driver announces each checkout it starts, so CheckoutsEnd can demand that it
  // ends with the outcome the store's decisions justify.
  announce CheckoutStarted, c;
  new Customer((owner = owner, store = store, payments = payments, seat = c.seat,
                hold = c.hold, abandons = c.abandons, cancels = c.cancels));
}

// Two customers want the same seat. Every ordering of hold, payment, expiry and
// confirmation is explored: the second customer can be rejected, or can get the seat
// after the first hold expires, in which case the first must be refunded.
machine TwoCustomersOneSeat {
  start state Init {
    entry {
      var checkouts: seq[Checkout];
      checkouts += (0, (hold = 1, seat = 7, abandons = false, cancels = false));
      checkouts += (1, (hold = 2, seat = 7, abandons = false, cancels = false));
      StartCustomers(this, new SeatStore(), checkouts);
    }
    ignore CheckoutDone;
  }
}

// Three customers pick among two seats, and each may walk away after holding: some
// contend, some do not, and an abandoned hold must expire for the next customer. A fix
// that serialises everything or confuses seats shows up as a wrong rejection.
machine GeneratedCheckouts {
  start state Init {
    entry {
      var checkouts: seq[Checkout];
      var i: int;
      while (i < 3) {
        checkouts += (i, (hold = i + 1, seat = choose(2), abandons = $, cancels = false));
        i = i + 1;
      }
      StartCustomers(this, new SeatStore(), checkouts);
    }
    ignore CheckoutDone;
  }
}

// Lifecycle boundary: the seat is sold, and only then does a second customer try to
// hold it. Concurrent customers rarely reach this order within mutation budgets, and a
// store that forgets sold seats re-sells them here.
machine HoldAfterSale {
  var store: SeatStore;
  var payments: Payments;
  start state First {
    entry {
      store = new SeatStore();
      payments = new Payments();
      announce CheckoutsPlanned, 2;
      StartCheckout(this, store, payments, (hold = 1, seat = 7, abandons = false, cancels = false));
    }
    on CheckoutDone goto Second;
  }
  state Second {
    entry {
      StartCheckout(this, store, payments, (hold = 2, seat = 7, abandons = false, cancels = false));
    }
    ignore CheckoutDone;
  }
}

// Stale operations against a successor: customer 1 holds and cancels while customer 2
// wants the same seat. Customer 1's expiry timer still fires, and its cancel can
// arrive after the hold already expired; either can land while customer 2 holds the
// seat. A store that frees by seat alone (not by hold id) takes customer 2's seat.
machine CancelRacesRehold {
  start state Init {
    entry {
      var checkouts: seq[Checkout];
      checkouts += (0, (hold = 1, seat = 7, abandons = false, cancels = true));
      checkouts += (1, (hold = 2, seat = 7, abandons = false, cancels = false));
      StartCustomers(this, new SeatStore(), checkouts);
    }
    ignore CheckoutDone;
  }
}

machine BuggyTwoCustomersOneSeat {
  start state Init {
    entry {
      var checkouts: seq[Checkout];
      checkouts += (0, (hold = 1, seat = 7, abandons = false, cancels = false));
      checkouts += (1, (hold = 2, seat = 7, abandons = false, cancels = false));
      StartCustomers(this, new UnfencedSeatStore(), checkouts);
    }
    ignore CheckoutDone;
  }
}

// Covered: the first customer's hold expired while paying, so their confirmation is
// rejected and they are refunded, and the seat went to someone else.
scenario ExpiredWhilePaying observes Charged, Expired, ConfirmRejected, Refunded {
  start hot state WaitCharge {
    on Charged goto WaitExpiry;
  }
  hot state WaitExpiry {
    on Expired goto WaitRejection;
  }
  hot state WaitRejection {
    on ConfirmRejected goto WaitRefund;
  }
  hot state WaitRefund {
    on Refunded goto Covered;
  }
  cold state Covered {
  }
}

test tcTwoCustomersOneSeat [main=TwoCustomersOneSeat]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { TwoCustomersOneSeat, SeatStore, ExpiryTimer, Payments, Customer };

test tcGeneratedCheckouts [main=GeneratedCheckouts]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { GeneratedCheckouts, SeatStore, ExpiryTimer, Payments, Customer };

test tcCancelRacesRehold [main=CancelRacesRehold]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { CancelRacesRehold, SeatStore, ExpiryTimer, Payments, Customer };

test tcHoldAfterSale [main=HoldAfterSale]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { HoldAfterSale, SeatStore, ExpiryTimer, Payments, Customer };

// snappea: expect-bug "which does not hold it"
test tcBugConfirmIgnoresExpiry [main=BuggyTwoCustomersOneSeat]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { BuggyTwoCustomersOneSeat, UnfencedSeatStore, ExpiryTimer, Payments, Customer };
