# Handoff: experimenting with the agent evals

Start here when sharing PR #108 with another agent. The setup was first published
in commit `b433171` while trials were running. All eight missing-cell trials are
now complete: [factorial results](RESULTS.md). The prior completed experiment is
[single-file examples](../single-file-examples/RESULTS.md).

## What is being compared

All solvers are fresh `gpt-6.1-sol`, medium thinking, `fork_turns="none"`.
Two tasks, two independent repetitions per task/cell:

| CLI | No skill | Skill |
| --- | --- | --- |
| `snappea-compat` | Prior `onefile-sol-{flow,vcs}-compat-{01,02}` | New `factorial-sol-{flow,vcs}-compat-skill-{01,02}` |
| `snappea` | New `factorial-sol-{flow,vcs}-native-no-skill-{01,02}` | Prior `onefile-sol-{flow,vcs}-single-{01,02}` |

The skill treatment adds only:

> Use the P modeling skill at /opt/p-modeling/SKILL.md.

It mounts an immutable skill snapshot read-only at that path. The usual chosen-CLI
instruction and FIFO command spelling adapt to the CLI. Skill content is identical
for both CLIs, including its native-command examples; agents must discover how to
use their selected tool. No extra hints or private feedback are supplied.

The original eight Sol trials ran earlier. Tools, tasks and budgets match, but
execution block and tiny samples limit causal interpretation. Keep per-run source
quality, checker outcomes and implementation scores separate.

## Tasks and files to inspect

- [FIFO task](../modeling-skill/phases/flow/public/TASK.md): shared bounded queue,
  stable rejected/empty outcomes, competing clients, Python implementation and
  real observation tests. Public declarations/runtime are alongside it.
- [VCS task](../certified-bundles/public/TASK.md): revision-bound approvals/CI,
  atomic multi-branch publication, durable outcomes, crashes, growth and progress.
- [Skill](../../skills/p-modeling/SKILL.md), including self-contained original
  examples and an executable syntax reference.
- [Protocol](PROTOCOL.md): exact pins, prompt delta, controls, outcomes and limits.
- [experiment.py](experiment.py): exact eight-run provisioning recipe, baseline
  container creation followed by optional read-only skill mount and prompt update.
- [complete.py](complete.py): freeze submissions, invoke original graders and
  retain logs/Docker events even if grading fails.
- [analyze.py](analyze.py): integrity verification and separate supplemental checks.
- [source_assembly.py](source_assembly.py),
  [regressions](test_source_assembly.py): validated declaration-only assembly repair.
- [uptake.py](uptake.py): evidence of reference text actually returned after bridge
  truncation. Common-line matches need manual review; they do not prove reading.
- [combine.py](combine.py): joins the previous and new Sol cells without changing
  old outcomes. [blind.py](blind.py) creates neutral source copies for review.

FIFO keeps its public/private sources and runner under
`evals/modeling-skill/phases/`; VCS uses `evals/certified-bundles/`. Inspect private
fixtures only in the evaluator context, never in a solver context.

## Fresh checkout versus exact replay

The exact-cohort `experiment.py start` deliberately requires the previous local
run manifests plus pinned binaries/image. Those ignored artifacts are not in a
Git checkout. It is not a general fresh-checkout launcher. Do not weaken its pins
or edit its frozen experiment to start an unrelated trial.

For a **new experiment**, use the reusable runners below or copy the cohort
coordinator into a new eval directory and give it new run IDs, configuration and
protocol. Change the assignments/pins/parity checks in the copy. Freeze the new
configuration before delegation. A new build or altered skill is a new condition,
not an exact reproduction of these scores.

For **exact replay/inspection** on this existing workspace, raw trials are under:

- `evals/modeling-skill/phases/runs/<id>/`;
- `evals/certified-bundles/runs/<id>/`;
- `evals/sol-factorial/runs/` for cohort maps and verification inputs;
- `evals/certified-bundles/runs/skill-snapshots/<tree-sha>/`.

The prior raw evidence archive is
`/workspace/library-files/snappea-single-file-examples-evidence.tar.gz`
(SHA256 `02741dc2370b970ff28452de7a6b8e21d85cc8523a5e711f7f3858f8ed18c6a2`).
It includes the previous trials, immutable skill snapshots and binaries. This is
an existing-workspace path, not a GitHub download. Another workspace needs that
artifact supplied separately. The combined evidence archive is
`/workspace/library-files/snappea-sol-factorial-evidence.tar.gz`; it retains the
eight new trials and twelve prior trials so the earlier audits remain verifiable
(16 Sol trials, plus four earlier Luna format trials). Docker image IDs identify
local images, not published registry tags.

## Prepare a new local experiment

Prerequisites: Python 3.11+, Rust, working Docker, and an agent backend that can
launch fresh independent solvers. Run from repository root.

