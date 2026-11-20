---
diataxis: how-to
personas: [contributor]
questions:
  - How do I run the model-gaps eval for one agent and one arm?
  - How is a run graded without an LLM judge?
  - Which tasks separate snappea from the plain `p` command line, and why?
  - Which snappea output changes were tried, and what did they do?
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
N runs of one variant. `python3 modelscore.py PREFIX` scores the specs and tests of any
finished run against the task's hidden mistakes, whatever its mode. A solver gets
40 commands, 20 minutes and 120 seconds per command (the first leader-election runs
had 60, 40 and 180). Runs land in `runs/`, which is
not committed; `runs/<id>/snapshots/` holds the model after each command that changed
it. Put a file named `EXCLUDE` in a run directory to keep it out of the report.

## Results so far

Solvers were Haiku subagents. Counts are small; read them as a pilot.

### Build, `leader-election`: the arms differ

| Arm | Runs | Runs passing all 12 hidden tests | Hidden tests passed | Commands per run | Minutes per run |
| --- | --- | --- | --- | --- | --- |
| `snappea` | 6 | 6 | 72/72 | 14 | 10.0 |
| `compat` (`p`) | 6 | 2 | 56/72 | 16 | 4.9 |

Fisher's exact test on runs passing everything gives p = 0.03, one-sided.

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
client probe and a durable start.

`trajectory.py` grades the design after each command. In four snappea runs the first
design that compiled scored 8 of 12, the same as the failing compat runs, and the next
edit, made in answer to the lint errors, scored 12. The other two scored 12 at first
write. Every snappea design was final by minute 4.4. The runs then went on for 1.5 to
17.5 more minutes (median 4.4) adding tests, closing coverage gaps and reading
undetected mutants, and none of that changed the hidden score. So the eval shows both
things: one snappea finding closes a real gap, and most of a snappea run is spent
after the design has stopped improving.

### Guard, `offline-sync`: the arms differ

Three runs per condition; each run can catch 6 hidden mistakes.

| Condition | Mistakes caught | Per run | Minutes per run |
| --- | --- | --- | --- |
| `compat` (`p`) | 9/18 | 3, 2, 4 | 9.8 |
| `snappea` | 14/18 | 5, 4, 5 | 7.0 |
| `snappea`, focus build | 14/18 | 5, 5, 4 | 4.7 |
| `snappea`, lean (`--no-mutation --no-driver-facts`) | 12/18 | 4, 4, 4 | 5.0 |

No run raised a false alarm. The public spec leaves out the rules for replays,
conflicts and push bases. On the given model snappea's lint blocks with
`unchecked-event` on `Replayed` and `Conflicted` and `unused-payload-field` on five
fields, which names what the specs do not look at. Compat solvers wrote specs for the
rules they thought of and missed `first-base` and `delete-retried` more often. Most of
the gap survives without mutation testing (lean), so lint carries it; mutation testing
adds about one mistake in three runs.

One mistake, a replay answered with no `Replayed` announcement, was caught by one run
in twelve. Snappea listed it as the single undetected mutant in every run that ran
mutation testing, and solvers wrote it off as unobservable.

### Output variants

| Variant | What changes | `leader-election` build | `offline-sync` guard |
| --- | --- | --- | --- |
| base | nothing | 6/6 runs pass, 10.0 min | 14/18, 7.0 min |
| lean | no mutation testing, no driver limits | 3/4 runs pass, 5.5 min | 12/18, 5.0 min |
| focus | undetected mutants are a note, not a warning; at most five listed; re-assignments of the same value hidden | 3/3 runs pass, 5.5 min | 14/18, 4.7 min |

Focus keeps the scores of base at the cost of lean. Lean loses a little on both tasks.

### Blocking mutants

The default now runs only the mutants a spec or another machine could see (a removed
`send` or `announce`, an emptied body, a removed or forced branch) and fails the check
on each undetected one. `--strict` runs the rest. The console prints no mutant or lint
counts. Three runs per cell unless noted.

