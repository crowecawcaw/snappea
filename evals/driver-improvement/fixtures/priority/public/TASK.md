# Improve the priority queue's tests

Review this P model and improve its driver/tests. Keep the contract checks and
algorithm intact. Deliver `driver.p`, `tests.p`, and `REVIEW.md` explaining what
you changed, what faults your scenarios could expose, and remaining limits.
Each test execution covers one service instance; use separate tests for fresh state.

Contract:

- Commands are sequential; wait for `eAck` after each `eCommand`.
- `op=0`: admit a missing item ID at its integer priority; return accepted=true.
  Admission of an already-active ID returns false and has no effect.
- `op=1`: set an active item's priority, preserving its admission position;
  return false with no effect for a missing ID.
- `op=2`: cancel an active ID; return false for a missing ID.
- `op=3`: remove the item with lowest integer priority. Ties use earliest current
  admission. Return -1 when empty. IDs are nonnegative integers; priorities may
  be negative, zero, or positive.
- An ID may be admitted again after cancellation or removal; it receives a new
  admission position. Other operations never change admission position.
- The protected model emits `eObserved`; the protected monitor checks outcomes
  independently. Send `eFinish` after each completed scenario; the monitor also
  requires at least eight commands and completion.

Edit only `driver.p` and `tests.p` (and write `REVIEW.md`). Do not edit the model,
monitor, events, or task; remove/weaken assertions; add suppressions; or bypass
checks. Keep scenarios bounded. You may use multiple explicit tests and bounded
choices when useful. Use only model responses; do not fabricate observations.
