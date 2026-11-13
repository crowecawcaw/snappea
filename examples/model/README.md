# P model: inventory reservations

A P model of the inventory service that the [language examples](../README.md)
implement, checked against the same [InventoryCorrect spec](PSpec/inventory.p).

* [`PSrc/Inventory.p`](PSrc/Inventory.p): the service for one product, and orders that
  reserve stock and may cancel. The service `announce`s the spec's events after each
  stock change, where the applications call `emit`.
* [`PTst/TestDriver.p`](PTst/TestDriver.p): three orders of 1 or 2 units compete for 3
  units of stock.
* [`PTst/TestScript.p`](PTst/TestScript.p): one test case per design, and the
  `OrdersAnswered` spec: every order gets an answer.

The model has two designs for a reservation:

* **Atomic** (`tcAtomicReserve`): one request checks stock and reserves it. This is
  what the applications do.
* **Check then reserve** (`tcCheckThenReserve`): an order reads the available stock,
  then commits a reservation the service no longer checks. Two orders can both see 3
  units and reserve 2 each.

## Run

From the repository root:

```sh
cargo build -p snappea --bin snappea
cd examples/model
../../target/debug/snappea check tcAtomicReserve --timeout 1m
../../target/debug/snappea check tcCheckThenReserve
```

The first explores every interleaving and data choice and finds no bug: no oversell,
and every order gets an answer. Scoped coverage
comments in the shared model identify the check-then-reserve paths that this test does
not exercise; those paths are exercised by the second test. Run `snappea check` without
a test name to select both designs:

```text
Checking Inventory: 1 test case
ok    tcAtomicReserve (all 27753 schedules, 4 workers, 3.01s)
🫛 The test case passed in 3.02s, peak memory 30 MB. Output: PCheckerOutput/
```

The second finds the oversell within a few schedules, and exits with 1:

```text
Checking Inventory: 1 test case
FAIL  tcCheckThenReserve (bug found after 8 schedules, 8 workers, 0.01s)
      Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
      Replay: snappea replay PCheckerOutput/tcCheckThenReserve/bug.schedule
1 of 1 test case failed in 0.02s, peak memory 10 MB. Output: PCheckerOutput/
```

The log in `PCheckerOutput/tcCheckThenReserve/bug.txt` shows both orders reading
3 units before either commits, and the `snappea replay` command runs the same schedule
again. `snappea check` with no test case checks both, so it exits with 1 too.

The spec is the same file the language examples check. A unit test of the Python
service runs one ordering per test; the model explores every ordering of concurrent
orders, which tests of an implementation do not reach. The
[CI workflow](../../.github/workflows/model-example.yml) runs both test cases and
checks that every example's `specs/inventory.p` links here.

## In CI

[`github-workflow.yml`](github-workflow.yml) is a workflow you can copy: it installs the
released `snappea` with the install script and checks this model on every pull request.
This repository runs the same steps with its own build of `snappea` in
[check-model.yml](../../.github/workflows/check-model.yml). See [GitHub Actions](https://crowecawcaw.github.io/snappea/docs/github-actions.html).
