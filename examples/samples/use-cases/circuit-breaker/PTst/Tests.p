// The environment (the flaky dependency, the timer service, the callers) and the test
// cases. Only the breaker is the design under test; everything here is a driver.

// The dependency answers calls in the order they arrive, from a script of outcomes
// (true = success); past the script it succeeds. With `reorder`, each answer travels
// back on its own connection (a DepReply machine), so answers can overtake each other:
// a slow failure can arrive after later calls were answered. Without it, answers come
// back in order, which keeps the search small where reordering is not the point.
machine Dependency {
  var script: seq[bool];
  var reorder: bool;
  var served: int;

  start state Serving {
    entry (cfg: (script: seq[bool], reorder: bool)) {
      script = cfg.script;
      reorder = cfg.reorder;
    }
    on eDepCall do (c: (breaker: machine, call: int)) {
      var ok: bool;
      ok = true;
      if (served < sizeof(script)) { ok = script[served]; }
      served = served + 1;
      announce eDepAnswered, (call = c.call, ok = ok);
      if (reorder) {
        new DepReply((breaker = c.breaker, call = c.call, ok = ok));
      } else {
        send c.breaker, eDepResult, (call = c.call, ok = ok);
      }
    }
  }
}

machine DepReply {
  start state Sending {
    entry (r: (breaker: machine, call: int, ok: bool)) {
      send r.breaker, eDepResult, (call = r.call, ok = r.ok);
    }
  }
}

// The timer service fires each cooldown once, whenever the scheduler runs it after the
// breaker asked. With `duplicates`, the timer library delivers every firing twice
// (at-least-once delivery).
machine TimerService {
  var duplicates: bool;

  start state Serving {
    entry (d: bool) {
      duplicates = d;
    }
    on eStartCooldown do (t: (breaker: machine, period: int)) {
      announce eCooldownElapsed, (period = t.period,);
      send t.breaker, eCooldown, (period = t.period,);
      if (duplicates) {
        send t.breaker, eCooldown, (period = t.period,);
      }
    }
  }
}

// A caller makes `calls` calls one after another. A rejected call is retried at once,
// with a new call id, up to `retries` times (a caller polling until the breaker lets it
// through). Call ids start at `firstId` so callers never share one.
type tCaller = (owner: machine, breaker: machine, firstId: int, calls: int, retries: int);
// Tells the caller's owner (the test's main machine) that the caller finished.
event eCallerFinished;

machine Caller {
  var cfg: tCaller;
  var next: int;
  var made: int;
  var retriesLeft: int;

  start state Calling {
    entry (c: tCaller) {
      cfg = c;
      next = c.firstId;
      retriesLeft = c.retries;
      Call();
    }
    on eCallResult do (r: (call: int, outcome: tOutcome)) {
      assert r.call == next - 1, format("caller got an answer for call {0}, it is waiting for {1}", r.call, next - 1);
      announce eCallAnswered, r;
      if (r.outcome == Rejected && retriesLeft > 0) {
        retriesLeft = retriesLeft - 1;
        Call();
        return;
      }
      made = made + 1;
      retriesLeft = cfg.retries;
      if (made < cfg.calls) {
        Call();
        return;
      }
      announce eCallerDone;
      send cfg.owner, eCallerFinished;
    }
  }

  fun Call() {
    announce eCallStarted, (call = next,);
    send cfg.breaker, eCall, (caller = this, call = next);
    next = next + 1;
  }
}

fun Script(a: bool, b: bool, c: bool, d: bool): seq[bool] {
  var s: seq[bool];
  s += (0, a);
  s += (1, b);
  s += (2, c);
  s += (3, d);
  return s;
}

fun NewBreaker(script: seq[bool], reorder: bool, threshold: int, duplicates: bool): Breaker {
  announce eBreakerPlanned, (threshold = threshold,);
  return new Breaker((dependency = new Dependency((script = script, reorder = reorder)),
                      timers = new TimerService(duplicates),
                      threshold = threshold));
}

// Two callers, three calls, against a dependency that fails twice, then recovers
// (threshold 2): calls in flight when the breaker opens, rejections, the trial. Every interleaving of calls, answers and the cooldown.
machine ConcurrentCallers {
  start state Init {
    entry {
      var b: Breaker;
      b = NewBreaker(Script(false, false, true, true), false, 2, false);
      announce eCallersPlanned, 2;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 2, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
    }
    ignore eCallerFinished;
  }
}

// One failure opens the breaker (threshold 1). Only then do two callers start, each
// retrying a rejected call, so both can reach the half-open window together.
machine HalfOpenRace {
  var b: machine;
  start state First {
    entry {
      b = NewBreaker(Script(false, true, true, true), false, 1, false);
      announce eCallersPlanned, 3;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 1, retries = 0));
    }
    on eCallerFinished goto Racing;
  }
  state Racing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 0));
    }
    ignore eCallerFinished;
  }
}

