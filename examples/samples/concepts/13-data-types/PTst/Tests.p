// A shopper fills its own cart, then checks out `checkouts` times (a second checkout is
// a double click on the pay button).
type Shopping = (store: Store, cart: int, adds: seq[Line], removes: seq[Sku], checkouts: int);

machine Shopper {
  var plan: Shopping;
  var done: int;

  start state Shopping {
    entry (p: Shopping) {
      var line: Line;
      var sku: Sku;
      plan = p;
      foreach (line in plan.adds) {
        announce Added, (cart = plan.cart, line = line);
        send plan.store, Add, (cart = plan.cart, line = line);
      }
      foreach (sku in plan.removes) {
        announce Removed, (cart = plan.cart, sku = sku);
        send plan.store, Remove, (cart = plan.cart, sku = sku);
      }
      send plan.store, Checkout, (client = this, cart = plan.cart);
    }
    on Receipt do {
      done = done + 1;
      if (done < plan.checkouts) {
        send plan.store, Checkout, (client = this, cart = plan.cart);
      }
    }
  }
}

fun OpenStore(checkouts: int): Store {
  var stock: map[Sku, int];
  stock[Apple] = 2;
  stock[Pear] = 1;
  announce Opened, (stock = stock, checkouts = checkouts);
  return new Store(stock);
}

fun Line(sku: Sku, qty: int): Line { return (sku = sku, qty = qty); }

// Two shoppers want the last apples. Whoever checks out first is paid; the other is
// refused, and the stock never goes negative. The first shopper then clicks pay again.
machine TwoShoppers {
  start state Init {
    entry {
      var store: Store;
      var a: seq[Line];
      var b: seq[Line];
      var removes: seq[Sku];
      store = OpenStore(3);
      a += (0, Line(Apple, 1));
      a += (1, Line(Apple, 1));
      a += (2, Line(Pear, 1));
      new Shopper((store = store, cart = 1, adds = a, removes = default(seq[Sku]), checkouts = 2));
      b += (0, Line(Apple, 1));
      b += (1, Line(Pear, 1));
      removes += (0, Pear);
      new Shopper((store = store, cart = 2, adds = b, removes = removes, checkouts = 1));
    }
  }
}

// Empty carts: one shopper never adds anything; one adds an item and removes it again.
machine EmptyCarts {
  start state Init {
    entry {
      var store: Store;
      var adds: seq[Line];
      var removes: seq[Sku];
      var skus: set[Sku];
      store = OpenStore(2);
      new Shopper((store = store, cart = 1, adds = adds, removes = removes, checkouts = 1));
      skus += (Apple);
      skus += (Pear);
      // choose(set) picks any element; the checker tries each.
      adds += (0, Line(choose(skus), 1));
      removes += (0, adds[0].sku);
      new Shopper((store = store, cart = 2, adds = adds, removes = removes, checkouts = 1));
    }
  }
}

test tcTwoShoppers [main=TwoShoppers]:
  assert CartContract in { TwoShoppers, Store, Shopper };

test tcEmptyCarts [main=EmptyCarts]:
  assert CartContract in { EmptyCarts, Store, Shopper };
