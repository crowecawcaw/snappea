# Bounded histories: inputs and schedules

This tiny stock example separates three choices that are easy to mix up:

- **Fixed input history:** `FixedHistory` sends purchases `a`, `b`, then `c`, waiting for each result before sending the next. That order and each request's units are fixed.
- **Schedule exploration:** `ScheduledMain` gives three buyers fixed purchase inputs, then Snappea chooses how their sends and the store's handler steps interleave. Request values stay fixed while the schedule varies.
- **Generated input histories:** `GeneratedHistory` makes three sequential choices. At each step it chooses purchase or replenishment with `$`, and quantity 1 or 2 with `choose(2) + 1`. Each action is applied to the current stock produced by earlier history steps; Snappea explores the resulting input sequences and their enabled scheduling steps.

All cases check the same independent `CapacityOracle`. It starts with two units, requires every replenishment to succeed, and derives whether each purchase must be accepted from its own remaining-stock state. It does not read `Store.remaining` or trust the store's decision. The hand-calculated histories below check this monitor directly; they are not evidence from an application.

## Check the models

From this directory, with the repository's `target/release` directory on `PATH`:

```sh
snappea check bounded-history.p tcSchedulerChoosesInterleaving --no-mutation
snappea check bounded-history.p tcFixedInputHistory --no-mutation
snappea check bounded-history.p tcBrokenInputHistory --no-mutation
snappea check bounded-history.p tcGeneratedHistory --no-mutation
snappea check bounded-history.p tcBrokenGeneratedHistory --no-mutation
```

The scheduler, fixed-history, and generated-history checks exit 0. The fixed history's first two purchases consume the stock, so the third must be denied. The broken fixed and generated controls exit 1 with the oracle's purchase/stock diagnostic. The generated control accepts every purchase; generation finds a purchase of 2 when only 1 remains.

Three actions and quantities in `{1, 2}` keep the input tree small while preserving the relevant boundary: purchases can fit or exceed current stock, and replenishment can change what later purchases are legal. Extending the history or quantity range increases histories and schedules; choose the smallest bounds that still exercise the contract cases you need.

## Check hand-calculated histories

The monitor's initial stock is 2. In `valid.jsonl`, two accepted one-unit purchases leave zero, so the third one-unit purchase must be denied. `invalid.jsonl` differs only in that third result; acceptance would overdraw stock.

```sh
snappea observe bounded-history.p valid.jsonl --spec CapacityOracle --strict
snappea observe bounded-history.p invalid.jsonl --spec CapacityOracle --strict
```

The valid trace exits 0; the invalid trace exits 1 and reports that request `c` was accepted with zero remaining. These hand-written traces validate the monitor behavior only. To validate an implementation, record decisions from its real calls and pass those events to `snappea observe`.

## Bounds

The model has initial stock 2, one process-local store, generated histories of exactly three actions, and quantities 1 or 2. The handler's stock check and update are one atomic transition. It omits crashes, persistence, concurrent stores, request retries, non-positive quantities, and unbounded histories. Search covers the stated finite model, not every production execution.
