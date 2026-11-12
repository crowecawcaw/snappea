// The order table. Every writer (checkout, warehouse, cancel) changes an order only
// through one conditional status transition, the real atomic primitive:
//
//   UPDATE orders SET status = :next WHERE id = :order AND status IN (:allowed)
//
// The reply says whether the row changed and what status it had before, so the
// caller can tell "already cancelled" from "already shipped".

enum tStatus { Placed, Authorized, Picked, Shipped, Cancelled }

event eCreate: (client: machine, order: int);
event eCreated: (order: int);
event eTransition: (client: machine, order: int, allowed: set[tStatus], next: tStatus);
event eTransitionResp: (order: int, next: tStatus, ok: bool, prior: tStatus);

machine OrderStore {
  var status: map[int, tStatus];

  start state Serving {
    on eCreate do (r: (client: machine, order: int)) {
      status[r.order] = Placed;
      announce eOrderCreated, (order = r.order,);
      send r.client, eCreated, (order = r.order,);
    }
    on eTransition do (r: (client: machine, order: int, allowed: set[tStatus], next: tStatus)) {
      var prior: tStatus;
      prior = status[r.order];
      if (prior in r.allowed) {
        status[r.order] = r.next;
        announce eStatusChanged, (order = r.order, from = prior, next = r.next);
        send r.client, eTransitionResp, (order = r.order, next = r.next, ok = true, prior = prior);
        return;
      }
      announce eTransitionRefused, (order = r.order, next = r.next, prior = prior);
      send r.client, eTransitionResp, (order = r.order, next = r.next, ok = false, prior = prior);
    }
  }
}

// The WHERE clauses each writer uses.
fun Only(s: tStatus): set[tStatus] {
  var allowed: set[tStatus];
  allowed += (s);
  return allowed;
}

// Cancel succeeds only before the parcel ships.
fun BeforeShipping(): set[tStatus] {
  var allowed: set[tStatus];
  allowed += (Placed);
  allowed += (Authorized);
  allowed += (Picked);
  return allowed;
}