// Three concurrent calls all fail (threshold 2), and answers can overtake each other.
// Once two callers are answered (the breaker opened), the third answer may still be in
// flight. Then caller 1 calls twice: a trial succeeds and closes the breaker, then one
// failure. The slow failure may land after that; counting it would reopen the breaker
// after a single failure of the new closed period.
machine LateFailure {
  var b: machine;
  var finished: int;
  start state Opening {
    entry {
      var s: seq[bool];
      s = Script(false, false, false, true);
      s += (4, false);
      b = NewBreaker(s, true, 2, false);
      announce eCallersPlanned, 4;
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 40, calls = 1, retries = 0));
    }
    on eCallerFinished do {
      finished = finished + 1;
      if (finished == 2) { goto Reclosing; }
    }
  }
  state Reclosing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 10, calls = 2, retries = 0));
    }
    ignore eCallerFinished;
  }
}

// One caller, threshold 2, answers fail, succeed, fail, fail: the success resets the
// count, so only the fourth call opens the breaker. A fifth call is rejected or is the
// trial.
machine SuccessResetsCount {
  start state Init {
    entry {
      var b: Breaker;
      b = NewBreaker(Script(false, true, false, false), false, 2, false);
      announce eCallersPlanned, 1;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 5, retries = 0));
    }
    ignore eCallerFinished;
  }
}

// Every cooldown firing is delivered twice. The trial fails and the breaker reopens;
// the duplicate of the first cooldown can arrive during the trial or in the second
// open period, and must not start a half-open window early.
machine DuplicateCooldown {
  start state Init {
    entry {
      var b: Breaker;
      b = NewBreaker(Script(false, false, true, true), false, 1, true);
      announce eCallersPlanned, 2;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 3, retries = 2));
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 1));
    }
    ignore eCallerFinished;
  }
}

machine BuggyHalfOpenRace {
  var b: machine;
  start state First {
    entry {
      announce eBreakerPlanned, (threshold = 1,);
      b = new AdmitAllBreaker((dependency = new Dependency((script = Script(false, true, true, true), reorder = false)),
                               timers = new TimerService(false), threshold = 1));
      announce eCallersPlanned, 3;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 1, retries = 0));
    }
    on eCallerFinished goto Racing;
  }
  state Racing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 1));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 1));
    }
    ignore eCallerFinished;
  }
}

// Covered: a result sent before the breaker opened arrived after it had closed again.
scenario LateResultAfterReclose observes eDispatched, eBreakerChanged, eResultSeen {
  var early: set[int];
  var transitions: int;
  start hot state WaitOpen {
    on eDispatched do (d: (call: int)) { early += (d.call); }
    on eBreakerChanged do (c: (mode: tBreakerState)) { if (c.mode == Open) { goto WaitClose; } }
    ignore eResultSeen;
  }
  hot state WaitClose {
    on eResultSeen do (r: (call: int, ok: bool)) { early -= (r.call); }
    on eBreakerChanged do (c: (mode: tBreakerState)) {
      if (c.mode == Closed && sizeof(early) > 0) { goto WaitLate; }
    }
    ignore eDispatched;
  }
  hot state WaitLate {
    on eResultSeen do (r: (call: int, ok: bool)) { if (r.call in early) { goto Covered; } }
    ignore eDispatched, eBreakerChanged;
  }
  cold state Covered {
    ignore eDispatched, eBreakerChanged, eResultSeen;
  }
}

test tcConcurrentCallers [main=ConcurrentCallers]:
  assert BreakerContract, CallsAnswered in
  { ConcurrentCallers, Breaker, Dependency, DepReply, TimerService, Caller };

test tcHalfOpenRace [main=HalfOpenRace]:
  assert BreakerContract, CallsAnswered in
  { HalfOpenRace, Breaker, Dependency, DepReply, TimerService, Caller };

test tcLateFailure [main=LateFailure]:
  assert BreakerContract, CallsAnswered in
  { LateFailure, Breaker, Dependency, DepReply, TimerService, Caller };

test tcSuccessResetsCount [main=SuccessResetsCount]:
  assert BreakerContract, CallsAnswered in
  { SuccessResetsCount, Breaker, Dependency, DepReply, TimerService, Caller };

test tcDuplicateCooldown [main=DuplicateCooldown]:
  assert BreakerContract, CallsAnswered in
  { DuplicateCooldown, Breaker, Dependency, DepReply, TimerService, Caller };

// expect-bug: sent while a half-open trial was in flight
test tcBugHalfOpenAdmitsAll [main=BuggyHalfOpenRace]:
  assert BreakerContract, CallsAnswered in
  { BuggyHalfOpenRace, AdmitAllBreaker, Dependency, DepReply, TimerService, Caller };
