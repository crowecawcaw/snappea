---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - Why does "write the database, then publish an event" lose events?
  - How do I model a process crash and restart in P?
  - How does a transactional outbox fix it, and why does the consumer still need to deduplicate?
---

# Dual write: commit, then crash before publishing

A service handles an update by committing it to its database and then publishing an
event to a broker, so downstream services (search index, billing, notifications) learn
about it. The client gets an answer after both steps.

**Difficulty: medium.** Every line is correct; the bug is the gap between two durable
systems that no transaction spans.

**Contract.** Every committed change reaches downstream: it is published only if it
committed, with the committed value, and applied downstream exactly once. Every update
gets an answer, and a confirmed update has committed. (A reset connection leaves the
outcome unknown, which is allowed.)

**Atomic primitives**: one database transaction; the broker accepting a message. A
crash loses the process's memory (the in-flight request); the database and the broker
keep what they stored. The fault injector (`Crasher`) crashes the service, or the relay,
at most once, at a moment the checker picks.

## The flaw

The commit and the publish are two separate steps, and nothing durable records that a
publish is owed, so a crash between them leaves a committed change no event will ever
announce.

```text
db.commit(change);        // durable
broker.publish(change);   // the process can die before this line
return ok;
```

## The counterexample

From `tcBugCrashBetweenWriteAndPublish` (one update):

1. The client sends update 1 (value 101).
2. The service sends the commit; the database commits change 1.
3. The service crashes before it hears the result. The client's connection drops, so it
   sees an error.
4. The restarted service has no memory of change 1. It never publishes it.
5. The run ends with change 1 committed and downstream unaware of it:

```text
PImplementation.CommittedChangesPublished detected liveness bug in hot state 'CommittedNotPublished' at the end of program execution.
```

## Why ordinary tests miss it

Crashes in tests happen at test boundaries, not between two lines of a handler. Even
chaos testing rarely kills a process in that exact window, and when it does, the result
is a silently missing event, not an error: the client saw a failure and moved on, and
nobody compares the database with what downstream knows. The checker crashes the service
at every point and checks the contract at the end of each run.

## The fix

A transactional outbox. `OutboxService` commits the change and an outbox row in one
transaction, then answers; it never talks to the broker. `OutboxRelay` reads unpublished
rows (when the database notifies it, and on every start), publishes each, and marks it
published after the broker accepts it. A crash of the service cannot separate the change
from its outbox row. A crash of the relay loses nothing durable: the restarted relay
reads the outbox again.

The relay can crash after the broker accepted a row and before marking it; the row is
then published twice. The outbox gives at-least-once delivery, so the consumer skips
changes it already applied, by change id. `tcFixedRelayCrash` reaches that history (the
`DeliveredTwice` scenario confirms it) and the contract still holds.

Both processes tag requests with an epoch that a restart changes, so a reply addressed
to the process before the crash is ignored. Without it, a restarted service confirms its
next update with the previous update's commit result (`tcFixedStaleCommitResult`).

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `CommittedChangesPublished` (hot until every commit is applied downstream), `UpdatesAnswered` (hot until every update is answered), `DeliveredOnceWithoutRelayCrash` |
| [`PSrc/Common.p`](PSrc/Common.p) | Shared events; `EventStream`: the broker and the deduplicating consumer |
| [`PSrc/DualWriteService.p`](PSrc/DualWriteService.p) | Flawed: database, then publish |
| [`PSrc/Outbox.p`](PSrc/Outbox.p) | Fixed: database with outbox table, service, relay |
| [`PTst/Tests.p`](PTst/Tests.p) | Client, `Crasher`, `CrashAfterCommit`, drivers, the `DeliveredTwice` scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcFixedServiceCrash` | Outbox design, one update, service may crash anywhere | The same fault as the bug test, against the fix | passes |
| `tcFixedStaleCommitResult` | Service crashed right after each commit, two updates, no relay | A late commit result must not confirm the next update | passes |
| `tcFixedTwoUpdates` | Two updates, no faults | Commits, notifications and relay reads interleave; no duplicates without a crash | passes |
| `tcFixedRelayCrash` | One update, relay may crash anywhere | Crash between publish and mark: delivered twice, applied once | passes |
| `tcBugCrashBetweenWriteAndPublish` | Flawed design, one update, service may crash anywhere | **Expected bug**: `CommittedNotPublished` | Expected bug found |

## Run

```sh
snappea check DualWriteCrash.pproj 'tcFixed*' --deny mutation
snappea check DualWriteCrash.pproj tcBugCrashBetweenWriteAndPublish   # succeeds only when the declared bug is found
```

The negative controls use native `snappea: expect-bug` declarations. A matching
failure passes the test; no failure or a different failure fails it. Search coverage
depends on the budget: claim exhaustion only when the current run reports completion.

## Bounds and omissions

One crash per run, of one process; at most two updates; one relay. The database and the
broker never fail and messages are not lost. Clients do not retry after a reset. Not
modeled: several relays (they need a lease or `SELECT ... FOR UPDATE SKIP LOCKED`),
outbox cleanup, ordering guarantees across changes, a relay crash with two unpublished
rows (the state space grows past a few seconds).

Suppressions, all for equivalent mutants: emptying a crash handler or `Restart` (the
history where no crash happened, which the checker explores anyway); `epoch - 1` for
`epoch + 1` (any unused epoch works); forcing the relay's epoch check to true (a stale
acknowledgement is always for the row the restarted relay publishes first, because
reads return rows in id order).

## Testing the implementation

Emit `Committed` from the transaction, `Delivered`/`Applied` from the consumer, and
the client's request and answer. To force the bug, kill the service process (or throw)
right after the commit returns; with the outbox, assert the relay still publishes. Kill
the relay between the broker's acknowledgement and the mark, restart it, and assert the
consumer applies the change once.
