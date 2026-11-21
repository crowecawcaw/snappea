// A first environment and one smoke test: it creates the breaker with a dependency
// that always succeeds and makes one call. It checks nothing beyond the model running.
// The dependency, the timer service and the callers are drivers; write the ones you
// need to exercise DESIGN.md's "What can happen".

machine SmokeDependency {
  start state Serving {
    on DependencyCall do (c: (breaker: machine, call: int)) {
      announce DepAnswered, (call = c.call, ok = true);
      send c.breaker, DependencyResult, (call = c.call, ok = true);
    }
  }
}

machine SmokeTimers {
  start state Serving {
    on StartCooldown do (t: (breaker: machine, period: int)) {
      announce CooldownElapsed, (period = t.period,);
      send t.breaker, Cooldown, (period = t.period,);
    }
  }
}

machine Smoke {
  start state Init {
    entry {
      var b: machine;
      announce BreakerPlanned, (threshold = 2,);
      b = new Breaker((dependency = new SmokeDependency(), timers = new SmokeTimers(), threshold = 2));
      announce CallersPlanned, 1;
      announce CallStarted, (call = 1,);
      send b, CallRequested, (caller = this, call = 1);
    }
    on CallResult do (r: (call: int, outcome: CallOutcome)) {
      announce CallAnswered, r;
      announce CallerDone;
    }
  }
}

test tcSmoke [main=Smoke]:
  { Smoke, Breaker, SmokeDependency, SmokeTimers };
