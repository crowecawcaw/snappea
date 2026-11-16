---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - Which test scenarios find the bugs that matter?
  - What is a driver, and how do I compare design variants?
  - How do I act on coverage gaps and surviving mutants?
---

# Scenarios and drivers

## Scenarios that find bugs

- Two clients read the same version before either writes.
- Crash, lost-reply and replay histories: `snappea guide recovery`. A holder that
  never releases: `snappea guide spec-patterns`.
- Each precondition false on its own; two false at once can hide a missing check.
- Each value class; each limit just below, at and just above.
- Both sides of each lifecycle boundary; one test per terminal state, starting the
  second actor after the first finished.
- Stale operations against a successor's live state, not only empty state.
- Useful success as well as rejection: always rejecting can satisfy safety.
- Name the defect each test exposes; prefer scenarios where a plausible broken
  design behaves differently.
- Bound generated IDs and values; check a few hand-made valid and invalid histories
  first (`snappea guide example bounded-history`).
- Save every counterexample as a regression test.

## Drivers

- Drivers: the test's main machine, machines under `PTst/`, and machines marked
  `// snappea: driver`. Mutation leaves them alone, so specs still know what should
  happen when the design is broken.
- Drivers announce intent and plans; specs never learn them from design code.
  Networks, timers and external services are drivers that announce ground truth.
- Compare design variants with one thin driver per variant that only does the `new`;
  share the rest through functions taking the machine: `Run(new Server(..))`. A
  shared helper creating either variant puts both in every test's module, which the
  module check rejects; do not add the other variant's machine to the test.
- Funnel a machine's exits through one notification, so mutation reports one missing
  check, not one per exit.

## Coverage and surviving mutants

- Coverage shows what ran, not that assertions enforce the contract.
- For each survivor: missing scenario, missing check, equivalent behavior, or
  incomplete search? If removing a write survives, check state via a public read.
- Suppress best-effort operations with a reason (`snappea guide faults`); never an
  unresolved risk.
- A known-bad variant validates a spec only if it fails for the intended property.
- `--timed` drops orders the latency profile rules out; untimed keeps them. Run both.
