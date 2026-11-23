---
diataxis: how-to
personas: ["application-developer",  "coding-agent",  "p-user"]
questions:
  - "How do I model a service in P and check its design with snappea check?"
  - "How do I run the model checks in CI?"
---

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
a test name to select both designs. The first proves the atomic design:

```text
ok    tcAtomicReserve (proved, ...)
```

The second finds the oversell within a few schedules, and exits with 1:

```text
FAIL  tcCheckThenReserve (bug found after ...)
      Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
```

[Checking P models](../../docs/checking-models.md#errors-and-output) shows the whole
output of `snappea check` for this model.

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
[check-model.yml](../../.github/workflows/check-model.yml). See [GitHub Actions](https://snappea.dev/docs/github-actions.html).
