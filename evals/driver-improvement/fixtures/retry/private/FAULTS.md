# Contract-derived fault inventory

Selected before inspecting checker driver facts.

1. `retry_reapplies`: an accepted request's retry writes its original value again.
   Immediate retries cannot distinguish it; a different accepted request between
   the original and retry makes the stale side effect observable.
2. `rejected_not_memoized`: rejected compare/set requests are recomputed on retry.
   Reject a request whose expected version is in the future, advance to that
   version with other requests, then retry it. It must remain rejected.
3. `retry_reports_current_version`: a memo hit returns the live version rather
   than its original version. Another successful request between attempts exposes it.

A representation-preserving alternative stores memo values in two separate maps.
It must remain accepted. Mutants preserve the public model protocol.

## Retained-regression control

After initial calibration, add `new_write_wrong_value`: a newly accepted request
writes value+1. The original driver's first success already catches it. This
case measures preservation of existing detection; it is not one of the three
latent faults selected before report inspection. Initial detection is 1/4 and
the strong reference detects 4/4.
