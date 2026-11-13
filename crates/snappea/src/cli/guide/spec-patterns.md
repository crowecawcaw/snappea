---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - What must an independent spec check?
  - How do I track completion without going cold too early?
  - How do I test timeouts and eventual convergence?
---
# Pin down the contract

Use this checklist before accepting mutation survivors. A passing model only proves
what its specs actually require, within the modeled bounds.

- Compare the design's outgoing requests, decisions and replies with the driver's
  announced intent: operation kind, content, base version and correlation ID. A
  correct final result can hide an incorrect request.
- Assert generated IDs are unique. Track replies by ID so two replies for one
  request cannot stand in for the missing reply to another.
- Turn assumptions in comments into assertions, including who may retry or rejoin.
- Check an invariant from the moment it must hold, including initialization and
  intermediate phases, not only after a final phase flag.
- Decide whether fairness or bounded wait is part of the contract before suppressing
  survivors in a policy function. Progress alone does not specify a scheduling policy.
- Derive expected results independently. Sharing a design helper with the spec can
  change both sides of a comparison under mutation and hide a bug.

## Reusable building blocks

Import `BBRequestReplies` when the contract is one terminal reply per unique logical
request ID. Export `snappea guide example building-blocks --out vendor`, include its
`RequestReplies.pproj`, and select the spec in your test. The guide bundle's README
explains the `BBPlan`, `BBRequested` and `BBReplied` adapter events and a runnable
consumer. Use the block directly rather than copying another completion counter.
It checks planned completion and IDs; keep independent specs for reply contents.
Retries reuse a logical ID, and only actual terminal replies discharge obligations.

## Declare the work before starting it

A spec that starts cold can pass when the driver never does any work. Start hot,
require a nonzero plan, then announce each actor's intent before sending its request.
Keep the plan separate from observations emitted by the design: removing the design's
work must not remove the expectation too.

`snappea guide example bounded-history` contains a runnable `CapacityOracle`:
`eBegin` declares the client and command counts before clients are created;
`eRequested` records each command; independent maps and sets check decisions,
replies and inspections. The final client completion checks every planned count.
Its `zero-work.jsonl` and `incomplete.jsonl` probes demonstrate failures when no work
is planned or a promise remains outstanding.

A counter and a hot state suffice for interchangeable units of work. For requests
with identities, use a pending map or a requested map plus a replied set. On reply,
assert the ID was requested and has not replied already, check its contents, then
remove that obligation. Do not merely decrement a global count.

## Keep the spec hot until the whole plan settles

Think of three phases: `Starting` (hot, waiting for the plan), `Owing` (hot, waiting
for the promised effects and replies), and `Settled` (cold). Name hot states after
the promise they track, such as `TransferPending`; liveness failures report that name.

Do not go cold just because the pending map is temporarily empty: another actor may
not have announced its request yet. Require both all planned work to have arrived
and all its obligations to have settled. Recheck completion after every event that
can satisfy its condition, including crashes if they discharge planned work.

`snappea guide example durable-retry` demonstrates completion across retries and
crashes; `invalid-premature-done.jsonl` rejects a completion announcement before all
requested effects commit. `snappea guide example bounded-history` handles concurrent
IDs, and `snappea guide syntax` gives a smaller sequential example. Export any example
with `snappea guide example NAME --out DIR`; its header lists the passing and broken
checks to run.

## Make the timeout necessary in a test

Give every timeout a test where it is the only way forward, for example a holder
that crashes without releasing its lease. Removing the timer must fail a progress
spec. Also deliver a reply after the timeout, including after a retry or a later
operation; tag replies with operation IDs so an old reply cannot finish new work.
See `snappea guide faults` for timer and network recipes.

A timer can fire before useful work completes. State the actual promise: for example,
if giving up is allowed only after every node answered, record answers per node and
assert that condition on the give-up event. A timeout itself is not evidence that
all replies are lost. Bound failures, or state the fairness assumption needed for
progress; no protocol can guarantee a reply if every attempt may be lost forever.

Inject failures independently of the design's own success path. A crasher triggered
only by timer-start will disappear along with a mutant that removes timer-start.
Do not add a stop message merely to end a test if it also wakes blocked workers:
that can hide a missing wake-up. Let workers block when done; finite execution can
end when no machine has runnable work, while hot specs still detect unfinished work.

## Probe convergence after a modeled quiet point

To check “not stale indefinitely” in a finite test, stop new writes, wait for modeled
in-flight work to drain using explicit acknowledgements or a barrier, announce
`eQuiesced`, then perform a real read and compare it with the promised final state.
Silence or an arbitrary timer does not establish that all messages have drained.
This checks convergence after that bounded quiet phase, not unbounded eventual
consistency under continuing writes.

Extra probe clients and replies can multiply schedules. If the contract is about
the node's value at read time, an observation at that node can be cheaper than a
round-trip reply. Keep a real reply when reply delivery is part of the promise.
