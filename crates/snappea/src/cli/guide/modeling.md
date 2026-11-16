---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What part of a design should I model, and at what level of detail?
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
