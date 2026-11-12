// The cart contract, rebuilt from what shoppers asked for: a checkout pays for exactly
// the cart's lines at list price when every line is in stock, and is refused otherwise;
// stock never goes negative; a paid cart is emptied; every checkout gets a receipt.

// Announced by the driver: the opening stock and how many checkouts will follow.
event eOpened: (stock: map[tSku, int], checkouts: int);
// Announced by a shopper as it sends each request.
event eAdded: (cart: int, line: tLine);
event eRemoved: (cart: int, sku: tSku);
// Announced by the store when a checkout commits or is refused.
event eCheckedOut: tReceipt;

spec CartContract observes eOpened, eAdded, eRemoved, eCheckedOut {
  var stock: map[tSku, int];
  var carts: map[int, tCart];
  var receiptsDue: int;

  start state Idle {
    on eOpened goto Open with (o: (stock: map[tSku, int], checkouts: int)) {
      assert o.checkouts > 0, "planned no checkouts";
      stock = o.stock;
      receiptsDue = o.checkouts;
    }
    on eAdded do { assert false, "item added before the store opened"; }
    on eRemoved do { assert false, "item removed before the store opened"; }
    on eCheckedOut do { assert false, "checkout before the store opened"; }
  }

  // Hot until every planned checkout has its receipt.
  hot state Open {
    on eOpened do { assert false, "store opened twice"; }
    on eAdded do (a: (cart: int, line: tLine)) {
      if (!(a.cart in carts)) { carts[a.cart] = default(tCart); }
      if (!(a.line.sku in carts[a.cart])) { carts[a.cart][a.line.sku] = 0; }
      carts[a.cart][a.line.sku] = carts[a.cart][a.line.sku] + a.line.qty;
    }
    on eRemoved do (r: (cart: int, sku: tSku)) {
      carts[r.cart] -= (r.sku);
    }
    on eCheckedOut do (r: tReceipt) {
      var expected: tResult;
      expected = Expected(r.cart);
      assert r.result == expected, format("cart {0}: checkout returned {1}, expected {2}", r.cart, r.result, expected);
      if (r.result == Paid) { Pay(r); }
      receiptsDue = receiptsDue - 1;
      if (receiptsDue == 0) { goto Closed; }
    }
  }

  state Closed {
    on eOpened do { assert false, "store opened after it closed"; }
    on eAdded do { assert false, "item added after the last checkout"; }
    on eRemoved do { assert false, "item removed after the last checkout"; }
    on eCheckedOut do { assert false, "receipt after the last checkout"; }
  }

  fun Expected(cart: int): tResult {
    var sku: tSku;
    if (!(cart in carts) || sizeof(carts[cart]) == 0) { return EmptyCart; }
    foreach (sku in keys(carts[cart])) {
      if (carts[cart][sku] > stock[sku]) { return OutOfStock; }
    }
    return Paid;
  }

  fun Pay(r: tReceipt) {
    var line: tLine;
    var qty: int;
    var total: int;
    assert sizeof(r.lines) == sizeof(carts[r.cart]),
      format("cart {0} paid for {1} lines, but it has {2}", r.cart, sizeof(r.lines), sizeof(carts[r.cart]));
    foreach (line in r.lines) {
      assert line.sku in carts[r.cart] && line.qty == carts[r.cart][line.sku],
        format("cart {0} paid for {1} x {2}, which is not what it holds", r.cart, line.qty, line.sku);
      stock[line.sku] = stock[line.sku] - line.qty;
      total = total + line.qty * Price(line.sku);
    }
    assert r.total == total, format("cart {0} charged {1}, list price is {2}", r.cart, r.total, total);
    foreach (qty in values(stock)) {
      assert qty >= 0, format("stock went negative: {0}", stock);
    }
    // A paid cart is emptied: checking it out again finds nothing.
    carts -= (r.cart);
  }
}
