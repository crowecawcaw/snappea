---
name: p-modeling
description: Design stateful systems with P/Snappea, then check implementation behavior against the model's monitors.
---

# Design with P

- **Model and check the risky design before implementing it.** Resolve counterexamples while the algorithm is cheap to change.
- Use P for concurrency, shared state, stale information, multi-step updates, retries, and recovery. Use ordinary tests for deterministic transformations.
- P primarily challenges designs; secondarily, its monitors validate observed implementation behavior. Its value depends on the implementation matching the model.

## Design

- State the contract, atomic primitives, bounds, assumptions, and omissions. A focused model validates only its stated scope.
- Preserve failure opportunities: separate reads from later writes unless a real transaction, lock, or CAS makes them atomic. Local assignments do not yield; communication operations can.
- Keep relevant identities, versions, and durable/volatile distinctions. Prepared state is not committed state.
- Make actors compete for the **same resource**. For retry safety: complete A, let B advance that resource, then retry A; preserve B's state and return A's original result if promised.
- Derive monitor expectations from requests and history, not implementation-supplied validity flags. Check useful success as well as rejection where required.
- Use uncovered code and scenarios to guide testing. Inspect surviving mutations; neither full coverage nor mutation detection proves an omitted requirement.
- Record whether search completed. Finite exploration covers represented bounds; progress requires explicit scheduling/recovery assumptions.

## Implement and maintain

- Compare implementation reads, writes, retries, version checks, and crash state with modeled boundaries.
- **Save and run an implementation observation test.** Use helpers to capture real emitted events, then check a relevant monitor. A wrapper may record actual inputs, results, and public reads; hand-authored traces only test the monitor.
- Python: install `pysnappea`, import `snappea`. Use `check_spec(..., strict=True)` around emitting application calls; use `check_events` for captured events.
- Preserve relevant values and identities in adapters. Missing events, wrong partitions, or reordered emission limit evidence.
- Keep ordinary boundary, concurrency, and crash tests. Use observed coverage to find missing application tests; revisit the model when code or requirements change.
- Runtime emission is best-effort and must not break application code. Explicit test validation should fail on invalid or insufficient evidence.
- Deliver the checked model, assumptions/limits, unresolved findings, and saved observation tests when implementing.

## Read as needed

- [Modeling](guides/modeling.md): abstraction and scheduling boundaries.
- [Scenarios and oracles](guides/scenarios-and-oracles.md): independent expectations, coverage, mutations, progress.
- [Recovery](guides/recovery.md): durability, ambiguous commits, replay.
- [Implementation](guides/implementation.md): Python helpers and trustworthy observation.
- [P patterns](reference/p-patterns.md): syntax and commands.

## Original examples

Read the closest README and compare its correct/broken variants:

- [Reservation](examples/reservation/README.md): contention, conditional storage, independent oracle, saved Python tests.
- [Durable retry](examples/durable-retry/README.md): lost response, restart, old result versus current state.
- [Versioned publication](examples/versioned-publication/README.md): recompute and revalidate after CAS conflict.
