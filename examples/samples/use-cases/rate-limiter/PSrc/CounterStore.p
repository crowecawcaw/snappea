// The shared counter store (one Redis key, one DynamoDB item). It holds the record for
// the most recent window: which window it counts and how many requests it admitted.
//
// Real atomic primitives:
//   load: return (window, count, version)
//   cas:  if version == expected, write (window, count) and bump version
// Everything a gateway does between them can interleave with other gateways.

type CounterRecord = (window: int, count: int, version: int);
type CompareAndSwapRequest = (client: machine, req: int, expected: int, window: int, count: int);

event Load: machine;
event Loaded: CounterRecord;
event Cas: CompareAndSwapRequest;
event CasResp: (req: int, ok: bool);

machine CounterStore {
  var window: int;
  var count: int;
  var version: int;

  start state Serving {
    on Load do (client: machine) {
      send client, Loaded, (window = window, count = count, version = version);
    }
    on Cas do (c: CompareAndSwapRequest) {
      if (c.expected != version) {
        // Another gateway committed since this request loaded the counter.
        send c.client, CasResp, (req = c.req, ok = false);
        return;
      }
      window = c.window;
      count = c.count;
      // snappea-lint: allow mutation:swap-operator -- `version - 1` is equivalent: the CAS needs only a version never handed out before
      version = version + 1;
      // Commit point of an admission: announce what the store now holds.
      announce Admitted, (req = c.req, window = window, count = count);
      send c.client, CasResp, (req = c.req, ok = true);
    }
  }
}
