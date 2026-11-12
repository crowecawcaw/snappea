// A plausible broken warehouse, kept as a negative control. It picks with a
// conditional transition, then ships with a blind write: it already "knows" the order
// is picked, so it does not repeat the condition. A cancel that lands between picking
// and shipping is overwritten, and a cancelled (and voided) order ships.
// tcBugBlindShip shows the contract catching it.
machine BlindWarehouse {
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
      var everything: set[tStatus];
      if (!r.ok) {
        return;
      }
      if (r.next == Picked) {
        // BUG: an unconditional write (every status matches).
        everything = BeforeShipping();
        everything += (Shipped);
        everything += (Cancelled);
        send store, eTransition, (client = this, order = r.order, allowed = everything, next = Shipped);
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
