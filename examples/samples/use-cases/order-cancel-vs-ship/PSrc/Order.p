// The order workflow: checkout places the order and authorizes payment, the warehouse
// picks, ships and captures, and the customer's cancel request races with all of it.
// Each step is its own conditional transition on the order row, so anything can run
// between two steps of one actor.

// To whoever placed the order: it exists now and can be cancelled.
event eOrderPlaced: (order: int);
event eFulfil: (order: int, customer: machine);
// To the customer, once the parcel left and the payment was captured.
event eShippedNotice: (order: int);
event eCancelResp: (req: int, outcome: tCancelOutcome);

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
      send store, eCreate, (client = this, order = order);
    }
    on eCreated goto Authorizing with {
      send owner, eOrderPlaced, (order = order,);
      send payments, eAuthorize, (client = this, order = order);
    }
  }

  state Authorizing {
    on eAuthorizeResp do {
      send store, eTransition, (client = this, order = order, allowed = Only(Placed), next = Authorized);
    }
    on eTransitionResp do (r: (order: int, next: tStatus, ok: bool, prior: tStatus)) {
      if (r.ok) {
        send warehouse, eFulfil, (order = order, customer = owner);
      } else {
        // Cancelled before the hold was recorded on the order.
        send payments, eVoid, (order = order,);
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
      send input.store, eTransition, (client = this, order = input.order, allowed = BeforeShipping(), next = Cancelled);
    }
    on eTransitionResp do (r: (order: int, next: tStatus, ok: bool, prior: tStatus)) {
      if (r.ok && r.prior != Placed) {
        // The order carried an authorization: release it.
        send payments, eVoid, (order = r.order,);
      }
      if (r.ok || r.prior == Cancelled) {
        send owner, eCancelResp, (req = req, outcome = CancelAccepted);
        return;
      }
      send owner, eCancelResp, (req = req, outcome = CancelTooLate);
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
    on eFulfil do (r: (order: int, customer: machine)) {
      customers[r.order] = r.customer;
      send store, eTransition, (client = this, order = r.order, allowed = Only(Authorized), next = Picked);
    }
    on eTransitionResp do (r: (order: int, next: tStatus, ok: bool, prior: tStatus)) {
      if (!r.ok) {
        // Cancelled first: nothing to ship (restock if already picked).
        return;
      }
      if (r.next == Picked) {
        send store, eTransition, (client = this, order = r.order, allowed = Only(Picked), next = Shipped);
        return;
      }
      send payments, eCapture, (client = this, order = r.order);
    }
    on eCaptureResp do (r: (order: int, ok: bool)) {
      if (!r.ok) {
        send payments, eCapture, (client = this, order = r.order);
        return;
      }
      send customers[r.order], eShippedNotice, (order = r.order,);
    }
  }
}
