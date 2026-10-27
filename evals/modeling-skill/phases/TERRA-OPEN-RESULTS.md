# Terra open-ended design and implementation runs

**All six implementations pass their hidden contract cases. The skill coincides
with model-before-code and additional VCS observation tests, but no measured
implementation-correctness advantage. Model scope remains the main limitation.**

Six fresh Terra 6.1 / medium agents: two skill runs and one no-skill control for
each existing broader task. Both conditions use P/Snappea; treatment adds only
the skill sentence and read-only skill directory. These are design-and-build
tasks with fixed requirements/interfaces and freedom to choose the algorithm
and storage layout, not unconstrained product design.

- [Shared FIFO task](flow/public/TASK.md), [machine-readable results](terra-open-flow.json), [blind source/transcript audit](TERRA-OPEN-AUDIT-flow.md).
- [Protected VCS task](../../certified-bundles/public/TASK.md), [machine-readable results](terra-open-vcs.json), [blind source/transcript audit](TERRA-OPEN-AUDIT-vcs.md).

## Results

| Measurement | Skill 01 | Skill 02 | No-skill control |
|---|---|---|---|
| VCS: primary hidden cases, 64 scheduler seeds | **20/20** | **20/20** | **20/20** |
| VCS: existing supplemental 64-branch / 27,565-byte update probe | Pass | Pass | Pass |
| VCS: post-freeze native model check | Pass; complete | Pass; complete | Pass; complete |
| VCS: saved custom Python observation tests | 3; pass | 4; pass | None saved |
| FIFO: hidden implementation cases | **8/8** | **8/8** | **8/8** |
| FIFO: independent monitor histories | **8/8** | **8/8** | **8/8** |
| FIFO: saved Python suite | 4; pass | 5; pass | 4; pass |
| FIFO: fixed-budget model search | Complete | Complete | Incomplete; no bug found |

VCS custom test rechecks also include the four supplied public tests: 7 and 8
total tests pass. The control passed the public tests but saved no custom test
module. Native VCS model checks pass at a fixed five-second search budget with
mutation feedback budgeted separately. These are checks of the submitted
abstractions, not independent validation of the entire VCS contract.

The FIFO grader requires completed model exploration, so its overall validity
flag is true for the skill runs and false for the control. The control found no
assertion failure and met required coverage, but its larger concurrent case
remained incomplete at five seconds; its own ten-second run also reported
incomplete search. This is insufficient search evidence, not a demonstrated
implementation defect. Different scenario bounds make completion an unsuitable
standalone design-quality comparison. FIFO mutation exploration was incomplete
in all three runs.

The FIFO portable test-swap diagnostic rejects the uninstrumented reference in
all three cases, so mutation probes are skipped. No fault-detection score is
claimed from those tests. Their primary saved-test result is against each
agent's own implementation; the eight independent hidden cases are separate.

## What the agents actually designed

### Protected VCS

All three choose immutable content-addressed storage behind one shared root CAS.
A successful publication atomically exposes all heads, the published flag and
request outcome; rejected outcomes are memoized through the same visibility
point. Root-CAS losers reread and reevaluate. That is a credible architecture
for the provided primitive, with assumptions and limits recorded in the audit.

Skill 01 and the control use persistent radix maps, updating affected paths.
Skill 02 chunks a serialized whole-state snapshot and rereads/rewrites it for
mutations. That layout has a stronger history-dependent cost; the agent discloses
it. Both the primary growth cases and existing wide-input probe pass. Source
inspection suggests a scaling risk, not a demonstrated failure at the tested
bounds. An arbitrary-input universal call bound cannot be established for any
layout with bounded store records and unbounded valid string lengths.

The useful skill-associated differences are in the workflow:

- Both skill runs check a model before implementing Python. The control writes
  Python first and adds its model afterward despite the design-first prompt.
- Both skill runs save real observed implementation histories and separate
  race/crash tests. The control saves only its implementation, design note and
  P model. All share the same broad architecture and pass the same hidden cases.
- Skill 01 checks the skill's generic versioned-publication example, copied into
  `model.p`. It covers read/CAS/revalidation, but adds no VCS-specific heads,
  outcome memos, revisions, approvals, CI or crash state.
- Skill 02 authors a competing-request/duplicate-ID CAS abstraction with head
  and memo updated together. It omits immutable-tree preparation, multiple heads,
  evidence revisions and explicit crash transitions. The control's model has an
  even weaker shared Boolean memo and identical target payloads.
