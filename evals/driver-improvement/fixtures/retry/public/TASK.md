# Review the configuration model's tests

Review this passing model and improve its tests. The configuration record starts
at version 0, value 0. `eSet` supplies a request ID, expected version and new value.

- A new request succeeds exactly when its expected version equals the current
  version. Success increments version by one and replaces the value; rejection
  changes neither.
- Every result, including rejection, is durably memoized by request ID. A retry
  with the same ID and identical expected version and value returns its original `(accepted, version)` and
  has no further effect, even after other requests change the configuration.
- Request IDs, expected versions and values are nonnegative integers. Future expected versions
  are valid inputs. Different requests may overlap; operations are atomic.
- The independent monitor checks actual results and the resulting public state.

Each test execution covers one service instance; use separate tests for fresh state.

Edit only `driver.p` and `tests.p`; preserve the model, event protocol, monitor and
its assertions. Do not add suppressions, disable checks, or read model internals.
Keep finite bounds; add meaningful scenario/input choices where useful. Retain a
selected monitor and announce `eFinished` after completed scenarios. Save
`REVIEW.md` explaining what the tests establish and their remaining limits.

Run `snappea check events.p model.p monitor.p driver.p tests.p`.
