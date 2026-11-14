// The idempotency-key table. One record per key: in progress, or done with the
// result to return to every later request with that key.
//
// Real atomic primitives (one conditional write each, e.g. a unique-key insert):
//   claim:    absent -> in progress (caller owns the key); otherwise return the
//             record, and if it is still in progress register the caller to be told
//             the result. Check and register are one step, so the completion cannot
//             slip in between them and be missed.
//   complete: in progress -> done(result), then notify the registered callers.

enum ChargeResult { Approved, Declined }
enum ClaimStatus { Claimed, InProgress, Finished }

event Claim: (api: machine, key: int);
event ClaimResponse: (key: int, claim: ClaimStatus, result: ChargeResult);
event Complete: (key: int, result: ChargeResult);
// To callers that found the key in progress.
event KeyDone: (key: int, result: ChargeResult);

// done: false while the request that claimed the key is charging; waiters are the
// callers to tell when it finishes.
type KeyRecord = (done: bool, result: ChargeResult, waiters: set[machine]);

machine KeyStore {
  var records: map[int, KeyRecord];

  start state Serving {
    on Claim do (c: (api: machine, key: int)) {
      if (!(c.key in records)) {
        records[c.key] = (done = false, result = Declined, waiters = default(set[machine]));
        send c.api, ClaimResponse, (key = c.key, claim = Claimed, result = Declined);
        return;
      }
      if (records[c.key].done) {
        send c.api, ClaimResponse, (key = c.key, claim = Finished, result = records[c.key].result);
        return;
      }
      records[c.key].waiters += (c.api);
      send c.api, ClaimResponse, (key = c.key, claim = InProgress, result = Declined);
    }
    on Complete do (c: (key: int, result: ChargeResult)) {
      var w: machine;
      foreach (w in records[c.key].waiters) {
        send w, KeyDone, (key = c.key, result = c.result);
      }
      records[c.key] = (done = true, result = c.result, waiters = default(set[machine]));
    }
  }
}
