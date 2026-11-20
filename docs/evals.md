---
diataxis: reference
personas: [contributor]
questions:
  - Which evals measure whether snappea helps an agent, and what does each test?
  - What prompt does each eval give the agent?
  - What did each eval show, on which commit and date?
  - Where is the setup to run an eval again?
---

# Evals

Each eval gives a fresh agent a task in an offline sandbox and grades what it leaves
behind. They measure whether snappea's output changes what an agent ends up with: a
design, a model, an implementation or a bug report. Use them before and after a change
to the CLI, the guide or the checker's output.

The setup for each eval is in `evals/<name>/`, with a README that says how to run it.
Run output (prompts, transcripts, submissions, grades) stays in `runs/` there and is
not committed. Record short results on this page, with the commit the checker was
built from and the date.

Every sandbox prompt starts with the same bridge instructions: the agent's only tool is
a shell in the container, reached through `harness.py bridge <run>`, with a limit on
commands and minutes. The prompts below leave that part out.

## model-gaps

**Goal.** Does snappea's analysis (lint, coverage, mutation testing, driver limits)
change the model an agent ends up with, compared with the same checker behind the
upstream `p` command line? Grading is mechanical: the harness runs hidden tests, or
hidden mistaken designs, against what the agent wrote.

**Prompt.** "Complete the task in /work/TASK.md. When done, reply with a short
summary." `TASK.md` depends on the mode; the templates are in
[`harness.py`](../evals/model-gaps/harness.py). The two modes with results:

- Build: "Write the design in `PSrc/` so that the contract holds under everything
  DESIGN.md says can happen. ... Your `PSrc/` will be judged by running it, with the
  given `PSpec/`, against hidden test cases."
- Guard: "The team will keep changing the design. Make the specs and test cases strong
  enough that a change to `PSrc/` that breaks the contract in DESIGN.md ... makes a
  test case fail. ... You score one point for each mistake that some test case
  catches."

The arms differ in one sentence: "The P checker is installed as `snappea`; run
`snappea --help`." or the same with `p`.

**Results.** Solvers were Haiku subagents, two to six runs per cell. Read the counts
as a pilot.

Arms, 2026-11-20, binaries built from `8b2ceb7`:

| Task and mode | Grade | `snappea` | `compat` (`p`) |
| --- | --- | --- | --- |
| `leader-election` build | runs passing all 12 hidden tests | 6/6, 10.0 min | 2/6, 4.9 min |
| `offline-sync` guard | hidden mistakes caught | 14/18, 7.0 min | 9/18, 9.8 min |
| `online-migration` build | hidden tests | 27/27, 3.7 min | 27/27, 4.4 min |
| `offline-sync` build | hidden tests | 22/22, 3.9 min | 22/22, 2.8 min |
| `circuit-breaker` build | hidden tests | 10/10, 11.6 min | 10/10, 3.5 min |
| `token-refresh` harden | hidden tests | 22/22, 2.3 min | 22/22, 1.7 min |
| `lease-impl` impl | hidden scenarios | 38/38, 2.6 min | 38/38, 2.7 min |
| `online-migration` guard | hidden mistakes | 12/12 | 12/12 |
| `token-refresh` guard | hidden mistakes | 10/10 | 10/10 |

Two task and mode pairs separate the arms, both through lint rules that block the
check (`unselected-spec`, `unchecked-event`). The rest tie: given a DESIGN.md that
states every rule, Haiku gets the design right by reading. On the small review tasks
(`job-queue`, `consumer-group`, `seat-hold`) both arms found every seeded bug.

Output variants of `snappea`, same date:

| Variant | Built from | `leader-election` build | `offline-sync` guard |
| --- | --- | --- | --- |
| base | `8b2ceb7` | 6/6 runs pass, 10.0 min | 14/18, 7.0 min |
| no mutation testing, no driver limits | `8b2ceb7` with flags | 3/4 runs pass, 5.5 min | 12/18, 5.0 min |
| undetected mutants as a note, at most five listed | `8b2ceb7` | 3/3 runs pass, 5.5 min | 14/18, 4.7 min |
| only blocking mutants run, and they fail the check | before `dfa5c47` | 3/3 runs pass, 5.4 min | 14/18, 5.9 min |
| the same, with error-style lines naming the observing specs | `dfa5c47` | | 12/18, 4.5 min |

