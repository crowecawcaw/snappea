// A client-side circuit breaker in front of a flaky dependency.
//
//   Closed:   every call goes to the dependency. K consecutive failures -> Open.
//             A success resets the count.
//   Open:     every call is rejected at once. A cooldown timer is started; when it
//             fires -> HalfOpen.
//   HalfOpen: exactly one trial call goes to the dependency; other calls are rejected
//             while it is in flight. Trial success -> Closed, failure -> Open.
//
// The breaker handles one event at a time (one lock in a real implementation), so each
// handler is atomic. Calls, dependency replies and timer firings interleave freely.
// Every call is tagged with the breaker's period (its transition count) when it was
// sent, so a result that arrives after a transition is recognised as late and only
// answers its caller. The cooldown timer carries the period too, so a duplicate or
// stale firing cannot reopen the half-open window.

event CallRequested: (caller: machine, call: int);
event CallResult: (call: int, outcome: CallOutcome);
event DependencyCall: (breaker: machine, call: int);
event DependencyResult: (call: int, ok: bool);
// To the timer service: fire Cooldown with this period after the cooldown.
event StartCooldown: (breaker: machine, period: int);
event Cooldown: (period: int);

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
      announce CallResultObserved, r;
      caller = callers[r.call];
      // snappea-lint: allow mutation:remove-statement -- equivalent: call ids are never reused, so a finished call's entry is never read again
      callers -= (r.call);
      if (sentIn[r.call] == period) {
        // Not late: this result belongs to the current period.
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
      // snappea-lint: allow mutation:remove-statement -- equivalent: call ids are never reused, so a finished call's entry is never read again
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
