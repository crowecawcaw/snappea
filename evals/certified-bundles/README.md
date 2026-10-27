# Certified bundle design eval

A runnable Python implementation benchmark comparing agents with and without
P/Snappea. The task is a protected multi-repository VCS service: revision-bound
approvals/CI, atomic publication, overlapping requests, durable request outcomes,
and crash recovery. The baseline cohorts receive no modeling skill; the optional
skill comparison is described below.

This directory contains the eval setup. Run outputs, submissions, transcripts,
checker output, and generated reports stay local and are ignored by Git.
Run commands below from `evals/certified-bundles/`.

## Files

- `public/`: task requirements, Python interface, durable single-key CAS runtime,
  and public behavior examples.
- `private/oracle.py`: independent state machine and linearizability checker.
- `private/grade.py`: twenty primary cases covering certification, idempotency,
  concurrent histories, crash sweeps, generated traces, record growth, and progress.
- `private/stress_wide.py`: separate exploratory valid-input probe with a
  64-branch update map that must span multiple 16 KiB records.
- `private/reference.py`: calibration implementation; never supplied to agents.
- `private/calibration-p/`: source model contrasting split and atomic publication.
- `tools/calibrate.py`: nine known-wrong implementations the grader must reject.
- `tools/test_harness.py`: storage, oracle, and report self-checks.
- `tools/collaboration_run.py`: provision, freeze, and grade isolated solver trials.
- `tools/prompts.py`, `tools/delegation.py`: shared task prompts and solver instructions.
- `tools/snapshot_skill.py`, `tools/skill_snapshot.py`: content-addressed skill snapshots;
  the focused snapshot and paired-prompt checks are in `tools/test_skill_snapshot.py`.
- `tools/solver_shell.py`: budgeted, logged bridge into network-disabled containers.
- `tools/run_sonnet.py`: optional Anthropic API runner.
- `tools/regrade.py`, `tools/stress_all.py`, `tools/summarize.py`: grade and summarize
  selected local runs without counting calibration or smoke runs as agent trials.

## Check the harness

Use Python 3.11+. No Python packages are required for local harness checks:

```sh
python tools/test_harness.py -v
python private/grade.py private/reference.py --seeds 64 --output results/reference.json
python tools/calibrate.py
```

The current grader is v1.1.3. Each grade records hashes of the grader, runtime, and
oracle. Cases have a 180-second timeout; the public per-operation limit remains
5,000 store calls in quiescence. Generated schedules are bounded tests, not proofs.
The reference calibrates the primary suite; it is not a complete solution to the
separate exploratory wide-input probe.

## Prepare isolated runs

Solver runs use a shared offline image based on
`python:3.12-slim@sha256:cb33f7f771ecec2ef1225cf39b159591a040391bd73e4c01f00fdfc4c47611d2`
with `pytest==8.4.2`. From this eval directory, download the pinned wheels and
build the base and P images:

```sh
python -m pip download --only-binary=:all: --dest vendor/wheels pytest==8.4.2 pluggy==1.7.0 iniconfig==2.3.1 packaging==26.3 pygments==2.21.0
python tools/build_eval_images.py --wheelhouse vendor/wheels
export SNAPPEA_EVAL_IMAGE="$(docker image inspect snappea-eval:py312-pytest842-base --format '{{.Id}}')"
export SNAPPEA_EVAL_P_IMAGE="$(docker image inspect snappea-eval:py312-pytest842-pysnappea-source-0.1.0 --format '{{.Id}}')"
```

The builder uses the local pytest wheelhouse at
`vendor/wheels` by default; set
`SNAPPEA_EVAL_WHEELHOUSE` or pass `--wheelhouse` to use another local copy. It
does not access the network. The P image derives from the shared base and adds
a pure-Python `pysnappea` 0.1.0 wheel made from `python/src/snappea` in this
checkout, including its pytest entry point. Neither image contains the checker.
The builder prints both immutable Docker image IDs; use the base ID for the
Python arm and the P image ID for the P arm. The source wheel hash is printed
as well, so metadata can identify the exact source package artifact.

Smoke-check the P image with the currently patched checker mounted from the
host. The smoke run has no network, confirms import and pytest plugin discovery,
and passes one valid and one expected-failing `check_spec` scenario:

```sh
python tools/smoke_eval_image.py --checker ../../target/release/snappea
```

For P runs, stage the checker and docs:

```sh
python tools/bootstrap.py
# Or select a local build and checkout:
python tools/bootstrap.py --binary ../../target/release/snappea --source ../..
```

