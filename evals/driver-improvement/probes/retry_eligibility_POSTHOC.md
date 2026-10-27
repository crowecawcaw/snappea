# POSTHOC: retry crosses its future-version eligibility boundary

This is a source-informed exploratory probe, selected **after** the primary
inventory, runs, and scores were frozen. It does not change any primary score.
The frozen Luna drivers prompted the question: does detecting rejected-request
recomputation establish that a retry was tested after its original future
expected version became current?

The original `rejected_not_memoized` fault can be detected for a weaker reason:
it recomputes a rejection and returns the newer live version even while the
expected version still does not match. That is a real contract violation, but
finding it does not establish eligibility-boundary coverage.

The additional fault in `retry_eligibility.py` returns the original memo verbatim
unless a previously rejected request's expected version equals the live version.
At that point it incorrectly reconsiders and applies the request. Accepted request
memos remain respected, and there is no incidental changed-version result before
that eligibility point.

The standalone witness uses the archived strong driver: accept A at version 0,
reject R expecting version 2 while current version is 1, accept B expecting 1 to
reach version 2, then retry R unchanged. R must still return its original rejected
outcome and leave B's state intact.

Run from the repository root:

```sh
python evals/driver-improvement/probes/retry_eligibility.py
```

The probe verifies all four original frozen submission/input/harness manifests,
uses each archived public monitor and exact candidate driver/tests, checks both
correct representations, and runs the unchanged archived common checker settings
(two-second bound, mutation off, coverage gate bypass, driver display off).
Assertion failures only receive detection credit when both correct variants are
accepted and static instruction checks pass. Incomplete searches remain labeled;
acceptance is bounded evidence, not a proof.

Full outputs: `runs/probes-retry-eligibility/results.json` (ignored).
Tracked bounded results and hashes: `retry_eligibility_POSTHOC_summary.json`.
No frozen source, original variant inventory, primary grading output, or checker
implementation is modified.

## Bounded exploratory results

| Frozen retry driver | Reports | Correct + alternative accepted | Additional fault |
|---|---|---|---|
| Sol `a01` | Off | Yes; searches incomplete | Detected by fixed monitor |
| Sol `b01` | On | Yes; searches incomplete | Detected by fixed monitor |
| Luna `a01` | On | Yes; complete | Survives complete search |
| Luna `b01` | Off | Yes; complete | Survives complete search |

The original weak driver accepts this additional fault; the archived strong
reference rejects it through the fixed monitor while accepting both correct
representations. All four candidate input/submission manifests remain intact.
The moved test declaration in Luna `a01` is allowed: native checking actually
selects its monitor and accepts both correct sources. This probe does not use
v1's tests.p-only selection check.

This shows a real difference in the retained scenarios: both Sol drivers expose
reconsideration at the eligibility boundary, while neither Luna driver does
within its complete bounded search. It does **not** show a driver-report effect:
both report conditions agree within each model cohort. Primary detection of the
original recomputation fault remains valid, but did not establish this stronger
boundary property. The new fault was selected after source review; these results
are exploratory, not a replacement for the immutable primary inventory or a
population-level model comparison.