- The skill-run Python monitors are separate from their design-model monitors:
  a fixed-stage publish/head history in skill 01 and a small result/head oracle
  in skill 02. They preserve actual results, but do not demonstrate refinement
  of the design models or reuse one contract monitor across both stages.

These models check useful publication patterns. Their passes do not establish
that all the important VCS design obligations were modeled. Reusing an example
is helpful reference use, but running it unchanged is limited task-specific
design validation.

### Shared FIFO

All implementations use a shared versioned record with capacity, items, and
separate put/take outcome maps. A failed CAS causes a fresh load and computation;
replays preserve original results, including full rejection and empty takes.
All models represent separate storage loads and CAS, so they preserve actual
interference rather than treating the whole API as one atomic primitive.

Both skill runs build/check P before writing Python; the control writes Python
first. The skill runs force a stale-read collision in Python tests. Their
models check snapshots after modeled operations; the control's snapshots are
initial or hand-authored contract examples rather than reads after modeled work.
Its replay announcements can also occur after intervening commits, which can
misrepresent the replay's load linearization order.

There are limits in the skill models too: skill 01 models replay using an extra
CAS although its implementation returns from the memo-bearing load. That changes
interference and is disclosed as an abstraction. Skill 02 uses replay CAS in
both model and Python. Concurrency scenarios are bounded fixed workloads, and
some Python-tested races are absent from those models. No run generates varied
operation histories. Real serial observations do include retries after later
work on the same store; threaded checks use ordinary assertions and do not treat
post-return event append order as linearization order.

## Interpretation

This cohort supports a narrower conclusion than “the skill improves designs”:
it coincides with earlier modeling and extra saved implementation tests, while
the implementation score remains at ceiling in both conditions. Some design
models remain generic or omit significant failure boundaries. Completed checks,
saved observations, and matching implementation boundaries are separate forms
of evidence; none substitutes for the others.

Two skill trials and one control per task cannot estimate reliability or a
causal effect. This also does not compare P with Python-only agents. The tasks
remain useful for workflow and model-fidelity audits even when hidden-case
scores do not separate the conditions. No skill/tool content was tuned during
these runs.

## Protocol and reproduction

- Solver: fresh `gpt-6.1-terra`, medium, `fork_turns=none`.
- Skill tree: `75cb6bc852fb31b9d88dc026d12300f26dd307b5d32f62dae09fa31b64beaaae`.
- Checker: `2ea61378c21ff899f3977eaefc505753b566f2fa3a114d5288c4a8a60d864194`.
- Offline image: `sha256:00dbf3678a1e6fc660c1cc87fe05f9a8d2cdaeda2daa73fedd9941bb55ce5431`.
- Within each task, public inputs, tools, budgets and image match. Both tasks
  allow 900 seconds / 80 bridge calls. FIFO commands cap at 90 seconds; the
  existing VCS skill-comparison protocol caps them at 120 seconds.
- VCS additionally supplies ordinary docs and its inventory example to both
  conditions. Treatment alone mounts the skill. FIFO uses its existing setup.
- All submissions are frozen before hidden grading; public files remain
  unchanged. No solver receives hidden feedback or another run's artifacts.
- VCS reference calibration passes 20/20 at 64 seeds; seven harness tests pass.
  FIFO reference/fault and monitor calibration passes. Four snapshot/prompt/
  archived-grading regressions and five phase protocol regressions pass.
- VCS runner identity metadata is configurable, and new runs archive public,
  private and tooling inputs. Grading uses and verifies those archives. Older
  trials retain their historical fallback. No skill, CLI or runtime code changes.

FIFO reproduction uses [the phase runner](README.md), `--phase flow`,
`--model gpt-6.1-terra --reasoning-effort medium` and the pinned checker.
VCS uses the [certified-bundles runner](../../certified-bundles/README.md),
`--arm p --condition baseline` or `--condition skill --skill-snapshot PATH`, with
the same model settings and pinned image/binary environment. Delegate each saved
prompt verbatim. Identity flags label metadata; they do not spawn agents.

Raw bundle: `snappea-terra-open-evals-evidence.tar.gz`, per-file manifest and
SHA-256 sidecar under `/workspace/library-files/`. It preserves all six trials,
frozen sources/fixtures/harnesses, prompts, commands, grades, skill snapshot,
calibration, independent native/observation rechecks, and source audits. Working
copies and generated checker output are excluded; manifests verify file digests.
