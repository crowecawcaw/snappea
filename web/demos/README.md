---
diataxis: reference
personas: [contributor]
questions:
  - Where do Playground examples come from?
  - How are the readable examples validated?
---

# Playground examples

These are editable P models maintained for readers of the Playground. Bank,
commit, espresso, detector, Paxos and Raft are readable adaptations of the matching
`tests/upstream/Tutorial` projects; shared environment models come from its Common
folder. Retry adapts `design/time/RetryBudget`. Hello is the local lost-update
example. Upstream fixtures remain unchanged for compatibility testing.

Event names describe messages without an `e` prefix. Payload types use meaningful
names (with `Data` where needed to distinguish them from events). Handlers and
branches have one statement per line, and comments explain actor roles, model
choices and rules. Paxos uses acceptors and learners instead of jury/school.
The hello client exposes Reading, Writing and Finished as explicit states.

Check each model with all its `.p` files and the test named in `src/demos.ts`.
Commit and detector also need both `common` folders. Retry needs
`--latency web/demos/retry/PTst/latency.toml --timed`. Use `--warn all` to match the
Playground's exploratory defaults. A failing check is expected for deliberately
buggy examples; compilation failures are not. Names in latency profiles must
stay aligned with event declarations.
