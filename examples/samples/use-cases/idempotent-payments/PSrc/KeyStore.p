// The idempotency-key table. One record per key: in progress, or done with the
// result to return to every later request with that key.
//
// Real atomic primitives (one conditional write each, e.g. a unique-key insert):
//   claim:    absent -> in progress (caller owns the key); otherwise return the
//             record, and if it is still in progress register the caller to be told
//             the result. Check and register are one step, so the completion cannot
//             slip in between them and be missed.
//   complete: in progress -> done(result), then notify the registered callers.

enum tResult { Approved, Declined }
enum tClaim { Claimed, InProgress, Finished }

event eClaim: (api: machine, key: int);
event eClaimResp: (key: int, claim: tClaim, result: tResult);
event eComplete: (key: int, result: tResult);
// To callers that found the key in progress.
event eKeyDone: (key: int, result: tResult);

// done: false while the request that claimed the key is charging; waiters are the
// callers to tell when it finishes.
type tKeyRecord = (done: bool, result: tResult, waiters: set[machine]);

machine KeyStore {
  var records: map[int, tKeyRecord];

  start state Serving {
    on eClaim do (c: (api: machine, key: int)) {
      if (!(c.key in records)) {
        records[c.key] = (done = false, result = Declined, waiters = default(set[machine]));
        send c.api, eClaimResp, (key = c.key, claim = Claimed, result = Declined);
        return;
      }
      if (records[c.key].done) {
        send c.api, eClaimResp, (key = c.key, claim = Finished, result = records[c.key].result);
        return;
      }
      records[c.key].waiters += (c.api);
      send c.api, eClaimResp, (key = c.key, claim = InProgress, result = Declined);
    }
    on eComplete do (c: (key: int, result: tResult)) {
      var w: machine;
      foreach (w in records[c.key].waiters) {
        send w, eKeyDone, (key = c.key, result = c.result);
      }
      records[c.key] = (done = true, result = c.result, waiters = default(set[machine]));
    }
  }
}
