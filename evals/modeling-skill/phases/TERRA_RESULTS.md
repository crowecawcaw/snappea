# Terra 6.1 / medium results

**Terra/medium performed more reliably than Luna/high in this small cohort.**
All three streaming models pass native checking and all fifteen independent
monitor probes. All six saved test suites accept the correct implementations;
five reject every seeded fault, while the priority control misses one.
The skill's main modeling weakness remains: operation histories are fixed.

Nine fresh `gpt-6.1-terra` agents, `reasoning_effort=medium`, `fork_turns=none`,
repeated the [feedback-revision protocol](FEEDBACK.md): two skill trials and one
no-skill control for each phase. The task/checker/skill/fixture/budget hashes
match the corresponding Luna runs. No content changes, coaching, or hidden-score
feedback were supplied to solvers. Only runner identity metadata became
configurable; grading logic is unchanged.

- [Machine-readable results](terra-pilot.json)
- [Model source/transcript audit](TERRA-AUDIT-models.md)
- [Test source/transcript audit](TERRA-AUDIT-tests.md)
- [Previous Luna/high cohort](FEEDBACK_RESULTS.md)

## Adding tests

| Task / configuration | Skill 01 | Skill 02 | No-skill control |
|---|---:|---:|---:|
| Streaming, Terra/medium | **8/8 faults** | **8/8 faults** | **8/8 faults** |
| Streaming, Luna/high | 7/8 | 7/8 | 6/8 |
| Priority queue, Terra/medium | **8/8 faults** | **8/8 faults** | 7/8 |
| Priority queue, Luna/high | 8/8 | 7/8 | 7/8 |

Every Terra streaming suite accepts the correct service (1/1); every priority
suite accepts both correct representations (2/2). No grading setup errors or
timeouts occurred. These are whole-suite scores: direct Python assertions and
P monitors both contribute, so they do not establish the benefit of P alone.

Source review:

- All six suites capture actual service calls/results and check the supplied
  monitor. They preserve event arguments, output ordering, and call boundaries.
  Skill suites explicitly select strict options; the streaming control uses
  named-spec defaults, whose missing-event grace period remains weaker than
  explicit strict checking. Priority's public task requests strict checks in
  both conditions.
- Terra's streaming suites include negative/zero values, nonzero exact bucket
  boundaries, late-first arrival, corrections, finalization, and ordered output.
  Those cases close the concrete gaps in the prior Luna suites.
- Both streaming skill suites save negative monitor controls by corrupting
  captured histories. These test the monitor's sensitivity, separately from
  testing a faulty service. They are not fabricated successful observations.
- Priority skill 01 and the control also save seeded 150-operation mixed
  histories with monitor validation. These broaden exercised operations, but a
  fixed seed is not exhaustive generation and can still miss a critical tie.
- The priority control misses a defect that assigns a fresh admission order on
  every reprioritization. Its named test reprioritizes all three competitors in
  their original order before popping; fresh orders therefore reproduce the
  correct relative order. Its seeded mixed history does not expose that defect
  either. A pop immediately after moving the earliest item into a peer's
  priority would distinguish the two behaviors.
- The skill-01 priority duplicate example alone is also weak: illicitly raising
  an already-first item's priority leaves it first. The broader saved suite
  nevertheless rejects the seeded duplicate fault. Do not infer whole-suite
  weakness from one weak test, or discrimination from a scenario's name.

## Making models

| Run | Native check / search | Valid histories accepted | Invalid histories rejected |
|---|---|---:|---:|
| Skill 01 | Pass; complete, 6 schedules | **6/6** | **9/9** |
| Skill 02 | Pass; complete, 6 schedules | **6/6** | **9/9** |
| Control | Pass; complete, 802 schedules | **6/6** | **9/9** |

All select their monitors correctly and implement actual aggregation state and
output computation. The earlier Luna cohort had a missing monitor selection,
a valid-history rejection from a shared omission, and a monitor allowing an
invalid backward watermark. None of those failures appears in these probes.
Passing all probes is still bounded evidence, not a correctness proof.

- Both skill drivers choose their initial value from -1/0/1; the control chooses
  one of two values. Subsequent operations and their order are predetermined.
  The control's larger schedule count does not mean a larger operation-history
  input space. All remain limited design explorations.
- Both skill agents read the compact P reference and the bounded-history README.
  Both used limited value variation; this cohort does not show adoption of
  general operation-history generation. The control also
  varies an initial value without reading the skill.
- Neither skill-model transcript records separate manually calculated valid and
  invalid monitor probes. The audit includes additional **auditor-authored,
  post-freeze** probes in temporary copies; those are our validation, not solver
  adoption. Their case-level results and coverage-gate exits are preserved
  separately under `runs/terra-monitor-audit/`.
- A useful feedback response appears in both skill transcripts: mutation checking
  exposed a removed watermark-state assignment surviving because observations
  echoed internal state. Recording the actual requested watermark instead made
  that missing state update observable. This is a concrete quality improvement
  during those runs, rather than proof of a skill effect.
- Monitors derive expectations from public events, but largely mirror the
  algorithm's transition rules. Their shared logic still leaves correlated
  interpretation errors possible. They enforce output completeness when
  `eEndCall` arrives; a permanently missing final call marker is not detected at
  end of trace. The independent history probes do not cover every such omission.

## Interpretation

The Terra configuration closes the observed correctness gaps more consistently;
the skill/no-skill scores mostly reach the current task ceiling. The priority
result is a small descriptive skill difference, not a demonstrated effect size.
Two skill runs and one control per phase cannot establish general reliability.
The model and thinking level both differ from the Luna cohort, so this is a
configuration comparison, not an isolated model or reasoning-effort experiment.
These tasks compare skill versus no skill **with P available and requested**;
they are not a P-versus-Python-only evaluation.

The current suites remain useful for detecting regressions and studying the
workflow, but streaming fault scores no longer distinguish the configurations.
More challenging held-out histories or repeated trials would be needed to
measure differences. Keep operation-history diversity and independent oracle
calibration as explicit source-review criteria; native pass and schedule count
alone miss those weaknesses.

## Reproduction and provenance

Use the [shared runner](README.md) with matching delegation settings:

```sh
python evals/modeling-skill/phases/runner.py start --phase streaming/model --run-id terra-model-new --binary target/feedback/release/snappea --model gpt-6.1-terra --reasoning-effort medium
```

Other phases: `streaming/testing`, `priority/testing`; control:
`--condition baseline`. Spawn a fresh Terra 6.1 / medium agent with the saved
prompt verbatim. Finish/freeze and grade before inspection of hidden results.
The identity options record metadata; they do not spawn agents themselves.

- Skill: `75cb6bc852fb31b9d88dc026d12300f26dd307b5d32f62dae09fa31b64beaaae`.
- Checker: `2ea61378c21ff899f3977eaefc505753b566f2fa3a114d5288c4a8a60d864194`.
- Image: `sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
- Budgets: 600 seconds / 50 commands / 90 seconds per command, unchanged.
- All public inputs remained unchanged. Corresponding public/private fixture,
  skill, image, binary, and budget metadata match the prior Luna trials exactly.
- Five existing protocol regression tests pass. No tool/library/skill behavior
  changed; the runner's only change records the selected solver identity.

Raw evidence: `snappea-terra-evals-evidence.tar.gz`, with a SHA-256 sidecar and
per-file manifest in `/workspace/library-files/`. It preserves all nine frozen
trials, skill snapshot, prompts/commands, graders, grades, source audits, and the
separate post-freeze auditor probes. Work copies and generated checker output
are excluded; archived file digests are verified against the manifest.
