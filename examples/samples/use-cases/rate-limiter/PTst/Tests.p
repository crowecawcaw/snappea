// Drivers. The clock is the environment, so it lives here and is not mutated: the
// driver decides when a window ends. Gateways and stores are the design under test.

event eTick: machine;
event eTickDone;

// Wall-clock time, as window ids. Gateways read it with eNow; the driver advances it.
machine Clock {
  var window: int;
  start state Running {
    on eNow do (client: machine) { send client, eNowResp, window; }
    on eTick do (driver: machine) {
      window = window + 1;
      announce eTicked, window;
      send driver, eTickDone;
    }
  }
}

type tSystem = (clock: Clock, a: machine, b: machine);

// Two gateways sharing one store with load + compare-and-swap.
fun NewSystem(): tSystem {
  var clock: Clock;
  var store: CounterStore;
  clock = new Clock();
  store = new CounterStore();
  return (clock = clock, a = new Gateway((store = store, clock = clock)),
          b = new Gateway((store = store, clock = clock)));
}

fun Send(owner: machine, gateway: machine, req: int) {
  announce eRequested, req;
  send gateway, eRequest, (client = owner, req = req);
}

// The window already holds Limit() - 1 requests. Two gateways then each receive one
// request, and every order of their loads and CASes is explored: both can read the
// same count, so one CAS loses, reloads, finds the window full, and rejects.
machine LastSlotRace {
  var sys: tSystem;
  start state Warmup {
    entry (s: tSystem) {
      sys = s;
      announce eRequestsPlanned, 3;
      Send(this, sys.a, 1);
    }
    on eDecision goto Race with (d: tDecision) { announce eAnswered, d; }
  }
  state Race {
    entry {
      Send(this, sys.a, 2);
      Send(this, sys.b, 3);
    }
    on eDecision do (d: tDecision) { announce eAnswered, d; }
  }
}

// Window 0 is full. The clock ticks while two requests are in flight: a gateway can
// read window 0 and reject, read window 1 and reset the count, or read window 0 after
// the other gateway already started window 1 (its reading is stale; it must count in
// window 1, never write window 0 back).
machine RolloverRace {
  var sys: tSystem;
  start state Fill {
    entry (s: tSystem) {
      sys = s;
      announce eRequestsPlanned, 4;
      Send(this, sys.a, 1);
    }
    on eDecision goto Fill2 with (d: tDecision) { announce eAnswered, d; }
  }
  state Fill2 {
    entry { Send(this, sys.b, 2); }
    on eDecision goto Race with (d: tDecision) { announce eAnswered, d; }
  }
  state Race {
    entry {
      Send(this, sys.a, 3);
      Send(this, sys.b, 4);
      // Sent last, so the gateways' clock reads can land on either side of it.
      send sys.clock, eTick, this;
    }
    on eDecision do (d: tDecision) { announce eAnswered, d; }
    ignore eTickDone;
  }
}

// Lifecycle boundary: window 0 is full, the clock has ticked, and only then do two
// requests arrive. Both must be admitted; a gateway that trusts the old full record
// rejects them.
machine AfterRollover {
  var sys: tSystem;
  start state Fill {
    entry (s: tSystem) {
      sys = s;
      announce eRequestsPlanned, 4;
      Send(this, sys.a, 1);
    }
    on eDecision goto Fill2 with (d: tDecision) { announce eAnswered, d; }
  }
  state Fill2 {
    entry { Send(this, sys.b, 2); }
    on eDecision goto Tick with (d: tDecision) { announce eAnswered, d; }
  }
  state Tick {
    entry { send sys.clock, eTick, this; }
    on eTickDone goto Race;
  }
  state Race {
    entry {
      Send(this, sys.a, 3);
      Send(this, sys.b, 4);
    }
    on eDecision do (d: tDecision) { announce eAnswered, d; }
  }
}

machine MainLastSlotRace {
  start state Init { entry { new LastSlotRace(NewSystem()); } }
}
machine MainRolloverRace {
  start state Init { entry { new RolloverRace(NewSystem()); } }
}
machine MainAfterRollover {
  start state Init { entry { new AfterRollover(NewSystem()); } }
}
machine MainBugLoadThenStore {
  start state Init {
    entry {
      var clock: Clock;
      var store: PlainCounterStore;
      clock = new Clock();
      store = new PlainCounterStore();
      new LastSlotRace((clock = clock, a = new BlindGateway((store = store, clock = clock)),
                        b = new BlindGateway((store = store, clock = clock))));
    }
  }
}

// Covered: a gateway lost its CAS, reloaded, and rejected the request it would have
// admitted on its first load.
scenario LostCasThenReject observes eAdmitted, eRejected {
  start hot state WaitAdmit {
    on eAdmitted goto WaitReject;
    ignore eRejected;
  }
  hot state WaitReject {
    on eRejected goto Covered;
    ignore eAdmitted;
  }
  cold state Covered {
    ignore eAdmitted, eRejected;
  }
}

module RateLimiter = { Clock, CounterStore, Gateway };

test tcLastSlotRace [main=MainLastSlotRace]:
  assert WindowLimit, RequestsAnswered in
  (union RateLimiter, { MainLastSlotRace, LastSlotRace });

test tcRolloverRace [main=MainRolloverRace]:
  assert WindowLimit, RequestsAnswered in
  (union RateLimiter, { MainRolloverRace, RolloverRace });

test tcAfterRollover [main=MainAfterRollover]:
  assert WindowLimit, RequestsAnswered in
  (union RateLimiter, { MainAfterRollover, AfterRollover });

// expect-bug: window 0 admitted 3 requests, limit 2
test tcBugLoadThenStore [main=MainBugLoadThenStore]:
  assert WindowLimit, RequestsAnswered in
  { MainBugLoadThenStore, LastSlotRace, Clock, PlainCounterStore, BlindGateway };
