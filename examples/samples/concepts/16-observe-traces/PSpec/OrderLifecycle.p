// The lifecycle of ONE order, written so it can check either the model or an
// application's event log. In a log, each order id is a partition key, so
// `snappea observe` gives every order its own instance of this spec.
//
//   placed -> paid -> shipped
//   placed -> cancelled                (nothing to refund)
//   paid   -> cancelled                (refund the full amount)
//
// A cancel requested before any payment was requested must win: the order is never paid.

event OrderPlaced: (order: int, amount: int);
event OrderPaid: (order: int, amount: int);
event OrderShipped: int;
event OrderCancelled: (order: int, refunded: int);
// Requests, logged by the client as it sends them (before the service decides), so
// they can come before OrderPlaced.
event PayRequested: int;
event CancelRequested: int;

spec OrderLifecycle observes OrderPlaced, OrderPaid, OrderShipped, OrderCancelled, PayRequested, CancelRequested {
  // The order this monitor watches: the first event names it.
  var order: int;
  var known: bool;
  var amount: int;
  var payRequested: bool;
  var cancelledBeforePay: bool;

  start state New {
    on PayRequested do (o: int) { NotePayRequest(o); }
    on CancelRequested do (o: int) { NoteCancelRequest(o); }
    on OrderPlaced goto Placed with (p: (order: int, amount: int)) {
      MatchOrder(p.order);
      assert p.amount > 0, format("order {0} placed for {1}", p.order, p.amount);
      amount = p.amount;
    }
    on OrderPaid do (p: (order: int, amount: int)) {
      assert false, format("order {0} paid before it was placed", p.order);
    }
    on OrderShipped do (o: int) {
      assert false, format("order {0} shipped before it was placed", o);
    }
    on OrderCancelled do (c: (order: int, refunded: int)) {
      assert false, format("order {0} cancelled before it was placed", c.order);
    }
  }

  // Hot: an order must end up paid or cancelled.
  hot state Placed {
    on PayRequested do (o: int) { NotePayRequest(o); }
    on CancelRequested do (o: int) { NoteCancelRequest(o); }
    on OrderPlaced do (p: (order: int, amount: int)) {
      assert false, format("order {0} placed twice", p.order);
    }
    on OrderPaid goto Paid with (p: (order: int, amount: int)) {
      MatchOrder(p.order);
      assert payRequested, format("order {0} paid without a payment request", order);
      assert !cancelledBeforePay, format("order {0} paid although it was cancelled before payment was requested", order);
      assert p.amount == amount, format("order {0} paid {1}, but it costs {2}", order, p.amount, amount);
    }
    on OrderShipped do (o: int) { assert false, format("order {0} shipped before it was paid", o); }
    on OrderCancelled goto Closed with (c: (order: int, refunded: int)) {
      MatchOrder(c.order);
      assert c.refunded == 0, format("order {0} refunded {1}, but it was never paid", order, c.refunded);
    }
  }

  // Hot: a paid order must end up shipped or refunded.
  hot state Paid {
    // Once paid, the order of later requests no longer matters.
    ignore PayRequested, CancelRequested;
    on OrderPlaced do (p: (order: int, amount: int)) {
      assert false, format("order {0} placed twice", p.order);
    }
    on OrderPaid do (p: (order: int, amount: int)) {
      assert false, format("order {0} paid twice", p.order);
    }
    on OrderShipped goto Closed with (o: int) { MatchOrder(o); }
    on OrderCancelled goto Closed with (c: (order: int, refunded: int)) {
      MatchOrder(c.order);
      assert c.refunded == amount, format("order {0} refunded {1} of {2}", order, c.refunded, amount);
    }
  }

  state Closed {
    // A closed order ignores later requests (a retried payment is refused).
    ignore PayRequested, CancelRequested;
    on OrderPlaced do (p: (order: int, amount: int)) {
      assert false, format("order {0} placed again after it closed", p.order);
    }
    on OrderPaid do (p: (order: int, amount: int)) {
      assert false, format("order {0} paid after it closed", p.order);
    }
    on OrderShipped do (o: int) { assert false, format("order {0} shipped after it closed", o); }
    on OrderCancelled do (c: (order: int, refunded: int)) {
      assert false, format("order {0} cancelled after it closed", c.order);
    }
  }

  fun NotePayRequest(o: int) {
    MatchOrder(o);
    payRequested = true;
  }

  fun NoteCancelRequest(o: int) {
    MatchOrder(o);
    if (!payRequested) { cancelledBeforePay = true; }
  }

  fun MatchOrder(o: int) {
    if (!known) {
      order = o;
      known = true;
    }
    assert o == order, format("event for order {0} reached the monitor of order {1}", o, order);
  }
}
