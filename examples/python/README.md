# Python: inventory reservations

Three pytest projects run the same [application](inventory_app/__init__.py) against
the same [P spec](specs/inventory.p). They differ in how tests start recording and
check the spec. See the [shared overview](../README.md#how-the-checks-work) for what
the checks do and why.

## Install and run

From the repository root, with Python 3.9+ and a Rust toolchain (the local `snappea`
package builds the `snappea` binary):

```sh
python -m venv examples/.venv
. examples/.venv/bin/activate
python -m pip install ./python -e './examples/python[test]'

(cd examples/python/explicit && python -m pytest)
(cd examples/python/pytest && python -m pytest)
(cd examples/python/hypothesis && python -m pytest)
```

Run each project from its own directory so pytest uses its configuration. On
Windows, activate with `examples\.venv\Scripts\activate`. The
[Python CI workflow](../../.github/workflows/python.yml) runs all three on Linux and
macOS with Python 3.9 and 3.13.

## 1. Explicit checks in selected tests

Read [explicit/conftest.py](explicit/conftest.py), then
[explicit/test_inventory.py](explicit/test_inventory.py).

`check_spec("InventoryCorrect")` records inside its `with` block and checks when the
block exits. `mode = "off"` in [explicit/pyproject.toml](explicit/pyproject.toml)
turns off automatic checking; `spec_files` paths are relative to that file.

**Gotcha:** the fixture is a factory called *inside* the block. A pre-seeded fixture
would emit initial stock before recording starts.

The last test is the negative control, using `check_events` and `expect_violation`.

## 2. Automatic checks for every pytest test

Read [pytest/pyproject.toml](pytest/pyproject.toml),
[pytest/conftest.py](pytest/conftest.py), and
[pytest/test_inventory.py](pytest/test_inventory.py).

Installing `snappea` registers a pytest plugin. `mode = "auto"` checks every test with a
fresh recorder that also records fixture setup, so a function-scoped fixture can
seed stock. Tests make ordinary calls and assertions; `snappea_events` is optional.

**Gotcha:** the check runs when the test body ends, before fixture teardown, so
finish spec-relevant work in setup or the body. Don't use session-scoped application
state: it outlives each test's recorder.

A broken guard that lets two reservations of 2 through against 3 units fails with:

```text
snappea.SpecViolation: P spec InventoryCorrect was broken: reservation exceeds available stock

  Events (key "coffee"):
      1  eStocked   units=3
      2  eReserved  order=order-1 units=2
      3  eReserved  order=order-2 units=2  <- broke the spec here
```

The full report adds the emit location, the P assertion location, and a folder with
the events and checker output. `--snappea-coverage=coverage/monitor.json` in the pytest
options prints [coverage](../README.md#coverage) (the statements, branches and event
handlers of each spec the suite's events ran, then the P lines none ran) and merges it
into the JSON report; delete the file to start over.

## 3. Generated histories with Hypothesis

Read [hypothesis/test_inventory.py](hypothesis/test_inventory.py). A
`RuleBasedStateMachine` reserves, releases and makes rejected requests against the
real `Inventory`; an invariant checks returned stock against successful operations.

**Gotcha: the recorder belongs to the machine, not to pytest.** One pytest test runs
many histories, including shrink attempts, and Hypothesis builds a new machine for
each. `__init__` starts a `Recorder`, `@initialize` seeds stock inside it, and
`teardown` (which runs even if setup fails) stops and checks it. `SpecViolation` is an
`AssertionError`, so Hypothesis shrinks it. Automatic mode is off here: one recorder
around the pytest test would merge histories. In a real project, reset your service
and stop workers at the same boundary.

CI runs 30 histories of up to 25 steps; raise `max_examples` and
`stateful_step_count` to explore more. `deadline=None` allows for checker startup.
Failures are saved in `.hypothesis/` and replayed on the next run; reproduce a run
with `python -m pytest --hypothesis-seed=12345`.

## Adapt to your project

Application code imports only `emit` and calls it after each committed change, with
the entity ID as `key`. Point `[tool.snappea] spec_files` at your specs, then choose
`check_spec` for selected tests, `mode = "auto"` for every test, or a state machine
that owns its recorder for generated histories. See the
[Python guide](https://snappea.dev/docs/python.html) for marks, hot-state checks,
subprocesses and failure reports.
