// The cart contract, rebuilt from what shoppers asked for: a checkout pays for exactly
// the cart's lines at list price when every line is in stock, and is refused otherwise;
// stock never goes negative; a paid cart is emptied; every checkout gets a receipt.

// Announced by the driver: the opening stock and how many checkouts will follow.
event Opened: (stock: map[Sku, int], checkouts: int);
// Announced by a shopper as it sends each request.
event Added: (cart: int, line: Line);
event Removed: (cart: int, sku: Sku);
// Announced by the store when a checkout commits or is refused.
event CheckedOut: Receipt;

spec CartContract observes Opened, Added, Removed, CheckedOut {
  var stock: map[Sku, int];
  var carts: map[int, Cart];
  var receiptsDue: int;

  start state Idle {
    on Opened goto Open with (o: (stock: map[Sku, int], checkouts: int)) {
      assert o.checkouts > 0, "planned no checkouts";
      stock = o.stock;
      receiptsDue = o.checkouts;
    }
    on Added do { assert false, "item added before the store opened"; }
    on Removed do { assert false, "item removed before the store opened"; }
    on CheckedOut do { assert false, "checkout before the store opened"; }
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
    on CheckedOut do (r: Receipt) {
      var expected: Result;
      expected = Expected(r.cart);
      assert r.result == expected, format("cart {0}: checkout returned {1}, expected {2}", r.cart, r.result, expected);
      if (r.result == Paid) { Pay(r); }
      receiptsDue = receiptsDue - 1;
      if (receiptsDue == 0) { goto Closed; }
    }
  }

  state Closed {
    on Opened do { assert false, "store opened after it closed"; }
    on Added do { assert false, "item added after the last checkout"; }
    on Removed do { assert false, "item removed after the last checkout"; }
    on CheckedOut do { assert false, "receipt after the last checkout"; }
  }

  fun Expected(cart: int): Result {
    var sku: Sku;
    if (!(cart in carts) || sizeof(carts[cart]) == 0) { return EmptyCart; }
    foreach (sku in keys(carts[cart])) {
      if (carts[cart][sku] > stock[sku]) { return OutOfStock; }
    }
    return Paid;
  }

  fun Pay(r: Receipt) {
    var line: Line;
    var qty: int;
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
    foreach (qty in values(stock)) {
      assert qty >= 0, format("stock went negative: {0}", stock);
    }
    // A paid cart is emptied: checking it out again finds nothing.
    carts -= (r.cart);
  }
  // Independent price oracle: do not reuse the store's table or indexing code.
  fun ExpectedPrice(sku: Sku): int {
    if (sku == Apple) { return 50; }
    return 80;
  }
}