The blocking variants also passed `circuit-breaker` build in 3/3 runs at 7.2 minutes
(11.6 before), and caught every mistake in `token-refresh` and `online-migration`
guard. No run in any variant raised a false alarm. The scores are within noise of
each other; the time per run fell.

What the transcripts showed:

- One `offline-sync` mistake, a replay answered with no `Replayed` announcement, was
  caught by 1 run in 12 before mutants blocked and by 2 of 6 after. In the last
  variant two of three runs never ran mutation testing: one stopped on a lint failure,
  which skipped it, and one passed `--no-mutation`. Mutation testing now runs after a
  lint failure (`a1b855e`); that change has no runs yet.
- The run that saw the mutant could not write a spec for it without fixing the order
  of events, and left the check failing.
- Solvers filtered out `Driver limits` blocks and repeated help text with `grep -v`
  in every check. Lint help now prints once per report, and driver limits need `-v`.
- Long lists of undetected mutants, mostly resets of a value already there, took up
  to a third of a run's commands and changed little.

**Setup.** [`evals/model-gaps/`](../evals/model-gaps/README.md).

## design-review

**Goal.** Does an agent using P find more known design bugs in real code with the
`snappea` CLI than with `snappea-compat` as `p`? The tasks are historical snapshots of
MinIO distributed locking and PostgreSQL logical replication. A blind grader matches
the agent's report against bugs that upstream later fixed.

**Prompt.** "Complete the task in /work/TASK.md. When done, reply with a short
summary." `TASK.md` describes the subsystem and says: "Find design bugs in this
subsystem ... Use P to model the design and check it." It asks for a model and a
report with one section per bug. The arms differ in the sentence naming the checker.
The full text is in [`tasks/minio-dsync/TASK.md`](../evals/design-review/tasks/minio-dsync/TASK.md).

**Results.** None recorded.

**Setup.** [`evals/design-review/`](../evals/design-review/README.md).

## design-build

**Goal.** Does a checker help an agent find edge cases while it designs and builds a
distributed read/write lock? The agent writes Python against a fixed simulator. 28
hidden adversarial scenarios look for the flaws the real system shipped with. Arms:
`snappea`, `snappea-compat` as `p`, and no checker.

**Prompt.** "Complete the task in /work/TASK.md. When done, reply with a short
summary." The `snappea` arm is also told to run `snappea guide`, and the `compat` arm
that `p compile` and `p check` exist. The task is
[`tasks/dlock/public/TASK.md`](../evals/design-build/tasks/dlock/public/TASK.md).

**Results.** None recorded. `python harness.py calibrate` passes: the reference passes
every hidden test and every seeded flaw fails at least one.

**Setup.** [`evals/design-build/`](../evals/design-build/README.md).

## minimal-prompt

**Goal.** Given one sentence of direction, does an agent produce a correct
implementation and tests that catch real bugs? Use it to compare CLI, guide or prompt
changes. The tasks are small concurrent components over a compare-and-swap store.

**Prompt.** "Complete the task in /work/TASK.md. When done, reply with a short
summary." Each `TASK.md` states the product requirements and ends: "Use P to model the
design before implementing it, and to test the implementation. `snappea` and the
`pysnappea` Python package are installed."

**Results.** None recorded.

**Setup.** [`evals/minimal-prompt/`](../evals/minimal-prompt/README.md).

## certified-bundles

**Goal.** Does an agent that models a design in P first write a more correct Python
implementation than one that does not? The task is a protected multi-repository
version-control service with atomic publication and crash recovery. Twenty hidden
cases check the implementation against an independent oracle.

**Prompt.** Both arms: "Read TASK.md and contract.py." The Python arm continues "Write
a design. Then implement it in Python. Add Python tests." The P arm continues "First
write and check a P/Snappea model of the design. Only start the Python implementation
after the model passes. Use pysnappea to check events from the Python implementation
during tests." The prompts are in
[`tools/prompts.py`](../evals/certified-bundles/tools/prompts.py).

**Results.** None recorded.

**Setup.** [`evals/certified-bundles/`](../evals/certified-bundles/README.md).
