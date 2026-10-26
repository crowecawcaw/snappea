---
name: p-modeling
description: Model stateful designs with P/Snappea; validate implementation histories with the same monitors.
---

# P modeling

- **Design first:** model/check the risky algorithm before implementing it; counterexamples are cheaper to fix then.
- Use for concurrency, shared state, stale evidence, retries, and recovery. Deterministic transformations usually need ordinary tests.
- P checks the model. Implementation validation depends on matching its boundaries and observing actual behavior.

## Design

- State contract, atomic primitives, bounds, assumptions, omissions.
- Preserve interference: separate reads/later writes unless a real transaction, lock, or CAS makes them atomic. Local assignments do not yield; communication can.
- Bound generated input histories to contract-relevant choices; let schedule exploration vary interleavings. Keep a few manually calculated valid and invalid oracle probes to check the monitor itself. See the [bounded history generator](examples/bounded-history/README.md).
- Preserve relevant IDs, versions, durable/volatile state, and prepared/committed distinctions.
- Make competitors share a resource. Retry A after B advances that resource; check A's original result and B's retained state where promised.
- Model the algorithm, not just atomic API behavior: include reads, failed conditional writes, and retries when those are the design risk.
- Assert contract obligations, not incidental implementation choices.
- Derive monitor expectations independently from contract/history. Check useful success. Read public state immediately after rejected operations, before later work can hide side effects.
- Name each scenario's plausible defect. Test **every acceptance precondition** independently, including lifecycle boundaries; other failing guards can mask missing enforcement. Test stale operations against newer live state.
- Cover valid value classes and output boundaries, not just rejection guards: signed/zero values where allowed, empty/nonempty, and either side of a partition.
- Use coverage and mutation feedback to find gaps. Neither finds omitted requirements; report bounds and incomplete search. Progress needs recovery/scheduling assumptions.

## Implement and maintain

- Compare code's reads, writes, retries, and persistence with model boundaries.
- **Save/run observation tests:** capture real calls/events with Python helpers; check a relevant monitor. Hand-authored traces test only the monitor.
- Install `pysnappea`; import `snappea`. See [implementation](guides/implementation.md) for capture, strictness, ordering, and partitioning.
- Preserve IDs/values in adapters. Keep ordinary boundary/concurrency/crash tests; use observed coverage to guide additions.
- Retain counterexamples as regression tests; revisit models as code/contracts change.
- Runtime emission: best-effort, never break application code. Explicit validation: fail invalid/insufficient evidence.
- Deliver model, assumptions/limits, findings, and saved tests when implementing.

## References

- Guides: [modeling](guides/modeling.md), [scenarios/oracles](guides/scenarios-and-oracles.md), [recovery](guides/recovery.md), [P syntax/commands](reference/p-patterns.md).
- Original correct/broken examples: [reservation + Python tests](examples/reservation/README.md), [durable retry](examples/durable-retry/README.md), [versioned publication](examples/versioned-publication/README.md).
