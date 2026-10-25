# Phased modeling-skill evals

Separate capabilities before testing an end-to-end flow:

| Phase | Given | Agent delivers | Independent checks |
|---|---|---|---|
| [Model](modeling/public/TASK.md) | Fenced-lease contract, read/CAS primitives, event schema | P algorithm, contract monitor, scenarios, design note | Bounded check; 5 valid / 13 invalid monitor histories; source audit of races and abstraction |
| [Tests](testing/public/TASK.md) | Python lease implementation, fixed monitor | Saved observation tests | Correct implementation and valid allocation variant accepted; 9 seeded faults rejected; actual-call provenance audit |
| [Combined](flow/public/TASK.md) | Different contract: shared bounded FIFO, read/CAS storage | Model, implementation, saved observation tests | 8 implementation cases; 4 valid / 4 invalid monitor histories; saved tests against submitted implementation; optional portable reference/fault probes; source/chronology audit |

- Start with model/test phases. Fix harness defects before interpreting results; change skill content only for general modeling/testing lessons.
- Promote to a combined pilot once artifacts demonstrate the isolated capabilities; retain individual failures and residual gaps. Passing selected checks is not a correctness proof or a reliability estimate.
- Model checking and monitor quality are separate measurements. A monitor can pass every hidden history while the modeled algorithm omits the actual race.
- Test validity and defect detection are separate measurements. A test rejecting a permitted implementation is brittle even if it rejects every faulty variant.
- Coverage/schedule/test counts are diagnostics, not quality scores. Source review checks independent expectations, real boundaries, meaningful shared state, and observation provenance.
- Keep adaptive revisions separate. These development pilots have no causal skill-effect claim; `baseline` is available for a later matched comparison.

## Novel transfer checks

[Schema migration and streaming windows](NOVEL.md) add two different domains, with separate model/test tasks and an unchanged skill. See [cold results](NOVEL_RESULTS.md).

## Protocol

- Fresh `gpt-6-luna` agents, high thinking, `fork_turns=none`.
- Each gets only the saved prompt and bridge instructions. Skill condition adds one sentence: `Use the P modeling skill at /opt/p-modeling/SKILL.md.`
- Offline disposable container; pinned checker/image, read-only skill snapshot; no grader/other-run mounts.
- Model/test: 600 seconds from first bridge call, 50 calls. Combined: 900 seconds, 80 calls. Each command: 90 seconds. No coaching or hidden feedback during a run.
- Freeze submissions before grading. Preserve public inputs, prompts, command transcripts, manifests, skill hashes, and grader hashes. Newer trials also archive the grader source; initial test-v1 trials recorded its hash but not a source copy.
- Source audits are independent of hidden scores, with any supplied grade feedback disclosed. Report setup errors/timeouts separately; neither counts as rejecting a mutant. Zero discovered tests earn no credit.
- Model rechecks use 5 seconds per case and 5 seconds for mutation feedback; distinguish search completion from mutation-budget completion.

## Run

Build the checker and the existing P eval image (see [image setup](../../certified-bundles/README.md)). Then:

```sh
python evals/modeling-skill/phases/runner.py start --phase modeling --run-id my-model-01
cat evals/modeling-skill/phases/runs/my-model-01/prompt.txt
```

Delegate that exact prompt to a fresh Luna/high agent. It must use `bridge.py`; do not give it host/private access. After the agent finishes:

```sh
python evals/modeling-skill/phases/runner.py finish --run-id my-model-01
python evals/modeling-skill/phases/grade.py --run-id my-model-01
```

- Other phases: `--phase testing` or `--phase flow`.
- Optional matched control: `--condition baseline`; same task/tools, no skill sentence/mount.
- Results and raw evidence: ignored `runs/<id>/`; concise results: [RESULTS.md](RESULTS.md).
- `grade.py` rejects changed submissions, checker binaries, public inputs, or graders. Original archived graders are used where available. Changed-grader rechecks must be explicitly supplemental, never overwrite original grades.

## Validate the eval

```sh
python -m unittest discover -s evals/modeling-skill/phases -p test_protocol.py -v
python evals/modeling-skill/phases/calibrate_model.py --binary target/release/snappea
python evals/modeling-skill/phases/testing/private/calibration/calibrate.py
python evals/modeling-skill/phases/flow/private/calibrate.py --binary target/release/snappea
python skills/p-modeling/validate.py --binary target/release/snappea
```

Calibration suites exercise known-correct and known-bad controls; they never enter solver containers. Inspect implementation, monitor, observation, and audit results separately.
