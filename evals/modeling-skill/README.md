# Modeling skill evaluation

The skill lives at `skills/p-modeling/SKILL.md`. It is a portable directory: keep its guides, reference, and examples together when installing it into an agent's skill directory, or tell the agent to read its SKILL.md directly. It introduces no CLI commands.

## Reproduce the content checks

Install the repository's Python package and build the current checker, then run:

```sh
cargo build --release -p snappea
python -m pip install ./python
python skills/p-modeling/validate.py --binary target/release/snappea
python -m unittest discover -s evals/certified-bundles/tools -p test_skill_snapshot.py
```

The content validator checks local links, correct P models, documented negative controls, and Python observation against actual implementation calls. It uses temporary copies so checks do not modify an installed skill.

## Pilot comparison

Use the certified-bundles harness (see its README) with fresh Luna/high collaboration agents, `fork_turns=none`, and no host/private access. Conditions share the P checker, Python package/image, ordinary docs, public task, 900-second/80-call budget, and 120-second command cap.

Baseline task prompt:

```text
Read TASK.md and contract.py.

Use P/Snappea to model the design, then implement it in Python.

Deliver solution.py and DESIGN.md.
```

Treatment adds only:

```text
Use the P modeling skill at /opt/p-modeling/SKILL.md.
```

Neutral isolation/bridge instructions are identical except run ID. Treatment alone gets a read-only skill snapshot; control does not see it. Do not supply extra modeling/testing tips to solvers. Freeze each submission before hidden grading. Preserve manifests, command transcripts, exact prompts, image IDs, binary hashes, and skill snapshot hashes. A changed skill requires a new snapshot and fresh run IDs; do not pool revisions.

Primary outcome: frozen submission's existing 20 hidden contract cases (64 scheduler seeds). Report full-pass submissions and total cases separately. The existing wide-input probe is supplemental. Tool use, observation provenance, source/model correspondence, and coverage/mutation responses are independent audit findings, not substitutes for implementation correctness. Timeouts or unfinished valid submissions remain in the trial denominator; distinguish infrastructure-invalid trials. Three runs per condition are a pilot, not a reliable estimate of effect size.

## Review before authoring

A Luna/high reviewer assessed PLAN.md before guide/example authoring. Feedback incorporated: novice first-session path, justified atomic boundaries, bounded-model limits, explicit progress assumptions, event provenance and observation limitations, portable exact commands, and contemporaneous controls.

During content review, source inspection corrected a mistaken reviewer suggestion that all P handlers are atomic. Snappea yields at scheduling operations including send, receive and machine creation; local non-yielding updates can represent a real atomic primitive. The publication example was strengthened to recompute candidate data after conflict, rather than merely updating an evidence-version label. Local links and Python source-provenance checks were made portable.
