// The shared counter store (one Redis key, one DynamoDB item). It holds the record for
// the most recent window: which window it counts and how many requests it admitted.
//
// Real atomic primitives:
//   load: return (window, count, version)
//   cas:  if version == expected, write (window, count) and bump version
// Everything a gateway does between them can interleave with other gateways.

type tRecord = (window: int, count: int, version: int);
type tCas = (client: machine, req: int, expected: int, window: int, count: int);

event eLoad: machine;
event eLoaded: tRecord;
event eCas: tCas;
event eCasResp: bool;

machine CounterStore {
  var window: int;
  var count: int;
  var version: int;

  start state Serving {
    on eLoad do (client: machine) {
      send client, eLoaded, (window = window, count = count, version = version);
    }
    on eCas do (c: tCas) {
      if (c.expected != version) {
        // Another gateway wrote since this one loaded. (A metric or log line in a real
        // store client.) The LostCasThenReject scenario uses it to confirm the race
        // was reached; no contract clause needs it.
        // snappea-lint: allow mutation:remove-statement -- observability for the scenario only
        announce eCasLost, c.req;
        send c.client, eCasResp, false;
        return;
      }
      window = c.window;
      count = c.count;
      // snappea-lint: allow mutation:swap-operator -- `version - 1` is equivalent: the CAS needs only a version never handed out before
      version = version + 1;
      // Commit point of an admission: announce what the store now holds.
      announce eAdmitted, (req = c.req, window = window, count = count);
      send c.client, eCasResp, true;
    }
  }
}
