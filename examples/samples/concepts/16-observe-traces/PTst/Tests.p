// A customer places an order (the client picks the order id), then either cancels and
// pays anyway from a second browser tab, or pays and may cancel while the warehouse is
// shipping.
machine Customer {
  start state Shopping {
    entry (service: OrderService) {
      send service, Place, (order = 7, amount = 30);
      if ($) {
        SendCancel(service);
        SendPay(service);
        return;
      }
      SendPay(service);
      if ($) { SendCancel(service); }
    }
  }
}

// The customer's requests are logged too, so the spec knows what was asked and in
// which order, not only what the service decided.
fun SendPay(service: OrderService) {
  announce PayRequested, 7;
  send service, Pay, 30;
}

fun SendCancel(service: OrderService) {
  announce CancelRequested, 7;
  send service, Cancel;
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
