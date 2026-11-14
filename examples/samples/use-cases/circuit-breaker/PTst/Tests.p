// The environment (the flaky dependency, the timer service, the callers) and the test
// cases. Only the breaker is the design under test; everything here is a driver.

// The dependency answers calls in the order they arrive, from a script of outcomes
// (true = success); past the script it succeeds. The answer to call `slow` travels
// back on its own connection (a DepReply machine), so other answers can overtake it:
// a slow failure can arrive after later calls were answered. The rest come back in
// order, which keeps the search small.
machine Dependency {
  var script: seq[bool];
  var slow: int;
  var served: int;

  start state Serving {
    entry (cfg: (script: seq[bool], slow: int)) {
      script = cfg.script;
      slow = cfg.slow;
    }
    on DependencyCall do (c: (breaker: machine, call: int)) {
      var ok: bool;
      ok = true;
      if (served < sizeof(script)) { ok = script[served]; }
      served = served + 1;
      announce DepAnswered, (call = c.call, ok = ok);
      if (c.call == slow) {
        new DepReply((breaker = c.breaker, call = c.call, ok = ok));
      } else {
        send c.breaker, DependencyResult, (call = c.call, ok = ok);
      }
    }
  }
}

machine DepReply {
  start state Sending {
    entry (r: (breaker: machine, call: int, ok: bool)) {
      send r.breaker, DependencyResult, (call = r.call, ok = r.ok);
    }
  }
}

// The timer service fires each cooldown once, whenever the scheduler runs it after the
// breaker asked. With `duplicates`, the timer library delivers every firing a second
// time, arbitrarily later (at-least-once delivery).
machine TimerService {
  var duplicates: bool;

  start state Serving {
    entry (d: bool) {
      duplicates = d;
    }
    on StartCooldown do (t: (breaker: machine, period: int)) {
      announce CooldownElapsed, (period = t.period,);
      send t.breaker, Cooldown, (period = t.period,);
      if (duplicates) {
        new LateDuplicate(t);
      }
    }
  }
}

machine LateDuplicate {
  start state Sending {
    entry (t: (breaker: machine, period: int)) {
      send t.breaker, Cooldown, (period = t.period,);
    }
  }
}

// A caller makes `calls` calls one after another. A rejected call is retried at once,
// with a new call id, up to `retries` times (a caller polling until the breaker lets it
// through). Call ids start at `firstId` so callers never share one.
type CallerConfig = (owner: machine, breaker: machine, firstId: int, calls: int, retries: int);
// Tells the caller's owner (the test's main machine) that the caller finished.
event CallerFinished: (firstId: int);

machine Caller {
  var cfg: CallerConfig;
  var next: int;
  var made: int;
  var retriesLeft: int;

  start state Calling {
    entry (c: CallerConfig) {
      cfg = c;
      next = c.firstId;
      retriesLeft = c.retries;
      Call();
    }
    on CallResult do (r: (call: int, outcome: CallOutcome)) {
      assert r.call == next - 1, format("caller got an answer for call {0}, it is waiting for {1}", r.call, next - 1);
      announce CallAnswered, r;
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
      announce CallerDone;
      send cfg.owner, CallerFinished, (firstId = cfg.firstId,);
    }
  }

  fun Call() {
    announce CallStarted, (call = next,);
    send cfg.breaker, CallRequested, (caller = this, call = next);
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

fun NewBreaker(script: seq[bool], slow: int, threshold: int, duplicates: bool): Breaker {
  announce BreakerPlanned, (threshold = threshold,);
  return new Breaker((dependency = new Dependency((script = script, slow = slow)),
                      timers = new TimerService(duplicates),
                      threshold = threshold));
}

// Two callers, three calls, against a dependency that fails twice, then recovers
// (threshold 2): calls in flight when the breaker opens, rejections, the trial. Every interleaving of calls, answers and the cooldown.
machine ConcurrentCallers {
  start state Init {
    entry {
      var b: Breaker;
      b = NewBreaker(Script(false, false, true, true), -1, 2, false);
      announce CallersPlanned, 2;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 2, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
    }
    ignore CallerFinished;
  }
}

// One failure opens the breaker (threshold 1). Only then do two callers start, each
// retrying a rejected call, so both can reach the half-open window together.
machine HalfOpenRace {
  var b: machine;
  start state First {
    entry {
      b = NewBreaker(Script(false, true, true, true), -1, 1, false);
      announce CallersPlanned, 3;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 1, retries = 0));
    }
    on CallerFinished goto Racing;
  }
  state Racing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 0));
    }
    ignore CallerFinished;
  }
}

