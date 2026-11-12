// Drivers. Customers are part of the design under test (they run the checkout
// algorithm), so only these setup machines are drivers.

fun StartCustomers(owner: machine, store: machine, checkouts: seq[tCheckout]) {
  var payments: Payments;
  var c: tCheckout;
  payments = new Payments();
  announce eCheckoutsPlanned, sizeof(checkouts);
  foreach (c in checkouts) {
    StartCheckout(owner, store, payments, c);
  }
}

fun StartCheckout(owner: machine, store: machine, payments: Payments, c: tCheckout) {
  // The driver announces each checkout it starts, so CheckoutsEnd can demand that it
  // ends with the outcome the store's decisions justify.
  announce eCheckoutStarted, c;
  new Customer((owner = owner, store = store, payments = payments, seat = c.seat,
                hold = c.hold, abandons = c.abandons));
}

// Two customers want the same seat. Every ordering of hold, payment, expiry and
// confirmation is explored: the second customer can be rejected, or can get the seat
// after the first hold expires, in which case the first must be refunded.
machine TwoCustomersOneSeat {
  start state Init {
    entry {
      var checkouts: seq[tCheckout];
      checkouts += (0, (hold = 1, seat = 7, abandons = false));
      checkouts += (1, (hold = 2, seat = 7, abandons = false));
      StartCustomers(this, new SeatStore(), checkouts);
    }
    ignore eCheckoutDone;
  }
}

// Three customers pick among two seats, and each may walk away after holding: some
// contend, some do not, and an abandoned hold must expire for the next customer. A fix
// that serialises everything or confuses seats shows up as a wrong rejection.
machine GeneratedCheckouts {
  start state Init {
    entry {
      var checkouts: seq[tCheckout];
      var i: int;
      while (i < 3) {
        checkouts += (i, (hold = i + 1, seat = choose(2), abandons = $));
        i = i + 1;
      }
      StartCustomers(this, new SeatStore(), checkouts);
    }
    ignore eCheckoutDone;
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
      announce eCheckoutsPlanned, 2;
      StartCheckout(this, store, payments, (hold = 1, seat = 7, abandons = false));
    }
    on eCheckoutDone goto Second;
  }
  state Second {
    entry {
      StartCheckout(this, store, payments, (hold = 2, seat = 7, abandons = false));
    }
    ignore eCheckoutDone;
  }
}

machine BuggyTwoCustomersOneSeat {
  start state Init {
    entry {
      var checkouts: seq[tCheckout];
      checkouts += (0, (hold = 1, seat = 7, abandons = false));
      checkouts += (1, (hold = 2, seat = 7, abandons = false));
      StartCustomers(this, new UnfencedSeatStore(), checkouts);
    }
    ignore eCheckoutDone;
  }
}

// Covered: the first customer's hold expired while paying, so their confirmation is
// rejected and they are refunded, and the seat went to someone else.
scenario ExpiredWhilePaying observes eCharged, eExpired, eConfirmRejected, eRefunded {
  start hot state WaitCharge {
    on eCharged goto WaitExpiry;
    ignore eExpired, eConfirmRejected, eRefunded;
  }
  hot state WaitExpiry {
    on eExpired goto WaitRejection;
    ignore eCharged, eConfirmRejected, eRefunded;
  }
  hot state WaitRejection {
    on eConfirmRejected goto WaitRefund;
    ignore eCharged, eExpired, eRefunded;
  }
  hot state WaitRefund {
    on eRefunded goto Covered;
    ignore eCharged, eExpired, eConfirmRejected;
  }
  cold state Covered {
    ignore eCharged, eExpired, eConfirmRejected, eRefunded;
  }
}

test tcTwoCustomersOneSeat [main=TwoCustomersOneSeat]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { TwoCustomersOneSeat, SeatStore, ExpiryTimer, Payments, Customer };

test tcGeneratedCheckouts [main=GeneratedCheckouts]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { GeneratedCheckouts, SeatStore, ExpiryTimer, Payments, Customer };

test tcHoldAfterSale [main=HoldAfterSale]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { HoldAfterSale, SeatStore, ExpiryTimer, Payments, Customer };

// expect-bug: which does not hold it
test tcBugConfirmIgnoresExpiry [main=BuggyTwoCustomersOneSeat]:
  assert SeatsSoldOnce, ChargesResolved, CheckoutsEnd in
  { BuggyTwoCustomersOneSeat, UnfencedSeatStore, ExpiryTimer, Payments, Customer };
