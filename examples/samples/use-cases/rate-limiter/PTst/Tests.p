// Drivers. The clock is the environment, so it lives here and is not mutated: the
// driver decides when a window ends. Gateways and stores are the design under test.

event Tick: machine;
event TickDone;

// Wall-clock time, as window ids. Gateways read it with Now; the driver advances it.
machine Clock {
  var window: int;
  start state Running {
    on Now do (client: machine) { send client, NowResp, window; }
    on Tick do (driver: machine) {
      window = window + 1;
      announce Ticked, window;
      send driver, TickDone;
    }
  }
}

type GatewaySystem = (clock: Clock, a: machine, b: machine);

// Two gateways sharing one store with load + compare-and-swap.
fun NewSystem(): GatewaySystem {
  var clock: Clock;
  var store: CounterStore;
  clock = new Clock();
  store = new CounterStore();
  return (clock = clock, a = new Gateway((store = store, clock = clock)),
          b = new Gateway((store = store, clock = clock)));
}

fun Send(owner: machine, gateway: machine, req: int) {
  announce Requested, req;
  send gateway, Request, (client = owner, req = req);
}

// The window already holds Limit() - 1 requests. Two gateways then each receive one
// request, and every order of their loads and CASes is explored: both can read the
// same count, so one CAS loses, reloads, finds the window full, and rejects.
machine LastSlotRace {
  var sys: GatewaySystem;
  start state Warmup {
    entry (s: GatewaySystem) {
      sys = s;
      announce RequestsPlanned, 3;
      Send(this, sys.a, 1);
    }
    on Decision goto Race with (d: AdmissionDecision) { announce Answered, d; }
  }
  state Race {
    entry {
      Send(this, sys.a, 2);
      Send(this, sys.b, 3);
    }
    on Decision do (d: AdmissionDecision) { announce Answered, d; }
  }
}

// Window 0 is full. The clock ticks while two requests are in flight: a gateway can
// read window 0 and reject, read window 1 and reset the count, or read window 0 after
// the other gateway already started window 1 (its reading is stale; it must count in
// window 1, never write window 0 back).
machine RolloverRace {
  var sys: GatewaySystem;
  start state Fill {
    entry (s: GatewaySystem) {
      sys = s;
      announce RequestsPlanned, 4;
      Send(this, sys.a, 1);
    }
    on Decision goto Fill2 with (d: AdmissionDecision) { announce Answered, d; }
  }
  state Fill2 {
    entry { Send(this, sys.b, 2); }
    on Decision goto Race with (d: AdmissionDecision) { announce Answered, d; }
  }
  state Race {
    entry {
      Send(this, sys.a, 3);
      Send(this, sys.b, 4);
      // Sent last, so the gateways' clock reads can land on either side of it.
      send sys.clock, Tick, this;
    }
    on Decision do (d: AdmissionDecision) { announce Answered, d; }
    ignore TickDone;
  }
}

// Lifecycle boundary: window 0 is full, the clock has ticked, and only then do two
// requests arrive. Both must be admitted; a gateway that trusts the old full record
// rejects them.
machine AfterRollover {
  var sys: GatewaySystem;
  start state Fill {
    entry (s: GatewaySystem) {
      sys = s;
      announce RequestsPlanned, 4;
      Send(this, sys.a, 1);
    }
    on Decision goto Fill2 with (d: AdmissionDecision) { announce Answered, d; }
  }
  state Fill2 {
    entry { Send(this, sys.b, 2); }
    on Decision goto Ticking with (d: AdmissionDecision) { announce Answered, d; }
  }
  state Ticking {
    entry { send sys.clock, Tick, this; }
    on TickDone goto Race;
  }
  state Race {
    entry {
      Send(this, sys.a, 3);
      Send(this, sys.b, 4);
    }
    on Decision do (d: AdmissionDecision) { announce Answered, d; }
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

// Covered: a gateway lost its CAS, reloaded, and rejected that same request (the
// window filled between its load and its CAS).
scenario LostCasThenReject observes CasResult, Rejected {
  var loser: int;
  start hot state WaitLost {
    on CasResult do (r: (req: int, ok: bool)) {
      if (!r.ok) { loser = r.req; goto WaitReject; }
    }
  }
  hot state WaitReject {
    on Rejected do (p: (req: int, window: int)) {
      if (p.req == loser) { goto Covered; }
    }
  }
  cold state Covered {
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

// snappea: expect-bug "window 0 admitted 3 requests, limit 2"
test tcBugLoadThenStore [main=MainBugLoadThenStore]:
  assert WindowLimit, RequestsAnswered in
  { MainBugLoadThenStore, LastSlotRace, Clock, PlainCounterStore, BlindGateway };
