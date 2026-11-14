// The idempotency-key table. One record per key: in progress, or done with the
// result to return to every later request with that key.
//
// Real atomic primitives (one conditional write each, e.g. a unique-key insert):
//   claim:    absent -> in progress, and the caller owns the key; otherwise return
//             the record as it is (in progress, or done with its result)
//   complete: in progress -> done(result)

enum ChargeResult { Approved, Declined }
enum ClaimStatus { Claimed, InProgress, Finished }

event Claim: (api: machine, key: int);
event ClaimResponse: (claim: ClaimStatus, result: ChargeResult);
event Complete: (key: int, result: ChargeResult);

// done: false while the request that claimed the key is charging.
type KeyRecord = (done: bool, result: ChargeResult);

machine KeyStore {
  var records: map[int, KeyRecord];

  start state Serving {
    on Claim do (c: (api: machine, key: int)) {
      if (!(c.key in records)) {
        // commit point: the unique-key insert
        records[c.key] = (done = false, result = Declined);
        send c.api, ClaimResponse, (claim = Claimed, result = Declined);
        return;
      }
      if (records[c.key].done) {
        send c.api, ClaimResponse, (claim = Finished, result = records[c.key].result);
        return;
      }
      send c.api, ClaimResponse, (claim = InProgress, result = Declined);
    }
    on Complete do (c: (key: int, result: ChargeResult)) {
      records[c.key] = (done = true, result = c.result);
    }
  }
}
