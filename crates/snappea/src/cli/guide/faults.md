---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I model timers, crashes and lossy networks?
  - Where do fault injectors go so mutation testing judges the design?
  - How do I bound faults so liveness checks stay meaningful?
---

# Timers, crashes and lossy networks

- Mark each fault injector `// snappea: driver` (own line, before the machine), so
  mutation judges the design's handling, not the fault. Mark a timer the design arms
  too: when it fires is the environment's choice.
- Bound every fault with a counter: `$` is fair, so "every retry is lost" is not a
  liveness bug without a bound. One loss, duplicate or crash usually finds the bug.
- Network loss, duplication, reordering, late replies: import `BBNetwork`
  (`snappea guide example building-blocks --out vendor`, then include
  `vendor/building-blocks/Network.pproj` from `IncludeProject`).

## Timer with cancel

```p
event eStartTimer; event eCancelTimer; event eTimeout; event eFire;

// Fires any time after start unless cancelled. A timeout already sent can still
// arrive after a cancel: the client must ignore it.
// snappea: driver
machine Timer {
  var client: machine;
  start state Init { entry (c: machine) { client = c; goto Idle; } }
  state Idle { on eStartTimer goto Armed; ignore eCancelTimer, eFire; }
  state Armed {
    entry { send this, eFire; }
    on eFire goto Idle with { send client, eTimeout; }
    on eCancelTimer goto Idle;
    ignore eStartTimer;
  }
}
```

## Crash and restart in place

The crash handler resets volatile fields and bumps an epoch so work queued before the
crash is dropped. For a process replaced by a new machine: `snappea guide recovery`.

```p
event eCrash; event eReq: machine; event eResp; event eWork: int;

machine Server {
  var epoch: int;               // persisted: survives a crash
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

## Mutants around faults

- A survivor under `$` in an environment stand-in: mark that machine a driver.
- Emptying the handler of an event sent only as the whole branch of `if ($)`, and
  observed by no spec, is reported as likely equivalent (the fault never happened), with a suppression line.
- Best-effort operations (cancel a timer, a hint) must not be needed for
  correctness. Suppress their removal with a reason:

```p
// snappea-lint: allow mutation-nested:remove-statement,condition-false -- best-effort cancel
if (armed) { send timer, eCancelTimer; }
```
