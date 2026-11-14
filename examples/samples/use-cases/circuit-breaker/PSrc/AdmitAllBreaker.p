// Negative control, kept as a separate machine so its code is excluded from the
// passing tests' coverage: half-open lets every caller through instead of one trial,
// so a struggling dependency gets a burst of calls the moment the cooldown ends.
machine AdmitAllBreaker {
  var mode: BreakerState;
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

    on CallRequested do (c: (caller: machine, call: int)) {
      // BUG: only Open rejects; half-open admits every call.
      if (mode == Open) {
        announce CallRejected, (call = c.call,);
        send c.caller, CallResult, (call = c.call, outcome = Rejected);
        return;
      }
      callers[c.call] = c.caller;
      sentIn[c.call] = period;
      announce CallDispatched, (call = c.call,);
      send dependency, DependencyCall, (breaker = this, call = c.call);
    }

    on DependencyResult do (r: (call: int, ok: bool)) {
      var caller: machine;
      announce CallResultObserved, r;
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
        send caller, CallResult, (call = r.call, outcome = Succeeded);
      } else {
        send caller, CallResult, (call = r.call, outcome = Failed);
      }
    }

    on Cooldown do (t: (period: int)) {
      // Timers are created only on entry to Open. Every mode change advances
      // period, so a matching timer also proves we are still in that Open period.
      if (t.period == period) {
        Change(HalfOpen);
      }
    }
  }

  fun Change(target: BreakerState) {
    mode = target;
    period = period + 1;
    failures = 0;
    announce BreakerChanged, (mode = target,);
    if (target == Open) {
      send timers, StartCooldown, (breaker = this, period = period);
    }
  }
}
