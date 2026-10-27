# Driver-report blind spot in the initial fixture

The weak four-call driver passes required coverage but detects none of the three
contract-derived faults. Its driver report currently says only:

> creates 1 Configuration, never more

This is not a driver-role-classification or sequence-array dataflow failure.
`Main` is the selected entry and explicitly marked driver; `Configuration` is the
implementation. Sends are recorded dynamically in `DriverFacts::record_send`.

Two report filters explain the missing useful facts in the existing source:

- The driver sends only `eSet`, four times. `COUNT_LIMIT = 2` discards counts above
  two, while ordering facts compare different event types. No count or order
  fact remains for this repeated same-API workload.
- `eSet` has `(caller: machine, command: Command)`, and Command is a named tuple.
  `record_values` expands only the top-level tuple and `note` retains scalar
  values. It ignores both caller-machine values and the nested command tuple,
  so the two fixed request IDs, expected versions and values are not reported.

Relevant source: `crates/snappea/src/checker/driver_facts.rs`, `COUNT_LIMIT`,
`record_send`, `record_values`, `note`, and `facts`. This is an existing tooling
limitation, not a reason to infer no restrictions in the driver.

Faults were selected from the contract before this report was inspected; see
FAULTS.md. This fixture is intentionally retained as a report-blind negative
case. No tool changes are applied during the cohort.
