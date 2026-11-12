// The lifecycle of ONE order, written so it can check either the model or an
// application's event log. In a log, each order id is a partition key, so
// `snappea observe` gives every order its own instance of this spec.
//
//   placed -> paid -> shipped
//   placed -> cancelled                (nothing to refund)
//   paid   -> cancelled                (refund the full amount)
//
// A cancel requested before any payment was requested must win: the order is never paid.

event eOrderPlaced: (order: int, amount: int);
event eOrderPaid: (order: int, amount: int);
event eOrderShipped: int;
event eOrderCancelled: (order: int, refunded: int);
// Requests, logged by the client as it sends them (before the service decides), so
// they can come before eOrderPlaced.
event ePayRequested: int;
event eCancelRequested: int;

spec OrderLifecycle observes eOrderPlaced, eOrderPaid, eOrderShipped, eOrderCancelled, ePayRequested, eCancelRequested {
  // The order this monitor watches: the first event names it.
  var order: int;
  var known: bool;
  var amount: int;
  var payRequested: bool;
  var cancelledBeforePay: bool;

  start state New {
    on ePayRequested do (o: int) { PayRequested(o); }
    on eCancelRequested do (o: int) { CancelRequested(o); }
    on eOrderPlaced goto Placed with (p: (order: int, amount: int)) {
      Same(p.order);
      assert p.amount > 0, format("order {0} placed for {1}", p.order, p.amount);
      amount = p.amount;
    }
    on eOrderPaid do { assert false, "order paid before it was placed"; }
    on eOrderShipped do { assert false, "order shipped before it was placed"; }
    on eOrderCancelled do { assert false, "order cancelled before it was placed"; }
  }

  // Hot: an order must end up paid or cancelled.
  hot state Placed {
    on ePayRequested do (o: int) { PayRequested(o); }
    on eCancelRequested do (o: int) { CancelRequested(o); }
    on eOrderPlaced do { assert false, "order placed twice"; }
    on eOrderPaid goto Paid with (p: (order: int, amount: int)) {
      Same(p.order);
      assert payRequested, format("order {0} paid without a payment request", order);
      assert !cancelledBeforePay, format("order {0} paid although it was cancelled before payment was requested", order);
      assert p.amount == amount, format("order {0} paid {1}, but it costs {2}", order, p.amount, amount);
    }
    on eOrderShipped do { assert false, "order shipped before it was paid"; }
    on eOrderCancelled goto Closed with (c: (order: int, refunded: int)) {
      Same(c.order);
      assert c.refunded == 0, format("order {0} refunded {1}, but it was never paid", order, c.refunded);
    }
  }

  // Hot: a paid order must end up shipped or refunded.
  hot state Paid {
    // Once paid, the order of later requests no longer matters.
    ignore ePayRequested, eCancelRequested;
    on eOrderPlaced do { assert false, "order placed twice"; }
    on eOrderPaid do { assert false, "order paid twice"; }
    on eOrderShipped goto Closed with (o: int) { Same(o); }
    on eOrderCancelled goto Closed with (c: (order: int, refunded: int)) {
      Same(c.order);
      assert c.refunded == amount, format("order {0} refunded {1} of {2}", order, c.refunded, amount);
    }
  }

  state Closed {
    // A closed order ignores later requests (a retried payment is refused).
    ignore ePayRequested, eCancelRequested;
    on eOrderPlaced do { assert false, "order placed again after it closed"; }
    on eOrderPaid do { assert false, "order paid after it closed"; }
    on eOrderShipped do { assert false, "order shipped after it closed"; }
    on eOrderCancelled do { assert false, "order cancelled after it closed"; }
  }

  fun PayRequested(o: int) {
    Same(o);
    payRequested = true;
  }

  fun CancelRequested(o: int) {
    Same(o);
    if (!payRequested) { cancelledBeforePay = true; }
  }

  fun Same(o: int) {
    if (!known) {
      order = o;
      known = true;
    }
    assert o == order, format("event for order {0} reached the monitor of order {1}", o, order);
  }
}
