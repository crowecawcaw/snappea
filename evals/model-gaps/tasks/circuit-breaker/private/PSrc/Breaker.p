// The circuit breaker. See DESIGN.md; the events are in Events.p.
//
// The breaker handles one event at a time, so each handler is atomic. Every call is
// tagged with the breaker's period (its count of state changes) when it was sent, so
// a result that arrives after a change is recognised as late and only answers its
// caller. The cooldown carries the period too, so a duplicate or stale firing cannot
// start a half-open window.

machine Breaker {
  var mode: BreakerState;
  var threshold: int;
  var failures: int;
  var period: int;
  var trialInFlight: bool;
  var dependency: machine;
  var timers: machine;
  // call -> who asked, and the period it was sent in
  var callers: map[int, machine];
  var sentIn: map[int, int];

  start state Running {
    entry (input: (dependency: machine, timers: machine, threshold: int)) {
      dependency = input.dependency;
      timers = input.timers;
      threshold = input.threshold;
      // mode starts Closed, the enum's first value.
    }

    on CallRequested do (c: (caller: machine, call: int)) {
      // Only a half-open trial sets trialInFlight; its result clears it before
      // changing mode. Either an open breaker or an occupied trial rejects.
      if (mode == Open || trialInFlight) {
        announce CallRejected, (call = c.call,);
        send c.caller, CallResult, (call = c.call, outcome = Rejected);
        return;
      }
      if (mode == HalfOpen) {
        trialInFlight = true;
      }
      callers[c.call] = c.caller;
      sentIn[c.call] = period;
      announce CallDispatched, (call = c.call,);
      send dependency, DependencyCall, (breaker = this, call = c.call);
    }

    on DependencyResult do (r: (call: int, ok: bool)) {
      var caller: machine;
      var late: bool;
      announce CallResultObserved, r;
      caller = callers[r.call];
      // Late: sent before the last change of state.
      late = sentIn[r.call] != period;
      callers -= (r.call);
      sentIn -= (r.call);
      if (!late) {
        if (mode == Closed) {
          if (r.ok) {
            failures = 0;
          } else {
            failures = failures + 1;
            if (failures == threshold) { Change(Open); }
          }
        } else {
          // The half-open trial.
          trialInFlight = false;
          if (r.ok) { Change(Closed); } else { Change(Open); }
        }
      }
      // Every call is answered, late or not.
      if (r.ok) {
        send caller, CallResult, (call = r.call, outcome = Succeeded);
      } else {
        send caller, CallResult, (call = r.call, outcome = Failed);
      }
    }

    on Cooldown do (t: (period: int)) {
      // Timers are started only on entry to Open, and every change advances period,
      // so a matching timer also proves the breaker is still in that open period.
      if (t.period == period) {
        Change(HalfOpen);
      }
    }
  }

  fun Change(target: BreakerState) {
    mode = target;
    period = period + 1;
    // A new period counts from zero.
    failures = 0;
    announce BreakerChanged, (mode = target,);
    if (target == Open) {
      send timers, StartCooldown, (breaker = this, period = period);
    }
  }
}
