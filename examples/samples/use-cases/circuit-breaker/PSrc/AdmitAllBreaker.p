// Negative control, kept as a separate machine so its code is excluded from the
// passing tests' coverage: half-open lets every caller through instead of one trial,
// so a struggling dependency gets a burst of calls the moment the cooldown ends.
machine AdmitAllBreaker {
  var mode: tBreakerState;
  var threshold: int;
  var failures: int;
  var period: int;
  var dependency: machine;
  var timers: machine;
  var callers: map[int, machine];
  var sentIn: map[int, int];

  start state Running {
    entry (input: (dependency: machine, timers: machine, threshold: int)) {
      dependency = input.dependency;
      timers = input.timers;
      threshold = input.threshold;
      mode = Closed;
    }

    on eCall do (c: (caller: machine, call: int)) {
      // BUG: only Open rejects; half-open admits every call.
      if (mode == Open) {
        announce eShortCircuited, (call = c.call,);
        send c.caller, eCallResult, (call = c.call, outcome = Rejected);
        return;
      }
      callers[c.call] = c.caller;
      sentIn[c.call] = period;
      announce eDispatched, (call = c.call,);
      send dependency, eDepCall, (breaker = this, call = c.call);
    }

    on eDepResult do (r: (call: int, ok: bool)) {
      var caller: machine;
      announce eResultSeen, r;
      caller = callers[r.call];
      callers -= (r.call);
      if (sentIn[r.call] == period) {
        if (mode == Closed) {
          if (r.ok) {
            failures = 0;
          } else {
            failures = failures + 1;
            if (failures == threshold) { Change(Open); }
          }
        } else {
          if (r.ok) { Change(Closed); } else { Change(Open); }
        }
      }
      sentIn -= (r.call);
      if (r.ok) {
        send caller, eCallResult, (call = r.call, outcome = Succeeded);
      } else {
        send caller, eCallResult, (call = r.call, outcome = Failed);
      }
    }

    on eCooldown do (t: (period: int)) {
      if (mode == Open && t.period == period) {
        Change(HalfOpen);
      }
    }
  }

  fun Change(target: tBreakerState) {
    mode = target;
    period = period + 1;
    failures = 0;
    announce eBreakerChanged, (mode = target,);
    if (target == Open) {
      send timers, eStartCooldown, (breaker = this, period = period);
    }
  }
}
