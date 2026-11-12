// Drivers. Checkout, the warehouse and the cancel handler are the design under test;
// these machines only play the customer: place orders, then send cancels.

type tPlan = (order: int, cancels: int);
type tShop = (store: OrderStore, payments: Payments, warehouse: machine, orders: seq[tPlan]);

// Places every planned order and, as soon as an order exists, sends its cancels, so
// they race with checkout, picking, shipping and capture.
machine Shopper {
  var shop: tShop;
  var cancels: map[int, int];
  var nextReq: int;

  start state Shopping {
    entry (input: tShop) {
      var plan: tPlan;
      shop = input;
      // Announced before anything starts, so OrdersSettle can demand all of it.
      announce eOrdersPlanned, sizeof(shop.orders);
      foreach (plan in shop.orders) {
        announce eOrderRequested, plan;
        cancels[plan.order] = plan.cancels;
        new Checkout((owner = this, store = shop.store, payments = shop.payments,
                      warehouse = shop.warehouse, order = plan.order));
      }
    }
    on eOrderPlaced do (p: (order: int)) {
      while (cancels[p.order] > 0) {
        cancels[p.order] = cancels[p.order] - 1;
        nextReq = nextReq + 1;
        RequestCancel(this, shop.store, shop.payments, p.order, nextReq);
      }
    }
    on eCancelResp do (r: (req: int, outcome: tCancelOutcome)) { Answered(r); }
    on eShippedNotice do (r: (order: int)) { announce eShipNoticed, r; }
  }
}

fun RequestCancel(owner: machine, store: OrderStore, payments: Payments, order: int, req: int) {
  announce eCancelRequested, (req = req, order = order);
  new CancelRequest((owner = owner, store = store, payments = payments, order = order, req = req));
}

// The customer reports the answer it got, so OrdersSettle checks the real reply.
fun Answered(r: (req: int, outcome: tCancelOutcome)) {
  announce eCancelAnswered, r;
}

fun Open(failures: int, orders: seq[tPlan]) {
  var store: OrderStore;
  var payments: Payments;
  store = new OrderStore();
  payments = new Payments(failures);
  new Shopper((store = store, payments = payments,
               warehouse = new Warehouse((store = store, payments = payments)),
               orders = orders));
}

fun OneOrder(cancels: int): seq[tPlan] {
  var orders: seq[tPlan];
  orders += (0, (order = 1, cancels = cancels));
  return orders;
}

// The core race: one cancel against checkout, pick, ship and capture. Every
// interleaving is explored, including the cancel landing between "picked" and
// "shipped", and before checkout recorded the authorization.
machine CancelRacesWarehouse {
  start state Run {
    entry { Open(0, OneOrder(1)); }
  }
}

// The customer double-clicks: two cancels for one order. Both must get the answer
// that matches the order, and the hold is voided once.
machine DuplicateCancel {
  start state Run {
    entry { Open(0, OneOrder(2)); }
  }
}

// Capture fails once, either before the gateway committed or after (reply lost). The
// warehouse retries with the same key; the money is taken exactly once.
machine CaptureRetry {
  start state Run {
    entry { Open(1, OneOrder(0)); }
  }
}

// Two orders, each with zero or one cancel, and one capture failure somewhere: orders
// must not interfere through the shared store, gateway and warehouse.
machine GeneratedOrders {
  start state Run {
    entry {
      var orders: seq[tPlan];
      orders += (0, (order = 1, cancels = choose(2)));
      orders += (1, (order = 2, cancels = choose(2)));
      Open(1, orders);
    }
  }
}

// Lifecycle boundary: the cancel arrives only after the warehouse shipped and captured.
// It must be refused, and must not void the captured payment.
machine CancelAfterShip {
  var store: OrderStore;
  var payments: Payments;

  start state Shipping {
    entry {
      store = new OrderStore();
      payments = new Payments(0);
      announce eOrdersPlanned, 1;
      announce eOrderRequested, (order = 1, cancels = 1);
      new Checkout((owner = this, store = store, payments = payments,
                    warehouse = new Warehouse((store = store, payments = payments)), order = 1));
    }
    ignore eOrderPlaced;
    on eShippedNotice do (p: (order: int)) {
      announce eShipNoticed, p;
      RequestCancel(this, store, payments, p.order, 1);
    }
    on eCancelResp do (r: (req: int, outcome: tCancelOutcome)) { Answered(r); }
  }
}

// The negative control: the race of tcCancelRacesWarehouse, with a blind-writing warehouse.
machine BugBlindShip {
  start state Run {
    entry {
      var store: OrderStore;
      var payments: Payments;
      store = new OrderStore();
      payments = new Payments(0);
      new Shopper((store = store, payments = payments,
                   warehouse = new BlindWarehouse((store = store, payments = payments)),
                   orders = OneOrder(1)));
    }
  }
}

// Covered: the customer cancelled after the warehouse picked, before it shipped.
scenario CancelledWhilePicked observes eStatusChanged {
  start hot state Waiting {
    on eStatusChanged do (p: (order: int, from: tStatus, next: tStatus)) {
      if (p.from == Picked && p.next == Cancelled) { goto Covered; }
    }
  }
  cold state Covered {
    ignore eStatusChanged;
  }
}

test tcCancelRacesWarehouse [main=CancelRacesWarehouse]:
  assert OrderSafety, OrdersSettle in
  { CancelRacesWarehouse, Shopper, OrderStore, Payments, Checkout, Warehouse, CancelRequest };

test tcDuplicateCancel [main=DuplicateCancel]:
  assert OrderSafety, OrdersSettle in
  { DuplicateCancel, Shopper, OrderStore, Payments, Checkout, Warehouse, CancelRequest };

test tcCaptureRetry [main=CaptureRetry]:
  assert OrderSafety, OrdersSettle in
  { CaptureRetry, Shopper, OrderStore, Payments, Checkout, Warehouse, CancelRequest };

test tcCancelAfterShip [main=CancelAfterShip]:
  assert OrderSafety, OrdersSettle in
  { CancelAfterShip, OrderStore, Payments, Checkout, Warehouse, CancelRequest };

test tcGeneratedOrders [main=GeneratedOrders]:
  assert OrderSafety, OrdersSettle in
  { GeneratedOrders, Shopper, OrderStore, Payments, Checkout, Warehouse, CancelRequest };

// expect-bug: shipped after it was cancelled
test tcBugBlindShip [main=BugBlindShip]:
  assert OrderSafety, OrdersSettle in
  { BugBlindShip, Shopper, OrderStore, Payments, Checkout, BlindWarehouse, CancelRequest };
