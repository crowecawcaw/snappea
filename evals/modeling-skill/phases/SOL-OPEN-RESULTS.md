# Sol 6.1 / medium: broader design-and-build evals

**Sol shows more useful skill uptake in modeling practices, but no improvement
on the primary implementation score. A separate growth-cost witness exposes a
real limitation in one skill implementation. The model-quality verdict is mixed.**

Six fresh `gpt-6.1-sol` / medium agents repeated the [Terra broader-task setup](TERRA-OPEN-RESULTS.md): two skill runs and one no-skill control for each task.
Both conditions are asked to use P. Public/private fixtures, checker, image,
skill snapshot and budgets match the corresponding Terra runs; no tuning or
hidden feedback was given to solvers.

- [FIFO results](sol-open-flow.json), [blind source/transcript audit](SOL-OPEN-AUDIT-flow.md).
- [VCS results](sol-open-vcs.json), [blind source/transcript audit](SOL-OPEN-AUDIT-vcs.md).

## Primary results

| Measurement | Skill 01 | Skill 02 | No-skill control |
|---|---|---|---|
| VCS: hidden implementation cases, 64 seeds | **20/20** | **20/20** | **20/20** |
| VCS: existing 64-branch, 27,565-byte update probe | Pass | Pass | Pass |
| VCS: saved additional Python tests | 6; pass | 5; pass | 4; pass |
| VCS: saved Python observations reuse design monitor | Yes | Yes | No |
| FIFO: hidden implementation cases | **8/8** | **8/8** | **8/8** |
| FIFO: independent monitor histories | **8/8** | **8/8** | **8/8** |
| FIFO: saved Python tests | 9; pass | 6; pass | 7; pass |
| FIFO: fixed-budget model search | Complete | Complete | Incomplete; no bug found |

VCS test rechecks also include four supplied tests, for totals of 10/9/8.
All tests pass against their own implementations. The FIFO portable reference
swap is incompatible in all conditions, so its mutation probes are skipped;
no fault-detection score is inferred from that diagnostic. Primary hidden cases
remain separate from agent-written test quality.

As with Terra, the control FIFO model explores a larger fixed concurrent
workload without finishing. Both skill runs finish their chosen bounds; this
alone is not evidence that their algorithms or scenarios are better. Mutation
feedback also has incomplete evidence and surviving omissions.

## Did the skill help models?

### FIFO: useful operation generation, with a strong control too

All three model actual loads, failed/successful CAS, recomputation, FIFO state
and durable outcomes. All three independently reconstruct the contract in a P
monitor and reuse that same monitor to check actual Python histories. Their
concurrent tests search for interval-respecting linearizations: they preserve
real inputs/results and response-before-invocation constraints rather than
assuming completion order is the linearization order. All force real Python
CAS conflicts.

Both skill models generate bounded **operation choices**, beyond the initial
value variation seen in Terra:

- Skill 01 reduces an initial four-step generator to three, retaining bounded
  operation/ID/value choices alongside fixed boundary/contention scenarios.
- Skill 02 independently chooses put/take on each of two steps per client, with
  fixed per-client identity/value and separate shared/distinct-ID cases.
- The control's P mains use fixed scripts. Its Python suite nevertheless
  generates 500 seeded serial calls, saves raw concurrent intervals and witness
  traces, and directly tests the linearizer's handling of legal overlapping
  completion-order inversions. Those are strengths the skill runs do not all
  match.

All three create P before Python in this task; chronology is not a skill
separator here. None visibly repairs the Python queue algorithm because of a
model-discovered violation. Feedback improves scenarios, assertions and model
instrumentation. The control adds a hot progress observer and fixes its
counterexample-producing double count, but client completion still does not
prove that every required operation ran. Some omission/acknowledgement mutants
survive or remain unresolved. The skill models also lack complete per-operation
progress/observation obligations.

This is better bounded-model practice in the skill runs, with a capable
control that contributes different testing strengths. It is not a uniform win.

### VCS: stronger independent oracles, incomplete lifecycle coverage

Both skill runs build/check P before Python. The control writes Python first,
then models it. The skill models are adapted to bundles, rather than copies of
a generic example:

- Skill 01 separates read, immutable preparation and root CAS; models two heads,
  distinct publishers, actual success/conflict memos and A retrying after B.
  Coverage feedback exposes an unexercised retry assertion and prompts a real
  scenario change. A separate unsafe-version-check control fails as expected.
- Skill 02 models competing IDs, versioned CAS, return outcomes and nondeterministic
  ready/stale combinations. An undetected changed-return mutation prompts an
  independent return assertion, although mutation analysis is not rerun afterward.
