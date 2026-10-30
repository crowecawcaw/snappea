# Minimal-prompt eval: results

Since these cohorts, the skill has been removed: its workflow, guides and examples are
now `snappea guide` topics and `snappea guide example NAME`, so there is nothing to
install. Skill versions used here: v1 = commit `0930237~1`, v2 = `0930237` and `fd2c219`.

## Cohort 3: the CLI's built-in guide carries most of the gain

**With no skill at all, an agent told only "use P; snappea is installed" now writes
tests that catch every race defect.** It finds `snappea guide` from `snappea --help`
and follows it. On fencing and names, no-skill runs killed 0 of 4 race defects in
cohort 2 and 4 of 4 in cohort 3, and took about 4 minutes against 13 for skill v2.

8 fresh Sonnet runs, fencing and names × {no skill, skill v2} × 2, new binary (sha256
`00bffa70…`: `snappea guide`, the check-summary notes, the tuple hint). Three v2 runs
stopped by an account usage limit were rerun under new IDs and are excluded
(`runs/*/TRUNCATED`). Tables: `python report.py c3-`.

| condition | runs | hidden tests | defects killed | race defects killed | final check passed | forced interleavings | observation tests | DESIGN.md | final check skipped mutation/coverage | commands | minutes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| no skill, cohort 2 (old CLI) | 4 | 100% | 81% | 0/4 runs | 1/4 | 0/4 | 3/4 | 0/4 | 2/4 | 14 | 2 |
| no skill, cohort 3 (`snappea guide`) | 4 | 100% | 100% | 4/4 runs | 2/4 | 4/4 | 4/4 | 3/4 | 3/4 | 14 | 4 |
| skill v2, cohort 3 | 4 | 100% | 100% | 4/4 runs | 4/4 | 4/4 | 4/4 | 4/4 | 0/4 | 27 | 13 |

(The cohort-2 row is its fencing and names runs only, to compare like with like.)

- All four no-skill agents ran `snappea --help`, then `snappea guide`, `guide testing`
  and `guide syntax` within their first four commands.
- What v2 still adds: a finished model check. Guide-only agents often end on
  `--no-mutation` after mutation testing hits its time limit. The CLI now says how to
  finish untested mutants separately (`snappea check mutation --mutation-timeout 5m`);
  that change came after this cohort.
- The tests in these tasks are now at ceiling in both arms. The next cohort needs harder
  tasks (crash recovery, multi-step publication) to separate them.
- Caveats: cohort 2 and 3 differ in binary, in time and slightly in bridge wording (the
  bridge wording only tells the agent to send every command through it). Two runs per
  task and condition.

Library changes from cohort 3 evidence: coverage no longer requires a `return` after
`while (true)` (P requires it, but it can never run; one agent was stuck on it), and the
check summary counts `mutants_not_tested`.

## Cohort 2: skill v2 vs v1 vs none


**Skill v2 closes the gap the pilot found.** With v2, every run's tests caught every
race defect (a lost compare-and-swap, a stale retry), including on a held-out task
written before v2 was run. Without a skill, or with v1, at least one survived in 9
of 12 runs. All implementations passed every hidden test in every condition, so the
difference is in the tests and the process, not the code. It costs time: v2 runs took
about 17 minutes against 3-4.

18 fresh Sonnet agents (cohort 2), 3 tasks × 3 conditions × 2 runs, same prompt apart
from one skill line, same pinned `snappea` binary (sha256 `0e2d93f3…`), offline
sandboxes. Records: `runs/c2-*`. Regenerate the tables with `python report.py c2-`.

## By condition

