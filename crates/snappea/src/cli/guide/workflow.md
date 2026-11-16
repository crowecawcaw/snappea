---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I use P to design and test a feature with shared state, retries or recovery?
  - In what order do I model, check, implement and test?
  - When is a model done?
---

# Model a design with P, then test the code against it

Use P where bugs depend on ordering or failure: shared state, retries, concurrency,
timeouts, recovery. Do the steps in order. Read each guide page whole, not through
`head`. Topics: `snappea guide --help`. Runnable models: `snappea guide examples`.

## 1. Write the risks in DESIGN.md, before code

- The contract as checkable sentences ("a retry returns its first result and has no
  further effect").
- The real atomic primitives (one CAS on one key, a lock, a transaction). Everything
  else interleaves.
- The failure model: can messages be lost, duplicated, reordered or late? Can a node
  crash or restart mid-operation, and what survives? What do timers, leases and
  retries do? Answer every item of `snappea guide checklist`.
- 5-10 scenarios that could break the contract (`snappea guide scenarios`).
- What must eventually happen, and what could block it forever (a holder crashes, a
  release or reply is lost). Safety assertions cannot find these; each needs a
  liveness spec (`snappea guide spec-patterns`).

## 2. Model each concern's algorithm, not the API

- Model what you want to validate, not the whole system: one model per part with its
  own state and failure modes (usually 1-3), one test case per risk. Abstract what a
  model leaves out to its contract; where models meet, one model's spec asserts what
  another assumes (`snappea guide modeling`).
- A store machine holding exactly what the real store holds; its handlers are the
  real primitives (`load` returns (version, value); `cas` succeeds only at the
  current version).
- Client machines run the algorithm: load, receive, decide, cas, retry. A whole
  operation in one handler assumes the race away (`snappea guide modeling`).
- Faults come from bounded driver machines (`snappea guide faults`).
- A safety spec computing expected results from the driver's announced requests,
  not design code; a hot-state liveness spec per "must eventually".
- 2-3 clients, small `choose(n)` inputs, a fixed test per scenario. Clients retry
  until they succeed; giving up after N attempts hides a stuck state. Shape:
  `snappea guide example cas-retry`. Syntax: `snappea guide syntax`.

Reviewing existing code instead: list hypotheses from the checklist, group them by
machinery, one model per group and one test per hypothesis, expecting failure; write
a fixed variant only after the group is tried (`snappea guide modeling`).

## 3. Check until it passes for real

    snappea check A.pproj    # every test case of one model, then coverage

- Exit 1: a bug (fix the design, keep the scenario) or a coverage gap (add a
  scenario). Exit 0 with `summary.incomplete` > 0 is not a pass.
- A failing test lists up to 3 distinct failures (`--max-bugs`) and every spec still
  hot at the failure: read them all before changing the model. Lint failures do not
  stop the search; bugs come first.
- Do not finish with `--warn coverage`. Incomplete search: shrink bounds, split the
  model, or raise `--timeout`.
- Mutation testing is optional: `snappea check A.pproj --mutation` changes the model
  slightly and reports changes no test case detects. Use it once the model passes, to
  judge whether the specs check the contract. Act on an undetected mutant only when
  it shows a check the contract needs; suppress an equivalent one with a reason.
- Without a hot spec state the check is safety only: a lock stuck forever passes.
  Keep a spec hot until each liveness risk from step 1 resolves, and give each
  timeout, lease or retry a test where it is the only way forward.
- A bug found here is the payoff: record it, fix the design, then implement.

## 4. Implement, then test what the models explored

Force each scenario and model bug on the real code, add an observation test against
the P specs of every model, then break the code on purpose until every break fails a
test (`snappea guide testing`, `snappea guide python`).

## Done means

- `snappea check` exits 0 for every model, with complete search or stated bounds.
- Implementation tests pass, including forced-interleaving and observation tests;
  every deliberate break fails one.
- DESIGN.md (or the report) states the contract, primitives, failure model,
  models with their assumptions and seams, scenarios, bounds, what each model found
  and what is not covered, and answers every `snappea guide checklist` item: yes /
  no / not applicable, with the model that covers it or a reason.
