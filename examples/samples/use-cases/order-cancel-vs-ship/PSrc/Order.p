// The order workflow: checkout places the order and authorizes payment, the warehouse
// picks, ships and captures, and the customer's cancel request races with all of it.
// Each step is its own conditional transition on the order row, so anything can run
// between two steps of one actor.

// To whoever placed the order: it exists now and can be cancelled.
event OrderPlaced: (order: int);
event Fulfil: (order: int, customer: machine);
// To the customer, once the parcel left and the payment was captured.
event ShippedNotice: (order: int);
event CancelResp: (req: int, outcome: CancelOutcome);

// Placed -> authorize the card -> Authorized -> hand to the warehouse.
// If the customer cancelled while the authorization was in flight, the transition to
// Authorized fails and checkout voids the hold itself: the cancel saw no hold to void.
machine Checkout {
  var store: OrderStore;
  var payments: Payments;
  var warehouse: machine;
  var owner: machine;
  var order: int;

  start state Placing {
    entry (input: (owner: machine, store: OrderStore, payments: Payments, warehouse: machine, order: int)) {
      owner = input.owner;
      store = input.store;
      payments = input.payments;
      warehouse = input.warehouse;
      order = input.order;
      send store, Create, (client = this, order = order);
    }
    on Created goto Authorizing with {
      send owner, OrderPlaced, (order = order,);
      send payments, Authorize, (client = this, order = order);
    }
  }

  state Authorizing {
    on AuthorizeResp do {
      send store, Transition, (client = this, order = order, allowed = Only(Placed), next = Authorized);
    }
    on TransitionResp do (r: (order: int, next: Status, ok: bool, prior: Status)) {
      if (r.ok) {
        send warehouse, Fulfil, (order = order, customer = owner);
      } else {
        // Cancelled before the hold was recorded on the order.
        send payments, Void, (order = order,);
      }
    }
  }
}

// The cancel API handler, one per request. Cancelling twice is answered the same way
// both times, and only the request that changed the row voids the hold.
machine CancelRequest {
  var owner: machine;
  var payments: Payments;
  var req: int;

  start state Cancelling {
    entry (input: (owner: machine, store: OrderStore, payments: Payments, order: int, req: int)) {
      owner = input.owner;
      payments = input.payments;
      req = input.req;
      send input.store, Transition, (client = this, order = input.order, allowed = BeforeShipping(), next = Cancelled);
    }
    on TransitionResp do (r: (order: int, next: Status, ok: bool, prior: Status)) {
      if (r.ok && r.prior != Placed) {
        // The order carried an authorization: release it.
        send payments, Void, (order = r.order,);
      }
      if (r.ok || r.prior == Cancelled) {
        send owner, CancelResp, (req = req, outcome = CancelAccepted);
        return;
      }
      send owner, CancelResp, (req = req, outcome = CancelTooLate);
    }
  }
}

// Picks, then ships, then captures, as three separate steps. A cancel can land between
// any two. If the ship transition fails the order was cancelled after picking: the
// items go back on the shelf and the warehouse stops. A failed capture is retried with
// the same idempotency key until it succeeds.
machine Warehouse {
  var store: OrderStore;
  var payments: Payments;
  // order -> the customer to tell when it ships
  var customers: map[int, machine];

  start state Working {
    entry (input: (store: OrderStore, payments: Payments)) {
      store = input.store;
      payments = input.payments;
    }
    on Fulfil do (r: (order: int, customer: machine)) {
      customers[r.order] = r.customer;
      send store, Transition, (client = this, order = r.order, allowed = Only(Authorized), next = Picked);
    }
    on TransitionResp do (r: (order: int, next: Status, ok: bool, prior: Status)) {
      if (!r.ok) {
        // Cancelled first: nothing to ship (restock if already picked).
        return;
      }
      if (r.next == Picked) {
        send store, Transition, (client = this, order = r.order, allowed = Only(Picked), next = Shipped);
        return;
      }
      send payments, CapturePayment, (client = this, order = r.order);
    }
    on CaptureResp do (r: (order: int, ok: bool)) {
      if (!r.ok) {
        send payments, CapturePayment, (client = this, order = r.order);
        return;
      }
      send customers[r.order], ShippedNotice, (order = r.order,);
    }
  }
}