- Both use independent monitor state and the same design monitor in saved real
  Python observations. The adapters remain abstractions: bounded identities,
  numeric heads and selected sequential histories do not establish general
  model/implementation refinement.

The control models revision changes, approval sets, CI replacement and candidate
identity more broadly, but its Store and Client both call the same `Evaluate`
helper. Self-consistency through that shared evaluator is a weaker oracle than
an independently reconstructed contract; it has no separate P monitor or saved
Python-to-P observations. Its custom Python tests still provide useful
concurrency, crash, growth and lifecycle checks.

The skill models are narrower in important ways: neither explores actual
approval/CI invalidation across a concurrent revision. Skill 02 equates bundle
and publish-request identities, making a fresh request against an already
published bundle inexpressible; the published guard is removed after coverage
feedback. A complete selected search therefore does not imply requirement
coverage. Better independent monitors coexist with missing task states.

## Source-driven growth witness: not part of primary scoring

The auditor took the escaped-Unicode payload pattern from skill 02's saved test
and applied **the same** workload to all three frozen implementations:

- 64 branches, 176 distinct bundle submissions, each update map 85,293 compact
  ASCII-JSON bytes; no contention, crashes or runtime modifications.
- Calls measured as `Store.steps` delta divided by two, since each store method
  has one before and one after yield.
- This ran in host Python against each frozen public runtime, not under the
  primary container's CPU/memory caps. It measures logical store calls, not
  latency or a container resource benchmark.

| 176th submit | Skill 01 | Skill 02 | Control |
|---|---:|---:|---:|
| Store calls | **5,008** | 121 | 48 |
| Within the task's 5,000-call target | **No** | Yes | Yes |

Skill 01 serializes and rewrites all accumulated logical state through a linked
chunk chain. The others use persistent AVL layouts. The state-growth dependency
is observable here, rather than merely a source-level scalability concern.
This is a budget-target failure, not a demonstrated non-linearizable result.
It does not change the original 20/20 scores. The shared script, exact input
builder, commands, outputs and integrity checks are retained under
`certified-bundles/runs/sol-open-audit-vcs/` and in the evidence archive.

The earlier small-record history test and single wide-bundle probe each passed;
their combination exposed the limit. Passing them independently did not validate
large records after accumulated history. The skill did not guarantee a robust
storage-cost design.

## Native-check reproducibility caveat

VCS skill 02 completes its 6,304-schedule search in the independent five-second
recheck. The control remains incomplete, as it did in its own longer run.

Skill 01's saved output reports complete search at 30,304 schedules in 1.965s.
However fresh-container checks of the frozen source do **not** reproduce that
completion at five seconds or at its original 40-second/no-mutation budget.
Five-second seed 0/1/0 diagnostics are also incomplete. All reproduce required
coverage and find no assertion failure. Safe literal reconstruction of the
transcript's final model edits exactly matches the frozen source hash.

The cause is unknown. Retain the original result, independent outputs and source
reconstruction separately; do not treat the discrepancy as solver fabrication,
a proved model defect, or a diagnosed seed issue. For independent evaluation,
its completion remains unconfirmed. No checker or solver source was modified to
make these runs pass.

## Interpretation and provenance

The strongest positive evidence is specific: bounded P operation generation,
independent VCS oracles, and reuse of design monitors in real implementation
tests. These practices are more visible than in Terra's skill cohort. But the control has broader lifecycle modeling and useful generated
Python tests; one skill implementation exceeds the storage-call target. No
overall correctness benefit or causal effect size is established by two skill
runs and one control per task. These compare skill versus no skill with P
requested, not P versus Python-only work.

The same [reproduction protocol](TERRA-OPEN-RESULTS.md) applies with
`--model gpt-6.1-sol --reasoning-effort medium`. Skill tree
`75cb6bc852fb31b9d88dc026d12300f26dd307b5d32f62dae09fa31b64beaaae`, checker
`2ea61378c21ff899f3977eaefc505753b566f2fa3a114d5288c4a8a60d864194`, image
`sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
Budgets remain 900 seconds/80 calls; command caps are FIFO 90s and VCS 120s.
All public inputs stayed unchanged. Fixture/tool/skill/budget comparisons with
Terra are verified. No content tuning occurred during the cohort.

Raw bundle: `snappea-sol-open-evals-evidence.tar.gz`, per-file manifest and
SHA-256 sidecar in `/workspace/library-files/`. It contains all six trials,
exact checker binary, skill snapshot, original/supplemental results, source
reviews, growth witness and native-reproducibility diagnostics. Original
submission-manifest files, including negative-control traces, are retained;
working copies and caches are excluded. File digests are verified on archive
readback. Four snapshot/prompt/archive tests and five phase regressions pass.
