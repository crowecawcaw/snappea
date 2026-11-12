---
diataxis: how-to
personas: [contributor, coding-agent]
questions:
  - How do I add a sample to examples/samples?
  - What must a sample pass before it is accepted?
  - How do I mark a test case that is expected to find a bug?
---

# Writing a sample

Every sample is a P project that an agent can copy as a reference. It must be correct,
small enough to read in one sitting, and checked by the toolchain, not only by eye.

## Layout

```text
examples/samples/<group>/<name>/
  <Name>.pproj          # lists ./PSrc/, ./PSpec/, ./PTst/
  PSrc/                 # the design: machines that run the algorithm
  PSpec/                # specs (safety and progress) and the events they observe
  PTst/                 # drivers, scenarios, test declarations
  README.md             # front matter, contract, tests table, bounds, run commands
```

Groups: `use-cases/` (realistic designs), `concepts/` (one P concept each, small),
`flawed/` (a design with a deliberate flaw, plus its fix).

## Acceptance

From the repository root:

```sh
cargo build -p snappea --bin snappea
python3 examples/samples/check_samples.py <group>/<name> --workers 2
```

The script requires, for each project:

- every test case **not** marked as an expected bug passes
  `snappea check <tests> --deny mutation`: no lint finding, every statement, handler
  and `if` outcome covered, and every mutant detected or suppressed with a reason;
- every test case marked `// expect-bug: <text>` (on the line directly before its
  `test` declaration) fails, and its bug report contains `<text>`. Use a distinctive
  assertion message, so the test fails for the intended reason, not any reason.

Prefer exhaustive search (`all N schedules` in the output) for the core race; a
generated test may be sampled. Keep each test case under a few seconds.

## Quality bar

- **The contract is independent of the algorithm.** Specs rebuild the expected state
  from the event history and assert the actual results. Never trust a `valid` flag
  computed by the model.
- **Announce at the commit point**, inside the handler that performs the atomic
  primitive, not when a reply is received.
- **Safety and progress.** Add a hot-state spec for every promise of the form "every
  request gets an answer / every job completes". Without it, mutation testing will
  show that removing a reply or a send goes unnoticed.
- **Drivers announce intent.** Drivers (the test's main machine, machines under
  `PTst/`, machines marked `// snappea: driver`) are not mutated. Announce from the
  driver what it asked for (`eRequested(id, args)`) and, before starting, how much
  work it plans, so a spec can require that work to finish with the right results.
- **Real atomicity only.** One handler models one real atomic primitive. Split
  read → decide → write across messages wherever the real system can interleave.
- **Multiple actors.** Use at least two concurrent clients where the design has them,
  plus fixed tests for each scenario in the README: contention, boundaries (exactly
  at capacity), stale or retried operations, both sides of a lifecycle boundary
  (before and after expiry, sale, crash).
- **Negative control.** Where cheap, include a plausible broken variant as separate
  machines (so its code is excluded from the passing tests' coverage) and a
  `tcBug*` test marked `// expect-bug:`.
- **Suppressions carry reasons** and are rare. An undetected mutant is usually a
  missing assertion or scenario; suppress only equivalent mutants.
- **Scenario monitors** must `ignore` every observed event a state does not handle;
  an unhandled event in any monitor is a runtime error.

## README

Front matter (`diataxis: explanation` for use cases and flawed samples, `tutorial`
for concepts), then: the system and its contract in a few sentences; the real atomic
primitives; a files table; a test-case table (test, what it drives, why it matters,
expected result); run commands; bounds and omissions; how the specs carry over to
implementation tests (which events the code emits, and how to force the race).
For flawed samples, also: the flaw, the counterexample in plain words, and the fix.

Plain, short sentences. Say what the reader can do with it.
