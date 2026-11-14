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

**Difficulty: Medium.** Every line is correct; the bug is the gap between two durable
systems that no transaction spans.

**The flaw in one sentence:** the commit and the publish are two separate steps, and
nothing durable records that a publish is owed, so a crash between them leaves a
committed change no event will ever announce.

```text
db.commit(change);        // durable
broker.publish(change);   // the process can die before this line
return ok;
```

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `CommittedChangesPublished` (safety and progress, hot state
  `CommittedNotPublished`): a change is published only if it committed, with the
  committed value, and every committed change is applied downstream exactly once.
- `UpdatesAnswered` (progress, hot state `Answering`): every update gets an answer,
  a change commits only with the value its client asked for, and a confirmed update
  has committed. A reset connection leaves the outcome unknown, which is allowed.
- `DeliveredOnceWithoutRelayCrash` (safety, selected only where the relay cannot
  crash): the broker delivers each change once.

**Atomic primitives**: one database transaction; the broker accepting a message. A
crash loses the process's memory (the in-flight request); the database and the broker
keep what they stored. The fault injector (`Crasher`) crashes the service, or the relay,
at most once, at a moment the checker picks.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The three specs and the events they observe |
| [`PSrc/Common.p`](PSrc/Common.p) | Shared events; `EventStream`: the broker and the deduplicating consumer |
| [`PSrc/DualWriteService.p`](PSrc/DualWriteService.p) | Flawed: database, then publish |
| [`PSrc/Outbox.p`](PSrc/Outbox.p) | Fixed: database with outbox table, service, relay |
| [`PTst/Tests.p`](PTst/Tests.p) | Client, `Crasher`, `CrashAfterCommit`, drivers, the `DeliveredTwice` scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugCrashBetweenWriteAndPublish` | Flawed design, one update, service may crash anywhere | A crash between commit and publish loses the event | Expected bug: `CommittedChangesPublished detected liveness bug in hot state 'CommittedNotPublished'` |
| `tcFixedServiceCrash` | Outbox design, one update, service may crash anywhere | The same fault as the bug test, against the fix | Passes |
| `tcFixedStaleCommitResult` | Service crashed right after each commit, two updates, no relay | A late commit result must not confirm the next update | Passes |
| `tcFixedTwoUpdates` | Two updates, no faults | Commits, notifications and relay reads interleave; no duplicates without a crash | Passes |
| `tcFixedRelayCrash` | One update, relay may crash anywhere | Crash between publish and mark: delivered twice, applied once | Passes |

## The counterexample

From `tcBugCrashBetweenWriteAndPublish` (one update):

1. The client sends update 1 (value 101).
2. The service sends the commit; the database commits change 1.
3. The service crashes before it hears the result. The client's connection drops, so it
   sees an error.
4. The restarted service has no memory of change 1. It never publishes it.
5. The run ends with change 1 committed and downstream unaware of it:

```text
CommittedChangesPublished detected liveness bug in hot state 'CommittedNotPublished' at the end of program execution.
CommittedChangesPublished's fields: committed = {1: 101}, delivered = {}, applied = {}
```

The service waits in state handlers, not in a blocking `receive`, so nothing is
blocked: the run simply ends. Only the hot state of `CommittedChangesPublished` turns
the missing event into a failure, and its name says which promise was broken.

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

## Run

```sh
snappea check DualWriteCrash.pproj            # everything: the fix, the negative controls, coverage, mutation testing
snappea check DualWriteCrash.pproj 'tcBug*'   # only the negative controls: each passes only when its declared bug is found
```

Each negative control carries a native `// snappea: expect-bug` declaration: a matching
failure passes the test; no failure or a different failure fails it. Describe a
search as exhaustive only when its report says `search_complete`.

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
