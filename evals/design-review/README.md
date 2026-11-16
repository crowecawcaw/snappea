---
diataxis: how-to
personas: [contributor]
questions:
  - How do I measure whether snappea's tooling helps an agent find design bugs?
  - How do I run, grade and summarize a design-review eval run?
  - What counts as finding a known bug?
---

# Design-review eval

Measures whether an agent using P finds more known design bugs in real code with the
`snappea` CLI than with `snappea-compat` (upstream P's command line). Each run gives a
fresh agent a historical snapshot of a real subsystem and asks it to model the design in
P and report design bugs. A blind grader matches the report against an answer key of
bugs that upstream later fixed or documented.

## Tasks

| Task | Source | Known issues |
| --- | --- | --- |
| `minio-dsync` | MinIO distributed locking at `10fd53d6` (2020-03-14), 9 files, ~1,750 lines | 5 |
| `pg-logrep-sync` | PostgreSQL logical replication workers at `b38006ef` (2017-04-04, pre-release v10), 7 files, ~4,500 lines | 8 |

`minio-dsync`'s answer key (`tasks/minio-dsync/private/answer-key.md`) has mutual-exclusion
violations from stale-lock maintenance (K1) and lock-server restarts (K4), and
liveness failures from the maintenance lock (K2), fire-and-forget unlock (K3) and
expiry that never checks the owner (K5).

`pg-logrep-sync`'s key has worker-slot races and leaks (K1, K2, K6), orphaned and
resurrected sync state (K3, K4), a handover that skips changes (K5), a crash that
replays a transaction (K7) and an undetected DDL deadlock (K8). All were fixed during
the v10 beta in 2017. Its TASK.md states three facts about files not included.

Each key also lists non-issues and valid extras so graders agree. The solver gets the
source without Git history or tests. `task.json` names the subject the grader sees.

## Arms

The task and prompt are identical. Only the checker on PATH differs:

- `snappea`: `snappea` from `target/release` (guide, lint, hints, JSON output).
- `compat`: `snappea-compat` mounted as `p` (upstream command line and messages).

Neither arm gets other documentation; the compat arm relies on the model's own P
knowledge, as an upstream user without docs would.

## Run

Needs Docker (`dockerd` may need starting in cloud sessions) and a release build:

```sh
cargo build --release -p snappea --bins
cd evals/design-review
python harness.py prepare minio-dsync
python harness.py start --task minio-dsync --arm snappea --run-id i2-snappea-01 --model haiku
```

Give the printed prompt, unchanged, to a fresh agent (for example a Haiku subagent).
Its only tool is `harness.py bridge`, which runs commands in an offline container with
`/work/src`, `/work/TASK.md` and an empty `/work/model`. Limits: 80 commands, 45
minutes, 180 seconds per command. After it replies:

```sh
python harness.py finish i2-snappea-01 --reply "<final reply>"
python audit.py i2-snappea-01 <subagent transcript .jsonl>   # calls outside the bridge
python harness.py grade-prompt i2-snappea-01 > runs/i2-snappea-01/grade-prompt.txt
# A grader agent (not the solver) reads only grade-prompt.txt and returns JSON.
python harness.py record-grade i2-snappea-01 runs/i2-snappea-01/grade-raw.json
python report.py i2-
```

`grade-prompt` masks checker names in the report. `finish` reruns the frozen model
with the arm's checker in a fresh container (`final_check`) and counts checker runs
in the transcript that reported a bug. Put an `EXCLUDE` file in a run that should not
count, for example after an out-of-bridge command that read host files.

## Scores

- **found**: known issues whose mechanism and consequence the report explains (0-5 or 0-8).
- **checker_confirmed**: found or partial issues the report says a P counterexample
  showed. This is the measure most tied to the tooling; code reading alone can find
  issues in either arm.
- **other valid / invalid**: claims outside the key. Invalid claims count against a
  report.

## Limits

- One snapshot and one author's key. Code reading alone finds many of the issues, so
  separate `found` from `checker_confirmed`.
- Three of the five issues are liveness failures. A model with no liveness spec cannot
  confirm them.
- Runs vary a lot; compare several runs per arm.
- The answer key is in the repository. The solver is told to use only the bridge;
  check `audit.json`.
