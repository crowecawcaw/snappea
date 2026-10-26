# Feedback revision: generated histories and concise references

This development revision follows the [cold transfer results](NOVEL_RESULTS.md).
It tests a revised skill and clearer checker feedback; it does not isolate the
causal contribution of each edit.

## Changes under test

- Bounded-history example: fixed calls, schedule-only variation, generated
  operations/arguments, independent monitor, manually calculated valid/invalid
  histories, and a broken control.
- Compact executable P reference: tuples, declarations, maps, local iteration,
  choices, communication, test selection, and short checking commands.
- Skill: valid value classes and output boundaries alongside rejection guards;
  bounded history generation and independent monitor probes.
- Checker: first JSON field is a compact outcome summary with required uncovered
  source sites. Assertion failures, coverage failures, incomplete search, and
  mutation timeout are reported separately. Existing exit behavior is retained.

## Fresh trials

Use fresh Luna/high agents, the existing offline bridge and unchanged budgets.
Only the skill condition receives the skill sentence and read-only skill files.
Both conditions receive the same updated checker. Freeze the skill and binary
before starting; do not coach agents or revise content between these trials.

- Streaming model: two skill trials and one no-skill control.
- Streaming tests: two skill trials and one no-skill control.
- Held-out priority queue tests: two skill trials and one no-skill control.

The priority task was authored without consulting the skill. It checks stable
priority ordering, priority changes, cancellation/re-admission, and rejected
operations; it has no matching skill example. Its public task explicitly requests
strict observation in both conditions. Calibrate before launching solvers.

Assess separately:

- Model: computed service behavior; monitor acceptance of valid histories and
  rejection of invalid ones; generated input histories versus fixed workloads;
  declared bounds, completed search, coverage, and oracle independence.
- Tests: correct variants accepted and seeded faults rejected; actual-call
  observation provenance; valid value classes and lifecycle/order boundaries.
- Workflow: syntax repair, tool discovery, response to uncovered source sites,
  accurate reporting of failure causes and incomplete evidence.

These are small adaptive pilots. Compare the new matched conditions directly;
comparison with previous runs is descriptive because the skill and checker both
changed. A fault-detection ceiling is not evidence of a skill advantage. Do not
pool revisions or substitute monitor scores for algorithm quality.

## Reproduce

Build a separate binary so earlier frozen trials retain their original checker:

```sh
cargo build -p snappea --release --target-dir target/feedback
python skills/p-modeling/validate.py --binary target/feedback/release/snappea
python evals/modeling-skill/phases/calibrate_novel.py streaming --binary target/feedback/release/snappea
python evals/modeling-skill/phases/calibrate_novel.py priority --binary target/feedback/release/snappea
python evals/modeling-skill/phases/runner.py start --phase streaming/model --run-id feedback-model-new --binary target/feedback/release/snappea
```

Other phases: `streaming/testing`, `priority/testing`. Add `--condition baseline`
for controls. Delegate the saved prompt verbatim. Finish/freeze and grade as in
[the shared protocol](README.md). Runs archive public/private inputs, harness,
prompts, commands, skill snapshot hash, binary hash, and frozen submissions.
