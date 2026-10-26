# Priority queue testing calibration

This folder contains a frozen observation suite and deterministic fault controls
for `priority/testing`. It checks that tests exercise actual `Service` calls and
feed their captured results to the independent `QueueContract` monitor.

From the repository root, run:

```sh
SNAPPEA_BIN="$PWD/target/release/snappea" \
PYTHONPATH="$PWD/python" \
python evals/modeling-skill/phases/priority/testing/private/calibration/calibrate.py
```

The calibration expects the two correct implementations to pass and all eight
fault variants to fail through ordinary test assertions. The pinned local
checker used here reports version `0.1.0` and has SHA-256
`98cd2dadfb69ca790af0c0842991d236d65a0fb00c01cce67eccdcb48e175a2e`.

The controls cover signed-priority handling, tie order, reprioritization order,
cancel/reinsert order, duplicate mutation, empty-pop behavior, stale cancelled
work, and accepting reprioritization of a missing ID. The valid alternative
changes only the initial value of the private admission counter. The tests do
not inspect private state. The P monitor uses an admission-history sequence and
its own active, priority, and order maps. The pinned checker build used here
crashes when a `foreach` iterator is declared as a machine field because it
indexes that field in the handler-local frame; a handler-local iterator avoids
that tooling bug. This monitor uses an indexed sequence scan, so the calibration
does not depend on that broken iterator case. This does not indicate a general
`keys(map)` issue. Calibration also runs one valid and one deliberately invalid
history directly through `snappea observe`, independent of the Python suite.
