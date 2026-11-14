// The cart contract, rebuilt from what shoppers asked for: a checkout pays for exactly
// the cart's lines at list price when every line is in stock, and is refused otherwise;
// a paid cart is emptied; every checkout gets a receipt. The spec keeps its own stock,
// so a store that oversells refuses or charges differently from what the spec expects.

// Announced by the driver: the opening stock and how many checkouts will follow.
event Opened: (stock: map[Sku, int], checkouts: int);
// Announced by a shopper as it sends each request.
event Added: (cart: int, line: Line);
event Removed: (cart: int, sku: Sku);
spec CheckoutsMatchCarts observes Opened, Added, Removed, CheckoutResponse {
  var stock: map[Sku, int];
  var carts: map[int, Cart];
  var receiptsDue: int;

  start state Idle {
    on Opened goto Open with (o: (stock: map[Sku, int], checkouts: int)) {
      assert o.checkouts > 0, "planned no checkouts";
      stock = o.stock;
      receiptsDue = o.checkouts;
    }
    on Added do (a: (cart: int, line: Line)) {
      assert false, format("cart {0}: item added before the store opened", a.cart);
    }
    on Removed do (r: (cart: int, sku: Sku)) {
      assert false, format("cart {0}: item removed before the store opened", r.cart);
    }
    on CheckoutResponse do (r: Receipt) {
      assert false, format("cart {0}: checkout before the store opened", r.cart);
    }
  }

  // Hot until every planned checkout has its receipt.
  hot state Open {
    on Opened do { assert false, "store opened twice"; }
    on Added do (a: (cart: int, line: Line)) {
      if (!(a.cart in carts)) { carts[a.cart] = default(Cart); }
      if (!(a.line.sku in carts[a.cart])) { carts[a.cart][a.line.sku] = 0; }
      carts[a.cart][a.line.sku] = carts[a.cart][a.line.sku] + a.line.qty;
    }
    on Removed do (r: (cart: int, sku: Sku)) {
      carts[r.cart] -= (r.sku);
    }
    on CheckoutResponse do (r: Receipt) {
      var expected: CheckoutOutcome;
      expected = DecideOutcome(r.cart);
      assert r.result == expected, format("cart {0}: checkout returned {1}, expected {2}", r.cart, r.result, expected);
      if (r.result == Paid) { Pay(r); }
      receiptsDue = receiptsDue - 1;
      if (receiptsDue == 0) { goto Closed; }
    }
  }

  state Closed {
    on Opened do { assert false, "store opened after it closed"; }
    on Added do (a: (cart: int, line: Line)) {
      assert false, format("cart {0}: item added after the last checkout", a.cart);
    }
    on Removed do (r: (cart: int, sku: Sku)) {
      assert false, format("cart {0}: item removed after the last checkout", r.cart);
    }
    on CheckoutResponse do (r: Receipt) {
      assert false, format("cart {0}: receipt after the last checkout", r.cart);
    }
  }

  fun DecideOutcome(cart: int): CheckoutOutcome {
    var sku: Sku;
    if (!(cart in carts) || sizeof(carts[cart]) == 0) { return EmptyCart; }
    foreach (sku in keys(carts[cart])) {
      if (carts[cart][sku] > stock[sku]) { return OutOfStock; }
    }
    return Paid;
  }

  fun Pay(r: Receipt) {
    var line: Line;
    var total: int;
    assert sizeof(r.lines) == sizeof(carts[r.cart]),
      format("cart {0} paid for {1} lines, but it has {2}", r.cart, sizeof(r.lines), sizeof(carts[r.cart]));
    foreach (line in r.lines) {
      assert line.sku in carts[r.cart] && line.qty == carts[r.cart][line.sku],
        format("cart {0} paid for {1} x {2}, which is not what it holds", r.cart, line.qty, line.sku);
      stock[line.sku] = stock[line.sku] - line.qty;
      total = total + line.qty * ExpectedPrice(line.sku);
    }
    assert r.total == total, format("cart {0} charged {1}, list price is {2}", r.cart, r.total, total);
    // A paid cart is emptied: checking it out again finds nothing.
    carts -= (r.cart);
  }
  // Independent price oracle: do not reuse the store's table or indexing code.
  fun ExpectedPrice(sku: Sku): int {
    if (sku == Apple) { return 50; }
    return 80;
  }
}