The default downloads the checksum-verified Linux x86-64 v0.1.0 release and links
this checkout's docs/examples. Use `--source` to select a pinned checkout and record
its commit when comparing runs. Downloaded binaries and staged sources are ignored.
P provisioning checks that `snappea` resolves and executes on PATH and that
`import snappea` and the `pysnappea` distribution are available. The grader and
wide probe use each run's saved image so instrumentation imports still work. Solver
containers receive the mounted checker at `/opt/snappea`; the image
itself contains no checker binary. Keep solver containers network-disabled.

## Run independent agents

```sh
python tools/collaboration_run.py start --run-id luna-python-01 --arm python
python tools/collaboration_run.py start --run-id luna-p-01 --arm p
```

Using the collaboration tool, spawn each solver with `model="gpt-6-luna"`,
`reasoning_effort="high"`, `fork_turns="none"`, and its generated
`runs/<run-id>/delegation.txt` as the message. Do not pass this README, hidden tests,
the reference implementation, or other solver context to the solver. Only its
public workspace and (for the P arm) checker, docs, and examples are mounted.

After each solver's final response, freeze and grade:

```sh
python tools/collaboration_run.py finish --run-id luna-python-01 --seeds 64
python tools/collaboration_run.py finish --run-id luna-p-01 --seeds 64
python tools/summarize.py --prefix luna- --output results/summary.json
# Optional: regrade frozen submissions and run the separate exploratory probe.
python tools/regrade.py luna-python-01 luna-p-01 --seeds 64
python tools/stress_all.py --prefix luna- --output results/wide-stress.json
```

Never send hidden feedback back to a solver. Alternate arm launch order and use
identical task versions and budgets. The bridge allows 900 seconds from the first
command and 80 shell calls per solver. Token usage is not exposed by the
collaboration backend. Prompts, metadata, transcripts, frozen submissions, checksums,
and grades are generated under `runs/`; aggregates and calibration outputs go under
`results/`. Both directories are ignored, including newly generated submissions.

Containers use host UID/GID, no Linux capabilities, a read-only root, resource
limits, and no network. Agent instructions restrict commands to the bridge, but
do not revoke broader host-tool access. This is research isolation, not a
hostile-code security boundary. The grader assumes compliance with public store
and I/O restrictions.

## Compare the modeling skill

The current skill comparison uses the same P/Snappea task in both conditions.
The baseline prompt says: “Use P/Snappea to model the design, then implement it
in Python.” The treatment adds only: “Use the P modeling skill at
`/opt/p-modeling/SKILL.md`.” Both arms receive the same task, deliverables,
checker, docs, example, Python package, image, and bridge limits. The treatment
mounts the skill snapshot read-only at `/opt/p-modeling`.

Build the current checkout's release checker, stage its docs/example, and build
the pinned offline images before provisioning runs:

```sh
cargo build --release -p snappea
python tools/bootstrap.py --binary ../../target/release/snappea --source ../..
python -m pip download --only-binary=:all: --dest vendor/wheels pytest==8.4.2 pluggy==1.7.0 iniconfig==2.3.1 packaging==26.3 pygments==2.21.0
python tools/build_eval_images.py --wheelhouse vendor/wheels
export SNAPPEA_EVAL_IMAGE="$(docker image inspect snappea-eval:py312-pytest842-base --format '{{.Id}}')"
export SNAPPEA_EVAL_P_IMAGE="$(docker image inspect snappea-eval:py312-pytest842-pysnappea-source-0.1.0 --format '{{.Id}}')"
python tools/smoke_eval_image.py --checker ../../target/release/snappea
```

Freeze the authored skill and its examples before launching the cohort. The
snapshot is content-addressed, recorded with every treatment run, and mounted
read-only. Its source tree must contain `SKILL.md` and at least one file under
`examples/`:

```sh
python tools/snapshot_skill.py --source /path/to/p-modeling
```

Use the printed snapshot path for three baseline and three skill runs. Keep the
run IDs unique; alternate launch order and use fresh Luna high agents with
`fork_turns="none"` and each generated `runs/<run-id>/delegation.txt`:

```sh
python tools/collaboration_run.py start --run-id skill-base-01 --arm p --condition baseline
python tools/collaboration_run.py start --run-id skill-treatment-01 --arm p --condition skill --skill-snapshot /path/printed/above
```

The new conditions retain the existing 900-second and 80-call limits and use a
120-second per-command cap. The runner checks the installed checker, Python
package, pytest, and (for treatment) the read-only skill mount before the
delegation is ready. Finish each solver with the usual `finish --run-id ...`
command and summarize the two cohorts separately. Existing prompt variants and
saved historical run records remain intact.

