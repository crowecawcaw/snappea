// An API gateway node. For each request it reads the clock once, then runs a
// load / decide / compare-and-swap loop against the shared counter. A lost CAS means
// another gateway wrote first: reload and decide again, which may now reject.

// Requests admitted per window, across all gateways.
fun Limit(): int { return 2; }

event eRequest: (client: machine, req: int);
type tDecision = (req: int, admitted: bool);
event eDecision: tDecision;
// The clock: ask for the current window id.
event eNow: machine;
event eNowResp: int;

machine Gateway {
  var store: CounterStore;
  var clock: machine;

  start state Serving {
    entry (input: (store: CounterStore, clock: machine)) {
      store = input.store;
      clock = input.clock;
    }
    on eRequest do (r: (client: machine, req: int)) {
      var arrival: int;
      var rec: tRecord;
      var window: int;
      var count: int;
      var done: bool;
      var admitted: bool;
      var attempts: int;
      send clock, eNow, this;
      receive { case eNowResp: (w: int) { arrival = w; } }
      while (!done) {
        // snappea-lint: allow mutation -- only feeds the test bound below, which no passing schedule reaches
        attempts = attempts + 1;
        // Test bound, not a production retry policy: each test has one other gateway
        // with at most one write in flight, so a CAS can lose at most once. Without
        // this, a CAS that never succeeds is a livelock the checker finds slowly.
        assert attempts <= 2, format("request {0} lost its CAS twice with one competing write", r.req);
        send store, eLoad, this;
        receive { case eLoaded: (s: tRecord) { rec = s; } }
        window = arrival;
        count = rec.count;
        // Another gateway already counts a later window: our clock reading is stale,
        // and that window is the current one. Never write an older window back.
        // snappea-lint: allow mutation:swap-operator -- with `>=`, equal windows assign the same value
        if (rec.window > window) { window = rec.window; }
        // The record still counts an earlier window: this request starts a new one.
        if (rec.window < window) { count = 0; }
        if (count >= Limit()) {
          announce eRejected, (req = r.req, window = window);
          done = true;
        } else {
          send store, eCas, (client = this, req = r.req, expected = rec.version,
                             window = window, count = count + 1);
          receive { case eCasResp: (ok: bool) {
            // On a lost race, loop: reload and decide against the new count.
            admitted = ok;
            done = ok;
          } }
        }
      }
      send r.client, eDecision, (req = r.req, admitted = admitted);
    }
  }
}
