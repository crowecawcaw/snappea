// The data model of a small shopping cart service.

// Enums: a fixed set of named values. Each has an integer value (Apple = 0, Pear = 1).
enum Sku { Apple, Pear }
enum CheckoutOutcome { Paid, OutOfStock, EmptyCart }

// Type aliases name a type once. A named tuple has fields; a map has keys and values.
type Line = (sku: Sku, qty: int);
// sku -> quantity
type Cart = map[Sku, int];
type Receipt = (cart: int, result: CheckoutOutcome, total: int, lines: seq[Line]);

// Price in cents. `sku to int` converts the enum to its integer value to index a sequence.
fun Price(sku: Sku): int {
  var table: seq[int];
  table += (0, 50);
  table += (1, 80);
  return table[sku to int];
}
