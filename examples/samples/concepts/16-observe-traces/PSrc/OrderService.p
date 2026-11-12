// The model of the order service: one order, a customer who may cancel at any time and
// a warehouse that ships once the order is paid. Cancel and ship race; the service
// decides each one atomically, so exactly one of them wins after payment.

event ePlace: (order: int, amount: int);
event ePay: int;
event eCancel;
event eReadyToShip: machine;
event eShip;

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
    on ePlace do (p: (order: int, amount: int)) {
      id = p.order;
      amount = p.amount;
      announce eOrderPlaced, (order = p.order, amount = p.amount);
    }
    on ePay do (paidAmount: int) {
      if (closed) { return; }
      paid = true;
      announce eOrderPaid, (order = id, amount = paidAmount);
      send warehouse, eReadyToShip, this;
    }
    on eCancel do {
      if (closed) { return; }
      closed = true;
      if (paid) {
        announce eOrderCancelled, (order = id, refunded = amount);
      } else {
        announce eOrderCancelled, (order = id, refunded = 0);
      }
    }
    on eShip do {
      if (closed) { return; }
      closed = true;
      announce eOrderShipped, id;
    }
  }
}

machine Warehouse {
  start state Waiting {
    on eReadyToShip do (service: machine) {
      send service, eShip;
    }
  }
}
