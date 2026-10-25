# P modeling skill plan

## Goal and scope
Help a small coding agent use P/Snappea primarily to improve a design before implementing it, and secondarily to validate the implementation. Teach why each modeling choice matters. No CLI changes, copied upstream examples, or prescribed generation pipeline.

## Layers
- `SKILL.md`: short entry point: why/when P helps, design-to-implementation-to-maintenance lifecycle, limitations, progressive links. Modeling tests and implementation traces give evidence only for represented behavior. Keep runtime instrumentation best-effort.
- `guides/modeling.md`: preserving atomic boundaries, assumptions, identities, versions, dependencies, and justified abstractions. Model actual available primitives rather than assuming the desired guarantee.
- `guides/scenarios-and-oracles.md`: independent expectations, adversarial relationships, bounded choices, safety/progress and fairness, coverage/scenario/mutation interpretation. Full coverage cannot reveal omitted requirements.
- `guides/recovery.md`: durable versus volatile state, lost response after commit, restart, durable idempotency and conditional retries, termination after failures stop.
- `guides/implementation.md`: compare implementation and model, observe helpers and event provenance, safe instrumentation, Python test integration and boundaries. Real events cannot prove unobserved behavior.
- `reference/p-patterns.md`: compact tested syntax and mechanics for novices; real installed module is `snappea`, distribution `pysnappea`. Avoid syntax guessing and mandatory scaffolding.
- `examples/`: original small complete examples. Each README includes contract, preserved/omitted behavior, passing and deliberately broken variants, revealing execution, actual commands, and what evidence does not establish.

## Initial original examples
1. `reservation`: two clients competing for one resource; explicit read/conditional-write storage boundaries; independent correctness monitor; misleading atomic abstraction discussed.
2. `durable-retry`: commit followed by lost response, restart and retry of same ID; separate durable and volatile state; duplicate application defect.
3. `versioned-publication`: version-bound evidence and one atomic publication pointer referencing prepared immutable records, with stale evidence/revalidation under conflict. Keep domain separate from certified-bundles eval; do not encode its solution.
4. At least one real Python implementation/test uses captured implementation events with a shared P monitor, and detects a deliberately broken implementation. Boundary tests use small representative model capacity and separately check real encoded limits where relevant.

## Review and validation
Review this plan before authoring. Check original examples with current checker and native lint/coverage/mutation feedback. Require correct variants to pass and designated broken variants to fail for the documented reason; do not weaken checks to achieve clean feedback. Maintain a small repeatable validation script. Validate local links and code snippets. No claims of exhaustive implementation correctness.

## Evaluation
Use frozen certified-bundles contract/grader and identical current binary, docs, public files and tool budgets. Run Luna high in isolated containers using shell bridge only; no hidden-test exposure. Treatment prompt: use the skill, with no extra modeling/testing coaching. Fresh prompts may include task/deliverables/environment and neutral isolation instructions. Provision skill read-only at a known path, record content hashes/version in run metadata. Compare a contemporaneous P baseline without skill under the same environment; historical v8 is contextual only. Initially three treatment and three baseline trials; evaluate primary hidden correctness and separate wide-input probe, plus independently audit model fidelity, feedback usage, and actual observation-based tests. Tune after diagnosis, freeze each skill revision, and run fresh trials after meaningful changes. Do not score syntax checks, coverage counts, or P-first chronology as design success. Add targeted small probes only if they test gaps the main task cannot measure; freeze separate graders before runs.

## Plan review incorporated
- Entry point includes a concrete novice first session: property/question → actors and state → real atomic primitives and failure points → small adversarial model → feedback → implementation comparison. Explain reasons, not a mandatory pipeline.
- Explain when P is unnecessary (pure deterministic logic/simple transformations) and why it cannot prove unbounded implementation correctness.
- Make atomicity operational: only a real transaction/lock/CAS guarantee justifies grouping steps; a modeled guarantee must not be assumed in the transition that is supposed to establish it. Document justified abstractions per property.
- Progress requires appropriate eventual-recovery/fairness assumptions and hot-state checks; coverage/finite search do not establish fairness.
- Observation examples document actual event provenance, placement/order, completion and absence semantics, strict mode, unknown names, and dropped events. Instrumentation stays best-effort; explicit test validation can fail.
- Every example documents setup, exact commands, expected pass/fail reason, and what remains unestablished. First validate reservation and durable retry; publication must earn its place by teaching stale-version evidence rather than repeating transaction machinery.
- Pilot is three runs per arm with interleaved launch order, identical toolchain and baseline P prompt except the treatment's single skill-use sentence. Skill hashes and inputs are frozen. Incomplete/timeout valid submissions remain in denominator; infrastructure-invalid runs are separately identified. Primary grade, wide probe, and model fidelity audits are separate. Revised skill versions use fresh runs and are never pooled. Auditors review artifacts before being shown grades where practical.
