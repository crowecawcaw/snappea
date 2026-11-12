// The data model of a small shopping cart service.

// Enums: a fixed set of named values. Each has an integer value (Apple = 0, Pear = 1).
enum tSku { Apple, Pear }
enum tResult { Paid, OutOfStock, EmptyCart }

// Type aliases name a type once. A named tuple has fields; a map has keys and values.
type tLine = (sku: tSku, qty: int);
// sku -> quantity
type tCart = map[tSku, int];
type tReceipt = (cart: int, result: tResult, total: int, lines: seq[tLine]);

// Price in cents. `sku to int` converts the enum to its integer value to index a sequence.
fun Price(sku: tSku): int {
  var table: seq[int];
  table += (0, 50);
  table += (1, 80);
  return table[sku to int];
}
