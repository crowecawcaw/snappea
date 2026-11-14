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

The script runs native checks over every declared test, then mutation testing:

- Positive tests must pass lint and aggregate coverage. Mutation testing must detect
  every non-suppressed mutant using positive tests.
- Negative controls use `// snappea: expect-bug "distinctive failure message"`
  immediately before the test declaration. Every run on seeds 1, 2 and 3 must find
  the matching failure. A different failure or no failure fails the check.
- Matching expected failures contribute driver evidence, but cannot fill gaps in the
  passing design's coverage or detect mutants. Gaps exclusive to negative drivers
  are reported without failing coverage; positive and shared code stays required.
  Ordinary `snappea check` commands honor the same declarations.

The harness saves native JSON reports and replay artifacts under each sample's
`PCheckerOutput/samples/`. Read failure-identity advisories: matching a broad message
on three seeds does not prove that all three reached the intended defect.

Use auto search. Prefer a small, focused test for the core race and broader concurrent
workloads for exploration. Describe a run as exhaustive only when its report says
`search_complete`; a time budget or a particular seed does not guarantee completion.
Keep bounds grounded in the application contract, not in a desire for a green check.

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
  driver what it asked for (`Requested(id, args)`) and, before starting, how much
  work it plans, so a spec can require that work to finish with the right results.
- **Real atomicity only.** One handler models one real atomic primitive. Split
  read → decide → write across messages wherever the real system can interleave.
- **Multiple actors.** Use at least two concurrent clients where the design has them,
  plus fixed tests for each scenario in the README: contention, boundaries (exactly
  at capacity), stale or retried operations, both sides of a lifecycle boundary
  (before and after expiry, sale, crash).
- **Negative control.** Where cheap, include a plausible broken variant as separate
  machines (so its code is excluded from the passing tests' coverage) and a
  `tcBug*` test with a native `expect-bug` declaration. Keep normal completion paths
  intact on schedules that do not expose the flaw; report-only negative coverage
  is not a reason to remove protocol obligations.
- **Specs can divide responsibilities.** A safety spec and a progress spec may check
  different fields of the same event. Select them together in every test that relies
  on both: the payload lint validates meaningful reads across that test's selected
  specs, never across unrelated tests or scenario monitors. A per-spec advisory
  explains that another selected spec checks the field; no suppression is needed.
- **Suppressions carry reasons** and are rare. An undetected mutant is usually a
  missing assertion or scenario; suppress only equivalent mutants.
- **New lints** (`unchecked-state`, `unchecked-event`, `no-progress-spec`) flag fields
  and events no check depends on, and tests that wait for replies with no hot spec.
  Fix them by checking the value or adding a progress spec, not by suppressing.
- **Scenario monitors describe evidence.** They ignore unrelated observed events
  implicitly. Use explicit handlers only when an event changes the scenario's
  meaning. Safety and progress specs still fail on unhandled observed events.
- **Use domain names.** Name events `Request`, `LeaseExpired`, or `PaymentCaptured`,
  without an `e` prefix. Prefer names that distinguish intent, commitment and reply.
  Keep names consistent in source, README examples and observed execution logs.
- **Model the environment explicitly.** Drivers include clients and fault injectors;
  a driver marker is appropriate for those roles, not a way to exclude the algorithm
  from mutation testing. State delivery, failure and timing assumptions.
- **Reuse ordinary P building blocks.** Import the
  [network and reply blocks](../../crates/snappea/src/cli/guide/examples/building-blocks/README.md)
  where their contract fits. Keep domain-specific specs where they clarify the
  obligation. Repository samples can import the canonical projects; when copying a
  sample out of the repo, export and commit the blocks with
  `snappea guide example building-blocks --out vendor`, then adjust `IncludeProject`.
- **Review exceptions as feedback.** Before adding an ignore or suppression, check
  for a missing assertion, a missing race, an equivalent mutant, or a tool limitation.
  Explain intentional stale-message handling in the state where it matters. Do not
  manufacture redundant checks or design state solely to silence tooling.

## README

Front matter (`diataxis: explanation` for use cases and flawed samples, `tutorial`
for concepts), then: the system and its contract in a few sentences; the real atomic
primitives; a files table; a test-case table (test, what it drives, why it matters,
expected result); run commands; bounds and omissions; how the specs carry over to
implementation tests (which events the code emits, and how to force the race).
For flawed samples, also: the flaw, the counterexample in plain words, and the fix.

Plain, short sentences. Say what the reader can do with it.
