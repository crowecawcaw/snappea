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

event eCall: (caller: machine, call: int);
event eCallResult: (call: int, outcome: tOutcome);
event eDepCall: (breaker: machine, call: int);
event eDepResult: (call: int, ok: bool);
// To the timer service: fire eCooldown with this period after the cooldown.
event eStartCooldown: (breaker: machine, period: int);
event eCooldown: (period: int);

machine Breaker {
  var mode: tBreakerState;
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

    on eCall do (c: (caller: machine, call: int)) {
      if (mode == Open || (mode == HalfOpen && trialInFlight)) {
        announce eShortCircuited, (call = c.call,);
        send c.caller, eCallResult, (call = c.call, outcome = Rejected);
        return;
      }
      if (mode == HalfOpen) {
        trialInFlight = true;
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
      // snappea-lint: allow mutation:remove-statement -- forgetting a finished call; call ids are never reused, so keeping it is unobservable
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
      // snappea-lint: allow mutation:remove-statement -- as above: frees memory only
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