| Variant | `offline-sync` guard | `leader-election` build | `circuit-breaker` build | `token-refresh` guard | `online-migration` guard |
| --- | --- | --- | --- | --- | --- |
| block (first wording) | 14/18 (3, 5, 6), 5.9 min | 3/3 runs pass, 5.4 min | 3/3 runs pass, 7.2 min | | |
| block2 (error-style lines, help names the observing specs) | 12/18 (3, 4, 5), 4.5 min | | | 10/10 (2 runs) | 12/12 (2 runs) |

No run raised a false alarm. The scores are within noise of base and focus at this
sample size; time per run is lower than base on every task. What the transcripts show:

- The silent replay was caught by 2 of 3 block runs and 0 of 3 block2 runs, against 1
  in 12 before. In block2 only one run saw the mutant at all: one stopped with lint
  failing, which then skipped mutation testing, and one passed `--no-mutation` on
  every check. Mutation testing now runs after a lint failure for this reason.
- The run that did see it wrote that requiring `Replayed` "at a fixed point would fail
  on the correct design", and left the check failing rather than suppress. The help
  names the spec to extend but not how to require an event without fixing its order.
- On `token-refresh` both runs found all five mistakes and each left two or three
  blocking mutants in `Tab.p` undetected, calling one equivalent. A blocking mutant an
  agent cannot detect or justify costs time at the end of a run.
- A removed body or branch that only updates variables is now left to `--strict`: it
  blocked a guide example on an entry that sets fields to their default values.
- One `circuit-breaker` run silenced a forced branch with a blanket `allow mutation`.

### Tasks where the arms tie

| Task and mode | Grade | `snappea` | `compat` |
| --- | --- | --- | --- |
| `online-migration` build | hidden tests | 27/27, 3.7 min | 27/27, 4.4 min |
| `offline-sync` build | hidden tests | 22/22, 3.9 min | 22/22, 2.8 min |
| `circuit-breaker` build | hidden tests | 10/10, 11.6 min | 10/10, 3.5 min |
| `token-refresh` harden | hidden tests | 22/22, 2.3 min | 22/22, 1.7 min |
| `lease-impl` impl | hidden scenarios | 38/38, 2.6 min | 38/38, 2.7 min |
| `online-migration` guard | hidden mistakes | 12/12 | 12/12 |
| `token-refresh` guard | hidden mistakes | 10/10 | 10/10 |

Given a DESIGN.md that states every rule, Haiku writes a correct design, or finds
every seeded bug by reading, with either tool. In `lease-impl` half the solvers never
ran the checker on their logs and still passed. On `circuit-breaker` snappea cost three
times the minutes for the same score: with no specs given, lint and coverage kept the
solver working on a model whose design was already right.

### Build and review on the smaller tasks: no difference

On `job-queue` and `consumer-group` build, both arms passed every hidden test (one run
per cell). In review mode on all three tasks, both arms found 10 of 10 seeded bugs with
no false alarms; compat used 15 commands per run and snappea 18.

These designs are under 200 lines with one bug per handler. The solver finds the bugs
by reading, and the checker only confirms them. A task like this cannot separate the
arms, whatever the tool does.

## What makes a task discriminate

The two tasks with headroom share these properties. Keep them when adding tasks.

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
  there. Solvers read them, changed little, and reported the count. The longest run
  (21 minutes, design final at minute 3.3) spent seven of its 22 commands on mutants.
  This is the clearest low-value output. The focus variant above addresses it.
- The hidden tests cannot credit a better test suite, only a better design. Work after
  the design is final may still have value the eval does not measure.
- A lint failure skips coverage and mutation testing, so a solver learns about gaps
  one round later than it could.

## Limits

- Three to six runs per condition. Two tasks separate the arms, both through lint
  rules that block the check; seven task and mode pairs do not.
- A stronger solver model would likely close the gaps that remain; a weaker one might
  open gaps on the tied tasks. Only Haiku was tested.
- The compat arm is snappea's engine behind upstream's command line, not upstream P.
- The solver image has no `python3`; solvers that reach for it lose a command.
- Hidden tests run for 10 seconds each with a fixed seed. A flaw that needs a deeper
  search can pass.
