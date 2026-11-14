---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - Which data types does P have, and how do I use maps, sets and sequences?
  - How do I iterate, test membership, and build default values in P?
  - How do I convert between enums and integers?
---

# Data types: two shoppers want the last apples

P's data types are few and value-typed: `int`, `bool`, `string`, enums, tuples, named
tuples, `seq`, `set`, `map`, `machine` and `event`. This sample uses each in a small
cart service: shoppers fill their own carts and check out against shared stock, so two
checkouts can compete for the last items, and a spec recomputes every receipt from
what the shoppers asked for.

**Contract:**

- A checkout is paid when every line of the cart is in stock, refused as `OutOfStock`
  otherwise, and `EmptyCart` when the cart is missing or empty
  (`CheckoutsMatchCarts`).
- A paid receipt lists exactly the cart's lines and charges their list price
  (`CheckoutsMatchCarts`).
- A paid cart is emptied: checking it out again finds nothing (`CheckoutsMatchCarts`).
- Every checkout gets a receipt (`CheckoutsMatchCarts` stays hot until then).

The spec keeps its own stock, rebuilt from the opening stock and the paid receipts, so
a store that oversells refuses or charges differently from what the spec expects.

**Atomic primitives:** each store handler is one transaction: add a line, remove a
line, or check out (check every line, then take every line, or take none).

## Files

| File | What it holds |
| --- | --- |
| [`PSrc/Types.p`](PSrc/Types.p) | Enums, type aliases and the price table |
| [`PSrc/Store.p`](PSrc/Store.p) | The store: carts, stock, and the all-or-nothing checkout |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `CheckoutsMatchCarts`, with an independent price oracle |
| [`PTst/Tests.p`](PTst/Tests.p) | The `Shopper` driver and two test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoShoppers` | Two shoppers want the last two apples; one changes its mind about a pear; the first clicks pay twice | Contention for stock, adding a sku twice, removal, a paid cart is emptied | Passes |
| `tcEmptyCarts` | One shopper never adds; one adds an item (either sku) and removes it | Both kinds of empty cart: missing and emptied | Passes |

## The concept

| Need | P | Where |
| --- | --- | --- |
| A fixed set of values | `enum Sku { Apple, Pear }` | [`PSrc/Types.p`](PSrc/Types.p) |
| Name a type once | `type Cart = map[Sku, int];` | `Types.p` |
| A record | named tuple `(sku: Sku, qty: int)`; build it `(sku = Apple, qty = 1)` | `Types.p` |
| Empty or zero value | `default(Cart)`, `default(seq[Sku])` | [`PSrc/Store.p`](PSrc/Store.p) |
| Map: put, read, remove, test | `m[k] = v;` `m[k]` `m -= (k);` `k in m` | `Store.p` |
| Sequence: append, index, length | `s += (sizeof(s), v);` `s[i]` `sizeof(s)` | `Store.p` |
| Set: add, test, pick any | `s += (v);` `v in s` `choose(s)` | [`PTst/Tests.p`](PTst/Tests.p) |
| Iterate | `foreach (k in keys(m))`, `foreach (x in seq)`; `values(m)` for a map's values | `Store.p`, [`PSpec/Contract.p`](PSpec/Contract.p) |
| Enum to integer | `sku to int` | `Types.p` |
| Message text | `format("cart {0} charged {1}", cart, total)` | `Contract.p` |

All values are copied on assignment: `items = carts[cart];` takes a snapshot, and
changing `items` does not change the map. Nested updates work in place:
`carts[c][sku] = carts[c][sku] + qty;`.

Give each name one meaning. The type is `Receipt`, the reply event that carries it is
`CheckoutResponse`, and the helper that builds a line is `MakeLine`: a type, an event
and a function never share a name.

## The key lines

Checkout is all-or-nothing: check every line, then take every line.

```p
foreach (sku in keys(items)) {
  if (items[sku] > stock[sku]) { return (cart = cart, result = OutOfStock, total = 0, lines = lines); }
}
foreach (sku in keys(items)) {
  stock[sku] = stock[sku] - items[sku];
  total = total + items[sku] * Price(sku);
  lines += (sizeof(lines), (sku = sku, qty = items[sku]));
}
carts -= (cart);
```

The spec observes the store's reply, `CheckoutResponse`, which carries the receipt.
It keeps its own copy of each cart (from `Added` and `Removed`, announced by the
shoppers) and of the stock, decides the expected outcome, and for a payment checks
each receipt line and the total at list price. It computes prices with its own
`ExpectedPrice`, not the store's indexed table: sharing that code would let the same
price bug change both the result and its oracle.

## Common mistakes

- **Reading a missing map key.** It is a runtime error, not a default. Delete the
  `default(Cart)` initialisation in `Store.Add` and the check fails with
  `error: map 'carts' has no key 1 (in Store(2), state 'Open')`. Test `k in m` first.
- **`as` versus `to`.** `as` changes the static type (from `any`, or to a supertype);
  `to` converts a value: `sku to int`. `sku as int` fails with `types Sku and int are
  incomparable`. Converting an integer back to an enum is not supported; pick from a set
  instead (`choose(skus)`).
- **Reserved words as names.** `values`, `keys`, `to`, `format`, `default` are keywords;
  `var values: seq[int];` is a syntax error.
- **Appending with the wrong index.** `s += (i, v)` *inserts* at `i`; use `sizeof(s)` to
  append.

## Try this

In `Store.ProcessCheckout`, change `items[sku] > stock[sku]` to `items[sku] >= stock[sku]`.
A cart that takes exactly the remaining stock is now refused:

```text
Assertion Failed: PSpec/Contract.p:47:7 cart 1: checkout returned OutOfStock, expected Paid
```

## Run

```sh
cd examples/samples/concepts/13-data-types
snappea check --deny mutation
```

Coverage is checked over both tests together: `tcEmptyCarts` alone runs about half the
code, and that is fine; it exists for the `EmptyCart` branch.

## Bounds and omissions

Two skus, stock of 2 and 1, two shoppers with their own carts. Not modeled: prices that
change, reservations at add time, concurrent edits to one cart, payment failures.

## Testing the implementation

Emit `Added` and `Removed` from the cart API and `CheckoutResponse` with the receipt
where the checkout transaction replies. Run two checkouts that together exceed the
stock, and a checkout that takes exactly the remaining stock.
