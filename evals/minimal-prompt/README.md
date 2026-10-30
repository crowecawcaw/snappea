# Minimal-prompt eval

Measures whether an agent given a short prompt ("use P to model the design before
implementing it, and to test the implementation") produces a correct implementation
and tests that catch real bugs. Use it to compare CLI, guide or prompt changes: run
fresh agents on each condition, then compare the scores.

## Tasks

Each task is a small concurrent component over a store with only atomic load and
compare-and-swap. `TASK.md` states the product requirements; it does not list the
tricky consequences (a retry after the state changed, a lost compare-and-swap, values
at a boundary). Finding those is the modeling work.

| Task | Component | Hidden tests | Defects |
| --- | --- | --- | --- |
| `queue` | bounded FIFO queue with idempotent retries | 12 | 11 |
| `fencing` | lease lock with fencing tokens; per-key store, no transactions | 14 | 13 |
| `names` | unique usernames with rename; claim and release across keys | 10 | 11 |

`tasks/<task>/public/` is all the solver sees. `private/` holds a reference
implementation, hidden tests and named defects (`mutants.py`).
`python harness.py calibrate <task>` checks that the reference passes every hidden test
and that every defect fails at least one.

## Running a cohort

Needs Docker and a release build. From the repository root:

```sh
cargo build --release -p snappea --bin snappea
evals/minimal-prompt/image/build.sh            # image with pytest and pysnappea
mkdir -p evals/minimal-prompt/bin && cp target/release/snappea evals/minimal-prompt/bin/snappea-a
export SNAPPEA_EVAL_BINARY=$PWD/evals/minimal-prompt/bin/snappea-a   # pin one binary per cohort
```

Then, from `evals/minimal-prompt`, for each run:

```sh
python harness.py start --task fencing --run-id a-fencing-01 --model sonnet
# Give the printed prompt, unchanged, to a fresh agent with no other context.
# Its only tool is `harness.py bridge`, which runs commands in an offline container.
python harness.py finish a-fencing-01
python harness.py grade a-fencing-01
python harness.py transplant a-fencing-01        # prints a prompt for a helper agent
python harness.py grade_transplant a-fencing-01  # after the helper has written the mutants
python report.py a-                              # scores by condition
```

`--skill DIR` mounts a skill read-only at `/opt/skill` and adds one line to the prompt,
for comparing a skill against the CLI alone. `autograde.sh` and `autotransplant.sh`
grade frozen runs in the background.

The solver gets 60 commands and 40 minutes in an offline container with the task's
public files in `/work`, `snappea` on PATH and `pysnappea` installed. `runs/<run>/`
(ignored by git) keeps the prompt, the full command transcript, the frozen submission
and the grade.

## Scores

- **Hidden tests**: the agent's `solution.py` against deterministic race tests. They
  force two calls to read the same version, or run a competing call between a read and
  its compare-and-swap.
- **Transplanted defects** (the main test score): a helper agent writes each named
  defect into the agent's own `solution.py`, keeping its instrumentation. A transplant
  counts only if it fails some hidden tests and passes others; the agent's own test
  suite then runs against it. `race_kill` is the share of defects that only show under
  an interleaving.
- **Reference defects**: the same defects as edits to the private reference. Only agent
  tests that pass on the reference count, so tests tied to the agent's internals drop
  out; this undercounts suites built on the agent's own `emit` calls.
- **Own mutants**: generic AST mutations of the agent's code (`ast_mutants.py`).
- **Process**: from the transcript: whether the first `snappea check` came before the
  first write of `solution.py`, whether the last check skipped mutation or coverage,
  and whether the tests force interleavings or check real calls against a spec.

## Limits

- The eval author wrote the tasks, hidden tests and defects; they cover the bugs the
  author thought of. Strong solvers reach the ceiling on all three tasks.
- Use several runs per task and condition; single runs vary a lot.
- Solvers sometimes run a first command on the host by mistake. Check transcripts, and
  put an `EXCLUDE` file (saying why) in runs that should not count.
