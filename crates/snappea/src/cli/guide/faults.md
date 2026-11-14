# Timers, crashes and lossy networks

Faults are the environment, not the design. Model them in machines marked
`// snappea: driver` (on its own line before the machine): drivers are not mutated, so
mutation testing judges the design's handling of a fault, not the fault itself.

## Reusable building blocks

For bounded loss, duplication, reordering and late replies, import the published
`BBNetwork` building block instead of writing another courier. Export it with
`snappea guide example building-blocks --out vendor`, then include
`vendor/building-blocks/Network.pproj` from your project's `IncludeProject`.
The bundle's README defines its budgets and delivery contract; its consumer example
shows the import. These are ordinary P sources that you can pin and review with your
model, not built-in channel syntax. Couriers still add scheduling choices.

The recipes below explain smaller mechanisms you may need to adapt.

## Timer with cancel

```p
event eStartTimer; event eCancelTimer; event eTimeout; event eFire;

// Fires at any time after it is started, unless cancelled first. A timeout already
// sent can still arrive after a cancel: the client must ignore a late one.
// snappea: driver
machine Timer {
  var client: machine;
  start state Init { entry (c: machine) { client = c; goto Idle; } }
  state Idle {
    on eStartTimer goto Armed;
    ignore eCancelTimer, eFire;
  }
  state Armed {
    entry { send this, eFire; }
    on eFire goto Idle with { send client, eTimeout; }
    on eCancelTimer goto Idle;
    ignore eStartTimer;
  }
}
```

## Crash and restart with an epoch

Keep durable fields; reset volatile ones in the crash handler. Bump an epoch so work
queued before the crash is recognised and dropped.

```p
event eCrash; event eReq: machine; event eResp; event eWork: int;

machine Server {
  var epoch: int;               // durable: survives a crash
  var pending: seq[machine];    // volatile: lost in a crash
  start state Serving {
    on eReq do (c: machine) { pending += (sizeof(pending), c); send this, eWork, epoch; }
    on eWork do (e: int) {
      if (e != epoch) { return; }   // queued before a crash
      send pending[0], eResp;
      pending -= (0);
    }
    on eCrash do { epoch = epoch + 1; pending = default(seq[machine]); }
  }
}

// At most one crash, at any point of the run.
// snappea: driver
machine Crasher { start state S { entry (s: Server) { if ($) { send s, eCrash; } } } }
```

To model a process that dies, `raise halt` in the crash handler and have the driver
create a new machine that recovers from a separate storage machine.

## Bounded lossy, duplicating network

```p
type tMsg = (dst: machine, e: event, v: any);
event eForward: tMsg;

// Loses at most `losses` messages in all, and duplicates some.
// snappea: driver
machine Network {
  var losses: int;
  start state Up {
    entry (maxLosses: int) { losses = maxLosses; }
    on eForward do (m: tMsg) {
      if (losses > 0 && $) { losses = losses - 1; return; }
      send m.dst, m.e, m.v;
      if ($) { send m.dst, m.e, m.v; }
    }
  }
}
```

- Bound the losses. Upstream P's `UnReliableSend` may lose every message: with a
  client that retries, "every retry is lost" is a cycle the model can repeat forever,
  and a liveness spec reports it. That is a liveness trap in the model, not a bug in
  the design. Bound losses with a counter, or use `$$` for a loss the network must
  eventually stop choosing.
- Bound duplicates and crashes the same way: once each is usually enough to find bugs.

## Mutation testing and faults

- Removing an `if` on a bare `$` or `choose()` with an empty branch, or the only
  statement of its other branch, is not run: the checker already explores the run
  where the branch is skipped.
- Emptying the handler of an event that every test case sends only as the whole branch
  of `if ($)` (a crash injected with `if ($) { send s, eCrash; }`), and no spec observes,
  is reported as likely equivalent: a handler that does nothing is the history where the
  fault never happened. The report gives the suppression line to add if you agree. An
  event sent in both branches of a choice is always delivered, so it does not qualify.
- A surviving mutant under a `$` condition in a machine that stands in for the
  environment means the machine needs `// snappea: driver`.
- Best-effort operations (cancelling a timer, warming a cache, sending a hint) are
  meant to be undetectable: the design must be correct with or without them. Suppress
  their removal with a reason; one comment can name several kinds:

```p
// snappea-lint: allow mutation-nested:remove-statement,condition-false -- best-effort cancel
if (armed) { send timer, eCancelTimer; }
```
