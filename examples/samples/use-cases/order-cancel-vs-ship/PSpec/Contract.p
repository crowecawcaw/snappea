// The order contract, stated over events the order store and the payment gateway
// announce at their commit points. The specs rebuild each order's status and each
// card hold from that history; they never read the store or trust a reply.

event eOrderCreated: (order: int);
event eStatusChanged: (order: int, from: tStatus, next: tStatus);
event eAuthorized: (order: int);
event eCaptured: (order: int);
event eVoided: (order: int);

// Safety: orders follow the lifecycle Placed -> Authorized -> Picked -> Shipped, and
// can be cancelled only before shipping. Money moves only to match: capture only a
// shipped order, void only a cancelled one, each at most once, never both.
spec OrderSafety observes eOrderCreated, eStatusChanged, eAuthorized, eCaptured, eVoided {
  var status: map[int, tStatus];
  var authorized: set[int];
  var settled: set[int];

  start state Watching {
    on eOrderCreated do (p: (order: int)) {
      assert !(p.order in status), format("order {0} created twice", p.order);
      status[p.order] = Placed;
    }
    on eStatusChanged do (p: (order: int, from: tStatus, next: tStatus)) {
      assert p.order in status, format("order {0} changed before it was created", p.order);
      assert p.from == status[p.order],
        format("order {0} changed from {1}, but it was {2}", p.order, p.from, status[p.order]);
      assert !(p.from == Cancelled && p.next == Shipped), format("order {0} shipped after it was cancelled", p.order);
      assert !(p.from == Shipped), format("order {0} changed to {1} after it shipped", p.order, p.next);
      assert Allowed(p.from, p.next), format("order {0} moved from {1} to {2}", p.order, p.from, p.next);
      status[p.order] = p.next;
    }
    on eAuthorized do (p: (order: int)) {
      assert p.order in status, format("payment for order {0} authorized before the order existed", p.order);
      assert !(p.order in authorized), format("order {0} authorized twice", p.order);
      authorized += (p.order);
    }
    on eCaptured do (p: (order: int)) {
      Settle(p.order);
      assert status[p.order] == Shipped, format("order {0} captured while {1}", p.order, status[p.order]);
    }
    on eVoided do (p: (order: int)) {
      Settle(p.order);
      assert status[p.order] == Cancelled, format("order {0} voided while {1}", p.order, status[p.order]);
    }
  }

  fun Settle(order: int) {
    assert order in authorized, format("order {0} settled without an authorization", order);
    assert !(order in settled), format("order {0} captured or voided twice", order);
    settled += (order);
  }

  fun Allowed(from: tStatus, next: tStatus): bool {
    if (next == Cancelled) {
      return from == Placed || from == Authorized || from == Picked;
    }
    return (from == Placed && next == Authorized) || (from == Authorized && next == Picked)
      || (from == Picked && next == Shipped);
  }
}

// Progress and answers. The driver announces how many orders it places and how many
// cancels it will send for each, before it starts; the spec stays hot until every
// order is shipped or cancelled, every card hold is captured or voided (no dangling
// holds), and every cancel is answered with what actually happened to the order.
// Every order gets a card hold, even one cancelled before checkout authorized it, so
// an order is finished only once its hold is settled.
enum tCancelOutcome { CancelAccepted, CancelTooLate }
event eOrdersPlanned: int;
event eOrderRequested: (order: int, cancels: int);
event eCancelRequested: (req: int, order: int);
event eCancelAnswered: (req: int, outcome: tCancelOutcome);

spec OrdersSettle observes eOrdersPlanned, eOrderRequested, eCancelRequested, eOrderCreated,
    eStatusChanged, eCaptured, eVoided, eCancelAnswered {
  var toRequest: int;
  var openCancels: int;
  // order -> cancels the driver still has to send
  var cancelsDue: map[int, int];
  var openOrders: set[int];
  var finished: set[int];
  var settled: set[int];
  var status: map[int, tStatus];
  // req -> order, until answered
  var asked: map[int, int];

  start state Idle {
    on eOrdersPlanned do (n: int) {
      assert n > 0, "planned no orders";
      toRequest = n;
      goto Busy;
    }
    on eOrderRequested do { assert false, "order requested without a plan"; }
    on eCancelRequested do { assert false, "cancel requested without a plan"; }
    on eOrderCreated do { assert false, "order created that nobody requested"; }
    on eStatusChanged do { assert false, "order changed while no work is open"; }
    on eCaptured do { assert false, "capture while no work is open"; }
    on eVoided do { assert false, "void while no work is open"; }
    on eCancelAnswered do { assert false, "cancel answered while no work is open"; }
  }

  hot state Busy {
    on eOrdersPlanned do { assert false, "orders planned twice"; }
    on eOrderRequested do (p: (order: int, cancels: int)) {
      assert !(p.order in cancelsDue), format("order {0} requested twice", p.order);
      toRequest = toRequest - 1;
      cancelsDue[p.order] = p.cancels;
      openOrders += (p.order);
      openCancels = openCancels + p.cancels;
    }
    on eCancelRequested do (p: (req: int, order: int)) {
      assert p.order in status, format("cancel {0} sent before order {1} was placed", p.req, p.order);
      assert cancelsDue[p.order] > 0, format("unplanned cancel {0} for order {1}", p.req, p.order);
      cancelsDue[p.order] = cancelsDue[p.order] - 1;
      asked[p.req] = p.order;
    }
    on eOrderCreated do (p: (order: int)) {
      assert p.order in cancelsDue, format("order {0} created but never requested", p.order);
      status[p.order] = Placed;
    }
    // snappea-lint: allow unused-payload-field -- OrderSafety checks the transition; this spec needs the new status
    on eStatusChanged do (p: (order: int, from: tStatus, next: tStatus)) {
      status[p.order] = p.next;
      if (p.next == Shipped || p.next == Cancelled) {
        finished += (p.order);
      }
      Check(p.order);
    }
    on eCaptured do (p: (order: int)) {
      Release(p.order);
    }
    on eVoided do (p: (order: int)) {
      Release(p.order);
    }
    on eCancelAnswered do (p: (req: int, outcome: tCancelOutcome)) {
      var order: int;
      assert p.req in asked, format("cancel {0} answered twice or never asked", p.req);
      order = asked[p.req];
      if (p.outcome == CancelAccepted) {
        assert status[order] == Cancelled,
          format("cancel {0} told the customer order {1} is cancelled, but it is {2}", p.req, order, status[order]);
      } else {
        assert status[order] == Shipped,
          format("cancel {0} told the customer order {1} already shipped, but it is {2}", p.req, order, status[order]);
      }
      asked -= (p.req);
      openCancels = openCancels - 1;
      Check(order);
    }
  }

  fun Release(order: int) {
    settled += (order);
    Check(order);
  }

  fun Check(order: int) {
    if (order in finished && order in settled) {
      openOrders -= (order);
    }
    if (toRequest == 0 && openCancels == 0 && sizeof(openOrders) == 0) {
      goto Idle;
    }
  }
}
