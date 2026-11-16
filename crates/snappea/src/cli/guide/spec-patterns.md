---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I check that something eventually happens?
  - How do I catch a lock or request that is stuck forever?
  - How do I test timeouts and eventual convergence?
  - What must an independent safety spec check?
---

# Specs: liveness and independent safety checks

A check proves only what specs assert, within the bounds. Bound every fault
(`snappea guide faults`); nothing progresses if every attempt can be lost.

## Liveness: a hot spec for every "must eventually"

- A spec in a `hot` state owes something. The check fails if a run ends, or can
  repeat forever, with the spec hot. Without a hot state, a lock held forever passes.
- Name hot states after the promise (`LockPending`); failures report the name.
- Phases: `Starting` (hot, awaiting the plan), `Owing` (hot), `Settled` (cold). Start
  hot and require a nonzero plan, announced by the driver before workers start, or a
  driver doing nothing passes.
- Go cold only when all planned work has arrived and every obligation settled, not
  when the pending map is momentarily empty. Recheck after every event that can
  settle it, including crashes.
- Track requests by ID (pending map, or requested map plus replied set): on a reply,
  assert requested and not yet replied, check contents, remove. Do not just count.
  One terminal reply per request ID: import `BBRequestReplies` from
  `snappea guide example building-blocks` (see its README).

## Stuck resources: make the timeout the only way forward

Runnable: `snappea guide example lease-lock` (no-expiry lock fails liveness; leases
plus fencing tokens pass).

- For every lock, lease or slot, test a holder that crashes, or whose release is
  lost, and never returns. Only a timeout, expiry or recovery can then free it;
  removing that mechanism must fail the liveness spec.
- Drivers must not give up. A client that stops after N attempts ends the run cleanly
  while the resource stays stuck, so no spec sees it. Retry until success: a retry
  loop that never succeeds is a cycle reported in the hot state. Do not test or send
  a growing attempt counter: the state never repeats, the search hits its depth
  bound, and liveness stays unverified. A driver that must stop instead probes after
  a quiet point (below) and asserts the resource is free.
- Deliver a reply after its timeout, also after a retry. Tag replies with operation
  IDs so an old reply cannot finish new work.
- Giving up satisfies "every request ends": assert the real promise on the give-up
  event (for example, every node answered).
- Inject failures independently of the success path: a crasher started by
  timer-start vanishes with a mutant that removes timer-start.
- Prefer notifications to poll loops. Do not add a stop message that wakes blocked
  workers; it hides a missing wake-up. Let finished workers block.

## Convergence after a quiet point

For "not stale forever": stop writes, drain in-flight work through explicit
acknowledgements or a barrier, announce `eQuiesced`, then read and compare with the
promised state. Silence or a timer does not prove draining. This checks convergence
after a bounded quiet phase only.

## Safety: check independently

- Derive expected results from the contract, the driver's announced intent and the
  history. Never reuse a design helper or trust the design's `valid` flag: a mutant
  changes both sides.
- Compare outgoing requests and replies with the intent (kind, content, base version,
  correlation ID); a correct final result can hide a wrong request.
- Announce at the store's commit, not after the reply. Where an announcement cannot
  prove a state change, check a later public read.
- Assert that generated IDs are unique. Assert only promised constraints. Check
  invariants from initialization on; turn assumptions in comments into assertions.

Examples: `snappea guide example bounded-history` (`zero-work.jsonl` and
`incomplete.jsonl` fail), `snappea guide example durable-retry`
(`invalid-premature-done.jsonl` fails).
