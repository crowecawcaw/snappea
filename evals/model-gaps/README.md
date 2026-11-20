---
diataxis: how-to
personas: [contributor]
questions:
  - How do I run the model-gaps eval for one agent and one arm?
  - How is a run graded without an LLM judge?
  - Which task separates snappea from the plain `p` command line, and why?
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

## Tasks

Each task is one of `examples/samples/use-cases`, reshaped.

| Task | Modes | Design size | Hidden tests | Seeded bugs |
| --- | --- | --- | --- | --- |
| `job-queue` | review, build | 2 machines | 4 | 4 |
| `consumer-group` | review, build | 2 machines | 6 | 3 |
| `seat-hold` | review | 2 machines | 4 | 3 |
| `leader-election` | build | 1 machine, crash and restart | 12 | none |

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
```

`start --binary PATH` pins a binary, so you can rebuild while runs are in progress or
compare two snappea builds. `--label` names the condition in the report. A solver gets
60 commands, 40 minutes and 180 seconds per command. Runs land in `runs/`, which is
not committed; `runs/<id>/snapshots/` holds the model after each command that changed
it. Put a file named `EXCLUDE` in a run directory to keep it out of the report.

## Results so far

Solvers were Haiku subagents. Counts are small; read them as a pilot.

### Build, `leader-election`: the arms differ

RESULTS_LE

Every failing compat run failed the same four hidden tests, the ones that crash a node
and require it to win a fresh term (`LeaderAfterRestart`). The public driver's one test
does not select that spec. Compat solvers wrote crash tests but left the spec
unselected, and said so in their notes ("LeaderAfterRestart has no test and was not
run"). Their designs either announced `Restarted` and also sent it, so specs saw it
twice, or only announced it, so the driver never heard.

Every snappea solver hit the lint error `unselected-spec` ("spec 'LeaderAfterRestart'
is selected by no model test") on its first clean run, and four of six also hit
`announce-and-send` on `Restarted`. Both block the check, so the solver
wrote the recovery test and fixed the design. Coverage gaps then led to tests for a
client probe and a durable start. The cost was about three more minutes per run.

### Build and review on the smaller tasks: no difference

On `job-queue` and `consumer-group` build, both arms passed every hidden test (one run
per cell). In review mode on all three tasks, both arms found 10 of 10 seeded bugs with
no false alarms; compat used 15 commands per run and snappea 18.

These designs are under 200 lines with one bug per handler. The solver finds the bugs
by reading, and the checker only confirms them. A task like this cannot separate the
arms, whatever the tool does.

## What makes a task discriminate

The one task with headroom has these properties. Keep them when adding tasks.

- **The gap is an omission, not a wrong line.** The solver must notice something that
  is missing: a spec nobody selected, a scenario no driver produces. Reading the code
  does not show it.
- **The starter model exercises part of the contract.** One test, several specs. A
  solver that checks only what it thought of stays green.
- **The design has more scenarios than a solver enumerates unprompted.** Crash and
  restart, superseded work, stale messages.
- **Hidden tests cover what the starter does not.**

Seeded-bug review on small designs has none of these.

## What the transcripts say about snappea's output

- Lint errors that block the check (`unselected-spec`, `unchecked-event`) changed
  outcomes. Coverage gaps named useful missing scenarios.
- Solvers piped every check through `grep -v` to drop source snippets, repeated help
  text, `Driver limits` blocks and their hint, in every check of every run. The
  warning `payload-field-checked-by-other-spec` on the given spec, which the task
  forbids changing, was printed 2 to 11 times per run. The help
  for a lint rule is now printed once per report, and the driver-limits hint once per
  run.
- Undetected-mutant lists were long and mostly resets that assign the value already
  there. Solvers read them, changed little, and reported the count. This is the
  clearest low-value output; hiding "never changes a value" mutants by default is
  worth testing with `--binary` and `--label`.
- A lint failure skips coverage and mutation testing, so a solver learns about gaps
  one round later than it could.

## Limits

- Six runs per arm on one task. The effect comes from one spec; a second task with a
  different omission is needed before generalizing.
- The compat arm is snappea's engine behind upstream's command line, not upstream P.
- The solver image has no `python3`; solvers that reach for it lose a command.
- Hidden tests run for 10 seconds each with a fixed seed. A flaw that needs a deeper
  search can pass.