```sh
cargo build --release -p snappea --bin snappea --bin snappea-compat
python evals/certified-bundles/tools/snapshot_skill.py --source skills/p-modeling
```

Build the offline image using the wheel-download/image-build instructions in
[certified-bundles README](../certified-bundles/README.md#prepare-isolated-runs).
That builder makes a source `pysnappea` wheel from this checkout; record its hash
and the image ID. Mount the checker separately. Do not silently substitute the
upstream download for a local build when testing current feedback features.

Configure the native binary/source/image for the VCS runner:

```sh
export SNAPPEA_EVAL_BINARY="$PWD/target/release/snappea"
export SNAPPEA_EVAL_SOURCE="$PWD"
export SNAPPEA_EVAL_P_IMAGE="$(docker image inspect snappea-eval:py312-pytest842-pysnappea-source-0.1.0 --format '{{.Id}}')"
```

Use a unique run ID. This starts a **native/no-skill FIFO** exploration trial:

```sh
python evals/modeling-skill/phases/runner.py start \
  --run-id explore-sol-flow-native-01 --phase flow --condition baseline \
  --tool snappea --binary target/release/snappea \
  --model gpt-6.1-sol --reasoning-effort medium
```

For native/no-skill VCS:

```sh
python evals/certified-bundles/tools/collaboration_run.py start \
  --run-id explore-sol-vcs-native-01 --arm p --condition baseline \
  --tool snappea --model gpt-6.1-sol --reasoning-effort medium
```

Replace `--tool snappea` with `--tool snappea-compat` for a no-skill compat trial;
the compat binary must be beside the native binary. Keep the native backend for
Python observation helpers identical across conditions.

The generic runners' `--tool` option currently requires `--condition baseline`.
For **CLI × skill** trials, use the explicit pattern in `experiment.py` rather
than combining unsupported flags:

1. Provision the chosen CLI with the baseline runner, before any solver call.
2. Recreate its container with the same workspace, image, CLI/backend and ordinary
   references, adding only the immutable skill mount. FIFO uses
   `runner.container_args`; VCS uses `run_sonnet.container_args`. For VCS pass the
   chosen CLI binary as its CLI override, not the native backend by accident.
3. Prepend the exact skill sentence to delegation/task prompt, record the skill
   tree manifest and actual prompt hashes, and preserve original metadata.
4. Verify in-container CLI/backend hashes, task/schema, budgets and all other
   inputs match controls. Freeze configuration before delegating any solver.

A single-CLI skill trial can also use the generic runner's existing skill mode
without `--tool`; see the earlier README's skill comparison instructions. Use the
cohort pattern above when you need both CLI selection and skill mounting.

## Delegate, freeze and evaluate

Read the generated FIFO `prompt.txt` or VCS `delegation.txt` and use that exact
text as the fresh solver's message. Match the recorded model/thinking settings.
Give no parent history, grader access, other runs, or follow-up hints. Each solver
uses only its recorded shell bridge. Containers have no network and receive only
public inputs, chosen CLI, shared Python backend and optional skill/reference mounts.

Budgets: 900 seconds from first call, 80 calls, FIFO 90-second commands, VCS
120-second commands. Save the final reply as `agent-final.txt`; freeze before
hidden grading. For the generic examples above:

```sh
python evals/modeling-skill/phases/runner.py finish --run-id explore-sol-flow-native-01
python evals/modeling-skill/phases/grade.py --run-id explore-sol-flow-native-01
python evals/certified-bundles/tools/collaboration_run.py finish \
  --run-id explore-sol-vcs-native-01 --seeds 64
```

For this exact cohort, `complete.py ID` performs the appropriate freeze/grade and
retains execution logs. Never send hidden results back to a solver or reroll a
modeling failure. Distinguish timeouts/setup errors from assertion detections.

**Known grading edge:** the original FIFO grader restores `events.p` wholesale.
Legal added declarations can therefore break model, saved-test, monitor-history
and portable-reference axes. The separate local recheck scripts apply the same
validated declaration-only repair to every candidate, retaining original results.
The pure Python implementation score is unchanged. See the protocol before
interpreting an original compile error as a model defect. A new coordinator can
reuse these scripts with its own map; do not change archived original grades.

Primary scores: FIFO eight hidden cases, VCS twenty. Separate VCS wide-bundle
probe: a valid 64-branch payload crossing 16 KiB records. Keep it supplemental.
Portable fault tests are interpretable only when the saved suite accepts the
valid reference. Five-second model incompletion is not a counterexample.

Review source before grades, then inspect complete transcripts for selected
monitors, oracle independence, CAS/retry fidelity, real public replies/state,
useful completion, delayed replay after state change, actual implementation
capture, reference visibility, model-first timing and feedback-driven changes.
Retain prompts, hashes, original/supplemental outcomes and source/transcript audits.
Do not equate passing code cases with a good model or a completed proof.
