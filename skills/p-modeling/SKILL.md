---
name: p-modeling
description: Use P/Snappea to challenge stateful system designs before implementation, then validate observable implementation behavior with the model's monitors.
---

# Design with P, validate with Snappea

P is primarily a **design aid**. A small executable model lets Snappea explore competing actions, retries, and failure histories while the design is still cheap to change. It is secondarily an **implementation validation tool**: the same monitor can check actual behavior during tests. P checks your model, not automatically the code you ship; its value depends on how faithfully the two correspond.

Use it when correctness depends on concurrency, shared state, multi-step updates, stale information, or recovery. Straightforward deterministic transformations usually need ordinary unit/property tests instead.

## What to leave behind, and why

For a design task, leave a checked model and a short explanation of the question, assumptions, omitted behavior, and unresolved findings. A focused model need not encode the whole application, but its first passing fragment does not validate the whole contract.

**When the task includes implementation, also save and run an implementation observation test.** Run real calls under a Snappea helper while the application emits events, then check a relevant model monitor. If emission is unsuitable, a test wrapper may record actual inputs and returned results. Keep that test with the code so later changes rerun the design's checks; an inline shell experiment does not provide that protection. If a requirement cannot be observed reliably, state the limitation rather than inventing evidence. [Reservation](examples/reservation/README.md) is a complete original example of both a design model and saved Python observation tests, including a defective implementation that the tests reject.

## During design: preserve the ways it could fail

Start with a contract claim that could be false. Name the independent actors, their state, and the real atomic primitives. Then build a small competing scenario and an independent monitor around the risky boundary. Check and revise this model before translating its algorithm into implementation.

A real `read → decide → write` spans separate operations unless a transaction, lock, or conditional write guarantees otherwise. Plain local assignments can collapse that race window; P can schedule at operations such as `send`, `receive`, and machine creation. Represent separate storage interactions explicitly when their interference matters. A prepared-but-unpublished record is different from committed state. Explain why each abstraction preserves the defect being investigated.

Keep identities and versions that affect the contract. Two clients must compete for the same resource to challenge contention. For idempotent retries, complete A, perform different work B, then retry A: returning A's old response must not reapply A's effects over B. Test rejected/stale operations as well as success. Small domains must still contain these relationships. See [modeling](guides/modeling.md) and [recovery](guides/recovery.md).

## Use feedback to choose better tests

Derive expectations from requests and history, not an implementation-supplied “valid” flag. Use uncovered statements, branches, handlers, and named scenarios to choose the next test. Investigate surviving mutations: a later public read may reveal a removed state update that an announcement-only monitor misses. Explain intentional exceptions rather than suppressing unresolved risks.

Full coverage cannot reveal an omitted requirement. Exhaustive search covers only the represented bounds; a timeout is incomplete evidence. Progress needs explicit recovery and scheduling assumptions, not just safe responses. [Scenarios and independent checks](guides/scenarios-and-oracles.md) explains these limits; [P patterns](reference/p-patterns.md) supplies syntax.

## Implementation and maintenance: keep the model connected

Compare the code's reads, writes, version checks, retries, and crash state with the modeled boundaries. Use the [implementation guide](guides/implementation.md) to build the saved observation test. The Python distribution is `pysnappea`; the import is `snappea`. Helpers record actual behavior and `observe` checks supplied histories. Hand-authored traces test a monitor, not the application. Missing events, wrong partition keys, or reordered emission can invalidate conclusions.

Keep ordinary interface, boundary, concurrency, and crash tests. Use observed monitor coverage to guide missing application tests. Revisit the model when requirements, code boundaries, or assumptions change, and replay production evidence where useful. Runtime instrumentation must remain best-effort and must not break application code; explicit test validation should fail on invalid or insufficient evidence.

## Choose an original example

- [Reservation](examples/reservation/README.md): contention, conditional storage, independent oracle, Python observation.
- [Durable retry](examples/durable-retry/README.md): commit/lost response/restart, old results versus current state.
- [Versioned publication](examples/versioned-publication/README.md): stale evidence and recomputation after conflict.

Read the closest example's README and its correct/broken variants. Adapt the reasoning and stated limits, not just the surface structure.
