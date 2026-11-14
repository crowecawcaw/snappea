---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - Which test scenarios find the bugs that matter?
  - How do I write an oracle that does not trust the model?
  - How do I check progress, time and observed logs?
---

# Scenarios and independent oracles

## Expectations

- Assert only promised constraints; allow contract-valid alternatives to incidental implementation choices. Keep algorithm-specific invariants distinct from the external contract.
- Derive expected state from the external contract, inputs, and history; do not trust an implementation's `valid` flag.
- Example: subtract committed reservations from initial stock; reject negative capacity independently of storage's decision logic. See reservation (`snappea guide example reservation`).
- Check later public reads when an announcement alone cannot establish that state changed.
- A broken variant validates the oracle only if it fails for the intended property, not a syntax error or unrelated assertion.

## Scenarios

- Contention: both clients read the same resource before either writes.
- Ambiguous commit: effect commits, response is lost, restart, retry the same ID.
- Old replay: complete A, let B change the same resource, retry A; distinguish A's saved result from current state.
- For **each acceptance precondition**, make it false while all others hold; combined invalid conditions can hide missing enforcement.
- Cover each meaningful valid value class and the output partition boundaries (for example, below/exactly-at/above capacity), not just one representative input.
- Exercise operations on both sides of lifecycle boundaries; querying state alone does not test enforcement.
- Add a lifecycle-boundary test per terminal state: start a second actor only after the first finished. Concurrent tests reach those states too rarely.
- Exercise stale operations against a successor's live state, not only empty/expired state.
- Include stale/rejected operations and useful success. Always rejecting can satisfy safety while violating the contract.
- Use bounded choices that preserve these relationships; schedule count alone says little.
- Bound a history generator's input IDs and values to preserve contract-relevant cases; let the checker explore schedules independently. Check a few manually calculated valid and invalid histories against the oracle before relying on generated histories. See the bounded history generator (`snappea guide example bounded-history`).

## Drivers

- The driver is the test's main machine, the machines under `PTst/`, and machines marked `// snappea: driver`. Mutation testing does not change drivers, so a spec still knows what should happen when the design is broken. Have drivers announce their intent and plans (the requested seat, the planned messages) instead of letting the spec learn them from design code.
- External services, networks and timers are environment stand-ins: drivers, not design. Put them under `PTst/` or mark them, and announce ground truth (what was really delivered or stored) from them.
- Compare design variants (flawed and fixed) with one thin driver per variant that does only the `new`, and share the rest through functions that take the created machine: `Run(new Server(..))`. A shared helper that creates either variant puts both in every test's module, which the module check rejects; adding the other variant's machine to the test is the wrong fix.
- With timeouts, giving up always satisfies "every request ends". Also require the work to be attempted or served.
- Prefer notifications to poll loops. A poll loop turns missing progress into a livelock, which is expensive to detect.
- Funnel a machine's exits through one notification point, so mutation reports one missing check, not one per exit.

## Choose discriminating tests

- For each important test, name a plausible defect and the observation that would expose it. Prefer a scenario where the proposed design and a plausible broken alternative behave differently.
- Check state after rejected/stale operations before later successful work can overwrite evidence of a side effect.
- Check resulting public state as well as returned outcomes; use independent expected values, not the same helper as the implementation.
- Where practical, confirm a known-bad variant fails for the intended reason. Keep this as a negative control, not a requirement for every test.
- Turn counterexamples into saved regression tests. Exercise real failure boundaries and relevant relationships, not just more random inputs.

## Feedback and limits

- Uncovered statements, branches, handlers, and scenarios identify behavior not exercised.
- An exercised assertion need not meaningfully constrain behavior. Full coverage cannot identify omitted requirements.
- Named scenario coverage establishes occurrence, not correctness; add independent assertions for results.
- Inspect surviving mutants: missing scenario/check, equivalent behavior, irrelevant abstraction, or incomplete search? Check budgets and completion before deciding.
- If removing a write survives, read the state through public behavior; announcement-only checks can miss it.
- Best-effort operations (cancel, warmup, hints) are expected survivors: suppress them with a reason that says why the design must not depend on them.
- Explain intentional exceptions. Do not suppress unresolved risks to obtain a passing command.
- Report asserted properties, bounds, assumptions, completion, and unresolved findings.

## Time and observed logs

- `--timed` drops orders the latency profile rules out, such as a reply arriving after its timeout fired. Untimed checking keeps them, which covers latencies outside the profile. Run both: they are complementary.
- A spec meant for `snappea observe` must accept every order the real system can log, not only the orders the model produces. The first event seen for a key may be any event.

## Progress

- Safety: “no duplicate effect.” Progress: “accepted work eventually completes.” Check useful success where promised, not merely a response.
- State delivery, scheduling, and recovery assumptions. Infinite outages cannot guarantee completion.
- Hot states detect outstanding obligations at checked execution ends when enabled; they do not establish scheduler fairness.
- See [checking models](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md).