// Three concurrent calls all fail (threshold 2); the answer to call 40 is slow. Once
// the other two callers are answered (the breaker opened), it may still be in flight. Then caller 1 calls twice: a trial succeeds and closes the breaker, then one
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
      b = NewBreaker(s, 40, 2, false);
      announce CallersPlanned, 4;
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 0));
      new Caller((owner = this, breaker = b, firstId = 40, calls = 1, retries = 0));
    }
    on CallerFinished do (c: (firstId: int)) {
      if (c.firstId != 40) { finished = finished + 1; }
      if (finished == 2) { goto Reclosing; }
    }
  }
  state Reclosing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 10, calls = 2, retries = 0));
    }
    ignore CallerFinished;
  }
}

// One caller, threshold 2, answers fail, succeed, fail, fail: the success resets the
// count, so only the fourth call opens the breaker. Rejected calls are retried; the
// trial succeeds, and two more failures must open the breaker again: the count
// starts from zero in the new closed period.
machine SuccessResetsCount {
  start state Init {
    entry {
      var b: Breaker;
      var s: seq[bool];
      s = Script(false, true, false, false);
      s += (4, true);
      s += (5, false);
      s += (6, false);
      b = NewBreaker(s, -1, 2, false);
      announce CallersPlanned, 1;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 8, retries = 2));
    }
    ignore CallerFinished;
  }
}

// Every cooldown firing is delivered twice (threshold 1). One caller keeps calling,
// retrying a rejected call once: the first trial fails and the breaker reopens, the
// second trial succeeds. The duplicate of the first cooldown can arrive during the
// first trial or in the second open period, and must not start a half-open window.
machine DuplicateCooldown {
  start state Init {
    entry {
      var b: Breaker;
      b = NewBreaker(Script(false, false, true, true), -1, 1, true);
      announce CallersPlanned, 1;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 3, retries = 1));
    }
    ignore CallerFinished;
  }
}

machine BuggyHalfOpenRace {
  var b: machine;
  start state First {
    entry {
      announce BreakerPlanned, (threshold = 1,);
      b = new AdmitAllBreaker((dependency = new Dependency((script = Script(false, true, true, true), slow = -1)),
                               timers = new TimerService(false), threshold = 1));
      announce CallersPlanned, 3;
      new Caller((owner = this, breaker = b, firstId = 10, calls = 1, retries = 0));
    }
    on CallerFinished goto Racing;
  }
  state Racing {
    entry {
      new Caller((owner = this, breaker = b, firstId = 20, calls = 1, retries = 1));
      new Caller((owner = this, breaker = b, firstId = 30, calls = 1, retries = 1));
    }
    ignore CallerFinished;
  }
}

// Covered: a result sent before the breaker opened arrived after it had closed again.
scenario LateResultAfterReclose observes CallDispatched, BreakerChanged, CallResultObserved {
  var early: set[int];
  start hot state WaitOpen {
    on CallDispatched do (d: (call: int)) { early += (d.call); }
    on BreakerChanged do (c: (mode: BreakerState)) { if (c.mode == Open) { goto WaitClose; } }
  }
  hot state WaitClose {
    on CallResultObserved do (r: (call: int, ok: bool)) { early -= (r.call); }
    on BreakerChanged do (c: (mode: BreakerState)) {
      if (c.mode == Closed && sizeof(early) > 0) { goto WaitLate; }
    }
  }
  hot state WaitLate {
    on CallResultObserved do (r: (call: int, ok: bool)) { if (r.call in early) { goto Covered; } }
  }
  cold state Covered {
  }
}

test tcConcurrentCallers [main=ConcurrentCallers]:
  assert BreakerContract, CallsAnswered in
  { ConcurrentCallers, Breaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };

test tcHalfOpenRace [main=HalfOpenRace]:
  assert BreakerContract, CallsAnswered in
  { HalfOpenRace, Breaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };

test tcLateFailure [main=LateFailure]:
  assert BreakerContract, CallsAnswered in
  { LateFailure, Breaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };

test tcSuccessResetsCount [main=SuccessResetsCount]:
  assert BreakerContract, CallsAnswered in
  { SuccessResetsCount, Breaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };

test tcDuplicateCooldown [main=DuplicateCooldown]:
  assert BreakerContract, CallsAnswered in
  { DuplicateCooldown, Breaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };

// snappea: expect-bug "sent while a half-open trial was in flight"
test tcBugHalfOpenAdmitsAll [main=BuggyHalfOpenRace]:
  assert BreakerContract, CallsAnswered in
  { BuggyHalfOpenRace, AdmitAllBreaker, Dependency, DepReply, TimerService, LateDuplicate, Caller };