| condition | runs | hidden tests | defects killed (transplanted) | race defects killed | final `snappea check` passed | tests force interleavings | observation tests | DESIGN.md | final check skipped mutation/coverage | commands | minutes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| no skill | 6 | 100% | 88% | 33% | 1/6 | 0/6 | 3/6 | 0/6 | 3/6 | 13 | 3 |
| skill v1 (PR #108) | 6 | 100% | 84% | 17% | 2/6 | 3/6 | 3/6 | 0/6 | 3/6 | 12 | 4 |
| skill v2 | 6 | 100% | 99% | 100% | 6/6 | 6/6 | 6/6 | 6/6 | 0/6 | 30 | 17 |

- **Race defects** are the transplanted defects that only appear when two calls
  interleave between a load and a compare-and-swap: ignoring a failed swap, retrying
  with a stale record. Per task, no skill / v1 / v2 killed them in: queue 2/2, 1/2, 2/2;
  fencing 0/2, 0/2, 2/2; names (held out) 0/2, 0/2, 2/2.
- The other defects (retry memos, boundaries, preconditions, namespaces) were mostly
  caught in every condition. Sonnet writes good ordinary tests unprompted.
- "Model first" (first `snappea check` before writing `solution.py`): 5/6, 3/6, 6/6.

## What changed between v1 and v2

The pilot (8 runs, `runs/queue-*`, `runs/fencing-*`) showed three failure modes:

1. **Tests miss the races the model explored.** Agents' Python tests used random
   thread stress, which almost never hits a race window. `write-ignores-cas` and
   `write-stale-retry` survived in all four fencing runs.
2. **Agents stop at a partial pass.** They finished with `--no-mutation` or
   `--allow-incomplete-coverage`, and reported "passes".
3. **Models make whole operations atomic**, which assumes the race away.

v2 rewrote `SKILL.md` as five steps with an explicit "done" bar, and added
`guides/testing-implementation.md` with a runnable example
(`examples/forced-interleaving/`): wrap the store so two calls read the same version, or
so a competing call runs between a load and its compare-and-swap; record real calls and
check them with `check_events`; then break the code on purpose and confirm a test fails.

## Library changes from the same evidence

- `snappea check` now says when a pass skipped mutation or the coverage gate, and
  reports undetected mutants in the JSON `summary` (`skipped`, `mutants_undetected`,
  `note`). Exit codes are unchanged.
- A one-field named tuple written without a trailing comma, `(items = items)`, now gets
  a hint to add the comma, instead of the generic "missing ';'".
- `snappea observe --help` names the Python package and its import name; one agent
  tried `import pysnappea`.

These landed after cohort 2 started; cohort 2 used the earlier binary for all runs.

## How the tests are scored

Three mutation scores, because no single one is fair to every design:

- **Transplanted defects (primary).** A helper agent writes each named defect into the
  agent's *own* `solution.py`, keeping its instrumentation. A transplanted mutant
  counts only if it fails some hidden tests and passes others. The agent's full test
  suite then runs against it. This works whether or not the tests rely on the agent's
  own `emit` calls or record format.
- **Reference mutants.** The same defects as fixed edits to a private reference. Only
  agent tests that pass on the reference count, so tests tied to the agent's internals
  are dropped. One v1 run had 0 of 14 tests runnable this way (score 0%) and killed
  every transplanted defect.
- **Generic AST mutants** of the agent's own code (flip comparisons, negate tests, drop
  early returns). These score high (82-100%) in every condition and barely separate
  them.

## Limits

- Two runs per task and condition. The race-defect gap (0/4 vs 2/2 on each of fencing
  and names) is large, but other differences here are within noise.
- v2 was written after seeing the pilot on queue and fencing. `names` was written
  before any v2 run, but v2's list of deliberate breaks includes "ignore the
  compare-and-swap result", a generic defect that appears in all three tasks.
- Hidden implementation tests are at ceiling (100% everywhere). They cannot show
  whether the skill improves code, only that it does not hurt it.
- The eval author wrote the tasks, the hidden tests, the defects and the reference.
  Two solver test suites found real bugs in the eval itself (a hidden test tied to a
  key format; a reference that accepted the Kelvin sign `K` as `k`); both were fixed
  and every run regraded.
- Process signals (`forced_interleaving`, `observation`, `model_first`) are regex
  heuristics over files and the command transcript.
- One v2 solver (`c2-fencing-v2-01`) ran three early commands on the host instead of the
  sandbox and listed eval file names (no contents); see its `DEVIATION.md`.

## Next

- A task where the implementation itself is likely to be wrong (crash recovery,
  multi-step publication), so the hidden tests stop being at ceiling.
- Cut v2's cost: most of the extra time is model checking and mutation runs. Try
  guidance on smaller bounds and `--timeout` first, then the full check once.
- Run v2 with other models (Haiku, Opus) and with the CLI alone, without the skill, to
  see how much of the gain the CLI's own help and output can carry.
