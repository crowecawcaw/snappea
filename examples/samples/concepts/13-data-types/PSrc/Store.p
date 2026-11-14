// The store keeps every cart and the shared stock. Adding, removing and checking out are
// one transaction each; checkout either takes every line from stock or none.

event Add: (cart: int, line: Line);
event Remove: (cart: int, sku: Sku);
event Checkout: (client: machine, cart: int);
event Receipt: Receipt;

machine Store {
  var stock: map[Sku, int];
  // cart id -> cart
  var carts: map[int, Cart];

  start state Open {
    entry (initial: map[Sku, int]) {
      stock = initial;
    }
    on Add do (a: (cart: int, line: Line)) {
      if (!(a.cart in carts)) {
        // default(T) is the empty or zero value of any type.
        carts[a.cart] = default(Cart);
      }
      if (a.line.sku in carts[a.cart]) {
        carts[a.cart][a.line.sku] = carts[a.cart][a.line.sku] + a.line.qty;
      } else {
        carts[a.cart][a.line.sku] = a.line.qty;
      }
    }
    on Remove do (r: (cart: int, sku: Sku)) {
      // Removing a key from a map: `m -= (key);`
      carts[r.cart] -= (r.sku);
    }
    on Checkout do (c: (client: machine, cart: int)) {
      var receipt: Receipt;
      receipt = Checkout(c.cart);
      announce CheckedOut, receipt;
      send c.client, Receipt, receipt;
    }
  }

  fun Checkout(cart: int): Receipt {
    var items: Cart;
    var sku: Sku;
    var lines: seq[Line];
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
