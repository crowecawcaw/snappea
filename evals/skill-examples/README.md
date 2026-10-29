# Example quality and Luna/high trials

Compare no skill, the old skill, and the reviewed revised skill on the same FIFO
and VCS tasks. [Results](RESULTS.md) and [verified records](results.json).
Two fresh agents per task/condition; twelve trials, no prethought
prompt. [Protocol](PROTOCOL.md); [independent example review](EXAMPLE-REVIEW.md).

The revised skill has a meaningful runnable syntax starter, bounded concurrent
histories, stable-ID CAS/retry clients, actual reply/state assertions, nonzero/hot
completion, broken controls and real Python capture. It states the atomicity,
ordering and finite workload assumptions in comments and README boundaries.

Before provisioning, create content-addressed old/revised snapshots with
`evals/certified-bundles/tools/skill_snapshot.py`, and put their absolute paths in
`runs/snapshots.json` under keys `old` and `revised`. The old snapshot is from
commit eb1f99176cc4bf85667d33720917f3fe6819a543. Snapshot code excludes generated
checker output/caches. Pinned binary/image requirements are in the protocol.

```sh
python skills/p-modeling/validate.py --binary target/driver-eval/release/snappea
python evals/skill-examples/experiment.py start
```

This freezes twelve prompts and matching inputs before delegation. Skill containers
are provisioned from the same base harness plus a read-only mount; no solver calls
occur during setup. Delegate each saved prompt to a fresh `gpt-6-luna` / high agent
with no inherited context. No private graders or other runs reach solvers.

After each agent stops:

```sh
python evals/skill-examples/experiment.py finish --run-id ID
```

Keep failed/incomplete runs. Source-audit submissions before assignment/results;
then inspect complete transcripts for reference uptake, actual test runs and timing.
After all twelve are frozen:

```sh
python evals/skill-examples/analyze.py verify
python evals/skill-examples/analyze.py supplement
python evals/skill-examples/analyze.py summarize
```

Supplemental checks reuse the prior validated FIFO source-assembly correction
(legal internal event additions) and existing VCS wide-bundle stress, uniformly.
Original grades remain untouched. A container helper's raw status is a unittest
classifier; model verdicts use checker JSON, selected tests/search completion and
exit code. Reference-incompatible saved tests cannot receive portable fault scores.

Raw runs, both exact snapshots, original/corrected results, complete commands,
review hashes, the pinned binary and mounted references should be archived intact.
Do not infer absence of a skill benefit from tied primary scores or infer a causal
effect from two runs per condition/task.

The original OOM grading failure is retained without a numeric score. Supplemental wide probes uniformly use the bounded wrapper; see the protocol execution note.
