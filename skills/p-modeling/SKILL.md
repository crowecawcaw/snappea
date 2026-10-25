---
name: p-modeling
description: Use P/Snappea to test risky system design decisions involving concurrency, state, retries, or failures, then check how faithfully implementation behavior matches the design.
---

# Design with P, validate with Snappea

P is primarily a **design aid**: make a small executable model while deciding how a system should work. Snappea explores choices and event orderings that ordinary examples miss. Finding a counterexample before implementing a protocol is cheaper than debugging the same mistake in application code.

Use this skill for competing operations, shared state, multi-step updates, stale information, retries, recovery, or progress after failures. Pure transformations and straightforward deterministic logic usually benefit more from ordinary unit/property tests. A large model of everything is rarely the best first model.

## During design

Start with a concrete question: **what invalid execution could make this design wrong?** Identify the actors, state, real atomic primitives, and points where another operation or failure can intervene. Build a small model around that question, using a separate driver to generate competing actions and a monitor to check the requirement independently. Check it while the design is still easy to change, before translating its algorithm into implementation.

The important choice is what the model permits. Local assignments without scheduling points can collapse a real race window. P can schedule at operations such as `send`, `receive`, and machine creation; a handler is not universally atomic. Represent separate storage `read` and conditional `write` calls as separate interactions so another actor can intervene. Preserve the intervening choices unless the real storage primitive guarantees their exclusion. Otherwise the model assumes the property it is meant to test. Small domains help search; small domains must still contain contention, stale versions, rejected operations, and other relationships that can break the design.

A focused model can answer one question without covering the whole contract. Keep track of the other requirements and assumptions, and model the other risky decisions rather than treating the first passing fragment as validation of the entire design. [Choosing a faithful model](guides/modeling.md) explains how; [P patterns](reference/p-patterns.md) supplies syntax when needed.

## Decide what a passing check means

A monitor should derive expected behavior from requests and history, not accept the implementation's claim that an operation is valid. Scenarios should challenge the guarantee: two clients need to compete for the **same** resource, and a retry needs to reuse the **same** identity after the relevant failure.

Use uncovered statements, outcomes, handlers, and named scenarios to choose the next test. Investigate surviving mutations: they can expose checks that never distinguish correct behavior from a nearby defect. Full coverage and mutation scores do not establish that omitted requirements were modeled. A timeout is incomplete exploration, not evidence of impossibility. Progress also needs explicit assumptions about failures stopping and scheduling.

Read [Scenarios and independent checks](guides/scenarios-and-oracles.md) for evidence quality; [Recovery and retries](guides/recovery.md) for durable state and lost responses. Fix the model or design when the evidence exposes a flaw; retain a regression scenario that reveals it. Suppress only a justified intentional exception, with its reason.

## From design to implementation, and back

P checks the model, **not automatically the code you ship**. Its benefit depends on their correspondence. Compare the implementation's storage calls, atomic boundaries, version checks, retry decisions, and recovery state with the model. A new boundary or cache assumption is a design change worth revisiting.

Use Snappea's language helpers and `observe` to check the same monitor against actual implementation events during tests. The Python distribution is `pysnappea`; the import is `snappea`. Prefer recording events emitted by the implementation; wrappers can record actual inputs/returned results when their limits are explicit. Hand-authored traces test the monitor, not the application. Missing or fabricated evidence cannot validate the implementation.

Keep ordinary interface, boundary, crash, and concurrency tests too. Use observed monitor coverage to find missing test behavior. At maintenance time, update the model and regression tests when requirements or assumptions change; replay production evidence where useful. Runtime instrumentation must remain best-effort and must not break application code; explicit test validation should fail on invalid or insufficient evidence. See [Implementation correspondence and observation](guides/implementation.md).

## Original worked examples

For a first session, open the reservation README, run its correct and broken cases, and compare the storage boundaries before adapting the model to your own question.

- [Reservation](examples/reservation/README.md): contention, storage boundaries, independent expectations, and checking real Python behavior.
- [Durable retry](examples/durable-retry/README.md): commit succeeds, response is lost, process restarts, request retries.
- [Versioned publication](examples/versioned-publication/README.md): stale evidence and revalidation at an optimistic publication boundary.

Each example explains why its abstraction is useful, includes a deliberately broken variant, and states what the checks leave open. Read the example closest to the design question; adapt the reasoning, not just its surface structure.
