// The model of the order service: one order, a customer who may cancel at any time and
// a warehouse that ships once the order is paid. Cancel and ship race; the service
// decides each one atomically, so exactly one of them wins after payment.

event Place: (order: int, amount: int);
event Pay: int;
event Cancel;
event ReadyToShip: machine;
event Ship;

machine OrderService {
  var id: int;
  var amount: int;
  var paid: bool;
  var closed: bool;
  var warehouse: machine;

  start state Open {
    entry (w: machine) {
      warehouse = w;
    }
    on Place do (p: (order: int, amount: int)) {
      id = p.order;
      amount = p.amount;
      announce OrderPlaced, (order = p.order, amount = p.amount);
    }
    on Pay do (paidAmount: int) {
      if (closed) { return; }
      paid = true;
      announce OrderPaid, (order = id, amount = paidAmount);
      send warehouse, ReadyToShip, this;
    }
    on Cancel do {
      if (closed) { return; }
      closed = true;
      if (paid) {
        announce OrderCancelled, (order = id, refunded = amount);
      } else {
        announce OrderCancelled, (order = id, refunded = 0);
      }
    }
    on Ship do {
      if (closed) { return; }
      closed = true;
      announce OrderShipped, id;
    }
  }
}

machine Warehouse {
  start state Waiting {
    on ReadyToShip do (service: machine) {
      send service, Ship;
    }
  }
}
