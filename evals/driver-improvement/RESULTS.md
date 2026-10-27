# Driver improvement: useful gaps, no demonstrated report effect

Twelve fresh agents reviewed existing correct P models and improved only their
drivers/tests: six Sol 6.1/medium, followed by six Luna/high. Three fixtures and
report-on/off arms, no skill. Both arms collect the same facts with identical
native arguments; only display differs. No hidden results reach solvers.

The experiment found actionable report limitations, finer test-quality
differences, and an eval bug. It does **not** establish a driver-report benefit.

- [Plan](PLAN.md), [separate Luna follow-up](FOLLOWUP-LUNA.md), [reproduction](README.md).
- [Verified trial records](results.json), [Sol source/transcript audit](AUDIT-SOL.md), [Luna audit](AUDIT-LUNA.md).

## Primary hidden faults

Counts below are **new latent faults detected**, excluding the one regression
control already caught by each starting driver.

| Model/task | Reports on | Reports off | Correct-search qualification |
|---|---:|---:|---|
| Sol priority | 5/5 | 5/5 | Generated searches incomplete |
| Sol retry | 3/3 | 3/3 | Generated/concurrent searches incomplete |
| Sol ownership | 3/3 | 3/3 | Complete |
| Luna priority | 4/5 | 3/5 | Complete |
| Luna retry | 3/3 | 2/3 | Complete |
| Luna ownership | 3/3 | 3/3 | Complete |

All twelve retain their starting regression detection. Neither correct
implementation produces a violation under any frozen submission; the Sol priority/retry searches
are incomplete, so that acceptance is qualified. Existing assertion
counterexamples against faulty variants are still real detections. No compile,
lint, coverage, hot-completion or timeout failure receives fault-detection credit.

Sol produces broad deterministic witnesses and generated scenarios in both arms.
Ownership tests enumerate all state/command combinations. Luna improves every
driver but leaves masked cases: negative priorities that do not change the winner,
duplicate priority corruption hidden by a later update or unchanged winner, and
an accepted request retried only immediately rather than after another write.

The two extra Luna detections with reports on are not credible causal evidence:
the priority-on scenarios are written **before** that run first receives driver
facts, and retry-on receives only an instance-count fact. Ownership-on broadens
after both coverage gaps and narrow-input facts; attribution is ambiguous.
There is one run per cell, not a treatment-effect estimate.

## Post-hoc boundary probes: a stronger quality distinction

Source review identified two obligations not adequately distinguished by the
original fault inventory. Separate probes were then authored and applied equally
to the frozen submissions. They are **not added to the primary scores**.

| Additional fault | Sol on/off | Luna on/off |
|---|---|---|
| Reconsider a rejected request only when its expected version later becomes current | Both detect | Both miss |
| Preserve a popped item's old admission position on re-admission, while cancellation reuse remains correct | Both detect | Both miss |

All Luna searches against these additional faulty models complete without finding
the fault. All four relevant drivers accept the correct controls for each probe;
Sol's correct-control searches retain their incomplete qualification.
Weak drivers miss each added fault and focused reference witnesses expose it
through the protected monitors. Frozen artifacts are hash-verified unchanged.

- [Retry eligibility probe and results](probes/retry_eligibility_POSTHOC.md).
- [Priority pop/reuse probe, construction and findings](probes/priority_pop_reuse.py).

The original rejection-recomputation mutant also changes a returned version.
A test can kill it without ever reaching the eligibility boundary. Similarly,
the original stale-admission mutant affects both cancellation and popping;
testing only cancellation can kill it. **A killed broad mutant does not establish
coverage of every path or symptom of that defect.** The focused probes separate
those obligations. They were chosen after inspecting source and are exploratory.

This shows a real scenario-quality difference between these model cohorts,
not a report-feature effect. Sol/Luna also differ in thinking setting; this is
descriptive evidence, not an estimated model effect.

## Concrete report gaps

1. **Nested arguments disappear.** Retry sends `(caller, command)`, with request
   ID, expected version and value inside `command`. The collector keeps only
   top-level scalar fields, so none of those restrictions appears.
2. **Repeated call counts disappear above two.** `COUNT_LIMIT = 2` excludes the
   retry driver's four sends. A single event type also has no cross-event-type
   order fact. Combined with nested payload loss, its entire report is
   `creates 1 Configuration, never more` despite a fixed four-call history.
3. **Value marginals lose command context.** Priority reports values `-1,0,1`,
   but zero comes from ignored fields of cancel/pop; admissions use only distinct
   -1/1. The report does not establish which operations used which values, much
   less equal-priority competition.

These are reproducible with the unmodified starting fixtures, independently of
agent performance. See [retry diagnosis](fixtures/retry/private/tooling_gap.md),
[collector](../../crates/snappea/src/checker/driver_facts.rs#L196), and
[count filtering](../../crates/snappea/src/checker/driver_facts.rs#L402).
Recursive scalar paths, visible fixed counts above two, and bounded full-command
patterns would address these specific information losses. They would not infer
all missing contract semantics automatically. No tool fixes were applied during
the cohorts.

## Instructions and eval reliability

- No protected-source drift is recorded after any agent call. Source audits find
  preserved assertions and no added assumptions, suppressions, fake observations
  or direct backend invocation.
- Some agents request timeout/mutation changes; one Sol agent requests
  `--no-driver-facts` twice. The wrapper clamps these, so none changes the study.
  These are observed requests, not proof of intent. Priority/retry task text is
  less explicit about fixed settings than ownership; treat that as an instruction
  clarity limitation. Requests for mutation seek additional validation, not weaker
  assertions. This study disables mutation to isolate report display, so it says
  nothing definitive about the full tool's benefit with mutation available.
- Luna retry-on briefly generates different arguments for a repeated ID. The
  monitor's explicit input guard rejects the invalid scenario; the agent fixes it
  by choosing once and reusing the value. This is concrete assertion-feedback
  improvement, not driver-report uptake.
- A grader wrongly requires the test declaration in `tests.p`. Luna retry-on
  legally moves it to `driver.p`, causing a false invalid result. The fix scans
  both editable sources, with regression tests. All twelve runs receive the same
  [selection-only correction](correct_selection.py), reusing verified executed
  results without checker reruns. Only that eligibility/score changes. Original
  grades remain archived alongside `supplemental-selection-v2` and provenance.

The main actionable result is narrower than “reports work”: these tests now
distinguish scenario quality, and the report currently omits information useful
for some of the demonstrated gaps. Better output is worth testing against the
same frozen fixtures; whether it changes agent behavior remains open.

## Validation and evidence

Fourteen harness/correction tests pass. Initial/reference drivers accept two
correct representations; reference drivers catch all 14 original faults.
Missing completion, zero work, invalid IDs and changed retry arguments are
rejected against correct models. The real on/off calibration verifies identical
native arguments and equal non-driver JSON except elapsed time, including sidecars.

`summarize.py` verifies all frozen inputs and matching binary/image/budgets/prompts
across the twelve trials. Source-first audits precede arm/hidden-result inspection.
The original inventories precede driver-report inspection; baseline regression
controls were added during preflight and labeled separately. Six initial
provisionings were replaced before any solver calls to clarify the no-`assume`
instruction; they are not trials. No solver received that prelaunch material or
hidden feedback. Raw trials, original/corrected grades, calibration, probes,
source notes and exact checker are preserved in
`/workspace/library-files/snappea-driver-improvement-evidence.tar.gz`.