The collaboration runner accepts `--model gpt-6.1-terra --reasoning-effort medium` to record the delegated configuration; match those settings when spawning the solver. New trials archive public/private/tooling inputs, and grading verifies and uses those frozen inputs. See [Terra broader-task results](../modeling-skill/phases/TERRA-OPEN-RESULTS.md).

## Interpret measurements

The primary measure is submissions passing all twenty primary cases. The
independent unit is a solver run; cases and schedule seeds are not independent
samples. Keep exploratory wide-input probes separate from the primary score.
Inspect P model relevance, monitor reachability, and requirements covered: a
successful checker exit alone does not establish coverage or implementation fidelity.
Record prompt, harness, checker, docs, and bridge versions for each comparison.

## Optional Sonnet backend

Configure `ANTHROPIC_API_KEY` through normal secret configuration and choose an
available exact model ID:

```sh
python tools/run_sonnet.py --run-id sonnet-python-01 --arm python --model claude-sonnet-4-6
python tools/run_sonnet.py --run-id sonnet-p-01 --arm p --model claude-sonnet-4-6
```

`python tools/test_runner_protocol.py` exercises the API loop, submission freezing,
and container grading with scripted responses and no live API call. Its outputs
are local smoke artifacts, excluded from agent comparisons.

## Compare checker output with one extra sentence

Prompt version 3 shares “Design and implement the service in Python.” The P arm
adds “First, model it with P/Snappea and check what could go wrong.” Tool paths
are supplied separately; there is no extra testing advice for either arm.

The completed output-comparison pilot used version 3. The next completed cohort used version 5:
“Write a design and a P/Snappea model. Check and refine the model until you're happy
with it, then implement it in Python.” The control says “Write a design. Then
implement it in Python.” The P prompt adds “Use pysnappea in Python tests.” and
the control adds “Add Python tests.” Shared deliverables appear after the task
instruction, so the prompt no longer leads with writing the Python file. Version
4 was not evaluated. Version 5 added Python tests and a shared pytest image, with
pysnappea available only to the P arm; keep this cohort separate from version 3.
Version 6 selected this wording: “First write and check a P/Snappea model of the
design. Only start the Python implementation after the model passes. Use pysnappea
in Python tests.” It was selected after a small single-prompt wording screen;
two runs per wording do not establish a reliable general ranking.
Version 7 changes only its final sentence to “Use pysnappea to check events from
the Python implementation during tests.” The library and its docstrings were
held unchanged for that prompt experiment.

To compare old and new output, use three independent cohorts: Python-only, P
with the baseline binary, and P with the modified binary. Keep both P cohorts'
docs/examples identical. Provision each with explicit binary/source overrides:

```sh
SNAPPEA_EVAL_BINARY=/absolute/path/to/baseline/snappea SNAPPEA_EVAL_SOURCE=/absolute/path/to/shared-source python tools/collaboration_run.py start --run-id pilot-base-01 --arm p
SNAPPEA_EVAL_BINARY=/absolute/path/to/modified/snappea SNAPPEA_EVAL_SOURCE=/absolute/path/to/shared-source python tools/collaboration_run.py start --run-id pilot-feedback-01 --arm p
python tools/collaboration_run.py start --run-id pilot-python-01 --arm python
```

Use fresh Luna high agents with generated delegations as above. Binary hashes
and docs Git revision are recorded in each P run's metadata. Freeze/grade before
inspecting hidden results. Summarize each cohort separately with `--prefix`;
inspect first model exploration versus first implementation write and whether
coverage feedback causes scenario changes. Handler coverage and distinct abstract
timelines measure the P model's exploration, not Python correctness or requirement
coverage. Preserve negative results and keep wide-input probes separate.

## Screen single-prompt wording

`SNAPPEA_EVAL_PROMPT_VARIANT` selects an experimental P-only prompt, recorded as
the current prompt version. Variants are `design_process`, `before_file`, `passing_first`, and `implementation_events` (the current default):

```sh
SNAPPEA_EVAL_PROMPT_VARIANT=design_process python tools/collaboration_run.py start --run-id wording-design-01 --arm p
```

Each agent receives a single prompt and works to completion. No phase gate blocks
Python writes. Save the exact generated delegation, then inspect whether a P model
actually passes before the first implementation write. Distinguish a passing
synthetic event trace from a P test case exploring schedules, and both from tests
that check actual service behavior. Use a few independent runs per wording for a
screen; do not claim a reliable ranking from two samples per variant.
