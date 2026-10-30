# Minimal-prompt eval

Does a short prompt ("use P to model the design before implementing it, and to test the
implementation") lead an agent to a good model, a correct implementation and strong tests?
This eval scores all three automatically, so a change to the skill, the CLI or the docs
can be measured without hand audits.

## Tasks

Each task is a small concurrent component over a store with only atomic load and
compare-and-swap. `TASK.md` states the product requirements; it does not list the
tricky consequences (retry after the state changed, a lost compare-and-swap, boundary
times). Finding those is the modeling work.

| Task | Component | Hidden tests | Mutants |
| --- | --- | --- | --- |
| `queue` | bounded FIFO queue with idempotent retries | 12 | 11 |
| `fencing` | lease lock with fencing tokens; per-key store, no transactions | 15 | 13 |

`tasks/<task>/public/` is all the solver sees. `private/` holds a reference
implementation, hidden tests and realistic faulty variants (mutants).
`python harness.py calibrate <task>` checks that the reference passes every hidden test
and every mutant fails at least one.

## Scores

`python harness.py grade <run>` writes `runs/<run>/grade.json`:

- **hidden**: hidden tests passed by the agent's `solution.py`. Race tests force two
  calls to read the same version, or inject a competing call between read and write,
  so they are deterministic.
- **killed**: mutants caught by the agent's saved tests. Each agent test is first run
  twice against the reference; tests that fail there (they assert on the agent's own
  internals, or are flaky) are dropped. A mutant is killed if a remaining test fails.
  This measures the tests, independent of whether the agent's own code is right.
- **obs-kill**: mutants killed by test files that import `snappea` (real calls checked
  against a P spec).
- **model check**: `snappea check` on the submitted P files (status, test cases,
  complete search), plus counts of spec monitors and hot states.
- **process**: from the command transcript: whether the first `snappea check` came
  before the first write of `solution.py`, number of checks, and bug-finding checks.

Limits: tests that need `emit` calls inside the agent's own `solution.py` cannot run
against the reference, so they are dropped from the mutation score. Mutants and hidden
tests are written by the eval author and cover the bugs they thought of.

## Running a trial

Needs Docker, a release build and the image:

```sh
cargo build --release -p snappea --bin snappea
cp -r python/src/snappea evals/minimal-prompt/image/snappea
pip download --only-binary=:all: --python-version 3.12 --dest evals/minimal-prompt/image/wheels pytest==8.4.2
docker build -t snappea-min-eval evals/minimal-prompt/image
```

Then, from `evals/minimal-prompt`:

```sh
python harness.py start --task queue --run-id queue-v2-01 --skill ../../skills/p-modeling --model sonnet
# give the printed prompt, unchanged, to a fresh agent; its only tool is `harness.py bridge`
python harness.py finish queue-v2-01
python harness.py grade queue-v2-01
python harness.py summary
```

The solver runs in an offline container with the task's public files in `/work`,
`snappea` on PATH, `pysnappea` installed and the skill (if any) read-only at
`/opt/skill`. Budget: 60 commands, 40 minutes. `runs/<run>/` keeps the prompt,
the full command transcript, the frozen submission and the grade.
