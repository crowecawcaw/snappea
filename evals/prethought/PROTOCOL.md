# Written failure scenarios before modeling: Luna/high pilot

Frozen before delegation, 2026-10-28.

- Eight fresh `gpt-6-luna` / high agents: shared FIFO and certified-bundles VCS;
  two repetitions per task per arm. No skill; native Snappea in every run.
- Control: existing no-skill task/delegation prompt. Treatment adds exactly:
  “Before modeling, write a short design note listing the main failure modes and
  concrete scenarios that would expose them, including concurrency. Use it to
  guide the model and tests.”
- Preserve original task contracts, hidden graders, pinned binary/image, budgets,
  docs/reference mounts and library. No new scenario hints or solver feedback.
- Binary: `target/driver-eval/release/snappea`, SHA256
  `b7ea15b06ee99225277adaeeee4dda33b2d2e6a6484b22ec3c13d9973d051fb2`.
  Image: `sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
- Budget: 900 seconds / 80 calls. FIFO 90 seconds/command; VCS 120.
  Native defaults, including mutation, remain available.
- Run IDs encode task/repetition and opaque a/b. Assignment alternates by task
  and repetition. Freeze all prompts/input hashes before delegation. Provisioned
  runs do not start their clock until the first solver call.
- Freeze each submission before hidden grading. Never feed grades to solvers.

## Outcomes

Report existing hidden implementation cases, independent monitor-history cases
(FIFO), saved-test execution and existing portable probes where supported.
Report checker completion, coverage and mutation separately from semantic quality.

Source-first audit before revealing arm/grades:
- Does the P model preserve competing actors and actual atomicity boundaries?
- Are properties independent of implementation logic, selected and exercised?
- Do scenarios expose incorrect outcomes, or merely invoke an operation whose
  defect is masked by later operations or equivalent observable outcomes?
- Are retry identity, intervening changes, lifecycle and progress represented?
- Are Python observations from actual implementation calls? Are concurrent
  histories checked with a valid ordering oracle?
Then inspect transcripts: was a note written before P, and which planned cases
actually became model behaviors/tests? Separate adherence from correctness.

This is an exploratory pilot, not an estimated causal effect. All runs count,
including failures to finish. Existing known eval ceilings remain a limitation.
Any additional source-driven probes must be labeled posthoc and cannot replace
primary scores. No tool/prompt tuning during this cohort.

## Reproduce

`python evals/prethought/experiment.py start` provisions all eight trials and
writes `runs/map.json`. Delegate each saved prompt to a fresh Luna/high agent.
`python evals/prethought/experiment.py finish --run-id ID` freezes and grades.
Raw runs remain under the existing task harness run directories; preserve them
with the map and experiment source when archiving evidence.

Pre-source-review supplement: also run the already-existing VCS wide-bundle
stress (`private/stress_wide.py`) on all four frozen VCS submissions. Keep this
separate from the original 20 cases, as in earlier cohorts. This checks the
explicit 64-branch / per-record size contract and is not a newly invented probe.
