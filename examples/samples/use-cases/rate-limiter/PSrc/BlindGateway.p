// Negative control, kept apart so its code is not part of the passing tests: the
// gateway loads the count, decides, then writes it back with a plain SET. Two gateways
// that load the same count both admit, and the window takes more than Limit().
// tcBugLoadThenStore shows the contract catching it.

event eSet: (client: machine, req: int, window: int, count: int);
event eSetOk;

// A store with load and unconditional set (a plain key-value GET/SET).
machine PlainCounterStore {
  var window: int;
  var count: int;

  start state Serving {
    on eLoad do (client: machine) {
      send client, eLoaded, (window = window, count = count, version = 0);
    }
    on eSet do (s: (client: machine, req: int, window: int, count: int)) {
      window = s.window;
      count = s.count;
      announce eAdmitted, (req = s.req, window = window, count = count);
      send s.client, eSetOk;
    }
  }
}

machine BlindGateway {
  var store: PlainCounterStore;
  var clock: machine;

  start state Serving {
    entry (input: (store: PlainCounterStore, clock: machine)) {
      store = input.store;
      clock = input.clock;
    }
    on eRequest do (r: (client: machine, req: int)) {
      var arrival: int;
      var rec: tRecord;
      var window: int;
      var count: int;
      send clock, eNow, this;
      receive { case eNowResp: (w: int) { arrival = w; } }
      send store, eLoad, this;
      receive { case eLoaded: (s: tRecord) { rec = s; } }
      window = arrival;
      count = rec.count;
      if (rec.window > window) { window = rec.window; }
      if (rec.window < window) { count = 0; }
      if (count >= Limit()) {
        announce eRejected, (req = r.req, window = window);
        send r.client, eDecision, (req = r.req, admitted = false);
        return;
      }
      // BUG: another gateway can write between the load above and this set.
      send store, eSet, (client = this, req = r.req, window = window, count = count + 1);
      receive { case eSetOk: { } }
      send r.client, eDecision, (req = r.req, admitted = true);
    }
  }
}
