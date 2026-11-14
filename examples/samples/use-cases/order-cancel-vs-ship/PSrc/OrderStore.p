// The order table. Every writer (checkout, warehouse, cancel) changes an order only
// through one conditional status transition, the real atomic primitive:
//
//   UPDATE orders SET status = :next WHERE id = :order AND status IN (:allowed)
//
// The reply says whether the row changed and what status it had before, so the
// caller can tell "already cancelled" from "already shipped".

enum Status { Placed, Authorized, Picked, Shipped, Cancelled }

event Create: (client: machine, order: int);
event Created: (order: int);
event Transition: (client: machine, order: int, allowed: set[Status], next: Status);
event TransitionResp: (order: int, next: Status, ok: bool, prior: Status);

machine OrderStore {
  var status: map[int, Status];

  start state Serving {
    on Create do (r: (client: machine, order: int)) {
      status[r.order] = Placed;
      announce OrderCreated, (order = r.order,);
      send r.client, Created, (order = r.order,);
    }
    on Transition do (r: (client: machine, order: int, allowed: set[Status], next: Status)) {
      var prior: Status;
      prior = status[r.order];
      if (prior in r.allowed) {
        status[r.order] = r.next;
        announce StatusChanged, (order = r.order, from = prior, next = r.next);
        send r.client, TransitionResp, (order = r.order, next = r.next, ok = true, prior = prior);
        return;
      }
      send r.client, TransitionResp, (order = r.order, next = r.next, ok = false, prior = prior);
    }
  }
}

// The WHERE clauses each writer uses.
fun Only(s: Status): set[Status] {
  var allowed: set[Status];
  allowed += (s);
  return allowed;
}

// Cancel succeeds only before the parcel ships.
fun BeforeShipping(): set[Status] {
  var allowed: set[Status];
  allowed += (Placed);
  allowed += (Authorized);
  allowed += (Picked);
  return allowed;
}
