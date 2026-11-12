// A customer places an order (the client picks the order id), then either cancels and
// pays anyway from a second browser tab, or pays and may cancel while the warehouse is
// shipping.
machine Customer {
  start state Shopping {
    entry (service: OrderService) {
      send service, ePlace, (order = 7, amount = 30);
      if ($) {
        Cancel(service);
        Pay(service);
        return;
      }
      Pay(service);
      if ($) { Cancel(service); }
    }
  }
}

// The customer's requests are logged too, so the spec knows what was asked and in
// which order, not only what the service decided.
fun Pay(service: OrderService) {
  announce ePayRequested, 7;
  send service, ePay, 30;
}

fun Cancel(service: OrderService) {
  announce eCancelRequested, 7;
  send service, eCancel;
}

machine OneOrder {
  start state Init {
    entry {
      new Customer(new OrderService(new Warehouse()));
    }
  }
}

test tcCancelRacesShipping [main=OneOrder]:
  assert OrderLifecycle in { OneOrder, Customer, OrderService, Warehouse };
