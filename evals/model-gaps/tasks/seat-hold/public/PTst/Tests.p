// Drivers. Customers are part of the design under test (they run the checkout
// algorithm), so only these setup machines are drivers.

fun StartCustomers(owner: machine, store: machine, checkouts: seq[Checkout]) {
  var payments: Payments;
  var c: Checkout;
  payments = new Payments();
  foreach (c in checkouts) {
    new Customer((owner = owner, store = store, payments = payments, seat = c.seat,
                  hold = c.hold, abandons = c.abandons, cancels = c.cancels));
  }
}

// Two customers want the same seat. Every ordering of hold, payment, expiry and
// confirmation is explored: the second customer can be rejected, or can get the seat
// after the first hold expires.
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

test tcTwoCustomersOneSeat [main=TwoCustomersOneSeat]:
  assert SeatsSoldOnce in { TwoCustomersOneSeat, SeatStore, ExpiryTimer, Payments, Customer };
