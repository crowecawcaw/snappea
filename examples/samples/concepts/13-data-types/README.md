---
diataxis: tutorial
personas: [application-developer, coding-agent]
questions:
  - Which data types does P have, and how do I use maps, sets and sequences?
  - How do I iterate, test membership, and build default values in P?
  - How do I convert between enums and integers?
---

# Data types: a shopping cart

P's data types are few and value-typed: `int`, `bool`, `string`, enums, tuples, named
tuples, `seq`, `set`, `map`, `machine` and `event`. This sample uses each in a small
cart service: shoppers fill carts and check out against shared stock, and a spec
recomputes every receipt from what the shoppers asked for.

## The concept

| Need | P | Where |
| --- | --- | --- |
| A fixed set of values | `enum tSku { Apple, Pear }` | [`PSrc/Types.p`](PSrc/Types.p) |
| Name a type once | `type tCart = map[tSku, int];` | `Types.p` |
| A record | named tuple `(sku: tSku, qty: int)`; build it `(sku = Apple, qty = 1)` | `Types.p` |
| Empty or zero value | `default(tCart)`, `default(seq[tSku])` | [`PSrc/Store.p`](PSrc/Store.p) |
| Map: put, read, remove, test | `m[k] = v;` `m[k]` `m -= (k);` `k in m` | `Store.p` |
| Sequence: append, index, length | `s += (sizeof(s), v);` `s[i]` `sizeof(s)` | `Store.p` |
| Set: add, test, pick any | `s += (v);` `v in s` `choose(s)` | [`PTst/Tests.p`](PTst/Tests.p) |
| Iterate | `foreach (k in keys(m))`, `foreach (v in values(m))`, `foreach (x in seq)` | `Store.p`, [`PSpec/Contract.p`](PSpec/Contract.p) |
| Enum to integer | `sku to int` | `Types.p` |
| Message text | `format("cart {0} charged {1}", cart, total)` | `Contract.p` |

All values are copied on assignment: `items = carts[cart];` takes a snapshot, and
changing `items` does not change the map. Nested updates work in place:
`carts[c][sku] = carts[c][sku] + qty;`.

## Key lines

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

The spec keeps its own copy of each cart (from `eAdded`/`eRemoved`, announced by the
shoppers) and of the stock, decides the expected `tResult`, and for a payment checks
each receipt line, the total at list price, and that no stock value went negative:

```p
foreach (qty in values(stock)) {
  assert qty >= 0, format("stock went negative: {0}", stock);
}
```

## Test cases

| Test | Drives | Why |
| --- | --- | --- |
| `tcTwoShoppers` | Two shoppers want the last two apples; one changes its mind about a pear; the first clicks pay twice | Contention for stock, adding a sku twice, removal, a paid cart is emptied |
| `tcEmptyCarts` | One shopper never adds; one adds an item (either sku) and removes it | Both kinds of empty cart: missing and emptied |

Both pass: `tcTwoShoppers` explores all schedules (about 8,000) with `--timeout 20s`,
`tcEmptyCarts` all 404 schedules.

```sh
snappea check --timeout 20s --deny mutation
```

## What the checker shows

```text
ok    tcTwoShoppers (all 7968 schedules, 3.66s)
ok    tcEmptyCarts (all 404 schedules, 0.09s)
      Coverage: 3/3 states, 50/79 statements, 10/22 branches, ...
Coverage of all test cases: 3/3 states, 88/88 statements, 22/22 branches, 8/8 event handlers, 26/26 condition outcomes, ...
Mutation testing: 41 of 41 mutants detected (0.2s)
```

Coverage is checked over all tests together: `tcEmptyCarts` alone runs half the code,
and that is fine; it exists for the `EmptyCart` branch.

## Common mistakes

- **Reading a missing map key.** It is a runtime error, not a default. Delete the
  `default(tCart)` initialisation in `Store.eAdd` and the first schedule fails with
  `Exception 'System.Collections.Generic.KeyNotFoundException' was thrown in Store(2)
  (state 'Open', action 'Anon_1'): The given key '1' was not present in the dictionary.`
  Test `k in m` first.
- **`as` versus `to`.** `as` changes the static type (from `any`, or to a supertype);
  `to` converts a value: `sku to int`. `sku as int` fails with `types tSku and int are
  incomparable`. Converting an integer back to an enum is not supported; pick from a set
  instead (`choose(skus)`).
- **Reserved words as names.** `values`, `keys`, `to`, `format`, `default` are keywords;
  `var values: seq[int];` is a syntax error.
- **Appending with the wrong index.** `s += (i, v)` *inserts* at `i`; use `sizeof(s)` to
  append.

## Try this

In `Store.Checkout`, change `items[sku] > stock[sku]` to `items[sku] >= stock[sku]`. A
cart that takes exactly the remaining stock is now refused:

```text
FAIL  tcTwoShoppers (bug found after 34 schedules, 0.02s)
      Assertion Failed: PSpec/Contract.p:43:7 cart 1: checkout returned 1, expected 0
```

`format` prints enum values as their integers: 1 is `OutOfStock`, 0 is `Paid`.

## Bounds and omissions

Two skus, stock of 2 and 1, two shoppers with their own carts. Not modeled: prices that
change, reservations at add time, concurrent edits to one cart, payment failures.

## Testing the implementation

Emit `eAdded`/`eRemoved` from the cart API and `eCheckedOut` with the receipt after the
checkout transaction commits. Run two checkouts that together exceed the stock, and a
checkout that takes exactly the remaining stock.
