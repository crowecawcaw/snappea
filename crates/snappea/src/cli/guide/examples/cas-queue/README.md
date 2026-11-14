---
diataxis: tutorial
personas: [coding-agent, application-developer]
questions:
  - How do I turn a load/CAS model into saved Python race and retry tests?
---

# Model a queue, then test the real calls

```sh
snappea guide example cas-queue --out /tmp
cd /tmp/cas-queue
python -m pip install pysnappea pytest  # distribution pysnappea; import snappea
snappea check Queue.pproj --timeout 30s --deny mutation --json
python -m pytest -q
```

Inspect the model report: `summary.incomplete` and `summary.coverage_gaps` must be
zero, `summary.mutants_undetected` must be zero, and no quality gate may be bypassed.
Exit 0 alone does not establish complete exploration. Increase the budget if needed;
never turn required coverage into a warning to call the model complete.

Read `Contract.p` first: it derives FIFO contents and original retry outcomes from
inputs, independently of the candidate algorithm. `Queue.p` separates atomic loads
and compare-and-swap from client decisions. `Queue.pproj` selects both files, so
plain `snappea check` uses the same contract. The contract is selected by a model
test; no observation-only-spec lint suppression is needed.

`queue_store.py` implements the same whole-record CAS algorithm. `store.py` supplies
atomic load and compare-and-swap with private snapshot copies. `test_queue.py` has
saved deterministic regressions, not a one-off observation command:

- Two callers read one version before either writes; assert the barrier fired and
  a CAS actually lost. The hooks match operations, never private storage-key names.
- A competitor fills the queue between load and CAS; the loser must recompute.
- Retry accepted, rejected and empty outcomes after another operation changes state,
  recreating the Queue object on each call. Inspect state before another retry can
  compensate for a duplicate effect.
- Encode real optional results as `(present, value)`, keeping `None` distinct from 0.
- Mutate the real implementation to forget rejected puts; the shared P monitor must
  reject its observed calls with `SpecViolation`.

The sequential observation tests report actual returns and reads in
a known order. Thread return order is not commit order. Concurrent races therefore
use ordinary assertions; neither the implementation nor its callbacks gain a lock
just to make an event log convenient.

The model bounds are two clients, two stable IDs, capacity 1 or 2, up to two initial
items, two item values, and one delayed retry per client. Initial calls race; delayed
retries run sequentially. The real implementation supports larger positive capacities.
No crashes, storage loss, request expiry or reclamation of the durable outcome memo
are modeled. The Python memo grows without bound; production retention is a separate design. Application tests check selected real histories, not all executions or
full equivalence with the model. See `blob-publication` for multi-write crash recovery.
