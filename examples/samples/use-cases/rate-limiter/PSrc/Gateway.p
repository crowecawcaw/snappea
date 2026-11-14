// An API gateway node. For each request it reads the clock once, then runs a
// load / decide / compare-and-swap loop against the shared counter. A lost CAS means
// another gateway wrote first: reload and decide again, which may now reject.

// Requests admitted per window, across all gateways.
fun Limit(): int { return 2; }

event Request: (client: machine, req: int);
type AdmissionDecision = (req: int, admitted: bool);
event Decision: AdmissionDecision;
// The clock: ask for the current window id.
event Now: machine;
event NowResp: int;

machine Gateway {
  var store: CounterStore;
  var clock: machine;

  start state Serving {
    entry (input: (store: CounterStore, clock: machine)) {
      store = input.store;
      clock = input.clock;
    }
    on Request do (r: (client: machine, req: int)) {
      var arrival: int;
      var rec: CounterRecord;
      var window: int;
      var count: int;
      var done: bool;
      var admitted: bool;
      send clock, Now, this;
      receive { case NowResp: (w: int) { arrival = w; } }
      while (!done) {
        send store, Load, this;
        receive { case Loaded: (s: CounterRecord) { rec = s; } }
        window = arrival;
        count = rec.count;
        // Another gateway already counts a later window: our clock reading is stale,
        // and that window is the current one. Never write an older window back.
        // snappea-lint: allow mutation:swap-operator -- with `>=`, equal windows assign the same value
        if (rec.window > window) { window = rec.window; }
        // The record still counts an earlier window: this request starts a new one.
        if (rec.window < window) { count = 0; }
        if (count >= Limit()) {
          announce Rejected, (req = r.req, window = window);
          done = true;
        } else {
          send store, Cas, (client = this, req = r.req, expected = rec.version,
                             window = window, count = count + 1);
          receive { case CasResp: (result: (req: int, ok: bool)) {
            assert result.req == r.req, "CAS response belongs to another request";
            // On a lost race, loop: reload and decide against the new count.
            admitted = result.ok;
            done = result.ok;
          } }
        }
      }
      send r.client, Decision, (req = r.req, admitted = admitted);
    }
  }
}
