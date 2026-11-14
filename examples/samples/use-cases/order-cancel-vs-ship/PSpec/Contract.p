// The order contract, stated over events the order store and the payment gateway
// announce at their commit points. The specs rebuild each order's status and each
// card hold from that history; they never read the store or trust a reply.

event OrderCreated: (order: int);
event StatusChanged: (order: int, from: Status, next: Status);
event PaymentAuthorized: (order: int);
event Captured: (order: int);
event Voided: (order: int);

// Safety: orders follow the lifecycle Placed -> Authorized -> Picked -> Shipped, and
// can be cancelled only before shipping. Money moves only to match: capture only a
// shipped order, void only a cancelled one, each at most once, never both. A status
// change is refused only when the order is already over (shipped or cancelled): a
// cancel refused while the parcel is still on the shelf breaks the promise.
spec OrderSafety observes OrderCreated, StatusChanged, TransitionResp, PaymentAuthorized, Captured, Voided {
  var status: map[int, Status];
  var authorized: set[int];
  var settled: set[int];

  start state Watching {
    on OrderCreated do (p: (order: int)) {
      assert !(p.order in status), format("order {0} created twice", p.order);
      status[p.order] = Placed;
    }
    on StatusChanged do (p: (order: int, from: Status, next: Status)) {
      assert p.order in status, format("order {0} changed before it was created", p.order);
      assert p.from == status[p.order],
        format("order {0} changed from {1}, but it was {2}", p.order, p.from, status[p.order]);
      assert !(p.from == Cancelled && p.next == Shipped), format("order {0} shipped after it was cancelled", p.order);
      assert !(p.from == Shipped), format("order {0} changed to {1} after it shipped", p.order, p.next);
      assert Allowed(p.from, p.next), format("order {0} moved from {1} to {2}", p.order, p.from, p.next);
      status[p.order] = p.next;
    }
    on TransitionResp do (reply: (order: int, next: Status, ok: bool, prior: Status)) {
      if (reply.ok) {
        assert status[reply.order] == reply.next, "successful transition did not reach its target";
      } else {
        assert reply.prior == status[reply.order], "refusal reported the wrong current status";
        assert !Allowed(reply.prior, reply.next), "a legal transition was refused";
      }
    }
    on PaymentAuthorized do (p: (order: int)) {
      assert p.order in status, format("payment for order {0} authorized before the order existed", p.order);
      assert !(p.order in authorized), format("order {0} authorized twice", p.order);
      authorized += (p.order);
    }
    on Captured do (p: (order: int)) {
      Settle(p.order);
      assert status[p.order] == Shipped, format("order {0} captured while {1}", p.order, status[p.order]);
    }
    on Voided do (p: (order: int)) {
      Settle(p.order);
      assert status[p.order] == Cancelled, format("order {0} voided while {1}", p.order, status[p.order]);
    }
  }

  fun Settle(order: int) {
    assert order in authorized, format("order {0} settled without an authorization", order);
    assert !(order in settled), format("order {0} captured or voided twice", order);
    settled += (order);
  }

  fun Allowed(from: Status, next: Status): bool {
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
// A shipped order is finished once its customer got the shipping notice.
// Every order gets a card hold, even one cancelled before checkout authorized it, so
// an order is finished only once its hold is settled.
enum CancelOutcome { CancelAccepted, CancelTooLate }
event OrdersPlanned: int;
event OrderRequested: (order: int, cancels: int);
event CancelRequested: (req: int, order: int);
event CancelAnswered: (req: int, outcome: CancelOutcome);
// Announced by the customer when the shipping notice arrives.
event ShipNoticed: (order: int);

spec OrdersSettle observes OrdersPlanned, OrderRequested, CancelRequested, OrderCreated,
    StatusChanged, Captured, Voided, CancelAnswered, ShipNoticed {
  var toRequest: int;
  var openCancels: int;
  // order -> cancels the driver still has to send
  var cancelsDue: map[int, int];
  var openOrders: set[int];
  var finished: set[int];
  var settled: set[int];
  var status: map[int, Status];
  // req -> order, until answered
  var asked: map[int, int];

  start state Idle {
    on OrdersPlanned do (n: int) {
      assert n > 0, "planned no orders";
      toRequest = n;
      goto Busy;
    }
    on OrderRequested do { assert false, "order requested without a plan"; }
    on CancelRequested do { assert false, "cancel requested without a plan"; }
    on OrderCreated do { assert false, "order created that nobody requested"; }
    on StatusChanged do { assert false, "order changed while no work is open"; }
    on Captured do { assert false, "capture while no work is open"; }
    on Voided do { assert false, "void while no work is open"; }
    on CancelAnswered do { assert false, "cancel answered while no work is open"; }
    on ShipNoticed do { assert false, "shipping notice while no work is open"; }
  }

  hot state Busy {
    on OrdersPlanned do { assert false, "orders planned twice"; }
    on OrderRequested do (p: (order: int, cancels: int)) {
      assert !(p.order in cancelsDue), format("order {0} requested twice", p.order);
      toRequest = toRequest - 1;
      cancelsDue[p.order] = p.cancels;
      openOrders += (p.order);
      openCancels = openCancels + p.cancels;
    }
    on CancelRequested do (p: (req: int, order: int)) {
      assert p.order in status, format("cancel {0} sent before order {1} was placed", p.req, p.order);
      assert cancelsDue[p.order] > 0, format("unplanned cancel {0} for order {1}", p.req, p.order);
      cancelsDue[p.order] = cancelsDue[p.order] - 1;
      asked[p.req] = p.order;
    }
    on OrderCreated do (p: (order: int)) {
      assert p.order in cancelsDue, format("order {0} created but never requested", p.order);
      status[p.order] = Placed;
    }
    on StatusChanged do (p: (order: int, from: Status, next: Status)) {
      status[p.order] = p.next;
      if (p.next == Cancelled) {
        finished += (p.order);
      }
      Check(p.order);
    }
    on ShipNoticed do (p: (order: int)) {
      assert status[p.order] == Shipped,
        format("customer told order {0} shipped, but it is {1}", p.order, status[p.order]);
      assert !(p.order in finished), format("customer told twice that order {0} shipped", p.order);
      finished += (p.order);
      Check(p.order);
    }
    on Captured do (p: (order: int)) {
      Release(p.order);
    }
    on Voided do (p: (order: int)) {
      Release(p.order);
    }
    on CancelAnswered do (p: (req: int, outcome: CancelOutcome)) {
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
