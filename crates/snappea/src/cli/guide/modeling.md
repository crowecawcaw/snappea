---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What part of a design should I model, and at what level of detail?
  - How many models should a design have, and how do they fit together?
  - How do I use models to find bugs in existing code?
  - Which atomic steps and interference must the model keep?
  - How do I keep the search small without hiding the bug?
---

# What to model

## Scope

- Model the algorithm, not its API: an atomic API spec does not check the
  read/CAS/retry protocol that implements it.
- Name actors, shared state, real primitives, and the IDs and versions that matter.
  Start with two competing clients, one resource, a few values. Keep each
  distinction that permits the defect.
- State atomicity, delay, duplication, persistence, bounds and omissions, and
  whether each omission could change the property.

## Split by concern, join at seams

Model what you want to validate, not the whole system. A whole-system model's search
does not finish, its first failure hides the others, and each change is expensive.

- One model per part of the design with its own state and its own failure modes:
  usually 1-3 per design. Not one per risk: risks that run through the same
  machinery share a model, with one test case per risk selecting that risk's specs.
- Split again only when a risk needs different actors, or adding it keeps the search
  from completing.
- Abstract what a model leaves out to its contract; state that assumption in
  DESIGN.md. Seams: where two models meet, one assumes a property and the other's
  spec asserts it. If no spec can, write a small model of just that interaction.
  Bugs often live at seams; list them.
- Specs stay shared with the implementation: one implementation test can check
  observed events against the specs of every model (`snappea guide python`).

Payment API with retries, two models:

- A, request path: client, API server, idempotency-key store. Lost replies,
  timeouts, retries, duplicate delivery, a crash between recording the key and
  replying. The charge is "an effect applied once per key".
- B, ledger: accounts, holds, captures and refunds running concurrently. Assumes
  each request arrives exactly once.
- Seam: B's exactly-once assumption is what A's spec "each key's effect is applied
  at most once" asserts.

One model is enough when every risk shares the machinery. A lease lock: crashed
holder, lost renewal or release, a holder waking after expiry, and restart all run
through one lease, so they are test cases on one model, each selecting its spec
(`snappea guide example lease-lock`).

## Reviewing existing code

Same rule, aimed at finding bugs:

1. List hypotheses: each `snappea guide checklist` item that could fail here.
2. Group them by the machinery they need; one model per group.
3. One test case and spec per hypothesis, written to fail if the bug is there. Mark
   a confirmed bug `// snappea: expect-bug "<message>"` so the others still run.
4. Only after every hypothesis in a group was tried, add a fixed variant and show
   the fix holds (`snappea guide scenarios`, design variants).

Job queue with a scheduler and workers: model 1, worker lifecycle (claim, heartbeat,
reassignment on timeout, cancel); model 2, result commit (write result, retry,
duplicate completion, crash between write and ack). Seam: the lifecycle model
assumes a reassigned job's old worker cannot commit; the commit model's spec checks
it (a fencing token or epoch on the write).

## Keep the interference

- Scheduling points are `send`, `receive` and `new`; assignments and `announce` do
  not yield. The point comes before the operation: at `send`, specs see the event,
  others may run, then it is queued. A dequeued event may wait while others run.
- So `announce eDone; send c, eReply` lets others act in between. Announce plans
  before the `new` that starts machines acting on them.
- A non-yielding group stands for one real atomic primitive (transaction, locked
  update, CAS). Split read, decide and write into messages when another actor can
  act in between; keep prepare, validate and publish separate when it matters.
- The store's isolation level is part of the model: one handler is serializable. If
  the real store is weaker, split the transaction's reads and writes.
- A per-resource model cannot check a global limit without the competing resources.
- A direct `send` arrives in causal order. For lag or a slow link, add a forwarding
  machine on that path.
- Runnable: `snappea guide example reservation`.

## Keep the search small

- Each extra concurrent actor, hop, retry loop or fault budget can multiply
  schedules 10-100x. Merge machines whose hop adds no order the contract cares about.
- Fix inputs (`choose`, `$`) in the driver before creating workers.
- Split fault classes (loss, duplication, crash) into separate tests unless the bug
  needs them together.
- To force a rare order, add a test-only machine that holds a message until another
  actor finishes.
- When search does not finish, `snappea check` shows where schedules branch; reduce
  the top ones first. `-v` shows this and collection sizes for every test case.

A passing search covers only the modeled transitions and bounds. Map each modeled
read, write, version check, retry and restart to its real primitive.
