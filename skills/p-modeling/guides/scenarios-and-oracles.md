# Scenarios and independent oracles

## Expectations

- Assert only promised constraints; allow contract-valid alternatives to incidental implementation choices. Keep algorithm-specific invariants distinct from the external contract.
- Derive expected state from the external contract, inputs, and history; do not trust an implementation's `valid` flag.
- Example: subtract committed reservations from initial stock; reject negative capacity independently of storage's decision logic. See [reservation](../examples/reservation/README.md).
- Check later public reads when an announcement alone cannot establish that state changed.
- A broken variant validates the oracle only if it fails for the intended property, not a syntax error or unrelated assertion.

## Scenarios

- Contention: both clients read the same resource before either writes.
- Ambiguous commit: effect commits, response is lost, restart, retry the same ID.
- Old replay: complete A, let B change the same resource, retry A; distinguish A's saved result from current state.
- For **each acceptance precondition**, make it false while all others hold; combined invalid conditions can hide missing enforcement.
- Cover each meaningful valid value class and the output partition boundaries (for example, below/exactly-at/above capacity), not just one representative input.
- Exercise operations on both sides of lifecycle boundaries; querying state alone does not test enforcement.
- Exercise stale operations against a successor's live state, not only empty/expired state.
- Include stale/rejected operations and useful success. Always rejecting can satisfy safety while violating the contract.
- Use bounded choices that preserve these relationships; schedule count alone says little.
- Bound a history generator's input IDs and values to preserve contract-relevant cases; let the checker explore schedules independently. Check a few manually calculated valid and invalid histories against the oracle before relying on generated histories. See the [bounded history generator](../examples/bounded-history/README.md).

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
- Explain intentional exceptions. Do not suppress unresolved risks to obtain a passing command.
- Report asserted properties, bounds, assumptions, completion, and unresolved findings.

## Progress

- Safety: “no duplicate effect.” Progress: “accepted work eventually completes.” Check useful success where promised, not merely a response.
- State delivery, scheduling, and recovery assumptions. Infinite outages cannot guarantee completion.
- Hot states detect outstanding obligations at checked execution ends when enabled; they do not establish scheduler fairness.
- See [checking models](https://github.com/crowecawcaw/snappea/blob/main/docs/checking-models.md).
