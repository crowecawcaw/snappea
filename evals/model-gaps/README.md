---
diataxis: how-to
personas: [contributor]
questions:
  - How do I run the model-gaps eval for one agent and one arm?
  - How is a run graded without an LLM judge?
  - How do I compare two snappea builds?
  - How do I add a task that has headroom?
---

# Model-gaps eval

Does snappea's analysis (lint, coverage, mutation testing, driver limits) change what an
agent ends up with, compared with the same checker behind the upstream `p` command
line? Both arms get the same sandbox, task and prompt. Only the checker on `PATH`
differs: `snappea`, or `snappea-compat` installed as `p`.

Grading is mechanical. The harness runs the checker itself; no judge reads the answer.

## Modes

**Build** (`--mode build`). The solver gets `DESIGN.md`, the full specs, a small driver
with one test, and empty machines. It writes the design in `PSrc/`. The grade is how
many hidden reference tests its design passes with the given specs.

**Review** (`--mode review`, the default). The solver gets a finished design that
passes its tests but carries seeded bugs from `private/bugs.toml`. It adds specs and
tests. A bug counts as found when one of the solver's tests passes on the fixed design
and fails on the design with only that bug. A test that fails on the fixed design is a
false alarm.

**Guard** (`--mode guard`). The solver gets the correct design and must strengthen
`PSpec/` and `PTst/`. The grade is how many hidden design mistakes (the task's seeded
bugs, one at a time) its tests catch. The solver cannot find them by reading, because
they are not there. Any task with seeded bugs can run in this mode.

**Own** (`--mode own`). Build and guard in one task: the solver writes the design from
empty machines and is told to leave specs and tests that protect it. Two grades: the
hidden reference tests its design passes, and the hidden design mistakes its specs and
tests catch on the reference design. Any task with a skeleton and seeded bugs can run
in this mode.

**Harden** (`--mode harden`). The solver gets a design with every seeded bug and fixes
`PSrc/`. Hidden reference tests judge the result.

**Impl** (`--mode impl`). The task brings its own `/work` and grader: the solver writes
application code and can check its event logs against the model with `observe`.

## Tasks

Each task is one of `examples/samples/use-cases`, reshaped.

| Task | Modes | Design size | Hidden tests | Seeded bugs |
| --- | --- | --- | --- | --- |
| `job-queue` | review, build | 2 machines | 4 | 4 |
| `consumer-group` | review, build | 2 machines | 6 | 3 |
| `seat-hold` | review | 2 machines | 4 | 3 |
| `leader-election` | build | 1 machine, crash and restart | 12 | none |
| `online-migration` | build, guard | 4 machines, dual write and cutover | 9 | 6 |
| `offline-sync` | build, guard | server and client, retries and late replies; public spec incomplete | 11 | 6 |
| `circuit-breaker` | build | 1 machine; no specs or drivers given | 5 | 6 |
| `token-refresh` | harden, guard | 2 machines, lock and rotation | 11 | 5 |
| `worker-drain` | build, guard | 3 machines, step timer, repeated and late answers; public spec incomplete | 12 | 6 |
| `idempotent-payments` | build, guard | key table and API server; processor timeouts, crash recovery; public spec incomplete | 12 | 6 |
| `quorum-kv` | build, guard | replica and coordinator; retries, late answers, coordinator crash; public spec incomplete | 12 | 6 |
| `lease-impl` | impl | Python lease lock checked with `observe` | 19 scenarios | 9 variants |

## Run it

Docker must be running and the release binaries built
(`cargo build --release -p snappea --bins`).

```sh
cd evals/model-gaps
python3 harness.py calibrate                 # reference tests pass and catch every bug
python3 harness.py start --mode build --task leader-election --arm snappea \
    --run-id le-snappea-01 --model haiku     # prints the solver prompt
# give the prompt to the agent; its only tool is `harness.py bridge <run>`
python3 harness.py finish le-snappea-01
python3 harness.py grade le-snappea-01
python3 report.py le-
python3 trajectory.py le-snappea-01          # when the design stopped improving
```

`start --binary PATH` pins a binary, so you can rebuild while runs are in progress or
compare two snappea builds. `--label` names the condition in the report.
`--check-flags="--no-mutation"` adds flags to every `check` the solver runs.
`./start.sh PREFIX TASK MODE N [LABEL BINARY [CHECK_FLAGS]]` starts N runs per arm, or
N runs of one variant. Without a label it reads the two arms' binaries from
`target/variants/snappea-base` and `target/variants/compat-base`; copy
`target/release/snappea` and `target/release/snappea-compat` there first, so a rebuild
does not change runs in progress. `python3 modelscore.py PREFIX` scores the specs and tests of any
finished run against the task's hidden mistakes, whatever its mode. A solver gets
40 commands, 20 minutes and 120 seconds per command (the first leader-election runs
had 60, 40 and 180). Runs land in `runs/`, which is
not committed; `runs/<id>/snapshots/` holds the model after each command that changed
it. Put a file named `EXCLUDE` in a run directory to keep it out of the report.

## What makes a task discriminate

The tasks that separated the arms (see [results](../../docs/evals.md#model-gaps))
share these properties. Keep them when adding tasks.

- **The gap is an omission, not a wrong line.** The solver must notice something that
  is missing: a spec nobody selected, a scenario no driver produces. Reading the code
  does not show it.
- **The starter model exercises part of the contract.** One test, several specs. A
  solver that checks only what it thought of stays green.
- **The design has more scenarios than a solver enumerates unprompted.** Crash and
  restart, superseded work, stale messages.
- **Hidden tests cover what the starter does not.**

- **The grade measures the model, not only the design.** Guard mode on a task whose
  public spec is incomplete separated the arms; build mode on the same task did not.

Seeded-bug review on small designs has none of these. Neither does a task whose
DESIGN.md spells out every rule and whose starter specs already check them: the solver
gets the design right by reading, and its own tests then cover the rest.

## Limits

- Runs vary; use at least three per condition and read small differences as noise.
- A stronger solver model would likely close the gaps that remain; a weaker one might
  open gaps on the tied tasks. Only Haiku was tested.
- The compat arm is snappea's engine behind upstream's command line, not upstream P.
- The solver image has no `python3`; solvers that reach for it lose a command.
- Hidden tests run for 10 seconds each with a fixed seed. A flaw that needs a deeper
  search can pass.

## Results

Run output stays in `runs/`, which is not committed. Short results, with the commit
and date of each run, are in [evals](../../docs/evals.md#model-gaps).
