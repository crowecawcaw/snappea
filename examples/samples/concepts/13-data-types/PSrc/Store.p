// The store keeps every cart and the shared stock. Adding, removing and checking out are
// one transaction each; checkout either takes every line from stock or none.

event eAdd: (cart: int, line: tLine);
event eRemove: (cart: int, sku: tSku);
event eCheckout: (client: machine, cart: int);
event eReceipt: tReceipt;

machine Store {
  var stock: map[tSku, int];
  // cart id -> cart
  var carts: map[int, tCart];

  start state Open {
    entry (initial: map[tSku, int]) {
      stock = initial;
    }
    on eAdd do (a: (cart: int, line: tLine)) {
      if (!(a.cart in carts)) {
        // default(T) is the empty or zero value of any type.
        carts[a.cart] = default(tCart);
      }
      if (a.line.sku in carts[a.cart]) {
        carts[a.cart][a.line.sku] = carts[a.cart][a.line.sku] + a.line.qty;
      } else {
        carts[a.cart][a.line.sku] = a.line.qty;
      }
    }
    on eRemove do (r: (cart: int, sku: tSku)) {
      // Removing a key from a map: `m -= (key);`
      carts[r.cart] -= (r.sku);
    }
    on eCheckout do (c: (client: machine, cart: int)) {
      var receipt: tReceipt;
      receipt = Checkout(c.cart);
      announce eCheckedOut, receipt;
      send c.client, eReceipt, receipt;
    }
  }

  fun Checkout(cart: int): tReceipt {
    var items: tCart;
    var sku: tSku;
    var lines: seq[tLine];
    var total: int;
    if (!(cart in carts) || sizeof(carts[cart]) == 0) {
      return (cart = cart, result = EmptyCart, total = 0, lines = lines);
    }
    items = carts[cart];
    foreach (sku in keys(items)) {
      if (items[sku] > stock[sku]) {
        return (cart = cart, result = OutOfStock, total = 0, lines = lines);
      }
    }
    foreach (sku in keys(items)) {
      stock[sku] = stock[sku] - items[sku];
      total = total + items[sku] * Price(sku);
      // Appending to a sequence: `s += (index, value);` with index == sizeof(s).
      lines += (sizeof(lines), (sku = sku, qty = items[sku]));
    }
    carts -= (cart);
    return (cart = cart, result = Paid, total = total, lines = lines);
  }
}
