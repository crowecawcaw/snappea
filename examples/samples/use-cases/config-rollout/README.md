---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a staged config rollout with a canary?
  - How do I model a rollback when pushes can be delayed and reordered?
  - How do I check that every node ends up on the latest version?
---

# Staged config rollout with canary and rollback

A config service rolls versioned configs out to three nodes in two stages: the canary
(node 0) first, then, once the canary acknowledges, the other two. A node applies a
pushed config only if its version is newer than the one it runs, and acknowledges
every push. The operator can issue a new version at any time, including mid-rollout;
the service abandons the current rollout and starts over at the canary. Pushes travel
on separate connections, so they are delayed and two pushes to one node can arrive in
either order.

**Rollback is a new, higher version that carries the old content.** Versions then only
go up, nodes keep one rule ("apply only newer versions"), and a delayed push of the bad
version can never be mistaken for the rollback. A node-side "rollback to version 1"
command would need a second rule and would race with the bad version's pushes still
in flight.

**Contract**

- A node applies only versions newer than what it runs, only versions pushed to it,
  with the content that version was issued with.
- The service pushes only the latest issued version (after a rollback, nothing pushes
  the bad version again) and pushes beyond the canary only after the canary applied it.
- Progress: once every planned version is issued, every node eventually runs the
  latest one, and the service declares that rollout done (only when it is).

**Atomic steps**: the service handles one message at a time; a node applies a config
atomically. Each push is a separate `Courier` machine, which is what lets pushes
overtake each other.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `RolloutSafety` and `RolloutCompletes` (progress, hot state) |
| [`PSrc/ConfigService.p`](PSrc/ConfigService.p) | The service, the node, and `Courier` (one push in flight) |
| [`PSrc/LastArrivalNode.p`](PSrc/LastArrivalNode.p) | Negative control: a node that applies whatever arrives last |
| [`PTst/Tests.p`](PTst/Tests.p) | The operator, a scenario, the test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcStagedRollout` | Version 2 to three nodes | The canary gate; acks; completion | pass, exhaustive |
| `tcRollbackDuringCanary` | Version 2, then rollback as version 3 (version 1's content) at any moment | Rollback while version 2's pushes are in flight; pushes of 2 and 3 to one node in either order; late acks of version 2 | pass, exhaustive when given ~90s (about 112,000 schedules); sampled under shorter budgets |
| `tcBugLastArrival` | `LastArrivalNode` in the rollback scenario | **Expected to fail**: `node 0 applied version 2 after version 3` | fail |

The `RollbackOvertookPush` scenario confirms a node applied version 3 while a push of
version 2 to it was still in flight (the rollback overtook the bad version).

## What building it taught

- **Announce completion.** The first service went quietly back to idle when all acks
  were in. Mutation showed that clearing the ack set, skipping the canary in the
  second stage, or never finishing were all invisible. The service now announces
  `eRolloutDone`, and the specs check it is true (every node runs that version) and
  that it happens (progress waits for it).
- **Tie applies to pushes.** Without "a node applies only what was pushed to it", the
  service's `ePushSent` announcement could be removed and the canary gate silently
  stopped being checked.
- **Nodes report what they run at start** (`eNodeStarted`), so the initial version is
  part of the checked history rather than an unchecked constructor argument.
- A separate network machine between the service and the couriers doubled the
  schedules; creating the courier directly keeps the rollback test exhaustive.

## Run

```sh
snappea check tcStagedRollout tcRollbackDuringCanary --timeout 2m --deny mutation
snappea check tcBugLastArrival      # exits 1: node 0 applied version 2 after version 3
```

## Bounds and omissions

Three nodes, one canary, at most two new versions. Pushes are delayed and reordered
but never lost or duplicated, so the service never retries. Not modeled: push loss and
retry, node restarts (a node losing its applied version), health checks deciding
whether to continue past the canary, percentage-based stages.

Suppressed mutants, both equivalent here:

- `p.version > version` → `>=` in `Node`: each version is pushed to each node once,
  so a push never equals the running version. With at-least-once delivery this would
  matter; add duplicate delivery to the courier if your network duplicates.
- Removing `goto Idle` after the rollout completes: once every ack is in, `Rest` and
  `Idle` handle every event alike.

## Testing the implementation

Emit `eIssued`, `ePushSent` and `eRolloutDone` from the service, `eNodeStarted` and
`eApplied` from the node agent. To force the races: hold the version 2 push to the
canary in a proxy, issue the rollback, deliver version 3, then release version 2 (the
node must ignore it); release a canary's version 2 ack after the rollback was issued
(the service must not advance version 2 to the other nodes).
